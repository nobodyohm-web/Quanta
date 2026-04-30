#![allow(dead_code)]
//! Hybrid post-quantum identity — infrastructure prête pour Ed25519 + ML-DSA-65 (FIPS 204)
//!
//! État actuel (Phase 2A) :
//!   - Couche Ed25519 : ACTIVE — signature classique complète
//!   - Couche ML-DSA-65 : INFRASTRUCTURE — interface définie, activation dès ml-dsa ≥ 0.2.0 stable
//!
//! La dépendance `ml-dsa = "0.1.0-rc.9"` cause un conflit de versions avec
//! `ed25519-dalek v2.2` (ed25519 v2.x vs v3.0-rc). À réactiver quand ml-dsa sera stable.
//!
//! Règle de vérification hybride (NIST + IETF) :
//!   Valide si AU MOINS UNE signature est correcte — protège si un des deux systèmes est cassé.

use ed25519_dalek::{
    Signer as _, Verifier as _,
    SigningKey as EdSk, VerifyingKey as EdVk, Signature as EdSig,
};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

// ─── Types ──────────────────────────────────────────────────────────────────

/// Signature hybride — classique + post-quantique.
///
/// Quand ML-DSA n'est pas activé, `quantum` est vide (Vec::new()).
/// La vérification hybride tombe en mode Ed25519-seul automatiquement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSignature {
    /// 64 octets — Ed25519 (classique, toujours présent)
    pub classical: Vec<u8>,
    /// 3293 octets — ML-DSA-65 (post-quantique, vide jusqu'à ml-dsa ≥ 0.2.0)
    pub quantum: Vec<u8>,
}

/// Identité hybride : paire de clés Ed25519 + (future) ML-DSA-65.
pub struct HybridIdentity {
    ed_sk:     EdSk,
    pub ed_vk: EdVk,
}

// ─── Implémentation ─────────────────────────────────────────────────────────

impl HybridIdentity {
    /// Génère une nouvelle identité hybride.
    pub fn generate() -> Self {
        let ed_sk = EdSk::generate(&mut OsRng);
        let ed_vk = ed_sk.verifying_key();
        Self { ed_sk, ed_vk }
    }

    /// Hex de la clé publique PQ (vide jusqu'à activation de ML-DSA).
    pub fn pq_vk_hex(&self) -> String {
        String::new() // sera rempli quand ml-dsa ≥ 0.2.0 sera stable
    }

    /// Signe un message.
    /// Couche classique : Ed25519 actif.
    /// Couche PQ        : vide (infrastructure prête).
    pub fn sign(&self, msg: &[u8]) -> HybridSignature {
        let classical = self.ed_sk.sign(msg).to_bytes().to_vec();
        HybridSignature { classical, quantum: Vec::new() }
    }

    // ── Vérification ────────────────────────────────────────────────────────

    /// Vérification hybride (OR) :
    /// - Si `quantum` non vide ET `pq_pk_hex` non vide → ML-DSA vérifié aussi
    /// - Sinon : Ed25519 seul (mode actuel)
    pub fn verify_hybrid(
        ed_pk_hex: &str,
        pq_pk_hex: &str,
        msg: &[u8],
        sig: &HybridSignature,
    ) -> bool {
        let ed_ok = verify_ed25519(ed_pk_hex, msg, &sig.classical);

        // Couche PQ — activée dès que quantum et pq_pk_hex sont non vides
        if !sig.quantum.is_empty() && !pq_pk_hex.is_empty() {
            let pq_ok = verify_ml_dsa_stub(pq_pk_hex, msg, &sig.quantum);
            return ed_ok || pq_ok;
        }

        ed_ok
    }

    /// Vérification Ed25519 seule (compatibilité ascendante).
    pub fn verify_classical(ed_pk_hex: &str, msg: &[u8], sig_bytes: &[u8]) -> bool {
        verify_ed25519(ed_pk_hex, msg, sig_bytes)
    }
}

// ─── Fonctions internes ──────────────────────────────────────────────────────

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

/// Stub ML-DSA — retourne toujours false jusqu'à l'activation de la dépendance.
/// Remplacer par l'implémentation réelle quand ml-dsa ≥ 0.2.0 est stable :
///
/// ```ignore
/// use ml_dsa::{MlDsa65, VerifyingKey, Signature};
/// use ml_dsa::signature::Verifier as _;
/// let vk = VerifyingKey::<MlDsa65>::try_from(pk_bytes).ok()?;
/// let s  = Signature::<MlDsa65>::try_from(sig_bytes).ok()?;
/// vk.verify(msg, &s).is_ok()
/// ```
fn verify_ml_dsa_stub(_pk_hex: &str, _msg: &[u8], _sig_bytes: &[u8]) -> bool {
    false
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classical_sign_verify_roundtrip() {
        let id  = HybridIdentity::generate();
        let msg = b"SOVA Phase 2A test vector";
        let sig = id.sign(msg);

        assert_eq!(sig.classical.len(), 64);
        assert!(sig.quantum.is_empty(), "PQ vide avant activation ml-dsa");

        let ed_pk = hex::encode(id.ed_vk.to_bytes());
        assert!(
            HybridIdentity::verify_hybrid(&ed_pk, "", msg, &sig),
            "Vérification Ed25519 doit réussir"
        );
    }

    #[test]
    fn tampered_message_fails() {
        let id  = HybridIdentity::generate();
        let sig = id.sign(b"original");
        let ed_pk = hex::encode(id.ed_vk.to_bytes());
        assert!(
            !HybridIdentity::verify_hybrid(&ed_pk, "", b"tampered", &sig),
            "Message altéré doit échouer"
        );
    }
}
