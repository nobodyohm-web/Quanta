//! `finality_rule` — the **justify/finalize** two-step rule: **GADGET-3**, where
//! finality stops being vacuous and becomes **real**
//! (`DESIGN-FINALITY-GADGET.md` §4, §11).
//!
//! Built on GADGET-1's epoch / checkpoint skeleton ([`super::finality`]) and
//! GADGET-2's super-majority certificate ([`super::finality_vote`]). This module
//! adds the **state** finality needs — the set of **justified** checkpoints (§1)
//! — and the single transition that consumes a certificate (§2):
//!
//! 1. **Justify** — given a *valid* ⅔ certificate for the link `source → target`,
//!    if `source` is **justified**, then `target` becomes **justified**.
//! 2. **Finalize** — if additionally `target` is the **direct child** of `source`
//!    (the next epoch, `source.epoch + 1`), then `source` becomes **finalized**.
//!    Two consecutive linked epochs: the Casper-FFG finalization condition.
//!
//! Nothing else finalizes: a checkpoint justified **alone** (no link to its
//! direct child) is *not* finalized — that is the heart the §4 teeth pin down
//! ("two-step, not one"). A **sub-quorum** certificate, or a link whose `source`
//! is **not justified**, changes nothing (no fraudulent justification).
//!
//! # Determinism (`sm/` sans-IO, C1)
//! Every transition is a **pure function** of (the certificate, the on-chain
//! stake snapshot): no clock, no entropy, no `HashMap` iteration order in the
//! verdict. The justified / finalized sets are `BTreeMap`-backed (epoch-ordered
//! iteration), so two nodes fed the same certificates reach the **same** sets
//! byte-for-byte — what the cross-node `FinalitySafety` invariant rests on.
//!
//! # Scope (held to GADGET-3)
//! This is *only* the justify/finalize rule. **No fork-choice** (GADGET-5) and
//! **no slashing** (GADGET-4) live here. The certificate is consumed against a
//! stake snapshot **supplied by the caller** — its provenance (re-keying
//! `validator_stakes()` to the finality-vote identity) is the open
//! ADR-002 / identity reconciliation flagged in GADGET-2 §4, deliberately not
//! resolved here, and no live gossip path is wired (mirroring GADGET-2).

use std::collections::{BTreeMap, HashMap};

use super::finality::{Checkpoint, FinalizedSet};
use super::finality_vote::FinalityCertificate;

// ─── §1 The justification state ───────────────────────────────────────────────

/// The set of **justified** checkpoints, keyed by epoch (design §4). Init
/// `{genesis}` (genesis is justified by definition), grown by the justify step
/// of the rule. The sibling of GADGET-1's [`FinalizedSet`]: a checkpoint must be
/// justified **before** it can be finalized. `BTreeMap` keeps `iter()`
/// epoch-ordered so any cross-node comparison is reproducible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JustifiedSet {
    by_epoch: BTreeMap<u64, Checkpoint>,
}

impl JustifiedSet {
    /// Initial state — **genesis only** (epoch 0, height 0), justified by
    /// definition (design §4). Nothing else is justified until a certificate
    /// links to it.
    pub fn genesis_only(genesis_hash: String) -> Self {
        let mut by_epoch = BTreeMap::new();
        by_epoch.insert(
            0,
            Checkpoint {
                epoch: 0,
                height: 0,
                hash: genesis_hash,
            },
        );
        Self { by_epoch }
    }

    /// The justified checkpoint at `epoch`, if any.
    pub fn get(&self, epoch: u64) -> Option<&Checkpoint> {
        self.by_epoch.get(&epoch)
    }

    /// Justified checkpoints in epoch order (deterministic).
    pub fn iter(&self) -> impl Iterator<Item = &Checkpoint> {
        self.by_epoch.values()
    }

