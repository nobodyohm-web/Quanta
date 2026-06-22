#![allow(dead_code)] // câblage gossip/commandes à l'itération suivante
//! Quanta — Registre P2P de **pseudos uniques** (`@handle`).
//!
//! Un pseudo est l'**adresse de wallet lisible** : on envoie des QUANTA à
//! `@alex` au lieu d'une clé publique de 64 caractères. C'est le cœur du
//! « facile d'accès » : se retrouver et se payer par un nom court et unique.
//!
//! Principe **Web3, sans serveur, sans DNS, sans nom de domaine** :
//! - Réserver `alex` produit un [`UsernameRecord`] signé Ed25519 par la clé du
//!   propriétaire, diffusé en gossip.
//! - L'unicité est garantie sans autorité centrale par une **résolution de
//!   conflit déterministe** : entre deux revendications du même pseudo, la plus
//!   ancienne (`claimed_at`) gagne ; à égalité, la clé publique la plus petite.
//!   Tous les nœuds convergent donc vers le même propriétaire, quel que soit
//!   l'ordre d'arrivée des messages.
//! - Lier le pseudo à la clé par signature empêche toute usurpation.
//!
//! Module *pur logique* : aucune lecture du ledger ni de la wall-clock (les
//! timestamps sont passés en argument), ce qui garantit le déterminisme exigé
//! par la convergence P2P. Gratuit : aucun paiement, aucune taxe.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Constantes ──────────────────────────────────────────────────────────────

/// Longueur min/max d'un pseudo (sans le `@`).
pub const USERNAME_MIN_LEN: usize = 3;
pub const USERNAME_MAX_LEN: usize = 20;

// ─── Types ───────────────────────────────────────────────────────────────────

/// Revendication signée d'un pseudo.
///
/// La signature couvre la concaténation canonique de tous les champs sauf
/// elle-même (voir [`signable_bytes`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsernameRecord {
    /// Pseudo canonique, lowercase, sans `@` (ex : `alex`).
    pub username: String,
    /// Wallet propriétaire = adresse résolue (Ed25519 hex, 64 chars).
    pub owner_pk: String,
    /// Date de revendication (epoch secs). Sert au départage déterministe.
    pub claimed_at: u64,
    /// Signature Ed25519 hex (128 chars) du propriétaire.
    pub signature: String,
}

/// Résultat de l'application d'une revendication au registre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyOutcome {
    /// Pseudo libre → inséré.
    Inserted,
    /// Revendication concurrente gagnante → a remplacé l'ancienne.
    Replaced,
    /// Déjà connue à l'identique (idempotent).
    AlreadyPresent,
    /// Le détenteur actuel gagne le conflit → revendication ignorée.
    Kept,
}

/// Erreur publique du module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UsernameError {
    InvalidUsername,
    InvalidOwner,
    InvalidSignature,
    InternalEncoding,
}

// ─── Normalisation & validation ──────────────────────────────────────────────

/// Normalise une saisie utilisateur en pseudo canonique : retire un `@` de
/// tête, trim, lowercase. N'effectue PAS la validation (utiliser
/// [`validate_username`] ensuite).
pub fn normalize_username(input: &str) -> String {
    input
        .trim()
        .trim_start_matches('@')
        .trim()
        .to_lowercase()
}

/// Valide un pseudo canonique.
/// Règles (propre & professionnel) :
/// - longueur ∈ [3, 20]
/// - ASCII `[a-z0-9_]`
/// - commence par une lettre
/// - pas de `_` final, pas de `__` consécutifs
pub fn validate_username(name: &str) -> Result<(), UsernameError> {
    let len = name.len();
    if !(USERNAME_MIN_LEN..=USERNAME_MAX_LEN).contains(&len) {
        return Err(UsernameError::InvalidUsername);
    }
    let bytes = name.as_bytes();
    // Première position : lettre minuscule.
    if !bytes[0].is_ascii_lowercase() {
        return Err(UsernameError::InvalidUsername);
    }
    if name.ends_with('_') || name.contains("__") {
        return Err(UsernameError::InvalidUsername);
    }
    let ok = name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !ok {
        return Err(UsernameError::InvalidUsername);
    }
    Ok(())
}

