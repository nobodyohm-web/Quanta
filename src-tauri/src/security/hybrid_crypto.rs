//! Quanta — primitive de signature post-quantique ML-DSA-65 (NIST FIPS 204).
//!
//! Ce module est la **source unique** de deux choses :
//!   - la **dérivation déterministe** de la paire ML-DSA-65 depuis la graine de
//!     32 octets de l'identité (`derive_ml_dsa`), via un XOF BLAKE3 — donc aucune
//!     matière secrète supplémentaire à persister : la clé PQ est recalculée au
//!     déverrouillage à partir de la même graine (zéro migration de vault) ;
//!   - l'**unique vérificateur** ML-DSA-65 du projet (`verify_ml_dsa`), réutilisé
//!     tel quel par l'autorité de transaction (PQ-MIG-3B), par les enveloppes
//!     gossip (PQ-ENVELOPE-1) et par le gadget de finalité (GADGET-2, ADR-005) —
//!     une seule source de vérité, aucune duplication de la primitive.
//!
//! ## Aucune couche hybride sur le chemin de l'autorité
//!
//! Le schéma hybride historique (signer Ed25519 **et** ML-DSA, avec repli
//! Ed25519-seul pour les identités antérieures) a été **retiré** : depuis
//! PQ-MIG-3B l'autorité de compte est **ML-DSA pure**, et depuis PQ-ENVELOPE-1
//! les enveloppes gossip le sont aussi (le repli Ed25519/legacy a été supprimé
//! au hard-fork v4). Ed25519 ne subsiste que comme identité de **transport**
//! (NodeId Iroh — dette upstream documentée dans CLAUDE.md), jamais comme
//! autorité sur des fonds.

use fips204::ml_dsa_65;
use fips204::traits::{SerDes, Verifier as _};
#[cfg(test)]
use fips204::traits::Signer as _;
use rand_core::{CryptoRng, RngCore};

/// Domaine de séparation pour la dérivation déterministe de la clé ML-DSA.
/// NE JAMAIS modifier : changerait la clé PQ de toutes les identités existantes.
const ML_DSA_DOMAIN: &[u8] = b"QUANTA-ML-DSA-65-derive-v1";

// ─── Dérivation déterministe de la clé ML-DSA ────────────────────────────────

/// CSPRNG déterministe adossé au XOF BLAKE3, graine = 32 octets secrets.
///
/// Sert UNIQUEMENT à dériver la paire ML-DSA-65 depuis la graine Ed25519 : la
/// même graine produit toujours la même clé PQ, donc rien de neuf à persister.
struct Blake3Rng {
    reader: blake3::OutputReader,
}

impl Blake3Rng {
    fn from_seed(seed: &[u8; 32]) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(ML_DSA_DOMAIN);
        h.update(seed);
        Self { reader: h.finalize_xof() }
    }
}

impl RngCore for Blake3Rng {
    fn next_u32(&mut self) -> u32 {
        let mut b = [0u8; 4];
        self.reader.fill(&mut b);
        u32::from_le_bytes(b)
    }
    fn next_u64(&mut self) -> u64 {
        let mut b = [0u8; 8];
        self.reader.fill(&mut b);
        u64::from_le_bytes(b)
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.reader.fill(dest);
    }
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
        self.reader.fill(dest);
        Ok(())
    }
}

// Le flux provient d'un XOF cryptographique (BLAKE3) → utilisable comme CSPRNG.
impl CryptoRng for Blake3Rng {}

/// Dérive la paire ML-DSA-65 depuis une graine Ed25519 de 32 octets.
/// Renvoie `(clé secrète, clé publique hex)`, ou `None` si la génération échoue
/// (ne se produit pas avec le XOF — repli sûr documenté côté appelant).
pub(crate) fn derive_ml_dsa(seed32: &[u8; 32]) -> Option<(ml_dsa_65::PrivateKey, String)> {
    let mut rng = Blake3Rng::from_seed(seed32);
    match ml_dsa_65::try_keygen_with_rng(&mut rng) {
        Ok((pk, sk)) => Some((sk, hex::encode(pk.into_bytes()))),
        Err(e) => {
            log::error!("◈ [HybridCrypto] échec keygen ML-DSA-65 : {e}");
            None
        }
    }
}

/// Deterministic ML-DSA-65 signature for the **simulation / DST harness only**.
///
/// ML-DSA's hedged signing draws `rnd` from `OsRng` (fault-attack resistance),
/// which makes a signature — and therefore any block whose Merkle root binds it
/// — non-reproducible. Here `rnd` is replaced by a BLAKE3-derived stream over
/// the message, so a given (key, message) always yields the **same** signature,
/// making seeded sweeps byte-reproducible (T0.8-HARDEN Phase 1).
///
/// Production signing is **untouched**: [`HybridIdentity::sign`] /
/// `CryptoEngine::sign_hybrid` keep `OsRng`. The sim runs on test keys with no
/// secrets at risk, so trading hedged entropy for determinism here never
/// weakens production.
///
/// SIGN-DET-VERIFY: gated `#[cfg(test)]` so the deterministic primitive is
/// **physically absent from release builds** — no production path can reach it,
/// even by accident, so the post-quantum hardness cannot silently downgrade.
#[cfg(test)]
pub(crate) fn ml_dsa_sign_deterministic(sk: &ml_dsa_65::PrivateKey, msg: &[u8]) -> Option<Vec<u8>> {
    let seed: [u8; 32] = *blake3::hash(msg).as_bytes();
    let mut rng = Blake3Rng::from_seed(&seed);
    sk.try_sign_with_rng(&mut rng, msg, &[]).ok().map(|s| s.to_vec())
}

