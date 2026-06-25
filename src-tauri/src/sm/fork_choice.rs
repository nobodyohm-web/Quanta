//! `fork_choice` — the **LMD-GHOST, finality-aware fork-choice engine**:
//! **GADGET-5A** of the finality gadget (`DESIGN-FINALITY-GADGET.md` §9).
//!
//! Longest-chain is the wrong rule for a stake-voted chain: it ignores *who*
//! backed a branch. GHOST follows, at each fork, the child carrying the **most
//! vote weight** — and, made finality-aware (Gasper-style), it is **anchored at
//! the last justified checkpoint** (GADGET-3) and **floored at the last finalized
//! one**, so it can never undo irreversible history.
//!
//! This module is the **engine only**. It computes a head from (a block tree, the
//! latest votes, the on-chain stake snapshot, the justified anchor, the finalized
//! floor). It does **not** resolve partitions or rewrite the ledger itself —
//! **GADGET-5B** (`Node::reconcile_fork` + `Ledger::reorg_to_fork`) consumes this
//! engine to do that at partition heal, flipping the multi-block partition test
//! from *diverge* to *reconcile* with global conservation. No live slashing here
//! either (design §9 STOP on both).
//!
//! Three pieces, in order:
//! 1. **§1 LMD** — keep each validator's **latest** vote ([`LatestVotes`]).
//! 2. **§2 GHOST weight** — a block's weight = Σ stake of validators whose latest
//!    vote **descends from** it ([`branch_weights`], private).
//! 3. **§3 the rule** — descend from the justified anchor to the heaviest child,
//!    never below the finalized floor ([`ghost_head`]).
//!
//! # Determinism (`sm/` sans-IO, C1)
//! Everything is a **pure function** of the inputs: no clock, no entropy, and
//! **no `HashMap` iteration order in the verdict**. The latest-vote store is
//! **order-independent** (two nodes that see the same votes in any order agree);
//! weights are integer sums (commutative, accumulated over a `BTreeMap`); the
//! tie-break at a fork is the **smallest child hash** (a total, deterministic
//! order). So the same votes + the same stake yield the **same head**, byte-for-byte.
//!
//! # Reuse (no redefinition)
//! Votes are GADGET-2 [`Vote`]s; the anchor/floor come from GADGET-3's
//! [`FinalityState`] ([`anchors`]); stake is the on-chain ML-DSA-keyed weight
//! (ONCHAIN-STAKE-1). The latest votes are taken to be **already GADGET-2
//! verified** when observed — the engine weighs by stake, so a non-validator
//! (stake 0) contributes nothing regardless.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::finality_rule::FinalityState;
use super::finality_vote::Vote;

// ─── §1 LMD — the latest vote per validator ────────────────────────────────────

/// **§1 latest-message store.** Per validator, their **latest** vote — the only
/// vote that counts for fork-choice weight (LMD: *latest message driven*). A
/// validator is keyed by its **ML-DSA address** (the vote identity, coherent with
/// stake since the re-keying). The store is **order-independent** so two nodes
/// that ingest the same votes in any order hold the same state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LatestVotes {
    by_validator: BTreeMap<String, Vote>,
}

impl LatestVotes {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Observe one (already GADGET-2-verified) vote. It **replaces** the
    /// validator's stored vote iff it is **strictly later** — a greater **target
    /// epoch** (the design's "latest" rule). A tie on epoch resolves
    /// deterministically to the **smaller target hash**, so `observe` is
    /// **order-independent**: the final store depends only on the *set* of votes,
    /// never the order they arrived (what cross-node agreement rests on). An older
    /// vote is ignored.
    pub fn observe(&mut self, vote: Vote) {
        let replace = match self.by_validator.get(&vote.validator) {
            None => true,
            Some(cur) => {
                vote.target.epoch > cur.target.epoch
                    || (vote.target.epoch == cur.target.epoch
                        && vote.target.hash < cur.target.hash)
            }
        };
        if replace {
            self.by_validator.insert(vote.validator.clone(), vote);
        }
    }

