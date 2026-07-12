//! `finality_live` — **LIVE-1**: the IO-layer glue that carries the finality
//! gadget from *proven-in-simulation* to *running on the network*
//! (`docs/DESIGN-LIVE-WIRING.md` §2.1–2.2, §3).
//!
//! The consensus **core** (`crate::sm::finality_vote` / `finality_rule` /
//! `fork_choice`) is decided and proven; this module **invents no rule**. It
//! only:
//!   1. **holds** the live gadget state — the latest votes
//!      ([`LatestVotes`], GADGET-5A), the justify/finalize state
//!      ([`FinalityState`], GADGET-3), the block tree, and a per-link vote pool;
//!   2. **feeds** it from the network — a received [`Vote`] is re-verified
//!      ([`Vote::verify`]) against the on-chain stake, observed into the
//!      fork-choice, and, once a link reaches a ⅔ certificate
//!      ([`MlDsaCertificate`]), applied to the finality rule;
//!   3. **bridges** the one identity seam between the pure core and the live
//!      ledger — a vote is keyed by the validator's **ML-DSA public key** (what
//!      its signature verifies against), while the ledger keys stake by the
//!      **address** `BLAKE3(ADDR_DOMAIN ‖ pk)`. [`Ledger::validator_stakes_by_pubkey`]
//!      re-keys the on-chain snapshot purely from the chain (every `Stake` tx
//!      reveals its `pq_public_key`), so the gadget sees a pubkey-keyed stake map
//!      whose total is the true staked weight.
//!
//! # The sans-IO frontier (Constitution §3, C1) — not crossed here
//! Every verdict — vote validity, certificate quorum, justify/finalize, the
//! fork-choice head — is produced by a **pure** `sm/` function over ordered
//! structures (`BTreeMap`/`BTreeSet`), fed **injected** data (the vote, the
//! chain-derived stake map). This module reads the ledger and appends to
//! `BTreeMap`-backed pools; it never lets a clock, `OsRng`, or `HashMap`
//! iteration order touch a verdict. Two nodes that observe the same votes over
//! the same chain reach the **same** finality state — the property GADGET-3/5
//! proved, preserved across the IO boundary. The core's own determinism test
//! (C1) is untouched; this glue is tested **separately** (see the module tests).

use std::collections::BTreeMap;

use crate::p2p::ledger::Ledger;
use crate::security::CryptoEngine;
use crate::sm::finality::{checkpoint_at_epoch, epoch_of_height, is_epoch_boundary, Checkpoint};
use crate::sm::finality_rule::FinalityState;
use crate::sm::finality_vote::{FinalityCertificate, MlDsaCertificate, Vote};
use crate::sm::fork_choice::{anchors, ghost_head, BlockTree, LatestVotes};

/// The epoch length the live gadget runs at — ADR-009's ratified default
/// (`EPOCH_LENGTH_BLOCKS = 32`). A single constant so the whole live path agrees
/// with the core (the pure functions are parametric in `epoch_len`).
pub use crate::sm::finality::EPOCH_LENGTH_BLOCKS;

/// A link `(source, target)` identifying one certificate the pool accumulates
/// votes for. Ordered (derives `Ord`) so the pool is a `BTreeMap` — no `HashMap`
/// iteration order reaches a verdict.
type Link = (Checkpoint, Checkpoint);

