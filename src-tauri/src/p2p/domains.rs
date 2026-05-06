#![allow(dead_code)] // V3 module — helpers exposés pour tests externes & V3.3
//! Torus V3 — Registre P2P de noms de domaine `*.torus`.
//!
//! Modèle économique : **Harberger Tax**.
//! Le propriétaire déclare une valeur (`value_micro_qta`). Il paie un loyer
//! mensuel proportionnel (1% / mois). N'importe qui peut racheter le domaine
//! à la valeur déclarée. Force la fixation honnête → anti-squatting.
//!
//! Les enregistrements sont signés Ed25519 et propagés via gossip.
//! La résolution `name -> target_pk` est locale (HashMap répliqué).
//!
//! Ce module est *pur logique* : il ne lit ni le ledger ni la wall-clock
//! (tous les timestamps sont passés en argument). Cela garantit la
//! déterminisme requis pour le consensus CRDT.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Constantes (mirrorent CLAUDE.md) ────────────────────────────────────────

/// Frais de claim initial (1 QUANTA).
pub const INITIAL_CLAIM_MICRO_QTA: u64 = 1_000_000;

/// Loyer Harberger : 1% par mois sur `value_micro_qta` (en basis points).
pub const HARBERGER_RATE_BPS: u64 = 100;

/// Période de grâce après défaut de paiement (30 jours).
pub const GRACE_PERIOD_SECS: u64 = 30 * 86_400;

/// Durée d'un cycle de loyer (1 mois ≈ 30 jours).
pub const RENT_CYCLE_SECS: u64 = 30 * 86_400;

/// Longueur min/max du label (`alex` dans `alex.torus`).
pub const LABEL_MIN_LEN: usize = 2;
pub const LABEL_MAX_LEN: usize = 40;

/// Suffixe TLD imposé.
pub const TLD: &str = ".torus";

// ─── Types ──────────────────────────────────────────────────────────────────

/// Enregistrement d'un domaine.
///
/// La signature couvre la concaténation canonique de tous les champs
/// (sauf elle-même). Voir [`signable_bytes`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainRecord {
    /// Nom complet, lowercase, ex: `alex.torus`.
    pub name: String,
    /// Wallet propriétaire (Ed25519 hex 64).
    pub owner_pk: String,
    /// Wallet vers lequel le domaine pointe (peut être différent du proprio).
    pub target_pk: String,
    /// Valeur déclarée Harberger en µQTA.
    pub value_micro_qta: u64,
    /// Timestamp du dernier paiement de loyer (epoch secs).
    pub last_paid_ts: u64,
    /// Timestamp de la dernière mise à jour (epoch secs).
    pub updated_at: u64,
    /// Version monotone — toute mise à jour incrémente.
    pub version: u64,
    /// Signature Ed25519 hex 128 du propriétaire actuel.
    pub signature: String,
}

/// Un sous-domaine délégué : `shop.alex.torus` → `target_pk`.
/// Doit être signé par le propriétaire du parent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubdomainGrant {
    /// Sous-domaine complet (ex: `shop.alex.torus`).
    pub name: String,
    /// Domaine parent (ex: `alex.torus`).
    pub parent: String,
    /// Wallet vers lequel pointe le sous-domaine.
    pub target_pk: String,
    pub created_at: u64,
    pub version: u64,
    /// Signature Ed25519 du propriétaire du parent.
    pub signature: String,
}

/// État d'un domaine vu par le moteur de loyer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RentState {
    /// Loyer à jour.
    Current,
    /// Loyer dû mais on est dans la période de grâce.
    Grace { due_micro_qta: u64, expires_ts: u64 },
    /// Période de grâce dépassée → le nom peut être réclamé par n'importe qui.
    Expired { due_micro_qta: u64 },
}

/// Erreur publique du module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DomainError {
    InvalidName,
    AlreadyRegistered,
    NotRegistered,
    NotOwner,
    InvalidSignature,
    StaleVersion,
    InsufficientPayment { needed: u64, given: u64 },
    OverbidValueMismatch { expected: u64, given: u64 },
    BadParent,
    InternalEncoding,
}

// ─── Validation des noms ────────────────────────────────────────────────────