    /// The validator's current latest vote, if any.
    pub fn get(&self, validator: &str) -> Option<&Vote> {
        self.by_validator.get(validator)
    }

    /// `(validator, latest vote)` pairs in validator order (deterministic).
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Vote)> {
        self.by_validator.iter()
    }

    /// Number of validators with a stored vote.
    pub fn len(&self) -> usize {
        self.by_validator.len()
    }

    /// Whether no validator has voted yet.
    pub fn is_empty(&self) -> bool {
        self.by_validator.is_empty()
    }
}

// ─── the block tree (fork-choice substrate) ────────────────────────────────────

/// The **block tree** GHOST walks: each block points to its parent, so competing
/// branches share ancestors. Hashes are the only identity used — heights are
/// irrelevant to fork-choice topology (GADGET-1 already pins epoch boundaries).
/// Children are held in a `BTreeSet` so iteration is **hash-sorted** — the source
/// of the deterministic tie-break.
#[derive(Debug, Clone, Default)]
pub struct BlockTree {
    /// child hash → parent hash.
    parent: BTreeMap<String, String>,
    /// parent hash → sorted child hashes.
    children: BTreeMap<String, BTreeSet<String>>,
}

impl BlockTree {
    /// An empty tree.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a **root** block (no parent) as a known node — the anchor a tree
    /// is built up from.
    pub fn add_root(&mut self, hash: &str) {
        self.children.entry(hash.to_string()).or_default();
    }

    /// Add `hash` as a child of `parent`. Idempotent; both become known nodes.
    /// (A block's parent is set once; re-adding with the same parent is a no-op.)
    pub fn add_block(&mut self, hash: &str, parent: &str) {
        self.parent.insert(hash.to_string(), parent.to_string());
        self.children
            .entry(parent.to_string())
            .or_default()
            .insert(hash.to_string());
        self.children.entry(hash.to_string()).or_default();
    }

    /// Whether `hash` is a known node.
    pub fn contains(&self, hash: &str) -> bool {
        self.children.contains_key(hash) || self.parent.contains_key(hash)
    }

    /// `hash`'s parent, if it has one (a root has none).
    pub fn parent_of(&self, hash: &str) -> Option<&str> {
        self.parent.get(hash).map(String::as_str)
    }

    /// `hash`'s children, **hash-sorted** (deterministic).
    pub fn children_of(&self, hash: &str) -> impl Iterator<Item = &String> {
        self.children.get(hash).into_iter().flatten()
    }

    /// Whether `desc` **is** `anc` or descends from it (walking parents up). The
    /// walk is **bounded by the node count**, so a malformed (cyclic) input can
    /// never hang — it returns `false` instead (panic/loop freedom, Constitution).
    pub fn is_descendant(&self, desc: &str, anc: &str) -> bool {
        let mut cur = desc;
        for _ in 0..=self.parent.len() {
            if cur == anc {
                return true;
            }
            match self.parent.get(cur) {
                Some(p) => cur = p.as_str(),
                None => return false,
            }
        }
        false
    }
}

// ─── §2 GHOST branch weight (by stake) ─────────────────────────────────────────

/// **§2 per-block support weight.** Each validator's latest-vote stake is
/// attributed to its **target block and every ancestor up to `anchor`** — so a
/// block's weight is the total stake whose latest vote descends from it. A vote
/// whose target is unknown to the tree, or off the `anchor`'s subtree, weighs
/// nothing (it cannot back this branch). Pure; integer sums (commutative), the
/// ancestor walk bounded by the node count (no hang on cyclic input).
fn branch_weights(
    tree: &BlockTree,
    latest: &LatestVotes,
    stakes: &HashMap<String, u64>,
    anchor: &str,
) -> BTreeMap<String, u64> {
    let mut weight: BTreeMap<String, u64> = BTreeMap::new();
    for (validator, vote) in latest.iter() {
        let target = vote.target.hash.as_str();
        // Only votes inside the anchored subtree contribute to its fork-choice.
        if !tree.is_descendant(target, anchor) {
            continue;
        }
        let stake = stakes.get(validator).copied().unwrap_or(0);
        if stake == 0 {
            continue; // not an active validator ⇒ no weight
        }
        // Credit `target` and each ancestor up to (and including) `anchor`.
        let mut cur = target;
        for _ in 0..=tree.parent.len() {
            let w = weight.entry(cur.to_string()).or_insert(0);
            *w = w.saturating_add(stake);
            if cur == anchor {
                break;
            }
            match tree.parent_of(cur) {
                Some(p) => cur = p,
                None => break, // unreachable for a descendant of anchor
            }
        }
    }
    weight
}

