//! `finality_vote` — finality **votes** (attestations) and the **epoch
//! certificate** (the ⅔ super-majority link): **GADGET-2** of the Casper-FFG
//! finality gadget (`DESIGN-FINALITY-GADGET.md` §3, §5).
//!
//! Built on GADGET-1's epoch / checkpoint skeleton ([`super::finality`]) and the
//! on-chain stake state (ONCHAIN-STAKE-1, `Ledger::validator_stakes`). **Nothing
//! finalizes here** — the two-step justify/finalize rule that *consumes* these
//! certificates is **GADGET-3**, deliberately not coded. This module builds only
//! the material the rule will use, plus its **pure, deterministic** verification.
//! The harness `FinalitySafety` invariant is untouched (still quasi-vacuous).
//!
//! # Crypto — post-quantum **pure** (ADR-005)
//! Votes are signed in **ML-DSA-65 only**. ADR-005 mandates « aucune primitive
//! classique sur le chemin de l'irréversibilité » → no Ed25519/BLS on the
//! finality path. Verification reuses the project's **single** ML-DSA verifier
//! ([`crate::security::hybrid_crypto::verify_ml_dsa`]); deterministic signing for
//! the DST harness reuses the existing `#[cfg(test)] ml_dsa_sign_deterministic`
//! (SIGN-DET). No new entropy path and **no production signing** is added here.
//!
//! # Determinism (`sm/` sans-IO, C1)
//! Vote verification and certificate validation are **pure functions** of (the
//! votes, the on-chain stake map): no clock, no entropy, and **no `HashMap`
//! iteration order in the verdict** — weights are summed (commutative) and voter
//! distinctness uses a `BTreeSet`. Stake is a function of the chain
//! (ONCHAIN-STAKE-1), so every node computes the **same** ⅔ threshold and the
//! **same** verdict. The ⅔ test is integer-only (no float).
//!
//! # Validator identity (flagged §4, Constitution stop-rule)
//! A validator's finality identity here is their **ML-DSA-65 public key** (hex):
//! it is BOTH the stake-weight key and the signature-verification key — the
//! natural consequence of ADR-005's PQ-pure finality. The live
//! `validator_stakes()` is currently keyed by the Ed25519 *account* key, so
//! wiring real votes needs either an account→ML-DSA-pk **binding registry** or a
//! validator set **re-keyed** by ML-DSA pk. (A purely-public binding is
//! impossible: the ML-DSA key is derived from the Ed25519 *secret*, unreachable
//! from the public key alone — so naïvely carrying both keys would let an
//! attacker pair a victim's account key with their own ML-DSA key and forge a
//! weighted vote. Making the ML-DSA key the identity closes that hole *within
//! this module*.) The reconciliation is a consensus/identity arbitration **left
//! to Alexandre** (Constitution §4); it blocks nothing now — nothing finalizes
//! and no live path is wired. GADGET-2 is written agnostic to the key string's
//! provenance.

use std::collections::{BTreeSet, HashMap};

use super::finality::{epoch_of_height, is_epoch_boundary, Checkpoint};

/// Domain separator for the canonical vote-signing bytes. **Never change**
/// without a protocol version bump — it would invalidate every existing vote
/// signature.
const VOTE_DOMAIN: &[u8] = b"QUANTA-FINALITY-VOTE-v1";

/// BFT quorum fraction = **⅔ of staked weight** (`QUORUM_NUM / QUORUM_DEN`).
/// Standard BFT super-majority; the whole gadget design assumes it. This is a
/// **réglable** default (§12 / ADR-005), *not* a graven promise — see ADR-006.
const QUORUM_NUM: u128 = 2;
const QUORUM_DEN: u128 = 3;

// ─── §1 The vote (attestation) ────────────────────────────────────────────────

/// A finality **vote**: the validator attests a super-majority *link*
/// `source → target` for `voting_epoch`. A pure value (no clock, no entropy),
/// signed in ML-DSA-65.
///
/// The `serde` derives (LIVE-1) let a vote cross the gossip wire
/// (`GossipMessage::FinalityVote`); they are on the **data carrier** only —
/// verification stays a pure function, C1 preserved.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Vote {
    /// Source checkpoint (GADGET-1) — the link's tail.
    pub source: Checkpoint,
    /// Target checkpoint being attested — a **strictly later** boundary.
    pub target: Checkpoint,
    /// The attested epoch (`== target.epoch` for a well-formed vote).
    pub voting_epoch: u64,
    /// Voter identity = the validator's **ML-DSA-65 public key** (hex). Doubles
    /// as the stake-weight key (see module doc, flagged §4).
    pub validator: String,
    /// ML-DSA-65 signature over [`Vote::signable_bytes`].
    pub signature: Vec<u8>,
}