/// **The live finality gadget state (LIVE-1).** Bundles the fork-choice latest
/// votes, the justify/finalize state, the block tree GHOST walks, and the
/// per-link vote pool that turns individual votes into ⅔ certificates. Held by
/// the node behind an `RwLock`; mutated only through [`FinalityTracker::ingest_vote`]
/// and [`FinalityTracker::observe_block`].
#[derive(Debug, Clone)]
pub struct FinalityTracker {
    /// GADGET-5A — each validator's latest vote (order-independent).
    latest: LatestVotes,
    /// GADGET-3 — the justified/finalized sets.
    state: FinalityState,
    /// GADGET-5A substrate — the block tree (child → parent).
    tree: BlockTree,
    /// Votes grouped by the link they attest, awaiting a ⅔ certificate. Cleared
    /// per link once that link's certificate has been applied (it cannot advance
    /// finality twice). Distinctness/validity are enforced by the certificate,
    /// not here.
    pool: BTreeMap<Link, Vec<Vote>>,
    /// The epoch length this tracker runs at — [`EPOCH_LENGTH_BLOCKS`] in
    /// production (`new`). Carried as a field (not the bare constant) because the
    /// `sm/` verdict functions are all **parametric in `epoch_len`**; this keeps
    /// the whole live path agreeing on one value and lets IO tests exercise the
    /// justify/finalize path at a small `E` without sealing 32 real blocks.
    epoch_len: u64,
}

/// What ingesting one vote advanced — for observability/tests. `accepted` false
/// ⇒ the vote failed verification (forged, stale, or from a non-validator) and
/// changed nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IngestOutcome {
    /// The vote verified and was observed into the fork-choice.
    pub accepted: bool,
    /// A ⅔ certificate formed on this vote's link and **justified** its target.
    pub justified: bool,
    /// …and **finalized** its source (two consecutive linked epochs).
    pub finalized: bool,
}

impl FinalityTracker {
    /// A fresh tracker anchored at `genesis_hash` (from the live ledger), running
    /// at the production epoch length ([`EPOCH_LENGTH_BLOCKS`]). Genesis is the
    /// only known block and the only justified/finalized checkpoint — exactly a
    /// fresh node's honest starting state.
    pub fn new(genesis_hash: String) -> Self {
        Self::with_epoch_len(genesis_hash, EPOCH_LENGTH_BLOCKS)
    }

    /// As [`Self::new`] but at an explicit `epoch_len` — for IO tests that drive
    /// the justify/finalize path at a small `E`. The `sm/` verdicts are parametric
    /// in `epoch_len`, so this changes only *how often* checkpoints fall, never
    /// the logic. `epoch_len` is clamped to `≥ 1` (mirrors the `sm/` helpers).
    pub fn with_epoch_len(genesis_hash: String, epoch_len: u64) -> Self {
        let mut tree = BlockTree::new();
        tree.add_root(&genesis_hash);
        Self {
            latest: LatestVotes::new(),
            state: FinalityState::genesis_only(genesis_hash),
            tree,
            pool: BTreeMap::new(),
            epoch_len: epoch_len.max(1),
        }
    }

    /// Read-only view of the finality state (justified/finalized sets) — what
    /// LIVE-2's proposal loop will anchor on.
    pub fn state(&self) -> &FinalityState {
        &self.state
    }

    /// Read-only view of the latest votes (fork-choice input).
    pub fn latest_votes(&self) -> &LatestVotes {
        &self.latest
    }

    /// **Learn the chain's block tree** from the live ledger (pure over the
    /// chain). Registers each block as a child of its predecessor so GHOST has a
    /// tree to walk. Idempotent — re-adding a known edge is a no-op — so it can
    /// be called after every integrated block. Genesis stays the root.
    pub fn observe_chain(&mut self, ledger: &Ledger) {
        let height = ledger.chain_height();
        for i in 1..height {
            let (Some(block), Some(prev)) = (ledger.block_at(i), ledger.block_at(i - 1)) else {
                continue;
            };
            self.tree.add_block(&block.hash, &prev.hash);
        }
    }