/// Valide un nom complet `label.torus`.
/// Règles : lowercase ASCII, `[a-z0-9-]`, len label ∈ [2, 40], pas de `--` initial.
pub fn validate_name(name: &str) -> Result<(), DomainError> {
    if !name.ends_with(TLD) {
        return Err(DomainError::InvalidName);
    }
    let label = &name[..name.len() - TLD.len()];
    if label.len() < LABEL_MIN_LEN || label.len() > LABEL_MAX_LEN {
        return Err(DomainError::InvalidName);
    }
    if label.starts_with('-') || label.ends_with('-') {
        return Err(DomainError::InvalidName);
    }
    let ok = label
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !ok {
        return Err(DomainError::InvalidName);
    }
    Ok(())
}

/// Valide un nom de sous-domaine `child.parent.torus`.
/// Renvoie `(child_label, parent)`.
pub fn validate_subdomain(name: &str) -> Result<(String, String), DomainError> {
    if !name.ends_with(TLD) {
        return Err(DomainError::InvalidName);
    }
    let parts: Vec<&str> = name.trim_end_matches(TLD).split('.').collect();
    if parts.len() < 2 {
        return Err(DomainError::InvalidName);
    }
    let child = parts[0];
    if child.len() < LABEL_MIN_LEN || child.len() > LABEL_MAX_LEN {
        return Err(DomainError::InvalidName);
    }
    let ok = child
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !ok {
        return Err(DomainError::InvalidName);
    }
    let parent_label = parts[1..].join(".");
    let parent = format!("{parent_label}{TLD}");
    validate_name(&parent)?;
    Ok((child.to_string(), parent))
}

// ─── Signature ──────────────────────────────────────────────────────────────

/// Bytes canoniques signés (tout sauf `signature`).
pub fn signable_bytes(rec: &DomainRecord) -> Vec<u8> {
    format!(
        "{}|{}|{}|{}|{}|{}|{}",
        rec.name,
        rec.owner_pk,
        rec.target_pk,
        rec.value_micro_qta,
        rec.last_paid_ts,
        rec.updated_at,
        rec.version,
    )
    .into_bytes()
}

pub fn signable_bytes_subdomain(g: &SubdomainGrant) -> Vec<u8> {
    format!(
        "SUB|{}|{}|{}|{}|{}",
        g.name, g.parent, g.target_pk, g.created_at, g.version
    )
    .into_bytes()
}

fn verify_sig(pk_hex: &str, sig_hex: &str, msg: &[u8]) -> Result<(), DomainError> {
    let pk_bytes = hex::decode(pk_hex).map_err(|_| DomainError::InternalEncoding)?;
    let sig_bytes = hex::decode(sig_hex).map_err(|_| DomainError::InternalEncoding)?;
    let pk_arr: [u8; 32] = pk_bytes
        .try_into()
        .map_err(|_| DomainError::InvalidSignature)?;
    let sig_arr: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| DomainError::InvalidSignature)?;
    let vk = VerifyingKey::from_bytes(&pk_arr).map_err(|_| DomainError::InvalidSignature)?;
    let sig = Signature::from_bytes(&sig_arr);
    vk.verify(msg, &sig).map_err(|_| DomainError::InvalidSignature)
}

/// Helper de signature (utilitaire — la prod utilise `CryptoEngine`).
pub fn sign_record(sk: &SigningKey, rec: &mut DomainRecord) {
    let sig = sk.sign(&signable_bytes(rec));
    rec.signature = hex::encode(sig.to_bytes());
}

pub fn sign_subdomain(sk: &SigningKey, g: &mut SubdomainGrant) {
    let sig = sk.sign(&signable_bytes_subdomain(g));
    g.signature = hex::encode(sig.to_bytes());
}

// ─── Loyer Harberger ────────────────────────────────────────────────────────

/// Loyer dû entre `last_paid_ts` et `now` à 1% / mois sur `value`.
/// Calcul déterministe en u128 pour éviter overflow.
pub fn rent_due(value_micro_qta: u64, last_paid_ts: u64, now: u64) -> u64 {
    if now <= last_paid_ts {
        return 0;
    }
    let elapsed = now - last_paid_ts;
    // rent = value × bps × elapsed / (10_000 × cycle)
    let num = (value_micro_qta as u128)
        .saturating_mul(HARBERGER_RATE_BPS as u128)
        .saturating_mul(elapsed as u128);
    let den = 10_000u128.saturating_mul(RENT_CYCLE_SECS as u128);
    (num / den.max(1)) as u64
}