impl Vote {
    /// Canonical, **unambiguous** bytes the voter signs: a domain separator plus
    /// length-prefixed fields, so no concatenation can collide. Excludes the
    /// signature itself. Fully deterministic.
    pub fn signable_bytes(&self) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(VOTE_DOMAIN);
        push_checkpoint(&mut b, &self.source);
        push_checkpoint(&mut b, &self.target);
        b.extend_from_slice(&self.voting_epoch.to_le_bytes());
        push_bytes(&mut b, self.validator.as_bytes());
        b
    }

    /// On-chain stake backing this voter (`0` ⇒ not an active validator). Pure.
    pub fn weight(&self, stakes: &HashMap<String, u64>) -> u64 {
        stakes.get(&self.validator).copied().unwrap_or(0)
    }

    /// Whether the vote's *link* is structurally well-formed (pure over the
    /// checkpoints + `E`):
    /// - `source` and `target` are genuine epoch-boundary checkpoints
    ///   (`height` a multiple of `E`, `epoch == height / E`);
    /// - the attested epoch matches the target;
    /// - **`target` descends from `source`** — here a *strictly later boundary*
    ///   (`target.height > source.height`). Full hash-ancestry along the chain is
    ///   **GADGET-3's** job (it has the chain and reasons about justification);
    ///   what's checkable from the checkpoint tuples alone — and all GADGET-2
    ///   needs — is the forward link. We deliberately do not anticipate the rule.
    fn link_well_formed(&self, epoch_len: u64) -> bool {
        let cp_ok = |c: &Checkpoint| {
            is_epoch_boundary(c.height, epoch_len) && c.epoch == epoch_of_height(c.height, epoch_len)
        };
        cp_ok(&self.source)
            && cp_ok(&self.target)
            && self.voting_epoch == self.target.epoch
            && self.target.height > self.source.height
    }

    /// Verify the vote against on-chain stake (**pure**). `true` iff the link is
    /// well-formed, the signer is an **active validator** (stake > 0), and the
    /// ML-DSA-65 signature is valid under the voter's PQ public key (= identity).
    pub fn verify(&self, stakes: &HashMap<String, u64>, epoch_len: u64) -> bool {
        if !self.link_well_formed(epoch_len) {
            return false;
        }
        if self.weight(stakes) == 0 {
            return false; // signer is not an active validator
        }
        crate::security::hybrid_crypto::verify_ml_dsa(
            &self.validator,
            &self.signable_bytes(),
            &self.signature,
        )
    }
}

// ─── §2 The epoch certificate, behind the ADR-005 abstraction ──────────────────

/// **The ADR-005 certificate abstraction.** A finality certificate is a
/// *verifiable super-majority link* `source → target`: a proof that validators
/// holding **≥ ⅔ of the staked weight** attested to this link. The aggregation
/// scheme that carries the proof — today a set of ML-DSA [`Vote`]s
/// ([`MlDsaCertificate`]), tomorrow possibly a BLS / SNARK aggregate — lives
/// **behind this trait**, so swapping it is a *local* change that leaves the
/// consumer (GADGET-3) untouched. This trait is the **single definition** of
/// "certificate" in the gadget (no second copy of the concept).
pub trait FinalityCertificate {
    /// The link's source checkpoint.
    fn source(&self) -> &Checkpoint;
    /// The link's target checkpoint.
    fn target(&self) -> &Checkpoint;

    /// Total backing weight = Σ on-chain stake of the **distinct, valid** voters
    /// (pure). `None` if the certificate is malformed — a bad vote, **mixed
    /// links**, or a **duplicated voter** — because a malformed certificate has
    /// no meaningful weight (and must never be summed into a quorum).
    fn backing_weight(&self, stakes: &HashMap<String, u64>, epoch_len: u64) -> Option<u64>;

    /// Fully valid iff well-formed **and** backing weight clears ⅔ of total
    /// stake (pure). This is the predicate GADGET-3 will gate finalization on.
    fn is_valid(&self, stakes: &HashMap<String, u64>, epoch_len: u64) -> bool {
        match (self.backing_weight(stakes, epoch_len), total_stake(stakes)) {
            (Some(backing), Some(total)) => meets_supermajority(backing, total),
            // malformed certificate, or stake sum overflow ⇒ refuse (no panic,
            // checked arithmetic — Constitution).
            _ => false,
        }
    }
}