fn valid_pk_hex(pk: &str) -> bool {
    pk.len() == 64 && hex::decode(pk).map(|b| b.len() == 32).unwrap_or(false)
}

// ─── Signature ───────────────────────────────────────────────────────────────

/// Bytes canoniques signés (tout sauf `signature`).
pub fn signable_bytes(rec: &UsernameRecord) -> Vec<u8> {
    format!("QUSER|{}|{}|{}", rec.username, rec.owner_pk, rec.claimed_at).into_bytes()
}

fn verify_sig(pk_hex: &str, sig_hex: &str, msg: &[u8]) -> Result<(), UsernameError> {
    let pk_bytes = hex::decode(pk_hex).map_err(|_| UsernameError::InternalEncoding)?;
    let sig_bytes = hex::decode(sig_hex).map_err(|_| UsernameError::InternalEncoding)?;
    let pk_arr: [u8; 32] = pk_bytes
        .try_into()
        .map_err(|_| UsernameError::InvalidSignature)?;
    let sig_arr: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| UsernameError::InvalidSignature)?;
    let vk = VerifyingKey::from_bytes(&pk_arr).map_err(|_| UsernameError::InvalidSignature)?;
    let sig = Signature::from_bytes(&sig_arr);
    vk.verify(msg, &sig)
        .map_err(|_| UsernameError::InvalidSignature)
}

/// Helper de signature (tests/util — la prod signe via `CryptoEngine`).
pub fn sign_record(sk: &SigningKey, rec: &mut UsernameRecord) {
    let sig = sk.sign(&signable_bytes(rec));
    rec.signature = hex::encode(sig.to_bytes());
}

/// Vrai si le challenger l'emporte sur le détenteur en place.
/// Ordre total déterministe : `claimed_at` croissant, puis `owner_pk` croissant.
/// (Indépendant de l'ordre d'arrivée → convergence garantie.)
fn challenger_wins(challenger: &UsernameRecord, incumbent: &UsernameRecord) -> bool {
    (challenger.claimed_at, challenger.owner_pk.as_str())
        < (incumbent.claimed_at, incumbent.owner_pk.as_str())
}

// ─── Code de connexion (« safety number ») ───────────────────────────────────

/// Empreinte courte et lisible de la clé publique. Permet de vérifier
/// hors-bande qu'on relie bien le bon compte : la personne dicte son code, on
/// le compare à celui dérivé de la clé résolue depuis son `@pseudo`. C'est un
/// garde-fou anti-usurpation / anti-faute-de-frappe — la sécurité réelle reste
/// la signature Ed25519. Déterministe : BLAKE3(clé) → 40 bits → 8 symboles
/// base32 Crockford (sans I/L/O/U, moins d'ambiguïté), formaté `ABCD-EFGH`.
pub fn connection_code(owner_pk_hex: &str) -> Option<String> {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let pk = hex::decode(owner_pk_hex).ok()?;
    if pk.len() != 32 {
        return None;
    }
    let digest = blake3::hash(&pk);
    let bytes = &digest.as_bytes()[..5]; // 40 bits
    let mut acc: u64 = 0;
    for &b in bytes {
        acc = (acc << 8) | b as u64;
    }
    let mut chars = [0u8; 8];
    for (i, slot) in chars.iter_mut().enumerate() {
        let shift = 5 * (7 - i);
        *slot = ALPHABET[((acc >> shift) & 0x1f) as usize];
    }
    let s = std::str::from_utf8(&chars).ok()?;
    Some(format!("{}-{}", &s[..4], &s[4..]))
}

/// Normalise un code saisi (retire tirets/espaces, uppercase) pour comparaison.
pub fn normalize_code(input: &str) -> String {
    input
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_uppercase()
}

// ─── Registre ────────────────────────────────────────────────────────────────

/// Snapshot sérialisable (persistance SQLite, comme les autres stores).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsernameRegistrySnapshot {
    pub records: Vec<UsernameRecord>,
}