// ─── §3 the GHOST rule, anchored to finality ───────────────────────────────────

/// **§3 the fork-choice head.** Start at `justified_anchor` (GADGET-3's last
/// justified checkpoint) and, at each fork, descend to the child of **greatest
/// weight** (§2), breaking ties by **smallest hash**, until a leaf — that leaf is
/// the head.
///
/// **Finality is the floor.** The result **always descends from
/// `finalized_floor`**: the walk only goes *down* from the anchor, and if the
/// anchor is unknown or does **not** descend from the floor (inconsistent input),
/// the engine falls back to the floor itself — it never selects a branch that
/// would contradict finalized history. Pure, deterministic.
pub fn ghost_head(
    tree: &BlockTree,
    latest: &LatestVotes,
    stakes: &HashMap<String, u64>,
    justified_anchor: &str,
    finalized_floor: &str,
) -> String {
    // Finality floor (absolute): the anchor must be a known block that descends
    // from the last finalized checkpoint. Otherwise stay on finalized ground.
    if !tree.contains(justified_anchor) || !tree.is_descendant(justified_anchor, finalized_floor) {
        return finalized_floor.to_string();
    }
    let weight = branch_weights(tree, latest, stakes, justified_anchor);
    let mut head = justified_anchor.to_string();
    // Descend to a leaf. Bounded by the node count (a tree has no cycles ⇒ this
    // terminates at a leaf well before the bound; the bound only caps bad input).
    for _ in 0..=tree.parent.len() {
        // children_of yields hashes ascending; keeping the FIRST strict-maximum
        // (`w > best`) yields the smallest-hash child among the heaviest — the
        // deterministic tie-break.
        let mut best: Option<(&String, u64)> = None;
        for child in tree.children_of(&head) {
            let w = weight.get(child).copied().unwrap_or(0);
            match best {
                Some((_, bw)) if w <= bw => {}
                _ => best = Some((child, w)),
            }
        }
        match best {
            Some((child, _)) => head = child.clone(),
            None => break, // leaf reached
        }
    }
    head
}

/// The fork-choice **anchor** (last justified) and **floor** (last finalized) for
/// a GADGET-3 [`FinalityState`]: the **highest-epoch** checkpoint in each set
/// (both are epoch-ordered `BTreeMap`s, so `iter().last()` is the latest). On a
/// genesis-only state both are genesis — so a fresh node already anchors
/// correctly. This is how `ghost_head` is fed from real finality state without
/// the engine reaching into GADGET-3's internals.
pub fn anchors(state: &FinalityState) -> (String, String) {
    let anchor = state
        .justified()
        .iter()
        .last()
        .map(|c| c.hash.clone())
        .unwrap_or_default();
    let floor = state
        .finalized()
        .iter()
        .last()
        .map(|c| c.hash.clone())
        .unwrap_or_default();
    (anchor, floor)
}