/// ML-DSA implementation of [`FinalityCertificate`]: a set of [`Vote`]s for
/// **one** link — the post-quantum-pure scheme of ADR-005.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MlDsaCertificate {
    source: Checkpoint,
    target: Checkpoint,
    votes: Vec<Vote>,
}

impl MlDsaCertificate {
    /// Assemble a certificate for the link `source → target` from `votes`.
    /// **No validation at construction** — validity is a pure function of the
    /// stake state, checked by [`FinalityCertificate::is_valid`], so one
    /// certificate can be (re)checked against any stake snapshot deterministically
    /// (e.g. by every node).
    pub fn new(source: Checkpoint, target: Checkpoint, votes: Vec<Vote>) -> Self {
        Self { source, target, votes }
    }

    /// The backing votes (read-only). The abstraction hides the representation
    /// from GADGET-3; this accessor exists for inspection / future gossip.
    pub fn votes(&self) -> &[Vote] {
        &self.votes
    }
}

impl FinalityCertificate for MlDsaCertificate {
    fn source(&self) -> &Checkpoint {
        &self.source
    }

    fn target(&self) -> &Checkpoint {
        &self.target
    }

    fn backing_weight(&self, stakes: &HashMap<String, u64>, epoch_len: u64) -> Option<u64> {
        if self.votes.is_empty() {
            return None; // an empty certificate proves nothing
        }
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut backing: u64 = 0;
        for vote in &self.votes {
            // Every vote must attest **this** certificate's link …
            if vote.source != self.source || vote.target != self.target {
                return None; // mixed links
            }
            // … be individually valid (signature + active validator + form) …
            if !vote.verify(stakes, epoch_len) {
                return None; // forged / invalid vote
            }
            // … and come from a **distinct** validator.
            if !seen.insert(vote.validator.as_str()) {
                return None; // double counting
            }
            backing = backing.checked_add(vote.weight(stakes))?;
        }
        Some(backing)
    }
}

// ─── §2 quorum maths (single source of truth, integer-only) ────────────────────

/// Whether `backing` clears the ⅔ super-majority of `total` — and is **non-zero**,
/// so an empty / degenerate certificate against zero total stake never slips
/// through. Integer-only (`u128`), no float, no overflow (`backing, total ≤ u64`).
fn meets_supermajority(backing: u64, total: u64) -> bool {
    backing > 0 && (backing as u128) * QUORUM_DEN >= (total as u128) * QUORUM_NUM
}

/// Σ of all staked weights (**checked**). `None` on overflow ⇒ caller refuses.
/// The real ceiling (100M QUANTA) is far below `u64::MAX`, but the Constitution
/// mandates checked arithmetic on weights — a malformed/adversarial map can't
/// wrap us into a false quorum.
fn total_stake(stakes: &HashMap<String, u64>) -> Option<u64> {
    stakes.values().try_fold(0u64, |acc, &v| acc.checked_add(v))
}

// ─── canonical encoding helpers ───────────────────────────────────────────────

/// Append a `u64` length prefix then the bytes — length-prefixing makes the
/// concatenation in [`Vote::signable_bytes`] injective (no field-boundary
/// ambiguity between, e.g., two adjacent strings).
fn push_bytes(buf: &mut Vec<u8>, data: &[u8]) {
    buf.extend_from_slice(&(data.len() as u64).to_le_bytes());
    buf.extend_from_slice(data);
}

/// Append a checkpoint canonically: `epoch ‖ height ‖ len(hash) ‖ hash`.
fn push_checkpoint(buf: &mut Vec<u8>, c: &Checkpoint) {
    buf.extend_from_slice(&c.epoch.to_le_bytes());
    buf.extend_from_slice(&c.height.to_le_bytes());
    push_bytes(buf, c.hash.as_bytes());
}

