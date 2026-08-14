//! `finality_slashing` — **accountable safety**: detecting the two — and only
//! two — Casper-FFG faults, the **proof** they leave, and the **penalty**:
//! **GADGET-4** of the finality gadget (`DESIGN-FINALITY-GADGET.md` §7,
//! [[ADR-003 — Slashing]]).
//!
//! GADGET-3 made finality *real*; it did not make it **answerable**. The
//! accountable-safety theorem says: if two **conflicting** checkpoints are ever
//! finalized, then validators holding **≥ ⅓ of the staked weight** broke one of
//! exactly two rules — and their **signed votes prove it**. This module makes
//! that executable. It does **not invent** rules: the two conditions *follow*
//! from the theorem (design §7).
//!
//! 1. **Double vote** — a validator signs two **different** votes for the **same
//!    target epoch**.
//! 2. **Surround** — a validator signs a vote whose `(source, target)` interval
//!    **strictly contains** another it already signed (earlier source **and**
//!    later target).
//!
//! Both are detected from GADGET-2 [`Vote`]s, so the evidence is a pair of
//! **non-repudiable ML-DSA signatures** — verifiable by anyone, no trusted
//! party. A valid proof makes the offender **slashable**: §3 reduces their stake
//! and (by default) **burns** the slashed µQTA, conserving the balance sheet.
//!
//! # Determinism (`sm/` sans-IO, C1)
//! Detection, proof verification and the penalty are **pure functions** of (the
//! votes, the on-chain stake snapshot): no clock, no entropy, and **no `HashMap`
//! iteration order in any verdict** — offenders are accumulated over a
//! `BTreeMap`/`BTreeSet` and weights summed with checked integer arithmetic. Two
//! nodes fed the same evidence reach the **same** verdict and slash the **same**
//! amount, byte-for-byte.
//!
//! # Scope (held to GADGET-4)
//! Detection + proof + the **mechanics** of the penalty. **No fork-choice**
//! (GADGET-5) lives here — accountable safety needs none of it (design §7 STOP).
//! Mirroring GADGET-2/3, the penalty operates on a **stake snapshot supplied by
//! the caller**, not the live ledger: wiring slashing onto the on-chain stake
//! state (a `STAKE → BURN` move that trips the harness conservation invariant
//! through real `locked_stake`/`burned` accounting) is the same deferred
//! identity/ledger reconciliation flagged in GADGET-2 §4, left to Alexandre — no
//! live gossip path is wired. The penalty **amounts** are **🛑 Alexandre's**
//! (marked constants below).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::finality_vote::Vote;

// ─── §3 policy constants — ratifiées par ADR-009 (§12 / ADR-003) ───────────────

/// **Slash fraction = `SLASH_NUM / SLASH_DEN` of the offender's bonded stake.**
/// **Ratifiée *ajustable* par ADR-009** (fraction) — a *réglable* default, not a
/// graven promise. Default = **full slash** (`1/1`): equivocation that can break
/// **finality** (irreversible history) is the gravest fault, so the maximal
/// deterrent is the simplest sane default. The tunable alternative is a partial
/// /correlation slash (e.g. a small base `1/32` scaled by how much stake
/// equivocated together). The *mechanism* below is fraction-generic — changing the
/// number by **fork** is a one-line change with no logic to revisit.
pub const SLASH_NUM: u64 = 1;
/// Denominator of the slash fraction (see [`SLASH_NUM`]). **ADR-009 : ajustable.**
pub const SLASH_DEN: u64 = 1;

/// **Where the slashed stake goes** — `true` = **burn** (default), `false` =
/// redistribute. **GRAVÉ par ADR-009** (brûle vs redistribue — invariant
/// monétaire) : *brûlé* — the simplest and monetarily soundest (it deepens
/// scarcity and pays no one to accuse, removing any incentive to manufacture
/// faults). Conservation holds either way ([`apply_slash`]).
pub const SLASH_BURN: bool = true;