    /// Whether `cp` is **exactly** the justified checkpoint at its epoch — same
    /// epoch *and* same hash. A different hash at that epoch is a *conflict*, not
    /// a match, so this returns `false`: the rule never builds a link on a
    /// checkpoint it has not itself justified.
    pub fn is_justified(&self, cp: &Checkpoint) -> bool {
        self.by_epoch.get(&cp.epoch) == Some(cp)
    }

    /// Record `cp` as justified (≤1 per epoch). Internal to the rule — the only
    /// caller is [`FinalityState::apply_certificate`]'s justify step.
    fn insert(&mut self, cp: Checkpoint) {
        self.by_epoch.insert(cp.epoch, cp);
    }
}

// ─── §2 The justify/finalize state machine (Casper two-step) ───────────────────

/// **GADGET-3 — the finality state machine.** Bundles the justified set (§1)
/// with GADGET-1's [`FinalizedSet`], and exposes the single transition
/// [`FinalityState::apply_certificate`]. The *place* GADGET-1 reserved for the
/// rule to branch in is now occupied: applying certificates **grows** the
/// finalized set beyond genesis, so the harness `FinalitySafety` invariant stops
/// guarding a vacuous (genesis-only) set and starts guarding **real** finalized
/// history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalityState {
    justified: JustifiedSet,
    finalized: FinalizedSet,
}

impl FinalityState {
    /// Genesis-only state: `{genesis}` is **both** justified and finalized by
    /// definition (design §4).
    pub fn genesis_only(genesis_hash: String) -> Self {
        Self {
            justified: JustifiedSet::genesis_only(genesis_hash.clone()),
            finalized: FinalizedSet::genesis_only(genesis_hash),
        }
    }

    /// Read-only view of the justified set (§1).
    pub fn justified(&self) -> &JustifiedSet {
        &self.justified
    }

    /// Read-only view of the finalized set — what the harness `FinalitySafety`
    /// invariant guards across nodes.
    pub fn finalized(&self) -> &FinalizedSet {
        &self.finalized
    }

    /// **The rule (two-step).** Apply one super-majority certificate (GADGET-2)
    /// for the link `source → target`, weighed against the `stakes` snapshot:
    ///
    /// 1. **Justify** — if `source` is justified, `target` becomes justified.
    /// 2. **Finalize** — if additionally `target` is the **direct child**
    ///    (`target.epoch == source.epoch + 1`), `source` becomes finalized.
    ///
    /// Rejects (changing **nothing**) when the certificate does not clear ⅔
    /// ([`FinalityCertificate::is_valid`]) or its `source` is **not justified**
    /// — no fraudulent justification. Finalization is **append-only**: an epoch
    /// already finalized is never overwritten (finality is irreversible). Pure —
    /// no clock, no entropy. Returns what the step advanced.
    pub fn apply_certificate<C: FinalityCertificate>(
        &mut self,
        cert: &C,
        stakes: &HashMap<String, u64>,
        epoch_len: u64,
    ) -> StepOutcome {
        // GADGET-2 gate: a sub-quorum / malformed certificate proves nothing.
        if !cert.is_valid(stakes, epoch_len) {
            return StepOutcome::default();
        }
        let source = cert.source();
        let target = cert.target();
        // Step 1 precondition: we only build a link on a checkpoint WE justified.
        if !self.justified.is_justified(source) {
            return StepOutcome::default();
        }
        let mut outcome = StepOutcome::default();
        // Step 1 — justify the target (idempotent; ≤1 justified per epoch).
        if !self.justified.is_justified(target) {
            self.justified.insert(target.clone());
            outcome.justified = true;
        }
        // Step 2 — a link to the *direct child* (two consecutive epochs)
        // finalizes the source. A skip-link (epoch gap > 1) justifies but never
        // finalizes — "two-step, not one". Append-only: never overwrite an
        // already-finalized epoch (irreversibility; also makes it idempotent).
        if target.epoch == source.epoch.saturating_add(1)
            && self.finalized.get(source.epoch).is_none()
        {
            self.finalized.insert(source.clone());
            outcome.finalized = true;
        }
        outcome
    }