// ─── §4 The teeth (GHOST must beat longest-chain, finality must floor it) ───────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sm::finality::Checkpoint;

    const E: u64 = 4; // small epoch length (parametric — any E ≥ 1)

    /// A latest-message vote for fork-choice: only `validator`, `target.hash` and
    /// `target.epoch` matter to the engine, so the source is a fixed stub and the
    /// signature empty (the engine never re-verifies — votes are pre-verified by
    /// GADGET-2 upstream; weight is gated by stake).
    fn vote(validator: &str, target_hash: &str, target_epoch: u64) -> Vote {
        Vote {
            source: Checkpoint { epoch: 0, height: 0, hash: "ROOT".into() },
            target: Checkpoint {
                epoch: target_epoch,
                height: target_epoch * E,
                hash: target_hash.into(),
            },
            voting_epoch: target_epoch,
            validator: validator.into(),
            signature: Vec::new(),
        }
    }

    /// Equal-stake map (`100` µQTA each) for the named validators.
    fn stakes(names: &[&str]) -> HashMap<String, u64> {
        names.iter().map(|n| (n.to_string(), 100u64)).collect()
    }

    // ── §4.1 weight beats length (the heart: GHOST vs longest-chain) ──────────

    #[test]
    fn gadget5a_weight_beats_length() {
        // Tree rooted at R:
        //   short, heavy branch:  R → X   (2 voters, 200 µQTA)
        //   long,  light branch:  R → Y1 → Y2 → Y3   (1 voter, 100 µQTA)
        // Longest-chain would pick Y3 (depth 3). GHOST picks X — more vote weight
        // decides at the FORK (R's children), not chain length.
        let mut tree = BlockTree::new();
        tree.add_root("R");
        tree.add_block("X", "R");
        tree.add_block("Y1", "R");
        tree.add_block("Y2", "Y1");
        tree.add_block("Y3", "Y2");

        let mut latest = LatestVotes::new();
        latest.observe(vote("a", "X", 1));
        latest.observe(vote("b", "X", 1));
        latest.observe(vote("c", "Y3", 1));
        let st = stakes(&["a", "b", "c"]);

        let head = ghost_head(&tree, &latest, &st, "R", "R");
        assert_eq!(head, "X", "the heavier (shorter) branch wins over the longer, lighter one");
    }

    #[test]
    fn gadget5a_branch_weight_sums_supporting_stake() {
        // Direct §2 check: a block's weight is the Σ stake of validators whose
        // latest vote descends from it. Here both a and b back X ⇒ weight(X)=200,
        // and that propagates up to the root R.
        let mut tree = BlockTree::new();
        tree.add_root("R");
        tree.add_block("X", "R");
        tree.add_block("Y", "R");
        let mut latest = LatestVotes::new();
        latest.observe(vote("a", "X", 1));
        latest.observe(vote("b", "X", 1));
        latest.observe(vote("c", "Y", 1));
        let st = stakes(&["a", "b", "c"]);
        let w = branch_weights(&tree, &latest, &st, "R");
        assert_eq!(w.get("X").copied(), Some(200), "X backed by a+b");
        assert_eq!(w.get("Y").copied(), Some(100), "Y backed by c");
        assert_eq!(w.get("R").copied(), Some(300), "all weight propagates to the root");
    }

    // ── §4.2 latest vote replaces (LMD) ──────────────────────────────────────

    #[test]
    fn gadget5a_latest_vote_replaces_old_one() {
        // a votes X (epoch 1) → head X. a then re-votes Y (epoch 2): the OLD vote
        // stops counting, weight shifts to Y, and the head flips. The validator's
        // weight follows its LATEST message only.
        let mut tree = BlockTree::new();
        tree.add_root("R");
        tree.add_block("X", "R");
        tree.add_block("Y", "R");
        let st = stakes(&["a"]);

        let mut latest = LatestVotes::new();
        latest.observe(vote("a", "X", 1));
        assert_eq!(ghost_head(&tree, &latest, &st, "R", "R"), "X", "first vote backs X");

        latest.observe(vote("a", "Y", 2)); // later target epoch ⇒ replaces
        assert_eq!(latest.get("a").map(|v| v.target.hash.as_str()), Some("Y"), "store holds the latest");
        assert_eq!(branch_weights(&tree, &latest, &st, "R").get("X").copied().unwrap_or(0), 0, "old vote gone");
        assert_eq!(ghost_head(&tree, &latest, &st, "R", "R"), "Y", "re-vote flips the head");
    }

    #[test]
    fn gadget5a_stale_vote_does_not_replace() {
        // A vote with a LOWER target epoch is stale and must be ignored.
        let mut latest = LatestVotes::new();
        latest.observe(vote("a", "Y", 2));
        latest.observe(vote("a", "X", 1)); // older ⇒ ignored
        assert_eq!(latest.get("a").map(|v| v.target.hash.as_str()), Some("Y"), "stale vote ignored");
    }

    // ── §4.3 finality floor (absolute) ───────────────────────────────────────

    #[test]
    fn gadget5a_finality_floor_is_absolute() {
        // A heavily-voted branch that does NOT descend from the finalized floor F
        // must be ignored. Anchored subtree: F → J → H (lightly voted). A separate
        // off-floor branch Z → C carries far more votes, but it conflicts with
        // finality, so GHOST never selects it.
        let mut tree = BlockTree::new();
        tree.add_root("F");
        tree.add_block("J", "F");
        tree.add_block("H", "J");
        tree.add_root("Z"); // a different root — NOT under F
        tree.add_block("C", "Z");

        let mut latest = LatestVotes::new();
        latest.observe(vote("h", "H", 3)); // one vote inside finality
        latest.observe(vote("c1", "C", 3)); // three votes OFF finality
        latest.observe(vote("c2", "C", 3));
        latest.observe(vote("c3", "C", 3));
        let st = stakes(&["h", "c1", "c2", "c3"]);

        let head = ghost_head(&tree, &latest, &st, "J", "F");
        assert_eq!(head, "H", "the head stays on the finalized branch despite the heavier off-floor fork");
        assert!(tree.is_descendant(&head, "F"), "the head always descends from the finalized floor");
        assert_ne!(head, "C", "a branch conflicting with finality is never chosen");
    }

    #[test]
    fn gadget5a_anchor_off_floor_falls_back_to_floor() {
        // Defensive: if the supplied anchor does NOT descend from the floor
        // (inconsistent input), the engine returns the floor — never below finality.
        let mut tree = BlockTree::new();
        tree.add_root("F");
        tree.add_root("Z");
        tree.add_block("C", "Z"); // C is off the F branch
        let latest = LatestVotes::new();
        let st = stakes(&["x"]);
        assert_eq!(ghost_head(&tree, &latest, &st, "C", "F"), "F", "off-floor anchor ⇒ fall back to the floor");
        assert_eq!(ghost_head(&tree, &latest, &st, "UNKNOWN", "F"), "F", "unknown anchor ⇒ fall back to the floor");
    }

    // ── §4.4 anchored at the last justified (not genesis) ─────────────────────

    #[test]
    fn gadget5a_anchors_track_last_justified_not_genesis() {
        use crate::security::hybrid_crypto::{derive_ml_dsa, ml_dsa_sign_deterministic};
        use crate::sm::finality_vote::MlDsaCertificate;

        // Genesis-only state: anchor AND floor are genesis.
        let mut state = FinalityState::genesis_only("GENESIS".into());
        assert_eq!(anchors(&state), ("GENESIS".to_string(), "GENESIS".to_string()), "fresh node anchors at genesis");

        // Justify c1 via a ⅔ certificate g→c1 (reusing GADGET-2/3). The anchor must
        // advance to c1 — the LAST justified — while the floor stays genesis (c1 is
        // justified, not yet finalized: two-step).
        let cp = |epoch: u64, hash: &str| Checkpoint { epoch, height: epoch * E, hash: hash.to_string() };
        let g = cp(0, "GENESIS");
        let c1 = cp(1, "c1-hash");
        let signed = |seed: u8| -> Vote {
            let (sk, pk) = derive_ml_dsa(&[seed; 32]).expect("derive");
            let mut v = Vote { source: g.clone(), target: c1.clone(), voting_epoch: 1, validator: pk, signature: Vec::new() };
            v.signature = ml_dsa_sign_deterministic(&sk, &v.signable_bytes()).expect("sign");
            v
        };
        let st: HashMap<String, u64> = [1u8, 2].iter().map(|s| (derive_ml_dsa(&[*s; 32]).expect("derive").1, 100u64)).collect();
        let cert = MlDsaCertificate::new(g.clone(), c1.clone(), vec![signed(1), signed(2)]);
        state.apply_certificate(&cert, &st, E);

        let (anchor, floor) = anchors(&state);
        assert_eq!(anchor, "c1-hash", "fork-choice anchors at the LAST justified checkpoint, not genesis");
        assert_eq!(floor, "GENESIS", "the floor is the last finalized (still genesis — c1 only justified)");

        // And GHOST from that anchor never reconsiders the genesis→c1 fork: a heavy
        // sibling c1' of c1 (also off the anchor) is ignored; the head stays under c1.
        let mut tree = BlockTree::new();
        tree.add_root("GENESIS");
        tree.add_block("c1-hash", "GENESIS");
        tree.add_block("c1-prime", "GENESIS"); // a conflicting epoch-1 sibling
        tree.add_block("h2", "c1-hash");
        let mut latest = LatestVotes::new();
        latest.observe(vote("a", "h2", 2));
        latest.observe(vote("b", "c1-prime", 2)); // weight off the anchor
        latest.observe(vote("c", "c1-prime", 2));
        let head = ghost_head(&tree, &latest, &stakes(&["a", "b", "c"]), &anchor, &floor);
        assert_eq!(head, "h2", "the engine extends the justified anchor, ignoring the off-anchor sibling");
    }

    // ── §4.5 deterministic tie-break + cross-node determinism ─────────────────

    #[test]
    fn gadget5a_equal_weight_tie_breaks_on_smallest_hash() {
        // Two children of equal weight (one voter each): the smaller hash wins.
        let mut tree = BlockTree::new();
        tree.add_root("R");
        tree.add_block("aaa", "R");
        tree.add_block("bbb", "R");
        let mut latest = LatestVotes::new();
        latest.observe(vote("v1", "aaa", 1));
        latest.observe(vote("v2", "bbb", 1));
        let head = ghost_head(&tree, &latest, &stakes(&["v1", "v2"]), "R", "R");
        assert_eq!(head, "aaa", "equal weight ⇒ smallest child hash wins (deterministic tie-break)");
    }

    #[test]
    fn gadget5a_head_is_deterministic_across_observation_order() {
        // Same votes + same stake ⇒ same head, regardless of the order votes are
        // observed (order-independent LMD). This is the cross-node agreement the
        // engine rests on (C1 in miniature).
        let mut tree = BlockTree::new();
        tree.add_root("R");
        tree.add_block("X", "R");
        tree.add_block("Y", "R");
        tree.add_block("X2", "X");
        let st = stakes(&["a", "b", "c"]);
        // Includes a stale re-vote (a: X@1 then X2@2) to exercise replacement.
        let votes = [vote("a", "X", 1), vote("b", "Y", 1), vote("c", "X2", 2), vote("a", "X2", 2)];

        let head_in_order = |order: &[usize]| -> String {
            let mut latest = LatestVotes::new();
            for &i in order {
                latest.observe(votes[i].clone());
            }
            ghost_head(&tree, &latest, &st, "R", "R")
        };
        let forward = head_in_order(&[0, 1, 2, 3]);
        let shuffled = head_in_order(&[3, 1, 0, 2]);
        let reversed = head_in_order(&[3, 2, 1, 0]);
        assert_eq!(forward, shuffled, "head is independent of observation order");
        assert_eq!(forward, reversed, "head is independent of observation order");
        assert_eq!(forward, "X2", "a+c back X2 (200) over b's Y (100) ⇒ head descends to X2");
    }
}