pub fn rent_state(rec: &DomainRecord, now: u64) -> RentState {
    let due = rent_due(rec.value_micro_qta, rec.last_paid_ts, now);
    if due == 0 {
        return RentState::Current;
    }
    let grace_end = rec.last_paid_ts + RENT_CYCLE_SECS + GRACE_PERIOD_SECS;
    if now <= grace_end {
        RentState::Grace {
            due_micro_qta: due,
            expires_ts: grace_end,
        }
    } else {
        RentState::Expired { due_micro_qta: due }
    }
}

// ─── Registre ───────────────────────────────────────────────────────────────

/// Snapshot sérialisable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainRegistrySnapshot {
    pub records: Vec<DomainRecord>,
    pub subs: Vec<SubdomainGrant>,
}

#[derive(Debug, Clone, Default)]
pub struct DomainRegistry {
    /// `name` → record (record canonique signé propriétaire).
    records: HashMap<String, DomainRecord>,
    /// `name` → grant (sous-domaines).
    subs: HashMap<String, SubdomainGrant>,
}

impl DomainRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> DomainRegistrySnapshot {
        DomainRegistrySnapshot {
            records: self.records.values().cloned().collect(),
            subs: self.subs.values().cloned().collect(),
        }
    }

    pub fn restore(snap: DomainRegistrySnapshot) -> Self {
        let mut s = Self::default();
        for r in snap.records {
            s.records.insert(r.name.clone(), r);
        }
        for g in snap.subs {
            s.subs.insert(g.name.clone(), g);
        }
        s
    }

    pub fn count(&self) -> usize {
        self.records.len()
    }

    /// Résolution `name → target_pk` (gère sous-domaines).
    /// Renvoie `None` si le domaine est expiré au-delà de la grâce.
    pub fn resolve(&self, name: &str, now: u64) -> Option<String> {
        if let Some(g) = self.subs.get(name) {
            // un sous-domaine n'est valide que si son parent l'est
            if let Some(p) = self.records.get(&g.parent) {
                if matches!(rent_state(p, now), RentState::Expired { .. }) {
                    return None;
                }
                return Some(g.target_pk.clone());
            }
            return None;
        }
        let rec = self.records.get(name)?;
        if matches!(rent_state(rec, now), RentState::Expired { .. }) {
            return None;
        }
        Some(rec.target_pk.clone())
    }

    pub fn get(&self, name: &str) -> Option<&DomainRecord> {
        self.records.get(name)
    }

    pub fn get_subdomain(&self, name: &str) -> Option<&SubdomainGrant> {
        self.subs.get(name)
    }

    pub fn list(&self) -> impl Iterator<Item = &DomainRecord> {
        self.records.values()
    }

    /// Crée et insère un nouveau domaine. `payment` doit ≥ INITIAL_CLAIM.
    /// Le caller (lib.rs) débite le wallet via le ledger avant d'appeler.
    pub fn claim(&mut self, rec: DomainRecord, payment_micro_qta: u64) -> Result<(), DomainError> {
        validate_name(&rec.name)?;
        if payment_micro_qta < INITIAL_CLAIM_MICRO_QTA {
            return Err(DomainError::InsufficientPayment {
                needed: INITIAL_CLAIM_MICRO_QTA,
                given: payment_micro_qta,
            });
        }
        if self.records.contains_key(&rec.name) {
            return Err(DomainError::AlreadyRegistered);
        }
        verify_sig(&rec.owner_pk, &rec.signature, &signable_bytes(&rec))?;
        if rec.owner_pk != rec.target_pk && rec.target_pk.len() != 64 {
            return Err(DomainError::InvalidSignature);
        }
        self.records.insert(rec.name.clone(), rec);
        Ok(())
    }

    /// Met à jour un record existant. Doit être signé par le propriétaire courant
    /// et avoir une `version > existante`.
    pub fn update(&mut self, rec: DomainRecord) -> Result<(), DomainError> {
        validate_name(&rec.name)?;
        let existing = self
            .records
            .get(&rec.name)
            .ok_or(DomainError::NotRegistered)?;
        if rec.owner_pk != existing.owner_pk {
            return Err(DomainError::NotOwner);
        }
        if rec.version <= existing.version {
            return Err(DomainError::StaleVersion);
        }
        verify_sig(&rec.owner_pk, &rec.signature, &signable_bytes(&rec))?;
        self.records.insert(rec.name.clone(), rec);
        Ok(())
    }

    /// Paie le loyer Harberger. Met `last_paid_ts = now`, incrémente `version`.
    /// Renvoie le nouveau record signé à propager.
    pub fn pay_rent(
        &mut self,
        name: &str,
        owner_sk: &SigningKey,
        payment_micro_qta: u64,
        now: u64,
    ) -> Result<DomainRecord, DomainError> {
        let rec = self
            .records
            .get(name)
            .cloned()
            .ok_or(DomainError::NotRegistered)?;
        let owner_pk_hex = hex::encode(owner_sk.verifying_key().as_bytes());
        if rec.owner_pk != owner_pk_hex {
            return Err(DomainError::NotOwner);
        }
        let due = rent_due(rec.value_micro_qta, rec.last_paid_ts, now);
        if payment_micro_qta < due {
            return Err(DomainError::InsufficientPayment {
                needed: due,
                given: payment_micro_qta,
            });
        }
        let mut new_rec = DomainRecord {
            last_paid_ts: now,
            updated_at: now,
            version: rec.version + 1,
            signature: String::new(),
            ..rec
        };
        sign_record(owner_sk, &mut new_rec);
        self.records.insert(name.to_string(), new_rec.clone());
        Ok(new_rec)
    }

    /// Rachat Harberger : un challenger paie `expected = current.value_micro_qta`
    /// au propriétaire actuel et reprend le domaine à sa propre nouvelle valeur.
    pub fn overbid(
        &mut self,
        name: &str,
        challenger_sk: &SigningKey,
        new_target_pk: String,
        new_value_micro_qta: u64,
        payment_to_owner_micro_qta: u64,
        now: u64,
    ) -> Result<DomainRecord, DomainError> {
        let existing = self
            .records
            .get(name)
            .cloned()
            .ok_or(DomainError::NotRegistered)?;
        if payment_to_owner_micro_qta != existing.value_micro_qta {
            return Err(DomainError::OverbidValueMismatch {
                expected: existing.value_micro_qta,
                given: payment_to_owner_micro_qta,
            });
        }
        let challenger_pk = hex::encode(challenger_sk.verifying_key().as_bytes());
        let mut new_rec = DomainRecord {
            name: name.to_string(),
            owner_pk: challenger_pk,
            target_pk: new_target_pk,
            value_micro_qta: new_value_micro_qta,
            last_paid_ts: now,
            updated_at: now,
            version: existing.version + 1,
            signature: String::new(),
        };
        sign_record(challenger_sk, &mut new_rec);
        self.records.insert(name.to_string(), new_rec.clone());
        Ok(new_rec)
    }

    /// Réclame un domaine expiré (loyer + grâce dépassés). `payment` ≥ INITIAL_CLAIM.
    pub fn reclaim_expired(
        &mut self,
        name: &str,
        challenger_sk: &SigningKey,
        new_target_pk: String,
        new_value_micro_qta: u64,
        payment_micro_qta: u64,
        now: u64,
    ) -> Result<DomainRecord, DomainError> {
        let existing = self
            .records
            .get(name)
            .cloned()
            .ok_or(DomainError::NotRegistered)?;
        if !matches!(rent_state(&existing, now), RentState::Expired { .. }) {
            return Err(DomainError::NotOwner); // pas encore expiré
        }
        if payment_micro_qta < INITIAL_CLAIM_MICRO_QTA {
            return Err(DomainError::InsufficientPayment {
                needed: INITIAL_CLAIM_MICRO_QTA,
                given: payment_micro_qta,
            });
        }
        let challenger_pk = hex::encode(challenger_sk.verifying_key().as_bytes());
        let mut new_rec = DomainRecord {
            name: name.to_string(),
            owner_pk: challenger_pk,
            target_pk: new_target_pk,
            value_micro_qta: new_value_micro_qta,
            last_paid_ts: now,
            updated_at: now,
            version: existing.version + 1,
            signature: String::new(),
        };
        sign_record(challenger_sk, &mut new_rec);
        self.records.insert(name.to_string(), new_rec.clone());
        Ok(new_rec)
    }

    /// V3.3 — Applique un overbid déjà signé par le challenger.
    ///
    /// Vérifie : nom valide, domaine existe, version monotone, signature challenger,
    /// `value` strictement non nul. Le caller (commande Tauri) a déjà débité le
    /// paiement vers l'ancien propriétaire dans le ledger AVANT d'appeler.
    ///
    /// Différence avec `update()` : on accepte un nouveau `owner_pk` (≠ existing).
    pub fn apply_overbid_record(&mut self, rec: DomainRecord) -> Result<(), DomainError> {
        validate_name(&rec.name)?;
        let existing = self
            .records
            .get(&rec.name)
            .ok_or(DomainError::NotRegistered)?;
        if rec.version <= existing.version {
            return Err(DomainError::StaleVersion);
        }
        if rec.value_micro_qta == 0 {
            return Err(DomainError::InvalidName);
        }
        verify_sig(&rec.owner_pk, &rec.signature, &signable_bytes(&rec))?;
        self.records.insert(rec.name.clone(), rec);
        Ok(())
    }

    /// Insère un sous-domaine signé par le propriétaire du parent.
    pub fn grant_subdomain(&mut self, g: SubdomainGrant) -> Result<(), DomainError> {
        let (_child, parent) = validate_subdomain(&g.name)?;
        if parent != g.parent {
            return Err(DomainError::BadParent);
        }
        let parent_rec = self.records.get(&g.parent).ok_or(DomainError::BadParent)?;
        verify_sig(&parent_rec.owner_pk, &g.signature, &signable_bytes_subdomain(&g))?;
        if let Some(existing) = self.subs.get(&g.name) {
            if g.version <= existing.version {
                return Err(DomainError::StaleVersion);
            }
        }
        self.subs.insert(g.name.clone(), g);
        Ok(())
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    fn mk_sk(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn fresh_record(sk: &SigningKey, name: &str, value: u64, now: u64) -> DomainRecord {
        let pk = hex::encode(sk.verifying_key().as_bytes());
        let mut r = DomainRecord {
            name: name.into(),
            owner_pk: pk.clone(),
            target_pk: pk,
            value_micro_qta: value,
            last_paid_ts: now,
            updated_at: now,
            version: 1,
            signature: String::new(),
        };
        sign_record(sk, &mut r);
        r
    }

    #[test]
    fn validates_simple_name() {
        assert!(validate_name("alex.torus").is_ok());
        assert!(validate_name("a1.torus").is_ok());
        assert!(validate_name("my-shop.torus").is_ok());
    }

    #[test]
    fn rejects_bad_name() {
        assert!(validate_name("a.torus").is_err()); // trop court
        assert!(validate_name("Alex.torus").is_err()); // majuscule
        assert!(validate_name("alex.com").is_err()); // mauvais TLD
        assert!(validate_name("-bad.torus").is_err());
        assert!(validate_name("bad-.torus").is_err());
        assert!(validate_name("ab cd.torus").is_err());
    }

    #[test]
    fn claim_and_resolve() {
        let sk = mk_sk(1);
        let pk = hex::encode(sk.verifying_key().as_bytes());
        let mut reg = DomainRegistry::new();
        let r = fresh_record(&sk, "alex.torus", 5_000_000, 1_000);
        assert!(reg.claim(r, INITIAL_CLAIM_MICRO_QTA).is_ok());
        assert_eq!(reg.resolve("alex.torus", 1_001), Some(pk));
    }

    #[test]
    fn rejects_double_claim() {
        let sk = mk_sk(1);
        let mut reg = DomainRegistry::new();
        let r1 = fresh_record(&sk, "alex.torus", 1_000_000, 1);
        reg.claim(r1, INITIAL_CLAIM_MICRO_QTA).unwrap();
        let sk2 = mk_sk(2);
        let r2 = fresh_record(&sk2, "alex.torus", 1_000_000, 1);
        assert_eq!(
            reg.claim(r2, INITIAL_CLAIM_MICRO_QTA),
            Err(DomainError::AlreadyRegistered)
        );
    }

    #[test]
    fn rejects_underpaid_claim() {
        let sk = mk_sk(1);
        let mut reg = DomainRegistry::new();
        let r = fresh_record(&sk, "alex.torus", 1_000_000, 1);
        let res = reg.claim(r, 500_000);
        assert!(matches!(res, Err(DomainError::InsufficientPayment { .. })));
    }

    #[test]
    fn rejects_forged_signature() {
        let sk = mk_sk(1);
        let mut r = fresh_record(&sk, "alex.torus", 1_000_000, 1);
        r.target_pk = "0".repeat(64); // tamper after sig
        let mut reg = DomainRegistry::new();
        assert_eq!(
            reg.claim(r, INITIAL_CLAIM_MICRO_QTA),
            Err(DomainError::InvalidSignature)
        );
    }

    #[test]
    fn rent_due_is_proportional() {
        // 1 mois entier sur 1 QUANTA de valeur ⇒ 1% = 10_000 µQTA
        let due = rent_due(1_000_000, 0, RENT_CYCLE_SECS);
        assert_eq!(due, 10_000);
        // demi-mois ⇒ 5000 µQTA
        let due = rent_due(1_000_000, 0, RENT_CYCLE_SECS / 2);
        assert_eq!(due, 5_000);
    }

    #[test]
    fn rent_state_grace_then_expired() {
        let sk = mk_sk(1);
        let r = fresh_record(&sk, "alex.torus", 1_000_000, 0);
        // tout juste à l'échéance d'un cycle : on est dans Grace
        let s = rent_state(&r, RENT_CYCLE_SECS + 1);
        assert!(matches!(s, RentState::Grace { .. }));
        // après grâce
        let s = rent_state(&r, RENT_CYCLE_SECS + GRACE_PERIOD_SECS + 1);
        assert!(matches!(s, RentState::Expired { .. }));
    }

    #[test]
    fn pay_rent_resets_clock() {
        let sk = mk_sk(1);
        let mut reg = DomainRegistry::new();
        let r = fresh_record(&sk, "alex.torus", 1_000_000, 0);
        reg.claim(r, INITIAL_CLAIM_MICRO_QTA).unwrap();
        let now = RENT_CYCLE_SECS + 100;
        let due = rent_due(1_000_000, 0, now);
        let new_rec = reg.pay_rent("alex.torus", &sk, due, now).unwrap();
        assert_eq!(new_rec.last_paid_ts, now);
        assert_eq!(new_rec.version, 2);
    }

    #[test]
    fn pay_rent_underpaid_rejected() {
        let sk = mk_sk(1);
        let mut reg = DomainRegistry::new();
        let r = fresh_record(&sk, "alex.torus", 1_000_000, 0);
        reg.claim(r, INITIAL_CLAIM_MICRO_QTA).unwrap();
        let now = RENT_CYCLE_SECS + 1;
        let res = reg.pay_rent("alex.torus", &sk, 0, now);
        assert!(matches!(res, Err(DomainError::InsufficientPayment { .. })));
    }

    #[test]
    fn overbid_transfers_ownership() {
        let sk1 = mk_sk(1);
        let sk2 = mk_sk(2);
        let pk2 = hex::encode(sk2.verifying_key().as_bytes());
        let mut reg = DomainRegistry::new();
        let r = fresh_record(&sk1, "alex.torus", 5_000_000, 0);
        reg.claim(r, INITIAL_CLAIM_MICRO_QTA).unwrap();
        let new_rec = reg
            .overbid("alex.torus", &sk2, pk2.clone(), 10_000_000, 5_000_000, 100)
            .unwrap();
        assert_eq!(new_rec.owner_pk, pk2);
        assert_eq!(new_rec.value_micro_qta, 10_000_000);
        assert_eq!(reg.resolve("alex.torus", 101), Some(pk2));
    }

    #[test]
    fn overbid_value_mismatch_rejected() {
        let sk1 = mk_sk(1);
        let sk2 = mk_sk(2);
        let pk2 = hex::encode(sk2.verifying_key().as_bytes());
        let mut reg = DomainRegistry::new();
        reg.claim(fresh_record(&sk1, "alex.torus", 5_000_000, 0), INITIAL_CLAIM_MICRO_QTA)
            .unwrap();
        let res = reg.overbid("alex.torus", &sk2, pk2, 10_000_000, 1_000, 100);
        assert!(matches!(res, Err(DomainError::OverbidValueMismatch { .. })));
    }

    #[test]
    fn subdomain_grant_works() {
        let sk = mk_sk(1);
        let pk = hex::encode(sk.verifying_key().as_bytes());
        let mut reg = DomainRegistry::new();
        reg.claim(fresh_record(&sk, "alex.torus", 1_000_000, 0), INITIAL_CLAIM_MICRO_QTA)
            .unwrap();
        let mut g = SubdomainGrant {
            name: "shop.alex.torus".into(),
            parent: "alex.torus".into(),
            target_pk: pk.clone(),
            created_at: 100,
            version: 1,
            signature: String::new(),
        };
        sign_subdomain(&sk, &mut g);
        assert!(reg.grant_subdomain(g).is_ok());
        assert_eq!(reg.resolve("shop.alex.torus", 200), Some(pk));
    }

    #[test]
    fn subdomain_signed_by_wrong_owner_rejected() {
        let sk = mk_sk(1);
        let other = mk_sk(2);
        let mut reg = DomainRegistry::new();
        reg.claim(fresh_record(&sk, "alex.torus", 1_000_000, 0), INITIAL_CLAIM_MICRO_QTA)
            .unwrap();
        let mut g = SubdomainGrant {
            name: "shop.alex.torus".into(),
            parent: "alex.torus".into(),
            target_pk: hex::encode(other.verifying_key().as_bytes()),
            created_at: 100,
            version: 1,
            signature: String::new(),
        };
        sign_subdomain(&other, &mut g); // mauvais signataire
        assert_eq!(reg.grant_subdomain(g), Err(DomainError::InvalidSignature));
    }

    #[test]
    fn snapshot_round_trip() {
        let sk = mk_sk(1);
        let mut reg = DomainRegistry::new();
        reg.claim(fresh_record(&sk, "alex.torus", 1_000_000, 0), INITIAL_CLAIM_MICRO_QTA)
            .unwrap();
        let snap = reg.snapshot();
        let reg2 = DomainRegistry::restore(snap);
        assert_eq!(reg2.count(), 1);
        assert!(reg2.get("alex.torus").is_some());
    }

    #[test]
    fn reclaim_expired_works() {
        let sk1 = mk_sk(1);
        let sk2 = mk_sk(2);
        let pk2 = hex::encode(sk2.verifying_key().as_bytes());
        let mut reg = DomainRegistry::new();
        reg.claim(fresh_record(&sk1, "alex.torus", 1_000_000, 0), INITIAL_CLAIM_MICRO_QTA)
            .unwrap();
        // largement après grâce
        let now = RENT_CYCLE_SECS + GRACE_PERIOD_SECS + 1;
        let new_rec = reg
            .reclaim_expired("alex.torus", &sk2, pk2.clone(), 2_000_000, INITIAL_CLAIM_MICRO_QTA, now)
            .unwrap();
        assert_eq!(new_rec.owner_pk, pk2);
    }

    #[test]
    fn cannot_reclaim_if_not_expired() {
        let sk1 = mk_sk(1);
        let sk2 = mk_sk(2);
        let pk2 = hex::encode(sk2.verifying_key().as_bytes());
        let mut reg = DomainRegistry::new();
        reg.claim(fresh_record(&sk1, "alex.torus", 1_000_000, 0), INITIAL_CLAIM_MICRO_QTA)
            .unwrap();
        let res = reg.reclaim_expired("alex.torus", &sk2, pk2, 1_000_000, INITIAL_CLAIM_MICRO_QTA, 100);
        assert!(res.is_err());
    }
}