/// **Evidence window (block heights) a fault proof stays admissible.**
/// **Contrainte GRAVÉE par ADR-009 :** it MUST stay
/// `≤ UNBONDING_PERIOD_BLOCKS` — otherwise an offender unbonds and withdraws
/// their stake before the proof can land, and slashing is trivially bypassed
/// (the *unstake-and-run* hole the unbonding delay exists to close,
/// ONCHAIN-STAKE-1 §3). Default = the full unbonding period (the maximum the
/// constraint allows). The `const` assertion below makes a violation a **compile
/// error**, not a runtime surprise.
pub const SLASH_EVIDENCE_WINDOW_BLOCKS: u64 = crate::p2p::ledger::UNBONDING_PERIOD_BLOCKS;

/// Graven constraint (compile-time): the evidence window can never outlast
/// unbonding. If either constant is retuned past the other, the build breaks.
const _: () = assert!(SLASH_EVIDENCE_WINDOW_BLOCKS <= crate::p2p::ledger::UNBONDING_PERIOD_BLOCKS);

// ─── §1 The two faults & their detection (pure, on the votes) ──────────────────

/// The **two — and only two** — slashable faults (design §7, ADR-003). Anything
/// else two honest votes can look like is *legal* (a chain extension, a skip),
/// so this enum is deliberately closed: inventing a third condition would punish
/// honest behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// Two different votes for the **same target epoch**.
    DoubleVote,
    /// One vote's `(source, target)` interval **strictly surrounds** another's.
    Surround,
}

/// **§1 detection (pure, structural).** Whether two votes — taken to be from the
/// **same** validator — constitute a slashable fault. Returns the fault kind, or
/// `None` if the pair is legal.
///
/// This is the *structural* predicate over the checkpoints alone; it does **not**
/// check the signer identity or the signatures — that is [`verify_proof`]'s job.
/// The split lets a detector scan many candidate pairs cheaply, then prove only
/// the ones that bite. Deterministic, no clock/entropy.
///
/// Legal (⇒ `None`): the **same** vote twice (a duplicate, not a fault); two
/// votes with **different** target epochs whose intervals don't nest (a normal
/// chain extension or a skip). These are exactly the "no false accusation" cases.
pub fn detect_fault(a: &Vote, b: &Vote) -> Option<Fault> {
    // The same link twice is a re-publish, never a fault.
    if a.source == b.source && a.target == b.target {
        return None;
    }
    // Double vote: two DISTINCT votes sharing the SAME target epoch — a validator
    // may attest **at most one** checkpoint per target epoch.
    if a.target.epoch == b.target.epoch {
        return Some(Fault::DoubleVote);
    }
    // Surround: one interval strictly contains the other (either order).
    if interval_surrounds(a, b) || interval_surrounds(b, a) {
        return Some(Fault::Surround);
    }
    None
}

/// `outer` **strictly surrounds** `inner`: an **earlier** source epoch **and** a
/// **later** target epoch. (Strict on both ends — a shared endpoint is not a
/// surround; that case is caught by the double-vote rule when the targets share
/// an epoch, and is otherwise a legal extension.)
fn interval_surrounds(outer: &Vote, inner: &Vote) -> bool {
    outer.source.epoch < inner.source.epoch && inner.target.epoch < outer.target.epoch
}

// ─── §2 The fault proof (verifiable by anyone) ─────────────────────────────────

/// **§2 a proof of fault** — the two signed, contradictory votes. Self-contained
/// evidence: anyone can re-check it against the stake snapshot with
/// [`verify_proof`] (valid ML-DSA signatures + same validator + a fault
/// condition). No accusation is trusted on its face.
///
/// The `serde` derives (LIVE-3) let a proof cross the gossip wire
/// (`GossipMessage::FinalityFault`) and be embedded in a `Slash` tx's
/// `fault_proof` field — data-carrier only; `verify_proof` stays a pure function.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FaultProof {
    /// First incriminating vote.
    pub vote_a: Vote,
    /// Second incriminating vote (must name the **same** validator as `vote_a`).
    pub vote_b: Vote,
}