// ─── Vérification ─────────────────────────────────────────────────────────────

/// Vérification ML-DSA-65 (FIPS 204). Renvoie `false` sur toute donnée malformée.
///
/// `pub(crate)` : c'est l'**unique** vérificateur ML-DSA du projet, réutilisé tel
/// quel par le gadget de finalité (GADGET-2, votes post-quantiques purs d'ADR-005)
/// — aucune duplication de la primitive de vérification (une seule source de vérité).
pub(crate) fn verify_ml_dsa(pk_hex: &str, msg: &[u8], sig_bytes: &[u8]) -> bool {
    (|| -> Option<bool> {
        let pk_arr: [u8; ml_dsa_65::PK_LEN] = hex::decode(pk_hex).ok()?.try_into().ok()?;
        let sig_arr: [u8; ml_dsa_65::SIG_LEN] = sig_bytes.try_into().ok()?;
        let pk = ml_dsa_65::PublicKey::try_from_bytes(pk_arr).ok()?;
        Some(pk.verify(msg, &sig_arr, &[]))
    })()
    .unwrap_or(false)
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_pq_derivation() {
        // Même graine ⇒ même clé publique ML-DSA (dérivation déterministe :
        // c'est ce qui permet de ne rien persister de plus que la graine).
        let seed = [7u8; 32];
        let a = derive_ml_dsa(&seed).expect("keygen a");
        let b = derive_ml_dsa(&seed).expect("keygen b");
        assert_eq!(a.1, b.1, "la dérivation ML-DSA doit être déterministe");
    }

    #[test]
    fn distinct_seeds_derive_distinct_keys() {
        let a = derive_ml_dsa(&[1u8; 32]).expect("keygen a");
        let b = derive_ml_dsa(&[2u8; 32]).expect("keygen b");
        assert_ne!(a.1, b.1, "deux graines distinctes ⇒ deux identités PQ distinctes");
    }

    #[test]
    fn ml_dsa_sign_verify_roundtrip() {
        let (sk, pk_hex) = derive_ml_dsa(&[42u8; 32]).expect("keygen");
        let msg = b"QUANTA ML-DSA-65 vector";
        let sig = ml_dsa_sign_deterministic(&sk, msg).expect("sign");

        assert_eq!(sig.len(), ml_dsa_65::SIG_LEN);
        assert!(verify_ml_dsa(&pk_hex, msg, &sig), "la signature doit vérifier");
    }

    #[test]
    fn tampered_message_fails_verification() {
        let (sk, pk_hex) = derive_ml_dsa(&[42u8; 32]).expect("keygen");
        let sig = ml_dsa_sign_deterministic(&sk, b"original").expect("sign");
        assert!(
            !verify_ml_dsa(&pk_hex, b"tampered", &sig),
            "un message altéré doit échouer"
        );
    }

    #[test]
    fn tampered_signature_fails_verification() {
        let (sk, pk_hex) = derive_ml_dsa(&[42u8; 32]).expect("keygen");
        let mut sig = ml_dsa_sign_deterministic(&sk, b"msg").expect("sign");
        sig[0] ^= 0xFF;
        assert!(
            !verify_ml_dsa(&pk_hex, b"msg", &sig),
            "une signature corrompue doit échouer"
        );
    }

    #[test]
    fn foreign_key_cannot_verify() {
        let (sk, _) = derive_ml_dsa(&[1u8; 32]).expect("keygen a");
        let (_, other_pk) = derive_ml_dsa(&[2u8; 32]).expect("keygen b");
        let sig = ml_dsa_sign_deterministic(&sk, b"msg").expect("sign");
        assert!(
            !verify_ml_dsa(&other_pk, b"msg", &sig),
            "une autre clé publique ne doit jamais vérifier la signature"
        );
    }

    #[test]
    fn malformed_inputs_are_rejected_without_panicking() {
        let (sk, pk_hex) = derive_ml_dsa(&[3u8; 32]).expect("keygen");
        let sig = ml_dsa_sign_deterministic(&sk, b"msg").expect("sign");
        assert!(!verify_ml_dsa("not-hex", b"msg", &sig));
        assert!(!verify_ml_dsa("", b"msg", &sig));
        assert!(!verify_ml_dsa(&pk_hex, b"msg", &[]));
        assert!(!verify_ml_dsa(&pk_hex, b"msg", &sig[..sig.len() - 1]));
    }
}