// ─── §4 The teeth (anti-vacuity) ──────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::hybrid_crypto::{derive_ml_dsa, ml_dsa_sign_deterministic};

    const E: u64 = 4; // small epoch length for the tests (parametric — any E ≥ 1)

    fn cp(epoch: u64, height: u64, hash: &str) -> Checkpoint {
        Checkpoint { epoch, height, hash: hash.to_string() }
    }

    /// Derive a deterministic ML-DSA validator from `seed`, and **sign** the vote
    /// `source → target @ voting_epoch` via the SIGN-DET path (reproducible in
    /// the harness). Returns the validator's identity (ML-DSA pk hex) + the
    /// signed vote.
    fn signed_vote(
        seed: u8,
        source: Checkpoint,
        target: Checkpoint,
        voting_epoch: u64,
    ) -> (String, Vote) {
        let (sk, pk_hex) = derive_ml_dsa(&[seed; 32]).expect("derive ml-dsa keypair");
        let mut v = Vote {
            source,
            target,
            voting_epoch,
            validator: pk_hex.clone(),
            signature: Vec::new(),
        };
        v.signature = ml_dsa_sign_deterministic(&sk, &v.signable_bytes()).expect("sign");
        (pk_hex, v)
    }

    /// Three equal-stake validators (100 each, total 300); ⅔ = 200, so any two
    /// of them form a quorum and any one does not. Returns the stake map + the
    /// three signed votes for the canonical link `genesis(0,0) → (1,4)@1`.
    fn three_validator_fixture() -> (HashMap<String, u64>, Checkpoint, Checkpoint, [Vote; 3]) {
        let g = cp(0, 0, "GENESIS");
        let t = cp(1, 4, "h4");
        let (pa, va) = signed_vote(1, g.clone(), t.clone(), 1);
        let (pb, vb) = signed_vote(2, g.clone(), t.clone(), 1);
        let (pc, vc) = signed_vote(3, g.clone(), t.clone(), 1);
        let mut stakes = HashMap::new();
        stakes.insert(pa, 100);
        stakes.insert(pb, 100);
        stakes.insert(pc, 100);
        (stakes, g, t, [va, vb, vc])
    }

    // ── vote-level teeth (strengthen §1) ─────────────────────────────────────

    #[test]
    fn gadget2_valid_vote_verifies() {
        let (stakes, _g, _t, [va, ..]) = three_validator_fixture();
        assert!(va.verify(&stakes, E), "a well-formed, staked, signed vote must verify");
    }

    #[test]
    fn gadget2_vote_from_non_validator_is_rejected() {
        // Signed correctly, but the signer holds **no stake** ⇒ not active.
        let g = cp(0, 0, "GENESIS");
        let t = cp(1, 4, "h4");
        let (_pk, v) = signed_vote(7, g, t, 1);
        let empty = HashMap::new(); // no validators
        assert!(!v.verify(&empty, E), "a non-validator's vote must be rejected");
    }

    #[test]
    fn gadget2_vote_with_non_descending_link_is_rejected() {
        // target is NOT a strictly-later boundary than source ⇒ malformed link.
        let source = cp(1, 4, "h4");
        let target = cp(0, 0, "GENESIS"); // height 0 < 4 → not descending
        let (_pk, v) = signed_vote(1, source, target, 0);
        let (stakes, ..) = three_validator_fixture();
        assert!(!v.verify(&stakes, E), "a non-descending link must be rejected");
    }

    #[test]
    fn gadget2_vote_with_malformed_checkpoint_is_rejected() {
        // height 5 is not a multiple of E=4 ⇒ not an epoch boundary.
        let g = cp(0, 0, "GENESIS");
        let bad = cp(1, 5, "h5");
        let (_pk, v) = signed_vote(1, g, bad, 1);
        let (stakes, ..) = three_validator_fixture();
        assert!(!v.verify(&stakes, E), "a non-boundary checkpoint must be rejected");
    }

    #[test]
    fn gadget2_tampered_signable_field_breaks_verification() {
        // Sign for one link, then mutate the target hash: the signature no longer
        // matches signable_bytes ⇒ ML-DSA verify fails (binding is real).
        let g = cp(0, 0, "GENESIS");
        let t = cp(1, 4, "h4");
        let (_pk, mut v) = signed_vote(1, g, t, 1);
        v.target.hash = "h4-tampered".into();
        let (stakes, ..) = three_validator_fixture();
        assert!(!v.verify(&stakes, E), "mutating a signed field must break verification");
    }

    // ── §4 certificate teeth (the six mandated) ──────────────────────────────

    #[test]
    fn gadget2_certificate_below_quorum_is_rejected() {
        // 1 of 3 validators (100/300) — below ⅔ (200). Math-level reject (not
        // malformed): backing is computed, but the threshold is not met.
        let (stakes, g, t, [va, ..]) = three_validator_fixture();
        let cert = MlDsaCertificate::new(g, t, vec![va]);
        assert_eq!(cert.backing_weight(&stakes, E), Some(100), "backing is the single voter's stake");
        assert!(!cert.is_valid(&stakes, E), "below ⅔ must be rejected");
    }

    #[test]
    fn gadget2_certificate_with_forged_vote_is_rejected() {
        // Two voters that WOULD reach ⅔, but one signature is corrupted.
        let (stakes, g, t, [va, mut vb, _vc]) = three_validator_fixture();
        vb.signature[0] ^= 0xFF; // flip a signature byte → invalid ML-DSA sig
        let cert = MlDsaCertificate::new(g, t, vec![va, vb]);
        assert_eq!(cert.backing_weight(&stakes, E), None, "a forged vote makes the cert malformed");
        assert!(!cert.is_valid(&stakes, E), "a certificate carrying a forged vote must be rejected");
    }

    #[test]
    fn gadget2_certificate_mixing_links_is_rejected() {
        // Both votes are individually valid, but for DIFFERENT links.
        let (mut stakes, g, t, [va, ..]) = three_validator_fixture();
        let other_target = cp(2, 8, "h8");
        let (pk_d, v_other) = signed_vote(9, g.clone(), other_target.clone(), 2);
        stakes.insert(pk_d, 100); // make the off-link voter active too
        assert!(v_other.verify(&stakes, E), "the off-link vote is valid on ITS own link");
        // Certificate claims link g→t but carries a vote for g→other_target.
        let cert = MlDsaCertificate::new(g, t, vec![va, v_other]);
        assert_eq!(cert.backing_weight(&stakes, E), None, "mixed links ⇒ malformed");
        assert!(!cert.is_valid(&stakes, E), "a certificate mixing links must be rejected");
    }

    #[test]
    fn gadget2_certificate_double_counting_is_rejected() {
        // The SAME validator counted twice would naïvely reach ⅔ (200/300);
        // distinctness must reject it instead.
        let (stakes, g, t, [va, ..]) = three_validator_fixture();
        let cert = MlDsaCertificate::new(g, t, vec![va.clone(), va]);
        assert_eq!(cert.backing_weight(&stakes, E), None, "a repeated voter ⇒ malformed");
        assert!(!cert.is_valid(&stakes, E), "double-counting a validator must be rejected");
    }

    #[test]
    fn gadget2_valid_two_thirds_certificate_is_accepted() {
        // 2 of 3 validators (200/300) = exactly ⅔ ⇒ accepted (threshold is ≥).
        let (stakes, g, t, [va, vb, _vc]) = three_validator_fixture();
        let cert = MlDsaCertificate::new(g.clone(), t.clone(), vec![va, vb]);
        assert_eq!(cert.source(), &g);
        assert_eq!(cert.target(), &t);
        assert_eq!(cert.votes().len(), 2);
        assert_eq!(cert.backing_weight(&stakes, E), Some(200), "two voters back 200 µQTA");
        assert!(cert.is_valid(&stakes, E), "a ⅔ link with distinct valid voters must be accepted");
    }

    #[test]
    fn gadget2_verdict_is_deterministic_across_nodes() {
        // Two "nodes" build the SAME chain-derived stake snapshot and the SAME
        // votes independently; the certificate verdict must be byte-identical.
        let (stakes_a, g, t, votes_a) = three_validator_fixture();
        let (stakes_b, _g2, _t2, votes_b) = three_validator_fixture();

        // SIGN-DET: the same (key, message) yields the same signature on both
        // nodes — the cross-node agreement the gadget rests on.
        assert_eq!(votes_a[0].signature, votes_b[0].signature, "deterministic ML-DSA signing");
        assert_eq!(votes_a, votes_b, "identical votes on both nodes");

        let cert_a = MlDsaCertificate::new(g.clone(), t.clone(), vec![votes_a[0].clone(), votes_a[1].clone()]);
        let cert_b = MlDsaCertificate::new(g, t, vec![votes_b[0].clone(), votes_b[1].clone()]);

        assert_eq!(
            cert_a.backing_weight(&stakes_a, E),
            cert_b.backing_weight(&stakes_b, E),
            "same votes + same stake ⇒ same backing weight",
        );
        assert_eq!(
            cert_a.is_valid(&stakes_a, E),
            cert_b.is_valid(&stakes_b, E),
            "same votes + same stake ⇒ same verdict on two nodes",
        );
        assert!(cert_a.is_valid(&stakes_a, E));
    }
}
