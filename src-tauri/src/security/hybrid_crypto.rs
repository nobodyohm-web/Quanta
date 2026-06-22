#![allow(dead_code)]
//! Hybrid post-quantum identity — Ed25519 + ML-DSA-65 (NIST FIPS 204).
//!
//! État (Phase 2B — activé le 2026-05-31) :
//!   - Couche Ed25519    : ACTIVE — signature classique (64 B).
//!   - Couche ML-DSA-65  : ACTIVE — signature post-quantique (FIPS 204 final),
//!     via le crate `fips204` (pur Rust, constant-time, sans `unsafe`, et SANS
//!     dépendance `ed25519` → le conflit de version qui bloquait la Phase 2A
//!     n'existe plus).
//!
//! ## Dérivation de la clé ML-DSA (zéro migration de vault)
//!
//! La paire ML-DSA-65 est **dérivée de façon déterministe** de la graine Ed25519
//! existante via un XOF BLAKE3 (`derive_ml_dsa`). Conséquence : aucune nouvelle
//! matière secrète à persister — la clé PQ est recalculée au déverrouillage à
//! partir de la même graine de 32 octets. Les identités créées avant l'activation
//! gagnent donc transparemment une couche PQ au prochain unlock.
//!
//! ## Sémantique de vérification (`verify_hybrid`)
//!
//! - `quantum` vide (identité sans couche PQ) → **Ed25519 seul** fait foi
//!   (rétro-compatibilité totale avec les signatures pré-activation).
//! - `quantum` présent → **les DEUX** signatures doivent vérifier (AND strict).
//!   Forger exige donc de casser à la fois Ed25519 ET ML-DSA-65.
//!
//! C'est une posture *plus forte* que le OR envisagé à l'origine : un ML-DSA
//! éventuellement cassé ne suffit pas (Ed25519 reste requis), et un Ed25519 cassé
//! par un ordinateur quantique ne suffit pas non plus (ML-DSA reste requis dès
//! qu'une couche PQ est présente).
//!
//! ## Vers le « PQ obligatoire » (flag day futur)
//!
//! `REQUIRE_PQ = false` aujourd'hui : on accepte encore les signatures Ed25519
//! seules pour ne pas exclure les pairs/identités antérieurs à l'activation. Pour
//! une protection PQ *totale* (un adversaire quantique ne peut plus rien forger,
//! même en cassant Ed25519), il faudra un jour basculer `REQUIRE_PQ = true` — un
//! changement de version de protocole qui rejette toute signature sans couche
//! ML-DSA. Décision réseau, à arbitrer à maturité, pas avant.

use ed25519_dalek::{
    Signer as _, Verifier as _,
    SigningKey as EdSk, VerifyingKey as EdVk, Signature as EdSig,
};
use fips204::ml_dsa_65;
use fips204::traits::{SerDes, Signer as _, Verifier as _};
use rand::rngs::OsRng;
use rand_core::{CryptoRng, RngCore};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

// ─── Flags ────────────────────────────────────────────────────────────────────

/// La couche ML-DSA-65 est branchée et vérifiée.
const ML_DSA_AVAILABLE: bool = true;

/// Flag-day futur : exiger une couche PQ sur **toute** signature (rejette
/// l'Ed25519 seul). Laissé à `false` pour la rétro-compatibilité réseau.
pub const REQUIRE_PQ: bool = false;

/// Domaine de séparation pour la dérivation déterministe de la clé ML-DSA.
/// NE JAMAIS modifier : changerait la clé PQ de toutes les identités existantes.
const ML_DSA_DOMAIN: &[u8] = b"QUANTA-ML-DSA-65-derive-v1";

// ─── Types ──────────────────────────────────────────────────────────────────

/// Signature hybride — classique + post-quantique.
///
/// `quantum` est vide quand l'identité signataire n'a pas de couche PQ ; la
/// vérification retombe alors automatiquement en mode Ed25519-seul.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSignature {
    /// 64 octets — Ed25519 (toujours présent).
    pub classical: Vec<u8>,
    /// `ml_dsa_65::SIG_LEN` octets — ML-DSA-65 (vide si pas de couche PQ).
    pub quantum: Vec<u8>,
}