    /// **Ingest one finality vote (the LIVE-1 receive path).** Re-verifies the
    /// vote against the **chain-derived** pubkey-keyed stake snapshot (GADGET-2),
    /// observes it into the fork-choice, and — if its link now carries a ⅔
    /// certificate whose source is justified — advances the finality rule
    /// (GADGET-3). Every decision is a pure `sm/` function; this method only
    /// routes data. Returns what advanced.
    ///
    /// A vote that fails verification (forged signature, non-validator,
    /// malformed link) changes **nothing** — `accepted == false`.
    pub fn ingest_vote(&mut self, vote: Vote, ledger: &Ledger) -> IngestOutcome {
        let stakes = ledger.validator_stakes_by_pubkey();
        // GADGET-2 gate: only a well-formed, staked, correctly-signed vote counts.
        if !vote.verify(&stakes, self.epoch_len) {
            return IngestOutcome::default();
        }
        // Fork-choice (GADGET-5A): the latest vote per validator drives GHOST.
        self.latest.observe(vote.clone());

        // Accumulate toward a certificate for this vote's link.
        let link: Link = (vote.source.clone(), vote.target.clone());
        let entry = self.pool.entry(link.clone()).or_default();
        // De-dup a validator's repeat vote for the SAME link (idempotent gossip);
        // the certificate also rejects double-counting, this just bounds the pool.
        if !entry.iter().any(|v| v.validator == vote.validator) {
            entry.push(vote.clone());
        }

        let mut outcome = IngestOutcome { accepted: true, ..Default::default() };

        // Try to form and apply a ⅔ certificate for the link (GADGET-2 → 3).
        let votes = entry.clone();
        let cert = MlDsaCertificate::new(link.0.clone(), link.1.clone(), votes);
        if cert.is_valid(&stakes, self.epoch_len) {
            let step = self.state.apply_certificate(&cert, &stakes, self.epoch_len);
            outcome.justified = step.justified;
            outcome.finalized = step.finalized;
            // Once a link's certificate has been applied it can advance finality
            // no further; drop the pool entry so it can't be re-applied and to
            // bound memory. (A later, distinct link accumulates its own entry.)
            if step.justified || step.finalized {
                self.pool.remove(&link);
            }
        }
        outcome
    }

    /// **The live fork-choice head** (GADGET-5A), anchored at the last justified
    /// checkpoint and floored at the last finalized one. Pure over the tree, the
    /// latest votes, and the chain-derived stake. This is what LIVE-2 will build
    /// the next block on (replacing the interim `chain.last()`); LIVE-1 only
    /// exposes it for observability + tests.
    pub fn head(&self, ledger: &Ledger) -> String {
        let stakes = ledger.validator_stakes_by_pubkey();
        let (anchor, floor) = anchors(&self.state);
        ghost_head(&self.tree, &self.latest, &stakes, &anchor, &floor)
    }

    /// This tracker's epoch length.
    pub fn epoch_len(&self) -> u64 {
        self.epoch_len
    }

    /// The checkpoint of `epoch` on the live chain, if the chain reaches its
    /// boundary — the target a vote for `epoch` attests. Pure over the chain.
    pub fn checkpoint_for_epoch(&self, ledger: &Ledger, epoch: u64) -> Option<Checkpoint> {
        // The ledger's `chain` is the block slice `checkpoint_at_epoch` expects;
        // reconstruct it via the public `block_at` accessor (heights 0..height).
        let height = ledger.chain_height();
        let blocks: Vec<_> = (0..height).filter_map(|i| ledger.block_at(i).cloned()).collect();
        checkpoint_at_epoch(&blocks, epoch, self.epoch_len)
    }
}