impl FaultProof {
    /// Bundle two votes as a candidate proof. **No validation at construction** —
    /// validity is a pure function of the stake snapshot ([`verify_proof`]), so
    /// one proof can be (re)checked against any snapshot by any node.
    pub fn new(vote_a: Vote, vote_b: Vote) -> Self {
        Self { vote_a, vote_b }
    }

    /// The validator this proof incriminates (its `vote_a` signer). Only
    /// meaningful once [`verify_proof`] has confirmed both votes name the same
    /// validator.
    pub fn offender(&self) -> &str {
        &self.vote_a.validator
    }

    /// The structural fault the two votes form, if any (delegates to
    /// [`detect_fault`]).
    pub fn fault(&self) -> Option<Fault> {
        detect_fault(&self.vote_a, &self.vote_b)
    }

    /// **B-15** — l'époque de la faute : la **plus récente** des deux cibles.
    ///
    /// La plus récente, et non la plus ancienne : une preuve est recevable tant
    /// que la faute est récente, et une faute n'est constituée qu'une fois les
    /// deux votes émis. Prendre la plus ancienne périmerait une preuve dont la
    /// seconde moitié vient juste d'arriver — on punirait l'accusateur pour la
    /// lenteur du fautif.
    pub fn target_epoch(&self) -> u64 {
        self.vote_a.target.epoch.max(self.vote_b.target.epoch)
    }
}

/// **§2 verify a fault proof (pure).** `true` iff:
/// 1. both votes name the **same** validator (no cross-validator accusation),
/// 2. each vote is **individually valid** — well-formed link, an **active**
///    validator (stake > 0), and a valid **ML-DSA-65** signature — reusing
///    GADGET-2's [`Vote::verify`] (a forged signature ⇒ the vote fails here), and
/// 3. the two votes form a **slashable fault** ([`detect_fault`] ≠ `None`).
///
/// Rejects forged signatures, cross-validator pairs, and **legal** vote pairs —
/// so an honest validator can never be slashed (no false accusation). The
/// same-validator check is first and cheapest; the signatures are checked before
/// the structural fault so a forged proof is thrown out regardless of shape.
pub fn verify_proof(proof: &FaultProof, stakes: &HashMap<String, u64>, epoch_len: u64) -> bool {
    proof.vote_a.validator == proof.vote_b.validator
        && proof.vote_a.verify(stakes, epoch_len)
        && proof.vote_b.verify(stakes, epoch_len)
        && detect_fault(&proof.vote_a, &proof.vote_b).is_some()
}

// ─── §3 The penalty (conserving; amounts marked) ───────────────────────────────

/// The conserving outcome of slashing one offender. Every field is in µQTA, and
/// the two conservation identities **always** hold (see [`apply_slash`]):
/// `stake_before == remaining + slashed` and, by default, `burned == slashed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashOutcome {
    /// The slashed validator.
    pub offender: String,
    /// Their bonded stake **before** the slash.
    pub stake_before: u64,
    /// µQTA removed from their stake (`SLASH_NUM / SLASH_DEN` of `stake_before`).
    pub slashed: u64,
    /// µQTA **burned** (`== slashed` under the default [`SLASH_BURN`] policy; `0`
    /// if a future policy redistributes instead — then those µQTA are paid out
    /// elsewhere, never lost).
    pub burned: u64,
    /// Their bonded stake **after** the slash (`stake_before - slashed`).
    pub remaining: u64,
}

/// The slash amount for a given bonded stake: `stake · SLASH_NUM / SLASH_DEN`,
/// **checked** integer math (`u128` intermediate, no float, no overflow — stakes
/// are far below `u64::MAX`). Never exceeds `stake` for a fraction `≤ 1`.
fn slash_amount(stake: u64) -> u64 {
    let num = (stake as u128) * (SLASH_NUM as u128);
    let slashed = num / (SLASH_DEN.max(1) as u128);
    // Clamp to the stake (a fraction > 1 would be a misconfig; never slash more
    // than exists) and back to u64 (the quotient ≤ stake ≤ u64::MAX).
    slashed.min(stake as u128) as u64
}

