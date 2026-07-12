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

/// DoS bound: the maximum number of distinct pending certificate links the pool
/// holds. A vote's target hash is attacker-chosen (not checked against a real
/// block until GADGET-3), so without a cap a single min-stake validator could
/// grow the pool without limit. Generous enough to never bite honest traffic
/// (finality advances a handful of epochs at a time); on overflow the stalest
/// links are evicted deterministically. See [`FinalityTracker::ingest_vote`].
const MAX_PENDING_LINKS: usize = 4096;

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
    /// DoS bound on `pool` size ([`MAX_PENDING_LINKS`] in production). A field so a
    /// test can use a tiny cap without signing thousands of votes.
    max_pending_links: usize,
}

/// What ingesting one vote advanced — for observability/tests. `accepted` false
/// ⇒ the vote failed verification (forged, stale, or from a non-validator) and
/// changed nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IngestOutcome {
    /// The vote verified and was observed into the fork-choice.
    pub accepted: bool,
    /// A ⅔ certificate formed on this vote's link and **justified** its target.
    pub justified: bool,
    /// …and **finalized** its source (two consecutive linked epochs).
    pub finalized: bool,
    /// LIVE-3 — a **fault** the incoming vote revealed: it equivocates against the
    /// validator's previously-observed vote (double-vote or surround, GADGET-4).
    /// The caller gossips the proof (`GossipMessage::FinalityFault`) so every node
    /// can slash the offender. `None` on an honest vote.
    pub detected_fault: Option<crate::sm::finality_slashing::FaultProof>,
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
        Self::with_caps(genesis_hash, epoch_len, MAX_PENDING_LINKS)
    }

    /// As [`Self::with_epoch_len`] but with an explicit pool cap — for the DoS-bound
    /// test (a tiny cap avoids signing thousands of votes).
    pub fn with_caps(genesis_hash: String, epoch_len: u64, max_pending_links: usize) -> Self {
        let mut tree = BlockTree::new();
        tree.add_root(&genesis_hash);
        Self {
            latest: LatestVotes::new(),
            state: FinalityState::genesis_only(genesis_hash),
            tree,
            pool: BTreeMap::new(),
            epoch_len: epoch_len.max(1),
            max_pending_links: max_pending_links.max(1),
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

    /// Number of distinct pending certificate links (bounded by `MAX_PENDING_LINKS`).
    #[cfg(test)]
    pub(crate) fn pending_links(&self) -> usize {
        self.pool.len()
    }

    /// LIVE-2 — the chain height of the **last finalized** checkpoint (the finality
    /// floor to push into the ledger, [`Ledger::set_finalized_floor`]). Genesis
    /// (height 0) on a fresh node; rises as certificates finalize checkpoints.
    pub fn finalized_floor_height(&self) -> u64 {
        self.finalized_floor().0
    }

    /// LIVE-2 (HIGH-4) — the last finalized checkpoint's `(height, hash)`. The hash
    /// is what [`Ledger::set_finalized_floor`] checks against the block it actually
    /// holds at that height, so it never freezes the wrong block. Genesis on a fresh
    /// node.
    pub fn finalized_floor(&self) -> (u64, String) {
        self.state
            .finalized()
            .iter()
            .last()
            .map(|c| (c.height, c.hash.clone()))
            .unwrap_or((0, String::new()))
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

        // LIVE-3 — equivocation detection (GADGET-4). Before this vote replaces the
        // validator's stored latest vote, check whether the two form a slashable
        // fault (double-vote / surround). Both are individually verified (the stored
        // one when it was observed, this one just above), so the resulting proof
        // verifies — the caller gossips it (`FinalityFault`) to slash the offender.
        let detected_fault = self.latest.get(&vote.validator).and_then(|prior| {
            crate::sm::finality_slashing::detect_fault(prior, &vote)
                .map(|_| crate::sm::finality_slashing::FaultProof::new(prior.clone(), vote.clone()))
        });

        // Fork-choice (GADGET-5A): the latest vote per validator drives GHOST. The
        // latest-vote store is bounded by the validator set (one entry per key), so
        // it is not a memory-growth vector.
        self.latest.observe(vote.clone());

        let mut outcome = IngestOutcome {
            accepted: true,
            detected_fault,
            ..Default::default()
        };

        // ── DoS guard on the certificate pool (bounded memory) ──────────────────
        // A vote's `target` hash is NOT checked against a real block (that is
        // GADGET-3's job with the chain), so a single min-stake validator could
        // gossip endless well-formed votes with distinct target hashes — each a new
        // `Link` — and, never reaching ⅔ alone, never trigger the justify/finalize
        // prune → unbounded `pool` growth → OOM on every node. Two bounds close it:
        //   1. **Stale links can't advance finality** — a link whose target epoch is
        //      at/below the last finalized epoch is dead weight (finality is
        //      append-only, epochs only rise); never pool it.
        //   2. **Hard cap** — cap the number of distinct pending links; on overflow
        //      evict the lowest-key link (BTreeMap order → the lowest source epoch,
        //      the stalest; deterministic). Evicting a genuinely-accumulating link is
        //      harmless: its certificate re-forms as its votes re-gossip.
        // This is IO-layer liveness/memory state, not a consensus verdict — eviction
        // never changes the finalized set (a real ⅔ certificate finalizes regardless
        // of pool contents), so C1/safety are untouched.
        let finalized_epoch = self
            .state
            .finalized()
            .iter()
            .last()
            .map(|c| c.epoch)
            .unwrap_or(0);
        if vote.target.epoch <= finalized_epoch {
            return outcome; // stale — observed for fork-choice, but not pooled
        }

        let link: Link = (vote.source.clone(), vote.target.clone());
        let entry = self.pool.entry(link.clone()).or_default();
        // De-dup a validator's repeat vote for the SAME link (idempotent gossip);
        // the certificate also rejects double-counting, this just bounds the Vec.
        if !entry.iter().any(|v| v.validator == vote.validator) {
            entry.push(vote.clone());
        }

        // Try to form and apply a ⅔ certificate for the link (GADGET-2 → 3).
        let votes = entry.clone();
        let cert = MlDsaCertificate::new(link.0.clone(), link.1.clone(), votes);
        if cert.is_valid(&stakes, self.epoch_len) {
            let step = self.state.apply_certificate(&cert, &stakes, self.epoch_len);
            outcome.justified = step.justified;
            outcome.finalized = step.finalized;
            // Once a link's certificate has been applied it can advance finality no
            // further; drop the pool entry so it can't be re-applied and to bound memory.
            if step.justified || step.finalized {
                self.pool.remove(&link);
            }
        }

        // Hard cap (bound #2): evict the lowest-key (stalest) links until under cap.
        while self.pool.len() > self.max_pending_links {
            let Some(oldest) = self.pool.keys().next().cloned() else {
                break;
            };
            self.pool.remove(&oldest);
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

    // ─── LIVE-3 helpers + teeth ─────────────────────────────────────────────

    use crate::sm::finality_slashing::FaultProof;

    /// A **double-vote** fault proof by `crypto` at epoch length `epoch_len`: two
    /// DIFFERENT votes for the SAME target epoch (`genesis → c1` and
    /// `genesis → c1'`), both correctly ML-DSA-signed — a slashable equivocation
    /// (`detect_fault` ⇒ `DoubleVote`). The checkpoints sit on real `epoch_len`
    /// boundaries so the votes are well-formed under the verifier that will re-check
    /// them: the **ledger** slash path uses production `EPOCH_LENGTH_BLOCKS`, while a
    /// small-`E` tracker test passes its own `E`.
    fn double_vote_proof_at(crypto: &CryptoEngine, genesis: &str, epoch_len: u64) -> FaultProof {
        let g = Checkpoint { epoch: 0, height: 0, hash: genesis.to_string() };
        let c1 = Checkpoint { epoch: 1, height: epoch_len, hash: "c1-hash".into() };
        let c1_prime = Checkpoint { epoch: 1, height: epoch_len, hash: "c1-prime-hash".into() };
        FaultProof::new(
            signed_vote(crypto, &g, &c1),
            signed_vote(crypto, &g, &c1_prime),
        )
    }

    /// Ledger-path proof: well-formed under production `EPOCH_LENGTH_BLOCKS` (what
    /// `verify_block_slashes` / `build_slash_tx` re-check against).
    fn double_vote_proof(crypto: &CryptoEngine, genesis: &str) -> FaultProof {
        double_vote_proof_at(crypto, genesis, EPOCH_LENGTH_BLOCKS)
    }

    /// Manual conservation check on a bare ledger: `Σ spendable + locked_stake +
    /// burned == minted` (the harness invariant, computed directly here).
    fn conserves(l: &Ledger) -> bool {
        let spendable: u64 = l.all_balances().values().sum();
        spendable + l.locked_stake_total() + l.total_burned() == l.total_minted()
    }

    #[test]
    fn live3_slash_burns_bonded_stake_and_conserves() {
        // The heart of LIVE-3: slashing an equivocator destroys exactly its bonded
        // stake (STAKE → BURN) and the balance sheet still balances. A validator A
        // bonds 5 QTA, equivocates, and is slashed to zero — the 5 QTA move from the
        // locked-stake pool to burned, conservation preserved throughout.
        let a = identity(1);
        let mut ledger = staked_ledger(&[(&a, 5 * MICRO)]);
        let addr = a.pq_address_hex().unwrap();
        assert!(conserves(&ledger), "conserves before the slash");
        assert_eq!(ledger.staked_of(&addr), 5 * MICRO, "A is bonded 5 QTA");
        let burned_before = ledger.total_burned();

        // Equivocation proof → queue → seal into a block.
        let proof = double_vote_proof(&a, ledger.genesis_hash().as_str());
        let tx = ledger.queue_slash(&proof).expect("a bonded offender is slashable");
        assert_eq!(tx.amount, 5 * MICRO, "the slash destroys the full bonded stake (ADR-009)");
        let block = ledger.seal_if_pending_at(&addr, 0.0, "2026-07-12T00:00:00+00:00".into())
            .expect("seal the slash block");
        assert!(
            block.transactions.iter().any(|t| t.tx_type == crate::p2p::ledger::TxType::Slash),
            "the slash was included in the sealed block (not excluded)",
        );

        // Sealer side: the slash applied — bonded stake destroyed, coins burned,
        // conservation preserved.
        assert_eq!(ledger.staked_of(&addr), 0, "A's bonded stake is destroyed");
        assert_eq!(ledger.total_burned(), burned_before + 5 * MICRO, "5 QTA burned");
        assert!(conserves(&ledger), "conserves AFTER the slash (STAKE → BURN is neutral)");
        assert_eq!(ledger.validator_stakes().get(&addr), None, "A is no longer a validator");

        // Receiver side (the real convergence property): a FRESH node with the same
        // chain up to the stakes accepts the slash block (verify_block_slashes runs
        // at the correct pre-block state inside integrate) and reaches the SAME state.
        let mut receiver = staked_ledger(&[(&a, 5 * MICRO)]);
        assert_eq!(receiver.integrate_remote_block(block), Ok(true), "receiver accepts the slash block");
        assert_eq!(receiver.staked_of(&addr), 0, "receiver applies the slash identically");
        assert!(conserves(&receiver), "receiver conserves after applying the slash");
    }

    #[test]
    fn live3_forged_slash_is_rejected_by_block_validation() {
        // A malicious proposer cannot punish an innocent validator. Two attacks:
        // (a) a slash with NO real fault (two identical/legal votes → detect_fault
        // None), and (b) a slash of the WRONG amount. Both are rejected by
        // verify_block_slashes, so a block carrying them is refused.
        let a = identity(1);
        let b = identity(2);
        let ledger = staked_ledger(&[(&a, 5 * MICRO), (&b, 5 * MICRO)]);
        let addr_a = a.pq_address_hex().unwrap();

        // (a) No real fault: a "proof" of two IDENTICAL votes is not equivocation.
        // Checkpoints on real EPOCH_LENGTH_BLOCKS boundaries (the ledger verifier).
        let g = Checkpoint { epoch: 0, height: 0, hash: ledger.genesis_hash() };
        let c1 = Checkpoint { epoch: 1, height: EPOCH_LENGTH_BLOCKS, hash: "c1-hash".into() };
        let same = signed_vote(&a, &g, &c1);
        let bogus = FaultProof::new(same.clone(), same);
        let mut fake_tx = ledger.build_slash_tx(&double_vote_proof(&a, ledger.genesis_hash().as_str())).unwrap();
        fake_tx.fault_proof = Some(serde_json::to_string(&bogus).unwrap());
        let blk = Ledger::forge_block_at(
            ledger.chain_height(),
            &ledger.chain.last().unwrap().hash,
            "ts",
            &addr_a,
            vec![fake_tx],
        );
        assert!(ledger.verify_block_slashes(&blk).is_err(), "a slash with no real fault is rejected");

        // (b) Wrong amount: a real fault, but the slash destroys more than bonded.
        let real = double_vote_proof(&a, ledger.genesis_hash().as_str());
        let mut over_tx = ledger.build_slash_tx(&real).unwrap();
        over_tx.amount = 999 * MICRO; // ≠ A's 5 QTA bonded
        let blk2 = Ledger::forge_block_at(
            ledger.chain_height(),
            &ledger.chain.last().unwrap().hash,
            "ts",
            &addr_a,
            vec![over_tx],
        );
        assert!(ledger.verify_block_slashes(&blk2).is_err(), "a wrong-amount slash is rejected");
    }

    #[test]
    fn live3_equivocation_is_detected_on_vote_ingest() {
        // The producer side: when the tracker ingests a vote that equivocates
        // against a validator's stored vote, it surfaces a verifiable FaultProof
        // (which the dispatcher gossips as FinalityFault → slash).
        let a = identity(1);
        let ledger = staked_ledger(&[(&a, 5 * MICRO)]);
        let mut tracker = FinalityTracker::with_epoch_len(ledger.genesis_hash(), E);
        let g = cp(0, ledger.genesis_hash().as_str());

        // First honest vote — no fault.
        let out1 = tracker.ingest_vote(signed_vote(&a, &g, &cp(1, "c1-hash")), &ledger);
        assert!(out1.detected_fault.is_none(), "one honest vote is not a fault");

        // A conflicting vote for the SAME target epoch — a double-vote.
        let out2 = tracker.ingest_vote(signed_vote(&a, &g, &cp(1, "c1-prime")), &ledger);
        let proof = out2.detected_fault.expect("equivocation detected");
        assert!(
            crate::sm::finality_slashing::verify_proof(
                &proof,
                &ledger.validator_stakes_by_pubkey(),
                E,
            ),
            "the surfaced proof verifies against on-chain stake",
        );
    }

    #[test]
    fn live3_duplicate_slash_of_same_offender_is_rejected() {
        // CRITICAL (adversarial): a leader self-equivocates once, then puts the SAME
        // slash tx TWICE in a block. Each passes the stateless amount==staked check
        // against the same pre-block stake, but applying both would debit the STAKE
        // sink twice while `staked` saturates at 0 → permanent conservation break.
        // The sequential invalid_slash_indices must reject the second slash, and a
        // block carrying the duplicate must be refused.
        let a = identity(1);
        let mut ledger = staked_ledger(&[(&a, 5 * MICRO)]);
        let addr = a.pq_address_hex().unwrap();
        let slash = ledger.build_slash_tx(&double_vote_proof(&a, ledger.genesis_hash().as_str())).unwrap();
        let dup_block = Ledger::forge_block_at(
            ledger.chain_height(),
            &ledger.chain.last().unwrap().hash,
            "ts",
            &addr,
            vec![slash.clone(), slash], // the SAME slash twice
        );
        assert!(
            ledger.verify_block_slashes(&dup_block).is_err(),
            "a block slashing the same offender twice must be rejected",
        );
        // And integrating it changes nothing (conservation preserved).
        assert!(ledger.integrate_remote_block(dup_block).is_err(), "the duplicate-slash block is refused");
        assert_eq!(ledger.staked_of(&addr), 5 * MICRO, "A's stake untouched by the refused block");
        assert!(conserves(&ledger), "conservation intact");
    }

    #[test]
    fn live3_slash_with_concurrent_unstake_of_offender_is_rejected() {
        // CRITICAL (adversarial): a Slash and an Unstake for the SAME offender in one
        // block. The Unstake would move the offender's coins into an unbonding entry
        // that later MATURES and returns the slashed coins — a deferred conservation
        // break (double-count at maturation). invalid_slash_indices must reject the
        // slash when its offender also moves stake in the block.
        let a = identity(1);
        let ledger = staked_ledger(&[(&a, 5 * MICRO)]);
        let addr = a.pq_address_hex().unwrap();
        let slash = ledger.build_slash_tx(&double_vote_proof(&a, ledger.genesis_hash().as_str())).unwrap();
        // A minimal Unstake from the same offender (structural — the check keys on
        // tx_type + from, not the signature).
        let unstake = crate::p2p::ledger::Transaction {
            id: "u".into(),
            from: addr.clone(),
            to: "STAKE".into(),
            amount: 5 * MICRO,
            tx_type: crate::p2p::ledger::TxType::Unstake,
            timestamp: "ts".into(),
            signature: String::new(),
            hash: "uh".into(),
            nonce: 0,
            pq_signature: None,
            pq_public_key: None,
            fault_proof: None,
        };
        let block = Ledger::forge_block_at(
            ledger.chain_height(),
            &ledger.chain.last().unwrap().hash,
            "ts",
            &addr,
            vec![slash, unstake],
        );
        assert!(
            ledger.verify_block_slashes(&block).is_err(),
            "a slash coexisting with the offender's Unstake in one block must be rejected",
        );
    }

    #[test]
    fn live3_slash_survives_snapshot_restore_identically() {
        // CRITICAL (C1/determinism): after a slash is sealed, a node that restarts
        // (restore from snapshot) must reconstruct the SAME state as one that applied
        // it live — the slash debits the STAKE sink, never the offender's spendable.
        let a = identity(1);
        let mut ledger = staked_ledger(&[(&a, 5 * MICRO)]);
        let addr = a.pq_address_hex().unwrap();
        ledger.queue_slash(&double_vote_proof(&a, ledger.genesis_hash().as_str())).unwrap();
        ledger.seal_if_pending_at(&addr, 0.0, "2026-07-12T00:00:00+00:00".into()).unwrap();

        let live_staked = ledger.staked_of(&addr);
        let live_spendable = ledger.balance_of(&addr);
        let live_locked = ledger.locked_stake_total();
        let live_burned = ledger.total_burned();

        let restored = Ledger::restore(ledger.snapshot());
        assert_eq!(restored.staked_of(&addr), live_staked, "restored staked matches live");
        assert_eq!(restored.balance_of(&addr), live_spendable, "restored spendable matches live (NOT debited by the slash)");
        assert_eq!(restored.locked_stake_total(), live_locked, "restored locked-stake matches live");
        assert_eq!(restored.total_burned(), live_burned, "restored burned matches live");
        assert!(conserves(&restored), "restored node conserves");
    }

    #[test]
    fn audit_pool_is_bounded_against_a_vote_flood() {
        // HIGH (adversarial audit): a single min-stake validator could gossip endless
        // well-formed votes with distinct target hashes — each a new pool link, never
        // pruned (a sub-⅔ attacker never finalizes) → unbounded memory. The cap must
        // hold the pool at MAX_PENDING_LINKS regardless of how many the attacker sends.
        let a = identity(1);
        let ledger = staked_ledger(&[(&a, 5 * MICRO)]);
        // Tiny cap so the flood is cheap (the production cap MAX_PENDING_LINKS
        // exercises the SAME `while pool.len() > self.max_pending_links` eviction).
        const CAP: usize = 16;
        let mut tracker = FinalityTracker::with_caps(ledger.genesis_hash(), E, CAP);
        let g = cp(0, ledger.genesis_hash().as_str());
        // Flood well past the cap with distinct-target (epoch 1) votes.
        for i in 0..(CAP + 50) {
            let target = cp(1, &format!("flood-{i}"));
            let v = signed_vote(&a, &g, &target);
            assert!(tracker.ingest_vote(v, &ledger).accepted, "each flood vote is well-formed");
        }
        assert!(
            tracker.pending_links() <= CAP,
            "the pool stays bounded under a vote flood (got {})",
            tracker.pending_links(),
        );
    }

    #[test]
    fn audit_stale_links_below_finality_are_not_pooled() {
        // A vote whose target epoch is at/below the last finalized epoch can never
        // advance finality (finality is append-only) — it must not enter the pool
        // (else it is a free memory-growth vector below the floor).
        let a = identity(1);
        let b = identity(2);
        let ledger = staked_ledger(&[(&a, 5 * MICRO), (&b, 5 * MICRO)]);
        let mut tracker = FinalityTracker::with_epoch_len(ledger.genesis_hash(), E);
        let g = cp(0, ledger.genesis_hash().as_str());
        let c1 = cp(1, "c1-hash");
        let c2 = cp(2, "c2-hash");
        // Finalize c1 (epoch 1) via g→c1 then c1→c2.
        tracker.ingest_vote(signed_vote(&a, &g, &c1), &ledger);
        tracker.ingest_vote(signed_vote(&b, &g, &c1), &ledger);
        tracker.ingest_vote(signed_vote(&a, &c1, &c2), &ledger);
        tracker.ingest_vote(signed_vote(&b, &c1, &c2), &ledger);
        let before = tracker.pending_links();
        // A fresh vote for epoch 1 (≤ finalized epoch 1) is accepted for fork-choice
        // but NOT pooled.
        let stale = tracker.ingest_vote(signed_vote(&a, &g, &cp(1, "stale-epoch1")), &ledger);
        assert!(stale.accepted, "the stale vote still verifies (fork-choice observes it)");
        assert_eq!(tracker.pending_links(), before, "a link at/below the finalized epoch is not pooled");
    }

    #[test]
    fn live3_slash_of_unbonded_offender_is_noop() {
        // Nothing to slash: a proof against a key with no bonded stake queues
        // nothing (and would be rejected in-block anyway — verify_proof requires an
        // active validator). No panic, no spurious burn.
        let a = identity(1); // staked
        let stranger = identity(9); // NOT staked
        let mut ledger = staked_ledger(&[(&a, 5 * MICRO)]);
        let burned_before = ledger.total_burned();
        let proof = double_vote_proof(&stranger, ledger.genesis_hash().as_str());
        assert!(ledger.queue_slash(&proof).is_none(), "an unbonded offender is not slashable");
        assert_eq!(ledger.total_burned(), burned_before, "nothing burned");
        assert!(conserves(&ledger), "conservation untouched");
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
    fn live2_finalization_raises_the_tracker_floor() {
        // LIVE-2 — when gossiped votes finalize a checkpoint, the tracker's finality
        // floor rises to that checkpoint's height (what the dispatcher/mining-loop
        // then push into the ledger via `set_finalized_floor`). Fresh tracker floor
        // = 0 (genesis); after finalizing c1 it becomes c1's height.
        let a = identity(1);
        let b = identity(2);
        let ledger = staked_ledger(&[(&a, 5 * MICRO), (&b, 5 * MICRO)]);
        let mut tracker = FinalityTracker::with_epoch_len(ledger.genesis_hash(), E);
        assert_eq!(tracker.finalized_floor_height(), 0, "fresh: only genesis finalized");

        let g = cp(0, ledger.genesis_hash().as_str());
        let c1 = cp(1, "c1-hash");
        let c2 = cp(2, "c2-hash");
        // g→c1 (justify c1) then c1→c2 (finalize c1) — both ⅔.
        tracker.ingest_vote(signed_vote(&a, &g, &c1), &ledger);
        tracker.ingest_vote(signed_vote(&b, &g, &c1), &ledger);
        tracker.ingest_vote(signed_vote(&a, &c1, &c2), &ledger);
        let out = tracker.ingest_vote(signed_vote(&b, &c1, &c2), &ledger);
        assert!(out.finalized, "c1 finalized from the votes");
        assert_eq!(
            tracker.finalized_floor_height(),
            c1.height,
            "the tracker floor rose to the finalized checkpoint's height",
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