/// **The live cast path (LIVE-1).** Build the finality vote this node should
/// broadcast *now*, given the live ledger and the node's ML-DSA identity — or
/// `None` if there is nothing honest to attest (not on an epoch boundary, no
/// stake, or the source/target checkpoints don't exist yet).
///
/// The honest vote links the **last justified checkpoint** (`source`) to the
/// **current epoch-boundary checkpoint** (`target`): the attestation the gadget
/// consumes to justify (and, two in a row, finalize). Signing reuses the
/// engine's ML-DSA authority — no new key path. The caller serializes the vote
/// and gossips it as [`GossipMessage::FinalityVote`](crate::p2p::gossip::GossipMessage::FinalityVote).
///
/// # Determinism
/// The vote's *content* is a pure function of the chain + the identity; only the
/// signature uses the engine's (hedged) ML-DSA signer at the boundary — never a
/// verdict input. Two honest validators at the same chain state attest the
/// **same** link.
pub fn build_vote_to_cast(
    ledger: &Ledger,
    tracker: &FinalityTracker,
    crypto: &CryptoEngine,
) -> Option<Vote> {
    // Identity: this node's ML-DSA **primary** public key — the *authority* key the
    // ledger binds to the account (PQ-MIG-3B) and the one a vote's signature is
    // verified against — plus its address (`BLAKE3(ADDR_DOMAIN ‖ primary)`, used to
    // read on-chain stake). NB: `pq_identity_hex()` is the primary key; the
    // seed-derived legacy `ml_dsa` layer (`get_identity().pq_public_key_hex`) is a
    // *different* key and would not match the on-chain stake identity.
    let pk_hex = crypto.pq_identity_hex()?;
    let addr = crypto.pq_address_hex()?;

    // Only a bonded validator may vote (weight 0 ⇒ the vote would be rejected
    // anyway; don't spam the network with inert votes).
    if ledger.validator_stakes().get(&addr).copied().unwrap_or(0) == 0 {
        return None;
    }

    // Target: the checkpoint of the tip's epoch — only vote on an epoch boundary,
    // where a checkpoint actually exists.
    let tip_height = ledger.chain_height().saturating_sub(1);
    if !is_epoch_boundary(tip_height, tracker.epoch_len()) {
        return None;
    }
    let target_epoch = epoch_of_height(tip_height, tracker.epoch_len());
    let target = tracker.checkpoint_for_epoch(ledger, target_epoch)?;

    // Source: the last justified checkpoint (genesis on a fresh node). A vote
    // whose target is not strictly later than the source is not well-formed —
    // skip it (nothing new to attest yet).
    let source = tracker
        .state()
        .justified()
        .iter()
        .last()
        .cloned()
        .unwrap_or(Checkpoint {
            epoch: 0,
            height: 0,
            hash: ledger.genesis_hash(),
        });
    if target.height <= source.height {
        return None;
    }

    // Construct the unsigned vote, then sign its OWN canonical bytes
    // (`Vote::signable_bytes` — the exact pre-image `Vote::verify` re-checks), so
    // the signature binds every field. Production ML-DSA signing (`sign_pq`,
    // hedged) — the finality authority; the enclosing envelope's Ed25519 sig is
    // separate transport authentication.
    let mut vote = Vote {
        source,
        target: target.clone(),
        voting_epoch: target.epoch,
        validator: pk_hex,
        signature: Vec::new(),
    };
    vote.signature = crypto.sign_pq(&vote.signable_bytes()).ok()?;
    Some(vote)
}