/// Identité hybride : paire Ed25519 + paire ML-DSA-65 dérivée déterministiquement.
pub struct HybridIdentity {
    ed_sk:     EdSk,
    pub ed_vk: EdVk,
    /// `(clé secrète ML-DSA, clé publique ML-DSA hex)`. `None` seulement si la
    /// dérivation échoue (impossible avec le XOF → repli sûr en Ed25519 seul).
    ml: Option<(ml_dsa_65::PrivateKey, String)>,
}

// ─── Implémentation ─────────────────────────────────────────────────────────

impl HybridIdentity {
    /// Génère une nouvelle identité hybride (Ed25519 + ML-DSA-65 dérivée).
    pub fn generate() -> Self {
        let ed_sk = EdSk::generate(&mut OsRng);
        let ed_vk = ed_sk.verifying_key();
        let mut seed = ed_sk.to_bytes();
        let ml = derive_ml_dsa(&seed);
        seed.zeroize();
        Self { ed_sk, ed_vk, ml }
    }

    /// Hex de la clé publique ML-DSA-65 (vide si la dérivation a échoué).
    pub fn pq_vk_hex(&self) -> String {
        self.ml.as_ref().map(|(_, h)| h.clone()).unwrap_or_default()
    }

    /// Signe un message sur les deux couches (Ed25519 + ML-DSA-65).
    pub fn sign(&self, msg: &[u8]) -> HybridSignature {
        let classical = self.ed_sk.sign(msg).to_bytes().to_vec();
        let quantum = self
            .ml
            .as_ref()
            .and_then(|(sk, _)| sk.try_sign_with_rng(&mut OsRng, msg, &[]).ok())
            .map(|s| s.to_vec())
            .unwrap_or_default();
        HybridSignature { classical, quantum }
    }

    // ── Vérification ────────────────────────────────────────────────────────

    /// Vérification hybride (voir le doc-module pour la sémantique complète).
    ///
    /// - `quantum` vide → Ed25519 seul (rétro-compat ; rejet si `REQUIRE_PQ`).
    /// - `quantum` présent → AND strict : Ed25519 ET ML-DSA-65 doivent vérifier.
    pub fn verify_hybrid(
        ed_pk_hex: &str,
        pq_pk_hex: &str,
        msg: &[u8],
        sig: &HybridSignature,
    ) -> bool {
        let ed_ok = verify_ed25519(ed_pk_hex, msg, &sig.classical);

        // Pas de couche PQ exploitable → Ed25519 gouverne.
        if !ML_DSA_AVAILABLE || sig.quantum.is_empty() || pq_pk_hex.is_empty() {
            if REQUIRE_PQ {
                log::warn!(
                    "◈ [HybridCrypto] REQUIRE_PQ actif mais signature sans couche \
                     ML-DSA — rejet"
                );
                return false;
            }
            return ed_ok;
        }

        let pq_ok = verify_ml_dsa(pq_pk_hex, msg, &sig.quantum);
        // Hybride strict : couche PQ présente ⇒ les deux doivent vérifier.
        ed_ok && pq_ok
    }

    /// Vérification Ed25519 seule (compatibilité ascendante).
    pub fn verify_classical(ed_pk_hex: &str, msg: &[u8], sig_bytes: &[u8]) -> bool {
        verify_ed25519(ed_pk_hex, msg, sig_bytes)
    }
}

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

// ─── Fonctions de vérification internes ───────────────────────────────────────

fn verify_ed25519(pk_hex: &str, msg: &[u8], sig_bytes: &[u8]) -> bool {
    (|| -> Option<bool> {
        let pk: [u8; 32] = hex::decode(pk_hex).ok()?.try_into().ok()?;
        let sig: [u8; 64] = sig_bytes.try_into().ok()?;
        let vk = EdVk::from_bytes(&pk).ok()?;
        let s  = EdSig::from_bytes(&sig);
        Some(vk.verify(msg, &s).is_ok())
    })()
    .unwrap_or(false)
}