/// Registre répliqué `pseudo ↔ clé`.
#[derive(Debug, Clone, Default)]
pub struct UsernameRegistry {
    /// `username` → record (source de vérité).
    by_name: HashMap<String, UsernameRecord>,
    /// `owner_pk` → pseudo principal (dérivé, pour l'affichage en O(1)).
    by_pk: HashMap<String, String>,
}

impl UsernameRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> UsernameRegistrySnapshot {
        UsernameRegistrySnapshot {
            records: self.by_name.values().cloned().collect(),
        }
    }

    pub fn restore(snap: UsernameRegistrySnapshot) -> Self {
        let mut s = Self::default();
        for r in snap.records {
            s.by_name.insert(r.username.clone(), r);
        }
        s.rebuild_by_pk();
        s
    }

    pub fn count(&self) -> usize {
        self.by_name.len()
    }

    /// Reconstruit l'index inverse `pk → pseudo principal`. Le pseudo principal
    /// d'une clé qui en détient plusieurs est le plus ancien (`claimed_at`),
    /// départagé par ordre alphabétique — déterministe.
    fn rebuild_by_pk(&mut self) {
        self.by_pk.clear();
        for rec in self.by_name.values() {
            match self.by_pk.get(&rec.owner_pk) {
                Some(current) => {
                    let cur = &self.by_name[current];
                    if (rec.claimed_at, rec.username.as_str())
                        < (cur.claimed_at, cur.username.as_str())
                    {
                        self.by_pk.insert(rec.owner_pk.clone(), rec.username.clone());
                    }
                }
                None => {
                    self.by_pk.insert(rec.owner_pk.clone(), rec.username.clone());
                }
            }
        }
    }

    /// Résolution `@pseudo → clé publique` (l'adresse de wallet).
    /// Accepte une saisie brute (`@Alex`, `alex`, …) : normalise avant lookup.
    pub fn resolve(&self, input: &str) -> Option<String> {
        let name = normalize_username(input);
        self.by_name.get(&name).map(|r| r.owner_pk.clone())
    }

    /// Pseudo principal détenu par une clé (pour afficher `@alex` au lieu de la
    /// clé partout dans l'UI).
    pub fn username_of(&self, owner_pk: &str) -> Option<String> {
        self.by_pk.get(owner_pk).cloned()
    }

    /// Un pseudo est-il disponible (valide ET non pris) ?
    pub fn is_available(&self, input: &str) -> bool {
        let name = normalize_username(input);
        validate_username(&name).is_ok() && !self.by_name.contains_key(&name)
    }

    pub fn get(&self, input: &str) -> Option<&UsernameRecord> {
        self.by_name.get(&normalize_username(input))
    }

    pub fn list(&self) -> impl Iterator<Item = &UsernameRecord> {
        self.by_name.values()
    }

    /// Applique une revendication (locale ou reçue par gossip).
    /// Idempotent et commutatif : l'état final ne dépend pas de l'ordre
    /// d'application grâce au départage déterministe de [`challenger_wins`].
    pub fn apply(&mut self, rec: UsernameRecord) -> Result<ApplyOutcome, UsernameError> {
        validate_username(&rec.username)?;
        if !valid_pk_hex(&rec.owner_pk) {
            return Err(UsernameError::InvalidOwner);
        }
        verify_sig(&rec.owner_pk, &rec.signature, &signable_bytes(&rec))?;

        let outcome = match self.by_name.get(&rec.username) {
            None => {
                self.by_name.insert(rec.username.clone(), rec);
                ApplyOutcome::Inserted
            }
            Some(existing) if existing == &rec => ApplyOutcome::AlreadyPresent,
            Some(existing) => {
                if challenger_wins(&rec, existing) {
                    self.by_name.insert(rec.username.clone(), rec);
                    ApplyOutcome::Replaced
                } else {
                    ApplyOutcome::Kept
                }
            }
        };
        if matches!(outcome, ApplyOutcome::Inserted | ApplyOutcome::Replaced) {
            self.rebuild_by_pk();
        }
        Ok(outcome)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn key(seed: u8) -> (SigningKey, String) {
        let sk = SigningKey::from_bytes(&[seed; 32]);
        let pk = hex::encode(sk.verifying_key().as_bytes());
        (sk, pk)
    }

    fn record(seed: u8, username: &str, claimed_at: u64) -> UsernameRecord {
        let (sk, pk) = key(seed);
        let mut rec = UsernameRecord {
            username: username.to_string(),
            owner_pk: pk,
            claimed_at,
            signature: String::new(),
        };
        sign_record(&sk, &mut rec);
        rec
    }

    #[test]
    fn validate_accepts_clean_names() {
        for n in ["alex", "abc", "a_b", "user_42", "quanta2026", "a".repeat(20).as_str()] {
            assert!(validate_username(n).is_ok(), "should accept {n}");
        }
    }

    #[test]
    fn validate_rejects_bad_names() {
        for n in [
            "ab",                       // trop court
            "a".repeat(21).as_str(),    // trop long
            "Alex",                     // majuscule
            "1alex",                    // commence par chiffre
            "_alex",                    // commence par _
            "alex_",                    // _ final
            "al__ex",                   // __ consécutifs
            "al-ex",                    // tiret interdit
            "al ex",                    // espace
            "al@x",                     // @ interdit
            "élise",                    // non-ASCII
        ] {
            assert!(validate_username(n).is_err(), "should reject {n}");
        }
    }

    #[test]
    fn normalize_strips_at_and_lowercases() {
        assert_eq!(normalize_username("@Alex"), "alex");
        assert_eq!(normalize_username("  @QUANTA  "), "quanta");
        assert_eq!(normalize_username("Bob"), "bob");
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        let rec = record(1, "alex", 100);
        assert!(verify_sig(&rec.owner_pk, &rec.signature, &signable_bytes(&rec)).is_ok());
    }

    #[test]
    fn claim_resolve_and_reverse() {
        let mut reg = UsernameRegistry::new();
        let rec = record(1, "alex", 100);
        let pk = rec.owner_pk.clone();
        assert_eq!(reg.apply(rec).unwrap(), ApplyOutcome::Inserted);

        // résolution @pseudo -> clé (accepte les formes brutes)
        assert_eq!(reg.resolve("alex").as_deref(), Some(pk.as_str()));
        assert_eq!(reg.resolve("@Alex").as_deref(), Some(pk.as_str()));
        // inverse clé -> @pseudo (pour l'affichage)
        assert_eq!(reg.username_of(&pk).as_deref(), Some("alex"));
        assert_eq!(reg.count(), 1);
    }

    #[test]
    fn availability() {
        let mut reg = UsernameRegistry::new();
        assert!(reg.is_available("alex"));
        assert!(!reg.is_available("ab")); // invalide
        reg.apply(record(1, "alex", 100)).unwrap();
        assert!(!reg.is_available("alex")); // pris
        assert!(reg.is_available("bob"));
    }

    #[test]
    fn duplicate_same_record_is_idempotent() {
        let mut reg = UsernameRegistry::new();
        let rec = record(1, "alex", 100);
        assert_eq!(reg.apply(rec.clone()).unwrap(), ApplyOutcome::Inserted);
        assert_eq!(reg.apply(rec).unwrap(), ApplyOutcome::AlreadyPresent);
        assert_eq!(reg.count(), 1);
    }

    #[test]
    fn conflict_earliest_claim_wins_regardless_of_order() {
        // Deux clés différentes revendiquent "alex". claimed_at: A=100, B=200.
        let early = record(1, "alex", 100);
        let late = record(2, "alex", 200);
        let early_pk = early.owner_pk.clone();

        // Ordre 1 : late puis early → early (plus ancien) doit gagner.
        let mut r1 = UsernameRegistry::new();
        assert_eq!(r1.apply(late.clone()).unwrap(), ApplyOutcome::Inserted);
        assert_eq!(r1.apply(early.clone()).unwrap(), ApplyOutcome::Replaced);
        assert_eq!(r1.resolve("alex").as_deref(), Some(early_pk.as_str()));

        // Ordre 2 : early puis late → late doit être ignorée.
        let mut r2 = UsernameRegistry::new();
        assert_eq!(r2.apply(early).unwrap(), ApplyOutcome::Inserted);
        assert_eq!(r2.apply(late).unwrap(), ApplyOutcome::Kept);
        assert_eq!(r2.resolve("alex").as_deref(), Some(early_pk.as_str()));
    }

    #[test]
    fn conflict_tie_breaks_on_lowest_pk_order_independent() {
        // Même claimed_at → départage par owner_pk le plus petit.
        let a = record(1, "alex", 100);
        let b = record(2, "alex", 100);
        let winner = if a.owner_pk < b.owner_pk { &a } else { &b };
        let winner_pk = winner.owner_pk.clone();

        let mut r1 = UsernameRegistry::new();
        r1.apply(a.clone()).unwrap();
        r1.apply(b.clone()).unwrap();

        let mut r2 = UsernameRegistry::new();
        r2.apply(b).unwrap();
        r2.apply(a).unwrap();

        assert_eq!(r1.resolve("alex"), r2.resolve("alex"));
        assert_eq!(r1.resolve("alex").as_deref(), Some(winner_pk.as_str()));
    }

    #[test]
    fn rejects_forged_signature() {
        let mut reg = UsernameRegistry::new();
        let mut rec = record(1, "alex", 100);
        // Falsifie le pseudo après signature → signature invalide.
        rec.username = "mallory".to_string();
        assert_eq!(reg.apply(rec).unwrap_err(), UsernameError::InvalidSignature);
    }

    #[test]
    fn rejects_owner_mismatch() {
        // owner_pk ne correspond pas au signataire.
        let (_sk1, _pk1) = key(1);
        let (sk2, _pk2) = key(2);
        let (_sk3, pk3) = key(3);
        let mut rec = UsernameRecord {
            username: "alex".into(),
            owner_pk: pk3, // clé 3...
            claimed_at: 100,
            signature: String::new(),
        };
        sign_record(&sk2, &mut rec); // ...mais signé par la clé 2
        let mut reg = UsernameRegistry::new();
        assert_eq!(reg.apply(rec).unwrap_err(), UsernameError::InvalidSignature);
    }

    #[test]
    fn rejects_invalid_owner_hex() {
        let mut rec = record(1, "alex", 100);
        rec.owner_pk = "xyz".into();
        let mut reg = UsernameRegistry::new();
        assert_eq!(reg.apply(rec).unwrap_err(), UsernameError::InvalidOwner);
    }

    #[test]
    fn snapshot_restore_roundtrip() {
        let mut reg = UsernameRegistry::new();
        reg.apply(record(1, "alex", 100)).unwrap();
        reg.apply(record(2, "bob", 110)).unwrap();
        let pk_alex = reg.resolve("alex").unwrap();

        let snap = reg.snapshot();
        let restored = UsernameRegistry::restore(snap);
        assert_eq!(restored.count(), 2);
        assert_eq!(restored.resolve("alex").unwrap(), pk_alex);
        assert_eq!(restored.username_of(&pk_alex).as_deref(), Some("alex"));
    }

    #[test]
    fn connection_code_deterministic_and_distinct() {
        let (_s1, pk1) = key(1);
        let (_s2, pk2) = key(2);
        let c1 = connection_code(&pk1).unwrap();
        assert_eq!(c1, connection_code(&pk1).unwrap(), "déterministe");
        assert_eq!(c1.len(), 9); // ABCD-EFGH
        assert_eq!(c1.as_bytes()[4], b'-');
        assert!(c1.chars().all(|c| c == '-' || "0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(c)));
        assert_ne!(c1, connection_code(&pk2).unwrap(), "clés distinctes → codes distincts");
        assert!(connection_code("xyz").is_none()); // hex invalide
        assert_eq!(normalize_code("ab cd-ef gh"), "ABCDEFGH");
    }

    #[test]
    fn primary_username_is_earliest_for_a_key() {
        // Une même clé réserve deux pseudos → le principal (affichage) est le
        // plus ancien.
        let mut reg = UsernameRegistry::new();
        reg.apply(record(1, "zoe", 200)).unwrap();
        reg.apply(record(1, "alex", 100)).unwrap();
        let pk = reg.resolve("alex").unwrap();
        assert_eq!(reg.username_of(&pk).as_deref(), Some("alex"));
    }
}
