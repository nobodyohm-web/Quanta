#![allow(dead_code)]
//! Hybrid post-quantum identity — infrastructure prête pour Ed25519 + ML-DSA-65 (FIPS 204)
//!
//! État actuel (Phase 2A) :
//!   - Couche Ed25519 : ACTIVE — signature classique complète
//!   - Couche ML-DSA-65 : DÉSACTIVÉE — interface réservée, voir `ML_DSA_AVAILABLE`
//!
//! ## Pourquoi le stub plutôt qu'une vraie impl ?
//!
//! - `ml-dsa = "0.1.0-rc.9"` dépend de `ed25519 v3.0-rc`, alors qu'`ed25519-dalek v2.2`
//!   utilise `ed25519 v2.x`. Conflit de version irréconciliable côté Cargo.
//! - `pqcrypto-dilithium` lierait du C (Dilithium round-3) au lieu du standard final
//!   FIPS 204 ML-DSA-65 — incompatible interop.
//!
//! ## Étapes de réactivation
//!
//! Quand `ml-dsa ≥ 0.2` (FIPS 204 final, ed25519 v2-compatible) sortira :
//! 1. Ajouter `ml-dsa = "0.2"` à `Cargo.toml`.
//! 2. Implémenter `verify_ml_dsa()` (voir le placeholder ci-dessous).
//! 3. Faire générer la paire ML-DSA dans `HybridIdentity::generate()` et remplir
//!    `quantum` dans `sign()`.
//! 4. Basculer `ML_DSA_AVAILABLE` à `true`.
//! 5. **Lire la note "Mode hybride OR vs AND" ci-dessous avant de merger.**
//!
//! ## Mode hybride OR vs AND
//!
//! La règle actuelle est OR : valide si **au moins une** des deux signatures vérifie.
//! Ça protège contre la rupture d'un des deux systèmes mais ouvre une attaque si
//! ML-DSA est jamais cassé. Pour un service plus prudent, basculer en AND
//! (les deux doivent vérifier) ou en mode "PQ-only au-delà de la date X".
//! Décision à arbitrer à l'activation, pas avant.
//!
//! ## Posture de sécurité actuelle
//!
//! Tant que `ML_DSA_AVAILABLE = false` :
//! - `sign()` produit `quantum: Vec::new()`.
//! - Tout `quantum` non-vide reçu d'un peer est ignoré (le stub renvoie `false`).
//! - Donc seule Ed25519 protège. C'est volontaire et explicite.

use ed25519_dalek::{
    Signer as _, Verifier as _,
    SigningKey as EdSk, VerifyingKey as EdVk, Signature as EdSig,
};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

// ─── Activation flag ────────────────────────────────────────────────────────