/// **§3 the penalty mechanic (pure).** Reduce `offender`'s stake in `stakes`
/// **in place** by the marked fraction and return the conserving [`SlashOutcome`].
/// The slashed µQTA is **burned** by default ([`SLASH_BURN`]).
///
/// Conservation is **structural**: `slashed` leaves the stake map and is recorded
/// as `burned`, so `stake_before == remaining + slashed` and `burned == slashed`
/// — nothing is created or vanishes. (When `remaining` hits `0` the entry is
/// removed, so a fully-slashed validator drops out of the active set.) An
/// offender absent from the map slashes `0` (a no-op), never a panic.
///
/// Deterministic — no clock, no entropy. The safe entry point that slashes
/// **only on a verified proof** is [`slash_for_proof`].
pub fn apply_slash(stakes: &mut HashMap<String, u64>, offender: &str) -> SlashOutcome {
    let stake_before = stakes.get(offender).copied().unwrap_or(0);
    let slashed = slash_amount(stake_before);
    let remaining = stake_before - slashed; // slashed ≤ stake_before, no underflow
    if remaining == 0 {
        stakes.remove(offender);
    } else {
        stakes.insert(offender.to_string(), remaining);
    }
    let burned = if SLASH_BURN { slashed } else { 0 };
    SlashOutcome {
        offender: offender.to_string(),
        stake_before,
        slashed,
        burned,
        remaining,
    }
}

/// **§3 the safe penalty entry — verify, then slash.** [`verify_proof`] the
/// accusation first; only a **valid** proof slashes its offender ([`apply_slash`]).
/// Returns `None` (no state change) on an unproven accusation — slashing is
/// never applied on an unverified claim. The single function a caller should use
/// to turn evidence into a penalty.
pub fn slash_for_proof(
    stakes: &mut HashMap<String, u64>,
    proof: &FaultProof,
    epoch_len: u64,
) -> Option<SlashOutcome> {
    if !verify_proof(proof, stakes, epoch_len) {
        return None;
    }
    Some(apply_slash(stakes, proof.offender()))
}

// ─── §4 The accountable-safety measure ─────────────────────────────────────────

/// **§4 the accountable-safety measure (pure).** Given a pool of votes and the
/// stake snapshot, the total stake of the **distinct** validators whose votes
/// **prove** a slashable fault — each counted **once**. This is the quantity the
/// theorem bounds: if two conflicting checkpoints are finalized, the votes that
/// did it make this `≥ ⅓` of the total stake (two ⅔-quorums intersect in ≥ ⅓,
/// and the intersection equivocated).
///
/// Only **individually valid** votes (signature + active validator + form) count
/// — a forged vote proves nothing. Deterministic: validators are grouped over a
/// `BTreeMap`, offenders summed with checked integer arithmetic, so the verdict
/// is independent of input/`HashMap` order.
pub fn slashable_weight(votes: &[Vote], stakes: &HashMap<String, u64>, epoch_len: u64) -> u64 {
    // Group the VALID votes by validator (deterministic key order).
    let mut by_validator: BTreeMap<&str, Vec<&Vote>> = BTreeMap::new();
    for v in votes {
        if v.verify(stakes, epoch_len) {
            by_validator.entry(v.validator.as_str()).or_default().push(v);
        }
    }
    // A validator is slashable iff any pair of its valid votes is a fault. Count
    // its stake once. `BTreeSet` keeps the offender set order-independent.
    let mut offenders: BTreeSet<&str> = BTreeSet::new();
    for (validator, vs) in &by_validator {
        let faulty = vs
            .iter()
            .enumerate()
            .any(|(i, a)| vs[i + 1..].iter().any(|b| detect_fault(a, b).is_some()));
        if faulty {
            offenders.insert(validator);
        }
    }
    offenders.into_iter().fold(0u64, |acc, v| {
        acc.saturating_add(stakes.get(v).copied().unwrap_or(0))
    })
}