    /// **Test hook (planted finalization).** Record a checkpoint as finalized
    /// with **no justification check** — used only by the harness teeth test
    /// ([`crate::sm`] `sim`) to *plant* a cross-node conflict and prove the
    /// `FinalitySafety` invariant bites. The real, checked path is
    /// [`FinalityState::apply_certificate`].
    #[cfg(test)]
    pub(crate) fn record_finalized_for_test(&mut self, cp: Checkpoint) {
        self.finalized.insert(cp);
    }
}

/// What [`FinalityState::apply_certificate`] advanced: whether the `target` was
/// newly **justified** and/or the `source` newly **finalized**. Both `false`
/// means the certificate was rejected (sub-quorum / unjustified source) or
/// redundant (already applied). Lets a caller observe that the rule made real
/// progress without re-reading the sets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StepOutcome {
    /// The certificate's `target` became justified on this call.
    pub justified: bool,
    /// The certificate's `source` became finalized on this call.
    pub finalized: bool,
}

// ─── §4 The teeth (the rule must bite) ─────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::hybrid_crypto::{derive_ml_dsa, ml_dsa_sign_deterministic};
    use crate::sm::finality_vote::{MlDsaCertificate, Vote};

    const E: u64 = 4; // small epoch length for the tests (parametric — any E ≥ 1)

    /// A checkpoint at `epoch` (its boundary height is `epoch × E`, by
    /// definition), carrying `hash`.
    fn cp(epoch: u64, hash: &str) -> Checkpoint {
        Checkpoint {
            epoch,
            height: epoch * E,
            hash: hash.to_string(),
        }
    }

    /// A deterministic ML-DSA validator `seed`, signing the vote
    /// `source → target @ target.epoch` via the SIGN-DET path (reproducible in
    /// the harness — never hedged entropy in sim).
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

    /// Stake map for `n` equal-weight (100 µQTA each) validators, seeds `1..=n`
    /// (total `100·n`). With the whole committee signing a link, backing = total
    /// ≥ ⅔ — a quorum; a single signer (100) clears ⅔ only when `n == 1`.
    fn stake_map(n: u8) -> HashMap<String, u64> {
        (1..=n)
            .map(|s| (derive_ml_dsa(&[s; 32]).expect("derive").1, 100u64))
            .collect()
    }

    /// A certificate for `source → target` signed by the first `signers`
    /// validators (seeds `1..=signers`).
    fn cert(source: &Checkpoint, target: &Checkpoint, signers: u8) -> MlDsaCertificate {
        let votes = (1..=signers)
            .map(|s| signed_vote(s, source, target))
            .collect();
        MlDsaCertificate::new(source.clone(), target.clone(), votes)
    }

    // ── §4.1 justification ───────────────────────────────────────────────────

    #[test]
    fn gadget3_quorum_link_from_justified_source_justifies_target() {
        // A ⅔ certificate (both of 2 validators, 200/200) from the justified
        // genesis justifies its target.
        let mut st = FinalityState::genesis_only("GENESIS".into());
        let g = cp(0, "GENESIS");
        let c1 = cp(1, "h4");
        let stakes = stake_map(2);
        let out = st.apply_certificate(&cert(&g, &c1, 2), &stakes, E);
        assert!(out.justified, "a ⅔ link from a justified source must justify the target");
        assert!(st.justified().is_justified(&c1), "c1 is now justified");
    }

    // ── §4.2 no fraudulent justification ─────────────────────────────────────

    #[test]
    fn gadget3_unjustified_source_justifies_nothing() {
        // c1 was never justified ⇒ a (valid, ⅔) certificate c1→c2 changes nothing.
        let mut st = FinalityState::genesis_only("GENESIS".into());
        let c1 = cp(1, "h4");
        let c2 = cp(2, "h8");
        let stakes = stake_map(2);
        let out = st.apply_certificate(&cert(&c1, &c2, 2), &stakes, E);
        assert_eq!(out, StepOutcome::default(), "unjustified source ⇒ no change");
        assert!(st.justified().get(1).is_none(), "c1 must NOT be justified");
        assert!(st.justified().get(2).is_none(), "c2 must NOT be justified");
    }

    #[test]
    fn gadget3_subquorum_certificate_justifies_nothing() {
        // 3 validators (100 each, ⅔ = 200) but only ONE signs (100 < 200).
        let mut st = FinalityState::genesis_only("GENESIS".into());
        let g = cp(0, "GENESIS");
        let c1 = cp(1, "h4");
        let stakes = stake_map(3);
        let weak = cert(&g, &c1, 1);
        assert!(!weak.is_valid(&stakes, E), "1/3 of stake is below ⅔");
        let out = st.apply_certificate(&weak, &stakes, E);
        assert_eq!(out, StepOutcome::default(), "a sub-quorum certificate justifies nothing");
        assert!(st.justified().get(1).is_none(), "nothing justified below quorum");
    }

    // ── §4.3 two-step, not one (the heart) ───────────────────────────────────

    #[test]
    fn gadget3_justified_alone_is_not_finalized_two_step_not_one() {
        // Justify c1 via g→c1: c1 is justified but NOT finalized. Finalization
        // needs a SECOND link c1→c2 (c1's direct child). One step never finalizes.
        let mut st = FinalityState::genesis_only("GENESIS".into());
        let g = cp(0, "GENESIS");
        let c1 = cp(1, "h4");
        let c2 = cp(2, "h8");
        let stakes = stake_map(2);

        let out1 = st.apply_certificate(&cert(&g, &c1, 2), &stakes, E);
        assert!(out1.justified, "step 1 justifies c1");
        assert!(!out1.finalized, "justifying c1 alone finalizes nothing new");
        assert!(st.justified().is_justified(&c1), "c1 justified");
        assert!(
            st.finalized().get(1).is_none(),
            "ONE step: c1 is justified but MUST NOT be finalized"
        );

        let out2 = st.apply_certificate(&cert(&c1, &c2, 2), &stakes, E);
        assert!(out2.finalized, "step 2: the link c1→c2 finalizes c1");
        assert_eq!(st.finalized().get(1), Some(&c1), "c1 is now finalized");
        assert!(st.justified().is_justified(&c2), "and c2 is justified");
    }

    #[test]
    fn gadget3_skip_link_justifies_but_never_finalizes() {
        // A gap link g(epoch 0) → c2(epoch 2) is a valid justification jump, but
        // the epochs are NOT consecutive ⇒ it justifies c2 yet finalizes nothing.
        let mut st = FinalityState::genesis_only("GENESIS".into());
        let g = cp(0, "GENESIS");
        let c2 = cp(2, "h8");
        let stakes = stake_map(2);
        let out = st.apply_certificate(&cert(&g, &c2, 2), &stakes, E);
        assert!(out.justified, "the jump justifies c2");
        assert!(!out.finalized, "a non-consecutive link finalizes nothing");
        assert!(st.justified().is_justified(&c2), "c2 justified by the jump");
        assert!(st.finalized().get(1).is_none(), "nothing at epoch 1");
        assert!(st.finalized().get(2).is_none(), "c2 not finalized (gap link)");
        assert_eq!(st.finalized().iter().count(), 1, "genesis is still the only finalized point");
    }

    // ── §4.4 honest path finalizes ───────────────────────────────────────────

    #[test]
    fn gadget3_honest_path_finalizes_expected_checkpoints() {
        // A chain of consecutive certificates g→c1→c2→c3 finalizes g, c1, c2
        // (each finalized when its direct child is justified by a link from it).
        let mut st = FinalityState::genesis_only("GENESIS".into());
        let g = cp(0, "GENESIS");
        let c1 = cp(1, "h4");
        let c2 = cp(2, "h8");
        let c3 = cp(3, "h12");
        let stakes = stake_map(2);

        st.apply_certificate(&cert(&g, &c1, 2), &stakes, E);
        st.apply_certificate(&cert(&c1, &c2, 2), &stakes, E);
        st.apply_certificate(&cert(&c2, &c3, 2), &stakes, E);

        for c in [&g, &c1, &c2, &c3] {
            assert!(st.justified().is_justified(c), "epoch {} should be justified", c.epoch);
        }
        assert_eq!(st.finalized().get(1), Some(&c1), "c1 finalized");
        assert_eq!(st.finalized().get(2), Some(&c2), "c2 finalized");
        assert!(
            st.finalized().get(3).is_none(),
            "c3 is justified but NOT finalized — no link to its child yet (two-step)"
        );
        assert_eq!(st.finalized().iter().count(), 3, "genesis + c1 + c2 finalized — it grew");
    }

    // ── §4.5 FinalitySafety on the honest path ───────────────────────────────

    #[test]
    fn gadget3_honest_rule_finalizes_no_conflicting_checkpoints() {
        // The honest rule never finalizes two conflicting checkpoints at the same
        // epoch: finalization is append-only and gated on a justified source
        // (≤1 per epoch). A second, CONFLICTING certificate at epoch 1 cannot
        // dislodge the already-finalized c1.
        let mut st = FinalityState::genesis_only("GENESIS".into());
        let g = cp(0, "GENESIS");
        let c1 = cp(1, "h4");
        let c2 = cp(2, "h8");
        let stakes = stake_map(2);

        st.apply_certificate(&cert(&g, &c1, 2), &stakes, E);
        st.apply_certificate(&cert(&c1, &c2, 2), &stakes, E);
        assert_eq!(st.finalized().get(1), Some(&c1), "c1 is finalized");

        // A conflicting checkpoint c1' at epoch 1 — its source g is justified, so
        // the cert is "valid", but c1 is already finalized (append-only) and
        // already justified at epoch 1, so neither the justified nor the finalized
        // entry at epoch 1 is overwritten. No conflict can be finalized.
        let c1_conflict = cp(1, "h4-conflict");
        let c2b = cp(2, "h8b");
        let out = st.apply_certificate(&cert(&c1_conflict, &c2b, 2), &stakes, E);
        assert_eq!(out, StepOutcome::default(), "a conflicting source is not justified ⇒ inert");
        assert_eq!(st.finalized().get(1), Some(&c1), "epoch 1 stays finalized to the ORIGINAL c1");
        assert!(st.justified().is_justified(&c1), "epoch 1 justification unchanged");
    }

    // ── §4.6 determinism across nodes ────────────────────────────────────────

    #[test]
    fn gadget3_rule_is_deterministic_across_nodes() {
        // Two "nodes" apply the SAME certificates over the SAME stake snapshot;
        // their justified / finalized sets must be byte-identical.
        let stakes = stake_map(2);
        let g = cp(0, "GENESIS");
        let c1 = cp(1, "h4");
        let c2 = cp(2, "h8");
        let mut node_a = FinalityState::genesis_only("GENESIS".into());
        let mut node_b = FinalityState::genesis_only("GENESIS".into());
        for (s, t) in [(&g, &c1), (&c1, &c2)] {
            node_a.apply_certificate(&cert(s, t, 2), &stakes, E);
            node_b.apply_certificate(&cert(s, t, 2), &stakes, E);
        }
        assert_eq!(
            node_a, node_b,
            "same certs + same stake ⇒ identical justified/finalized sets on two nodes"
        );
        assert_eq!(node_a.finalized().get(1), Some(&c1), "and finalization is real (not vacuous)");
    }
}