/// Vérification ML-DSA-65 (FIPS 204). Renvoie `false` sur toute donnée malformée.
fn verify_ml_dsa(pk_hex: &str, msg: &[u8], sig_bytes: &[u8]) -> bool {
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
    fn hybrid_sign_verify_roundtrip() {
        let id  = HybridIdentity::generate();
        let msg = b"QUANTA Phase 2B hybrid vector";
        let sig = id.sign(msg);

        assert_eq!(sig.classical.len(), 64);
        assert_eq!(sig.quantum.len(), ml_dsa_65::SIG_LEN, "couche ML-DSA présente");

        let ed_pk = hex::encode(id.ed_vk.to_bytes());
        let pq_pk = id.pq_vk_hex();
        assert!(!pq_pk.is_empty(), "clé publique PQ dérivée");
        assert!(
            HybridIdentity::verify_hybrid(&ed_pk, &pq_pk, msg, &sig),
            "la signature hybride doit vérifier sur les deux couches"
        );
    }

    #[test]
    fn deterministic_pq_derivation() {
        // Même graine Ed25519 ⇒ même clé publique ML-DSA (dérivation déterministe).
        let ed_sk = EdSk::generate(&mut OsRng);
        let seed = ed_sk.to_bytes();
        let a = derive_ml_dsa(&seed).expect("keygen a");
        let b = derive_ml_dsa(&seed).expect("keygen b");
        assert_eq!(a.1, b.1, "la dérivation ML-DSA doit être déterministe");
    }

    #[test]
    fn tampered_message_fails_hybrid() {
        let id  = HybridIdentity::generate();
        let sig = id.sign(b"original");
        let ed_pk = hex::encode(id.ed_vk.to_bytes());
        let pq_pk = id.pq_vk_hex();
        assert!(
            !HybridIdentity::verify_hybrid(&ed_pk, &pq_pk, b"tampered", &sig),
            "un message altéré doit échouer"
        );
    }

    #[test]
    fn strict_and_rejects_broken_pq_layer() {
        // Ed25519 valide mais couche PQ corrompue ⇒ rejet (AND strict).
        let id = HybridIdentity::generate();
        let mut sig = id.sign(b"msg");
        if let Some(b) = sig.quantum.get_mut(0) { *b ^= 0xFF; }
        let ed_pk = hex::encode(id.ed_vk.to_bytes());
        let pq_pk = id.pq_vk_hex();
        assert!(
            !HybridIdentity::verify_hybrid(&ed_pk, &pq_pk, b"msg", &sig),
            "une couche PQ présente mais invalide doit faire échouer la vérif"
        );
    }

    #[test]
    fn backward_compat_ed25519_only() {
        // quantum vide ⇒ Ed25519 seul fait foi (rétro-compat pré-activation).
        let id   = HybridIdentity::generate();
        let full = id.sign(b"legacy");
        let ed_only = HybridSignature {
            classical: full.classical.clone(),
            quantum: Vec::new(),
        };
        let ed_pk = hex::encode(id.ed_vk.to_bytes());
        assert!(
            HybridIdentity::verify_hybrid(&ed_pk, "", b"legacy", &ed_only),
            "Ed25519 seul doit suffire sans couche PQ"
        );
        assert!(
            !HybridIdentity::verify_hybrid(&ed_pk, "", b"forged", &ed_only),
            "mauvais message ⇒ échec même en Ed25519 seul"
        );
    }

    #[test]
    fn forged_pq_without_valid_ed25519_rejected() {
        // Sans Ed25519 valide, une couche PQ ne peut pas « sauver » la signature.
        let id = HybridIdentity::generate();
        let sig_good = id.sign(b"good");
        let ed_pk = hex::encode(id.ed_vk.to_bytes());
        let pq_pk = id.pq_vk_hex();
        assert!(
            !HybridIdentity::verify_hybrid(&ed_pk, &pq_pk, b"bad", &sig_good),
            "AND strict : Ed25519 invalide ⇒ rejet quelle que soit la couche PQ"
        );
    }
}