/// Bascule unique pour activer la couche ML-DSA-65.
/// Tant qu'à `false`, `verify_ml_dsa()` ne fait que renvoyer `false` (échec sûr).
const ML_DSA_AVAILABLE: bool = false;

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
    /// Couche PQ        : vide tant que `ML_DSA_AVAILABLE = false`.
    pub fn sign(&self, msg: &[u8]) -> HybridSignature {
        let classical = self.ed_sk.sign(msg).to_bytes().to_vec();
        let sig = HybridSignature { classical, quantum: Vec::new() };
        // Invariant : un signataire local en mode stub ne doit JAMAIS produire
        // de couche quantum. Cristallise l'état actuel — si quelqu'un branche
        // un vrai signer ML-DSA sans flipper `ML_DSA_AVAILABLE`, on s'en aperçoit.
        #[allow(clippy::assertions_on_constants)]
        {
            debug_assert!(
                ML_DSA_AVAILABLE || sig.quantum.is_empty(),
                "stub mode must not produce ML-DSA signatures"
            );
        }
        sig
    }

    // ── Vérification ────────────────────────────────────────────────────────

    /// Vérification hybride.
    ///
    /// Sémantique actuelle (`ML_DSA_AVAILABLE = false`) :
    /// - La couche ML-DSA n'est jamais considérée valide.
    /// - Toute donnée `quantum` reçue est journalisée (pour visibilité) puis ignorée.
    /// - Le résultat est strictement le verdict Ed25519.
    ///
    /// Sémantique cible (`ML_DSA_AVAILABLE = true`) : règle OR — valide si au
    /// moins une des deux signatures passe (cf. note "Mode hybride OR vs AND").
    pub fn verify_hybrid(
        ed_pk_hex: &str,
        pq_pk_hex: &str,
        msg: &[u8],
        sig: &HybridSignature,
    ) -> bool {
        let ed_ok = verify_ed25519(ed_pk_hex, msg, &sig.classical);

        if sig.quantum.is_empty() || pq_pk_hex.is_empty() {
            return ed_ok;
        }

        if !ML_DSA_AVAILABLE {
            // Visibilité opérationnelle : un peer pousse une signature PQ qu'on
            // ne peut pas vérifier. Ne pas l'accepter, ne pas planter.
            log::warn!(
                "◈ [HybridCrypto] PQ signature présente mais ML-DSA désactivé — \
                 fallback Ed25519 seul ({}B quantum, {}B pq_pk)",
                sig.quantum.len(),
                pq_pk_hex.len() / 2
            );
            return ed_ok;
        }

        let pq_ok = verify_ml_dsa(pq_pk_hex, msg, &sig.quantum);
        ed_ok || pq_ok
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

/// Vérification ML-DSA-65 — désactivée tant que `ML_DSA_AVAILABLE = false`.
///
/// Le contrat est : ne **jamais** renvoyer `true` avant qu'une vraie implémentation
/// soit branchée et auditée. `verify_hybrid` n'appelle même pas cette fonction
/// quand `ML_DSA_AVAILABLE` est faux — la garde ici est une seconde ceinture.
///
/// Réactivation (à faire en un seul commit) :
/// ```ignore
/// use ml_dsa::{MlDsa65, VerifyingKey, Signature};
/// use ml_dsa::signature::Verifier as _;
/// let vk = VerifyingKey::<MlDsa65>::try_from(pk_bytes).ok()?;
/// let s  = Signature::<MlDsa65>::try_from(sig_bytes).ok()?;
/// vk.verify(msg, &s).is_ok()
/// ```
#[allow(clippy::assertions_on_constants)]
fn verify_ml_dsa(_pk_hex: &str, _msg: &[u8], _sig_bytes: &[u8]) -> bool {
    debug_assert!(
        ML_DSA_AVAILABLE,
        "verify_ml_dsa appelé alors que ML_DSA_AVAILABLE=false — \
         indique un bug logique dans verify_hybrid"
    );
    false
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classical_sign_verify_roundtrip() {
        let id  = HybridIdentity::generate();
        let msg = b"QUANTA Phase 2A test vector";
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

    /// En mode stub, une signature PQ fabriquée ne doit jamais "valider" via la
    /// branche quantum : seul Ed25519 fait foi. Si Ed25519 échoue, le verdict
    /// est `false` même si on prétend avoir une signature ML-DSA.
    #[test]
    fn stub_mode_rejects_forged_pq_signature() {
        let id = HybridIdentity::generate();
        let ed_pk = hex::encode(id.ed_vk.to_bytes());

        // Signature Ed25519 valide pour "good", mais on cherche à valider "bad"
        // en y attachant un blob "quantum" arbitraire et un faux pq_pk.
        let real_sig = id.sign(b"good");
        let forged = HybridSignature {
            classical: real_sig.classical.clone(),
            quantum: vec![0xAA; 3293], // taille ML-DSA-65 plausible
        };
        let fake_pq_pk = hex::encode([0xBBu8; 1952]); // taille ML-DSA-65 plausible

        // Ed25519 ne valide pas "bad" → quel que soit le quantum, le verdict est false.
        assert!(
            !HybridIdentity::verify_hybrid(&ed_pk, &fake_pq_pk, b"bad", &forged),
            "stub mode ne doit jamais accepter une signature uniquement via la couche PQ"
        );

        // Ed25519 valide "good" → verdict true même avec quantum bidon (fallback Ed25519).
        assert!(
            HybridIdentity::verify_hybrid(&ed_pk, &fake_pq_pk, b"good", &forged),
            "Ed25519 valide doit suffire même quand quantum est bruité"
        );
    }
}