// ─── LIVE-1 teeth — IO tested SEPARATELY from the core (the design's frontier) ──
//
// The `sm/` verdict logic (quorum, justify/finalize, fork-choice) already has C1
// + its own teeth. These tests exercise the **IO glue** only: the pubkey↔address
// bridge, the wire round-trip, the receive/ingest routing, and the cast builder.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::p2p::ledger::{Ledger, MICRO};
    use crate::security::CryptoEngine;

    const E: u64 = 2; // small epoch length: boundaries at heights 0, 2, 4 …

    /// A deterministic validator identity (Ed25519 + independent ML-DSA primary),
    /// mirroring `sm::sim::seeded_identity` — reproducible across the test.
    fn identity(seed: u8) -> CryptoEngine {
        let sk = [seed; 32];
        let mut c = CryptoEngine::new();
        c.import_keypair(&sk).expect("ed25519 secret");
        let mut pq_seed = [0u8; 32];
        pq_seed.copy_from_slice(&CryptoEngine::blake3_hash(
            &[b"QUANTA-PQ-PRIMARY-live1:".as_ref(), &sk].concat(),
        ));
        c.import_pq_identity(&pq_seed).expect("ml-dsa primary");
        c
    }

    /// A ledger where every `(identity, stake)` is **bonded through a real signed
    /// `Stake` tx** — so each staker's `pq_public_key` is revealed on-chain and
    /// `validator_stakes_by_pubkey()` can re-key them (genesis stakes carry no
    /// pubkey). Returns the sealed ledger.
    fn staked_ledger(validators: &[(&CryptoEngine, u64)]) -> Ledger {
        // Genesis gives each validator spendable balance (2× its stake), no genesis
        // stake — they bond via a signed tx below.
        let addrs: Vec<String> = validators
            .iter()
            .map(|(c, _)| c.pq_address_hex().expect("addr"))
            .collect();
        let alloc: Vec<(&str, u64, u64)> = addrs
            .iter()
            .zip(validators)
            .map(|(a, (_, stake))| (a.as_str(), stake * 2, 0u64))
            .collect();
        let mut ledger = Ledger::genesis_with_allocation(&alloc);
        for ((crypto, stake), addr) in validators.iter().zip(&addrs) {
            ledger
                .stake_tx_at(addr, *stake, crypto, "2026-07-01T00:00:00+00:00".into(), true)
                .expect("stake tx");
        }
        // Seal the stakes into block 1 (miner = first validator's address).
        ledger
            .seal_if_pending_at(&addrs[0], 0.0, "2026-07-01T00:01:00+00:00".to_string())
            .expect("seal block with stakes");
        ledger
    }

    fn cp(epoch: u64, hash: &str) -> Checkpoint {
        Checkpoint { epoch, height: epoch * E, hash: hash.to_string() }
    }

    /// A vote from `crypto` for the link `source → target`, signed over the vote's
    /// canonical bytes (the exact production pre-image).
    fn signed_vote(crypto: &CryptoEngine, source: &Checkpoint, target: &Checkpoint) -> Vote {
        let mut v = Vote {
            source: source.clone(),
            target: target.clone(),
            voting_epoch: target.epoch,
            validator: crypto.pq_identity_hex().unwrap(),
            signature: Vec::new(),
        };
        v.signature = crypto.sign_pq(&v.signable_bytes()).expect("sign vote");
        v
    }

    #[test]
    fn live1_stakes_rekeyed_by_pubkey_from_chain() {
        // The bridge: a validator bonded through a signed Stake tx appears in the
        // pubkey-keyed snapshot under its ML-DSA public key, with the address-keyed
        // stake — and the address↔pubkey binding is the graven one.
        let a = identity(1);
        let ledger = staked_ledger(&[(&a, 5 * MICRO)]);
        let pk = a.pq_identity_hex().unwrap();
        let addr = a.pq_address_hex().unwrap();

        let by_pk = ledger.validator_stakes_by_pubkey();
        assert_eq!(by_pk.get(&pk).copied(), Some(5 * MICRO), "stake re-keyed under the pubkey");
        assert_eq!(ledger.validator_stakes().get(&addr).copied(), Some(5 * MICRO), "and still address-keyed on-chain");
        assert!(
            CryptoEngine::address_hex_binds_key_hex(&addr, &pk),
            "the re-keyed pubkey is exactly the one that hashes to the staked address",
        );
    }

    #[test]
    fn live1_vote_roundtrips_through_gossip_json() {
        // Wire round-trip: a signed vote survives serialize → deserialize byte-for-
        // byte (the FinalityVote payload), and still verifies against the chain.
        let a = identity(1);
        let ledger = staked_ledger(&[(&a, 5 * MICRO)]);
        let vote = signed_vote(&a, &cp(0, "GENESIS"), &cp(1, "h2"));
        let json = serde_json::to_string(&vote).expect("serialize");
        let back: Vote = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(vote, back, "vote round-trips through the gossip JSON unchanged");
        assert!(
            back.verify(&ledger.validator_stakes_by_pubkey(), E),
            "the deserialized vote still verifies against on-chain stake",
        );
    }

    #[test]
    fn live1_ingest_rejects_forged_and_nonvalidator_votes() {
        // The receive gate: a vote from a non-staked key, and a tampered (forged)
        // vote, are both rejected and change nothing.
        let a = identity(1); // staked
        let stranger = identity(9); // NOT staked
        let ledger = staked_ledger(&[(&a, 5 * MICRO)]);
        let mut tracker = FinalityTracker::with_epoch_len(ledger.genesis_hash(), E);

        // Non-validator: correctly signed, but the signer holds no stake ⇒ weight 0.
        let non_val = signed_vote(&stranger, &cp(0, "GENESIS"), &cp(1, "h2"));
        assert!(!tracker.ingest_vote(non_val, &ledger).accepted, "non-validator vote rejected");

        // Forged: a real validator's vote with a flipped signature byte.
        let mut forged = signed_vote(&a, &cp(0, "GENESIS"), &cp(1, "h2"));
        forged.signature[0] ^= 0xFF;
        assert!(!tracker.ingest_vote(forged, &ledger).accepted, "forged vote rejected");

        assert!(tracker.latest_votes().is_empty(), "no rejected vote entered the fork-choice");
        assert!(tracker.state().finalized().get(1).is_none(), "nothing finalized from rejected votes");
    }

    #[test]
    fn live1_two_thirds_votes_finalize_via_gossip() {
        // The whole LIVE-1 path in miniature: two equal-stake validators gossip
        // votes for two consecutive links (g→c1, then c1→c2). Ingesting them drives
        // the live FinalityState to FINALIZE c1 — finality produced from gossiped
        // votes, not a planted state. (Finality operates on the votes + the
        // chain-derived stake; the chain need not itself reach c2's height.)
        let a = identity(1);
        let b = identity(2);
        let ledger = staked_ledger(&[(&a, 5 * MICRO), (&b, 5 * MICRO)]); // 200/200 both sign
        let mut tracker = FinalityTracker::with_epoch_len(ledger.genesis_hash(), E);

        let g = cp(0, ledger.genesis_hash().as_str());
        let c1 = cp(1, "c1-hash");
        let c2 = cp(2, "c2-hash");

        // Link g→c1: A votes (sub-quorum), then B votes → ⅔ certificate justifies c1.
        assert!(tracker.ingest_vote(signed_vote(&a, &g, &c1), &ledger).accepted);
        assert!(tracker.state().finalized().get(1).is_none(), "one vote is sub-quorum — nothing yet");
        let out = tracker.ingest_vote(signed_vote(&b, &g, &c1), &ledger);
        assert!(out.justified, "the ⅔ certificate justifies c1");
        assert!(tracker.state().justified().is_justified(&c1), "c1 justified from gossiped votes");

        // Link c1→c2 (direct child): a ⅔ certificate finalizes c1.
        tracker.ingest_vote(signed_vote(&a, &c1, &c2), &ledger);
        let fin = tracker.ingest_vote(signed_vote(&b, &c1, &c2), &ledger);
        assert!(fin.finalized, "the second ⅔ link finalizes c1 (two-step)");
        assert_eq!(
            tracker.state().finalized().get(1),
            Some(&c1),
            "the live gadget finalized c1 purely from gossiped votes",
        );
    }

    #[test]
    fn live1_non_validator_casts_nothing() {
        // The cast gate: a node with no bonded stake produces no vote (never spams
        // the network with inert attestations), even sitting on genesis.
        let observer = identity(7); // not staked
        let a = identity(1);
        let ledger = staked_ledger(&[(&a, 5 * MICRO)]); // observer is absent from the set
        let tracker = FinalityTracker::with_epoch_len(ledger.genesis_hash(), E);
        assert!(
            build_vote_to_cast(&ledger, &tracker, &observer).is_none(),
            "a non-validator casts no finality vote",
        );
    }
}