// ─── §5 The teeth (detection, proof, penalty, accountable safety) ──────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::hybrid_crypto::{derive_ml_dsa, ml_dsa_sign_deterministic};
    use crate::sm::finality::Checkpoint;

    const E: u64 = 4; // small epoch length (parametric — any E ≥ 1)

    /// A checkpoint at `epoch` (boundary height `epoch × E`), carrying `hash`.
    fn cp(epoch: u64, hash: &str) -> Checkpoint {
        Checkpoint {
            epoch,
            height: epoch * E,
            hash: hash.to_string(),
        }
    }

    /// A deterministic ML-DSA validator `seed`, signing the vote
    /// `source → target @ target.epoch` (SIGN-DET, reproducible in the harness).
    fn signed_vote(seed: u8, source: &Checkpoint, target: &Checkpoint) -> Vote {
        let (sk, pk_hex) = derive_ml_dsa(&[seed; 32]).expect("derive ml-dsa keypair");
        let mut v = Vote {
            source: source.clone(),
            target: target.clone(),
            voting_epoch: target.epoch,
            validator: pk_hex,
            signature: Vec::new(),
        };
        v.signature = ml_dsa_sign_deterministic(&sk, &v.signable_bytes()).expect("sign");
        v
    }

    /// Stake map for `n` equal-weight (100 µQTA each) validators, seeds `1..=n`.
    fn stake_map(n: u8) -> HashMap<String, u64> {
        (1..=n)
            .map(|s| (derive_ml_dsa(&[s; 32]).expect("derive").1, 100u64))
            .collect()
    }

    // ── §5.1 double vote: detected + proven ──────────────────────────────────

    #[test]
    fn gadget4_double_vote_is_detected_and_proven() {
        // Validator 1 signs TWO different checkpoints for the SAME target epoch 1.
        let g = cp(0, "GENESIS");
        let c1a = cp(1, "epoch1-A");
        let c1b = cp(1, "epoch1-B");
        let va = signed_vote(1, &g, &c1a);
        let vb = signed_vote(1, &g, &c1b);
        assert_eq!(detect_fault(&va, &vb), Some(Fault::DoubleVote), "same target epoch, different target");

        let stakes = stake_map(1);
        let proof = FaultProof::new(va, vb);
        assert!(verify_proof(&proof, &stakes, E), "two valid same-validator double votes prove a fault");
        assert_eq!(proof.fault(), Some(Fault::DoubleVote));
    }

    // ── §5.2 surround: detected + proven ─────────────────────────────────────

    #[test]
    fn gadget4_surround_is_detected_and_proven() {
        // Validator 1 signs an OUTER vote g(0)→c3(3) and an INNER c1(1)→c2(2):
        // earlier source AND later target ⇒ the outer surrounds the inner.
        let g = cp(0, "GENESIS");
        let c1 = cp(1, "h4");
        let c2 = cp(2, "h8");
        let c3 = cp(3, "h12");
        let outer = signed_vote(1, &g, &c3);
        let inner = signed_vote(1, &c1, &c2);
        assert_eq!(detect_fault(&outer, &inner), Some(Fault::Surround), "outer interval contains inner");
        assert_eq!(detect_fault(&inner, &outer), Some(Fault::Surround), "symmetric — order-independent");

        let stakes = stake_map(1);
        let proof = FaultProof::new(outer, inner);
        assert!(verify_proof(&proof, &stakes, E), "a valid surround pair proves a fault");
    }

    // ── §5.3 no false accusation (two distinct legal shapes) ─────────────────

    #[test]
    fn gadget4_legal_votes_same_source_are_not_a_fault() {
        // Same source g, different target epochs (1 and 2): a validator naming a
        // later target from the same justified source is LEGAL — not a fault.
        let g = cp(0, "GENESIS");
        let c1 = cp(1, "h4");
        let c2 = cp(2, "h8");
        let va = signed_vote(1, &g, &c1);
        let vb = signed_vote(1, &g, &c2);
        assert_eq!(detect_fault(&va, &vb), None, "same source, different target epochs ⇒ legal");
        let stakes = stake_map(1);
        assert!(!verify_proof(&FaultProof::new(va, vb), &stakes, E), "legal votes are NOT a proof");
    }

    #[test]
    fn gadget4_legal_chain_extension_is_not_a_fault() {
        // Consecutive, non-nesting links c1→c2 then c2→c3: different target
        // epochs, neither interval surrounds the other ⇒ an honest chain
        // extension, no false accusation.
        let c1 = cp(1, "h4");
        let c2 = cp(2, "h8");
        let c3 = cp(3, "h12");
        let va = signed_vote(1, &c1, &c2);
        let vb = signed_vote(1, &c2, &c3);
        assert_eq!(detect_fault(&va, &vb), None, "adjacent non-nesting links ⇒ legal");
        let stakes = stake_map(1);
        assert!(!verify_proof(&FaultProof::new(va, vb), &stakes, E));
    }

    #[test]
    fn gadget4_same_vote_twice_is_not_a_fault() {
        // Re-publishing the very same vote is a duplicate, never an equivocation.
        let g = cp(0, "GENESIS");
        let c1 = cp(1, "h4");
        let v = signed_vote(1, &g, &c1);
        assert_eq!(detect_fault(&v, &v.clone()), None, "the same link twice is not a fault");
    }

    // ── §5.4 forged proof rejected ───────────────────────────────────────────

    #[test]
    fn gadget4_forged_proof_is_rejected() {
        // A structurally-perfect double vote, but one ML-DSA signature is corrupted
        // ⇒ that vote fails GADGET-2 verification ⇒ the proof is rejected. A forged
        // accusation cannot slash anyone.
        let g = cp(0, "GENESIS");
        let c1a = cp(1, "epoch1-A");
        let c1b = cp(1, "epoch1-B");
        let va = signed_vote(1, &g, &c1a);
        let mut vb = signed_vote(1, &g, &c1b);
        assert_eq!(detect_fault(&va, &vb), Some(Fault::DoubleVote), "shape is a double vote …");
        vb.signature[0] ^= 0xFF; // … but the signature is forged
        let stakes = stake_map(1);
        assert!(!verify_proof(&FaultProof::new(va, vb), &stakes, E), "a forged signature ⇒ no proof");
    }

    #[test]
    fn gadget4_cross_validator_pair_is_not_a_proof() {
        // Two DIFFERENT validators each cast one (legal) vote for epoch 1. They
        // share a target epoch, so the STRUCTURAL predicate flags a double vote —
        // but a fault must incriminate ONE validator, so the proof is rejected.
        let g = cp(0, "GENESIS");
        let c1a = cp(1, "epoch1-A");
        let c1b = cp(1, "epoch1-B");
        let va = signed_vote(1, &g, &c1a);
        let vb = signed_vote(2, &g, &c1b); // a DIFFERENT validator
        let stakes = stake_map(2);
        assert!(va.verify(&stakes, E) && vb.verify(&stakes, E), "both votes are individually valid");
        assert!(
            !verify_proof(&FaultProof::new(va, vb), &stakes, E),
            "two different validators voting at the same epoch is NOT one validator's fault"
        );
    }

    // ── §5.5 penalty: reduces stake, burns, conserves ────────────────────────

    #[test]
    fn gadget4_slash_burns_and_conserves() {
        // A valid double-vote proof slashes the offender; the slashed µQTA is
        // burned and the balance sheet conserves.
        let g = cp(0, "GENESIS");
        let c1a = cp(1, "epoch1-A");
        let c1b = cp(1, "epoch1-B");
        let offender = derive_ml_dsa(&[1u8; 32]).expect("derive").1;
        let proof = FaultProof::new(signed_vote(1, &g, &c1a), signed_vote(1, &g, &c1b));

        let mut stakes = stake_map(1);
        let before = stakes.get(&offender).copied().unwrap();
        let out = slash_for_proof(&mut stakes, &proof, E).expect("valid proof slashes");

        assert_eq!(out.offender, offender);
        assert_eq!(out.stake_before, before);
        // Conservation — holds for ANY marked fraction:
        assert_eq!(out.remaining + out.slashed, out.stake_before, "stake is conserved across the slash");
        assert_eq!(out.burned, out.slashed, "the slashed stake is burned (default policy)");
        // The map reflects the reduction (full slash by default ⇒ removed).
        assert_eq!(stakes.get(&offender).copied().unwrap_or(0), out.remaining);
        assert!(out.slashed > 0, "a finality-breaking equivocation must cost the offender");
    }

    #[test]
    fn gadget4_unproven_accusation_slashes_nothing() {
        // A LEGAL vote pair (same source) yields no proof ⇒ slash_for_proof is a
        // no-op and the stake is untouched.
        let g = cp(0, "GENESIS");
        let c1 = cp(1, "h4");
        let c2 = cp(2, "h8");
        let proof = FaultProof::new(signed_vote(1, &g, &c1), signed_vote(1, &g, &c2));
        let mut stakes = stake_map(1);
        let snapshot = stakes.clone();
        assert!(slash_for_proof(&mut stakes, &proof, E).is_none(), "no proof ⇒ no slash");
        assert_eq!(stakes, snapshot, "an unproven accusation leaves stake untouched");
    }

    // ── §5.6 accountable-safety measure ≥ ⅓ (pure) ───────────────────────────

    #[test]
    fn gadget4_slashable_weight_covers_at_least_one_third() {
        // 3 equal-stake validators (100 each, total 300, ⅔ = 200). Two ⅔-quorums
        // back conflicting epoch-1 checkpoints: {1,2} → c1a and {2,3} → c1b. Their
        // intersection — validator 2 — double-voted, so ≥ ⅓ (exactly 100) is
        // provably slashable.
        let g = cp(0, "GENESIS");
        let c1a = cp(1, "epoch1-A");
        let c1b = cp(1, "epoch1-B");
        let stakes = stake_map(3);
        let total: u64 = stakes.values().sum();

        let votes = vec![
            signed_vote(1, &g, &c1a),
            signed_vote(2, &g, &c1a), // validator 2 on fork A …
            signed_vote(2, &g, &c1b), // … and on fork B ⇒ double vote
            signed_vote(3, &g, &c1b),
        ];
        let slashable = slashable_weight(&votes, &stakes, E);
        assert_eq!(slashable, 100, "only the intersecting validator (2) is slashable");
        assert!(slashable * 3 >= total, "accountable safety: ≥ ⅓ of stake ({slashable}/{total})");
    }

    #[test]
    fn gadget4_honest_votes_leave_nothing_slashable() {
        // The contrast: an all-honest vote pool (each validator one vote, distinct
        // target epochs, no nesting) proves nothing — slashable weight is zero.
        let g = cp(0, "GENESIS");
        let c1 = cp(1, "h4");
        let c2 = cp(2, "h8");
        let stakes = stake_map(3);
        let votes = vec![
            signed_vote(1, &g, &c1),
            signed_vote(2, &g, &c1),
            signed_vote(3, &c1, &c2),
        ];
        assert_eq!(slashable_weight(&votes, &stakes, E), 0, "no equivocation ⇒ nothing slashable");
    }

    // ── §5.7 determinism (C1 in miniature) ───────────────────────────────────

    #[test]
    fn gadget4_detection_and_penalty_are_deterministic() {
        // The whole pipeline — votes, detection, proof, slash, measure — rebuilt
        // twice must be byte-identical (SIGN-DET + pure integer verdicts).
        let build = || {
            let g = cp(0, "GENESIS");
            let c1a = cp(1, "epoch1-A");
            let c1b = cp(1, "epoch1-B");
            let stakes = stake_map(3);
            let votes = vec![
                signed_vote(1, &g, &c1a),
                signed_vote(2, &g, &c1a),
                signed_vote(2, &g, &c1b),
                signed_vote(3, &g, &c1b),
            ];
            let weight = slashable_weight(&votes, &stakes, E);
            let proof = FaultProof::new(votes[1].clone(), votes[2].clone());
            let mut s = stakes.clone();
            let out = slash_for_proof(&mut s, &proof, E);
            (votes, weight, out)
        };
        assert_eq!(build(), build(), "detection + penalty must be reproducible across nodes");
    }
}
