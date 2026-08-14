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

use crate::security::CryptoEngine;
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
    /// Wallet propriétaire = **adresse ML-DSA** résolue (hex, 64 chars).
    /// PQ-MIG-3B : `@pseudo` résout vers l'**adresse** (`BLAKE3(ADDR_DOMAIN ‖ clé
    /// ML-DSA)`), exactement l'identité de compte du ledger — payer `@alex`
    /// crédite donc une adresse **dépensable**, et non une clé Ed25519 morte.
    pub owner_pk: String,
    /// Clé publique ML-DSA-65 **révélée** (hex) qui se **lie** à `owner_pk` via
    /// `lie()` (l'adresse n'est qu'un hash : on ne peut pas vérifier une
    /// signature sans la clé). Une clé qui ne hashe pas vers `owner_pk` ⇒ rejet
    /// (même fermeture intrinsèque que CRYPTO-ID-1 côté tx).
    #[serde(default)]
    pub owner_key: String,
    /// Date de revendication (epoch secs). Sert au départage déterministe.
    pub claimed_at: u64,
    /// Signature **ML-DSA-65** hex du propriétaire sur [`signable_bytes`].
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
    /// R2 (CLAIM-WINDOW-1) — `claimed_at` hors bornes : antérieur au protocole,
    /// ou dans le futur au-delà de la dérive tolérée.
    InvalidClaimTime,
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

/// Bytes canoniques signés (tout sauf `signature`). PQ-MIG-3B : la clé révélée
/// `owner_key` est **couverte** par la signature — un tiers ne peut pas
/// substituer une autre clé après coup.
///
/// **MOY-3 + H-05 (AUDIT-2026-08-13) — encodage injectif et chaîne nommée.**
/// C'était `format!("QUSER|{{}}|{{}}|{{}}|{{}}", …)`. Le séparateur `|` n'était
/// interdit dans aucun champ : l'injectivité ne tenait qu'à un **accident de
/// format** (le pseudo est validé `[a-z0-9_]`, les clés sont hexadécimales), pas
/// à une règle. C'est exactement le motif de CRIT-1 sur les transactions, à un
/// domaine près — et le champ le plus proche de l'utilisateur.
///
/// Deux changements, un seul encodage : le format canonique du projet
/// (séparateur de domaine + champs **préfixés en longueur**, cf.
/// `p2p::ledger::tx_signing_preimage`), et l'ancrage sur [`crate::CHAIN_ID`] pour
/// qu'une revendication signée sur un réseau ne vaille rien sur un autre. Le
/// `claimed_at` entre en octets fixes, pas en décimal.
pub fn signable_bytes(rec: &UsernameRecord) -> Vec<u8> {
    let mut b = Vec::with_capacity(USERNAME_SIGN_DOMAIN.len() + 256);
    b.extend_from_slice(USERNAME_SIGN_DOMAIN);
    push_field(&mut b, crate::CHAIN_ID.as_bytes());
    push_field(&mut b, rec.username.as_bytes());
    push_field(&mut b, rec.owner_pk.as_bytes());
    push_field(&mut b, rec.owner_key.as_bytes());
    b.extend_from_slice(&rec.claimed_at.to_le_bytes());
    b
}

/// Séparateur de domaine de la préimage de revendication de pseudo. Distinct de
/// ceux des transactions, des en-têtes de bloc et des votes de finalité : une
/// signature valide sur l'un ne peut pas être rejouée sur un autre, **par
/// construction** et non parce que les formats diffèrent par chance (MOY-3).
const USERNAME_SIGN_DOMAIN: &[u8] = b"QUANTA/username-claim/v1\x00";

/// Champ préfixé par sa longueur (u32 little-endian) — même primitive que
/// `p2p::ledger::push_field`. C'est elle qui rend l'encodage **injectif** : aucun
/// contenu de champ ne peut simuler une frontière de champ.
fn push_field(buf: &mut Vec<u8>, field: &[u8]) {
    buf.extend_from_slice(&(field.len() as u32).to_le_bytes());
    buf.extend_from_slice(field);
}

/// PQ-MIG-3B — autorité de revendication **post-quantique** :
///   1. la clé révélée `owner_key` doit **se lier** à l'adresse `owner_pk`
///      (`lie()` : `owner_pk == BLAKE3(ADDR_DOMAIN ‖ owner_key)`) — sinon
///      n'importe qui pourrait signer une revendication pour l'adresse d'autrui ;
///   2. la signature ML-DSA-65 doit être valide pour cette clé sur les bytes
///      canoniques. Toute entrée malformée ⇒ erreur opaque (jamais de panique).
fn verify_sig(rec: &UsernameRecord) -> Result<(), UsernameError> {
    if !CryptoEngine::address_hex_binds_key_hex(&rec.owner_pk, &rec.owner_key) {
        return Err(UsernameError::InvalidOwner);
    }
    let sig = hex::decode(&rec.signature).map_err(|_| UsernameError::InternalEncoding)?;
    if CryptoEngine::verify_pq(&rec.owner_key, &signable_bytes(rec), &sig) {
        Ok(())
    } else {
        Err(UsernameError::InvalidSignature)
    }
}

/// Helper de signature (tests/util — la prod signe via `CryptoEngine`).
/// PQ-MIG-3B : renseigne `owner_pk` (adresse), `owner_key` (clé ML-DSA révélée)
/// puis signe en ML-DSA **déterministe** (reproductible). `engine` doit porter
/// une identité primaire ML-DSA.
#[cfg(test)]
pub fn sign_record(engine: &CryptoEngine, rec: &mut UsernameRecord) {
    rec.owner_pk = engine.pq_address_hex().expect("ml-dsa address");
    rec.owner_key = engine.pq_identity_hex().expect("ml-dsa primary");
    let sig = engine
        .sign_pq_det(&signable_bytes(rec))
        .expect("ml-dsa sign");
    rec.signature = hex::encode(sig);
}

/// **R2 (AUDIT-2026-08-13) — CLAIM-WINDOW-1 : les bornes qui rendent l'ancienneté
/// non falsifiable.**
///
/// La règle « le plus ancien gagne » s'appliquait à un `claimed_at: u64`
/// **choisi par le revendiquant** et couvert par sa propre signature :
/// `claimed_at = 0` battait tout le monde. Pour le prix d'une clé ML-DSA
/// (165 µs) et d'un message, n'importe qui prenait n'importe quel `@pseudo` —
/// et les paiements adressés à `@alex` partaient chez le voleur.
///
/// On ne peut pas *prouver* qu'on a revendiqué tôt sans horloge commune ; on
/// peut en revanche refuser les deux abus qui rendaient le vol gratuit :
///
/// 1. une date **antérieure au protocole lui-même** ou **dans le futur** ;
/// 2. une contestation **tardive**. Une revendication qu'on observe depuis plus
///    de [`CLAIM_CONTEST_WINDOW_SECS`] est **définitive** chez nous : plus aucun
///    `claimed_at` antidaté ne peut la déloger.
///
/// La convergence déterministe reste entière pendant la fenêtre — deux
/// revendications honnêtes concurrentes s'y départagent comme avant, dans
/// n'importe quel ordre d'arrivée. Elle cède au-delà, délibérément : entre
/// « deux nœuds peuvent diverger sur un pseudo après une partition de 24 h » et
/// « n'importe qui vole n'importe quel pseudo pour 165 µs », le choix est fait.
///
/// **Ce n'est pas la fin de l'histoire.** L'ancrage correct est une revendication
/// portée par la chaîne — l'ordre des blocs est la seule horloge non falsifiable
/// du système. C'est un changement de protocole, pas un correctif ; il est noté
/// et non fait ici.
///
/// Plancher absolu des revendications : `GENESIS_TIMESTAMP` (2026-07-18T00:00:00Z)
/// en secondes epoch. Rien ne peut prétendre être antérieur au protocole.
pub const CLAIM_EPOCH_FLOOR: u64 = 1_784_332_800;

/// Dérive d'horloge tolérée vers le futur (mêmes ordres de grandeur que la
/// fenêtre de fraîcheur des enveloppes gossip).
pub const CLAIM_MAX_FUTURE_SKEW_SECS: u64 = 300;

/// Durée pendant laquelle un pseudo reste contestable après notre **première
/// observation** de son détenteur. Au-delà, il est gelé.
pub const CLAIM_CONTEST_WINDOW_SECS: u64 = 86_400;

/// Vrai si `claimed_at` est dans les bornes admissibles à l'instant `now`
/// (secondes epoch). Purement local : `now` est injecté, le module reste sans IO.
fn claim_time_in_bounds(claimed_at: u64, now: u64) -> bool {
    claimed_at >= CLAIM_EPOCH_FLOOR && claimed_at <= now.saturating_add(CLAIM_MAX_FUTURE_SKEW_SECS)
}

/// Vrai si le challenger l'emporte sur le détenteur en place.
/// Ordre total déterministe : `claimed_at` croissant, puis `owner_pk` croissant.
/// (Indépendant de l'ordre d'arrivée → convergence garantie.)
///
/// R2 : ce départage n'est plus consulté qu'à l'intérieur de la fenêtre de
/// contestation — voir [`UsernameRegistry::apply_at`].
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
    /// R2 (CLAIM-WINDOW-1) — `username` → seconde epoch de notre **première
    /// observation** du détenteur courant. Observation LOCALE : elle n'est ni
    /// signée, ni gossipée, donc pas falsifiable par un pair. Un instantané
    /// antérieur au correctif n'en a pas : ses pseudos sont alors traités comme
    /// **définitifs**, le défaut sûr.
    #[serde(default)]
    pub first_seen: HashMap<String, u64>,
}

/// Registre répliqué `pseudo ↔ clé`.
#[derive(Debug, Clone, Default)]
pub struct UsernameRegistry {
    /// `username` → record (source de vérité).
    by_name: HashMap<String, UsernameRecord>,
    /// `owner_pk` → pseudo principal (dérivé, pour l'affichage en O(1)).
    by_pk: HashMap<String, String>,
    /// R2 — `username` → première observation locale du détenteur courant.
    first_seen: HashMap<String, u64>,
}

impl UsernameRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> UsernameRegistrySnapshot {
        UsernameRegistrySnapshot {
            records: self.by_name.values().cloned().collect(),
            first_seen: self.first_seen.clone(),
        }
    }

    pub fn restore(snap: UsernameRegistrySnapshot) -> Self {
        let mut s = Self::default();
        for r in snap.records {
            s.by_name.insert(r.username.clone(), r);
        }
        // R2 : un pseudo restauré sans date de première observation est réputé
        // définitif (`first_seen` absent ⇒ fenêtre fermée). C'est le défaut sûr :
        // une base d'avant le correctif ne rouvre pas la porte au vol.
        s.first_seen = snap.first_seen;
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

    /// Applique une revendication (locale ou reçue par gossip), horloge murale.
    /// Frontière production ; le cœur injecté est [`Self::apply_at`].
    pub fn apply(&mut self, rec: UsernameRecord) -> Result<ApplyOutcome, UsernameError> {
        let now = chrono::Utc::now().timestamp().max(0) as u64;
        self.apply_at(rec, now)
    }

    /// Applique une revendication à l'instant **injecté** `now` (secondes epoch).
    ///
    /// Idempotent et commutatif *dans la fenêtre de contestation* : l'état final
    /// n'y dépend pas de l'ordre d'application, grâce au départage déterministe
    /// de [`challenger_wins`].
    ///
    /// **R2 (CLAIM-WINDOW-1)** ajoute deux refus, et eux seuls :
    /// - `claimed_at` hors bornes ([`claim_time_in_bounds`]) — un `claimed_at = 0`
    ///   ne franchit plus la porte ;
    /// - contestation d'un pseudo observé depuis plus de
    ///   [`CLAIM_CONTEST_WINDOW_SECS`] — l'ancienneté antidatée ne déloge plus un
    ///   détenteur établi.
    pub fn apply_at(
        &mut self,
        rec: UsernameRecord,
        now: u64,
    ) -> Result<ApplyOutcome, UsernameError> {
        validate_username(&rec.username)?;
        // `owner_pk` is the 32-byte ML-DSA **address** (64 hex) — same shape check.
        if !valid_pk_hex(&rec.owner_pk) {
            return Err(UsernameError::InvalidOwner);
        }
        // R2 : la borne de date AVANT la signature — c'est le champ qui décide de
        // l'arbitrage, et le vérifier tôt évite de payer un verify ML-DSA pour une
        // revendication structurellement irrecevable.
        if !claim_time_in_bounds(rec.claimed_at, now) {
            return Err(UsernameError::InvalidClaimTime);
        }
        verify_sig(&rec)?;

        let outcome = match self.by_name.get(&rec.username) {
            None => {
                self.first_seen.insert(rec.username.clone(), now);
                self.by_name.insert(rec.username.clone(), rec);
                ApplyOutcome::Inserted
            }
            Some(existing) if existing == &rec => ApplyOutcome::AlreadyPresent,
            Some(existing) => {
                // R2 : la fenêtre de contestation. `first_seen` absent (instantané
                // d'avant le correctif) ⇒ pseudo réputé définitif.
                let contestable = self
                    .first_seen
                    .get(&rec.username)
                    .is_some_and(|seen| now <= seen.saturating_add(CLAIM_CONTEST_WINDOW_SECS));
                if contestable && challenger_wins(&rec, existing) {
                    // La fenêtre court depuis la PREMIÈRE observation du pseudo,
                    // pas de chaque changement de détenteur : sinon une chaîne de
                    // contestations la rouvrirait indéfiniment.
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

    /// A deterministic ML-DSA identity from a 1-byte seed (PQ-MIG-3B: the owner
    /// identity is post-quantum, not Ed25519). Same seed ⇒ same key ⇒ same
    /// address ⇒ reproducible records.
    fn engine(seed: u8) -> CryptoEngine {
        let mut c = CryptoEngine::new();
        c.import_pq_identity(&[seed; 32]).expect("ml-dsa primary");
        c
    }

    /// The ML-DSA address (the `owner_pk` value) of a seeded identity.
    fn addr(seed: u8) -> String {
        engine(seed).pq_address_hex().expect("ml-dsa address")
    }

    /// R2 (CLAIM-WINDOW-1) — les revendications sont désormais bornées dans le
    /// temps, donc les dates de test sont des secondes epoch réelles. `T0` est le
    /// plancher du protocole ; les anciens `claimed_at` symboliques (100, 200)
    /// deviennent `T0 + 100`, `T0 + 200` : les tests d'arbitrage gardent
    /// exactement leur sens, à une translation près.
    const T0: u64 = CLAIM_EPOCH_FLOOR;
    /// L'instant injecté par défaut dans les tests : juste après les
    /// revendications, largement dans la fenêtre de contestation.
    const NOW: u64 = T0 + 1_000;

    fn record(seed: u8, username: &str, claimed_at: u64) -> UsernameRecord {
        let mut rec = UsernameRecord {
            username: username.to_string(),
            owner_pk: String::new(),
            owner_key: String::new(),
            claimed_at: T0 + claimed_at,
            signature: String::new(),
        };
        sign_record(&engine(seed), &mut rec); // fills owner_pk + owner_key + signature
        rec
    }

    /// **MOY-3 + H-05 (AUDIT-2026-08-13) — la préimage de revendication était
    /// injective par accident, pas par règle, et ne nommait pas sa chaîne.**
    ///
    /// C'était `format!("QUSER|{}|{}|{}|{}", …)`, et `|` n'était interdit dans
    /// aucun champ : l'injectivité ne tenait qu'à la validation du pseudo et au
    /// format hexadécimal des clés. Même motif que CRIT-1 sur les transactions.
    #[test]
    fn moy3_the_claim_preimage_is_injective_and_names_the_chain() {
        let mk = |name: &str, pk: &str, key: &str, at: u64| UsernameRecord {
            username: name.into(),
            owner_pk: pk.into(),
            owner_key: key.into(),
            claimed_at: at,
            signature: String::new(),
        };
        // Deux enregistrements dont la concaténation naïve coïnciderait si les
        // champs n'étaient pas préfixés en longueur.
        let a = mk("ab", "cd", "ef", 1);
        let b = mk("a", "bcd", "ef", 1);
        assert_ne!(
            signable_bytes(&a),
            signable_bytes(&b),
            "un déplacement de frontière de champ doit changer la préimage"
        );
        // Le domaine et la chaîne sont dans les octets signés.
        let bytes = signable_bytes(&a);
        assert!(
            bytes.starts_with(b"QUANTA/username-claim/v1"),
            "la préimage porte son séparateur de domaine"
        );
        let hay = String::from_utf8_lossy(&bytes);
        assert!(
            hay.contains(crate::CHAIN_ID),
            "la préimage nomme la chaîne — sinon une revendication vaut sur tous les réseaux (H-05)"
        );
        // Le numérique entre en octets fixes : deux instants différents diffèrent.
        assert_ne!(signable_bytes(&a), signable_bytes(&mk("ab", "cd", "ef", 2)));
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
        // owner_pk is the 32-byte ML-DSA address; owner_key binds to it; the
        // ML-DSA signature verifies over the canonical bytes.
        assert_eq!(rec.owner_pk.len(), 64, "owner_pk is a 32-byte address");
        assert!(CryptoEngine::address_hex_binds_key_hex(&rec.owner_pk, &rec.owner_key));
        assert!(verify_sig(&rec).is_ok());
    }

    #[test]
    fn claim_resolve_and_reverse() {
        let mut reg = UsernameRegistry::new();
        let rec = record(1, "alex", 100);
        let pk = rec.owner_pk.clone();
        assert_eq!(reg.apply_at(rec, NOW).unwrap(), ApplyOutcome::Inserted);

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
        reg.apply_at(record(1, "alex", 100), NOW).unwrap();
        assert!(!reg.is_available("alex")); // pris
        assert!(reg.is_available("bob"));
    }

    #[test]
    fn duplicate_same_record_is_idempotent() {
        let mut reg = UsernameRegistry::new();
        let rec = record(1, "alex", 100);
        assert_eq!(reg.apply_at(rec.clone(), NOW).unwrap(), ApplyOutcome::Inserted);
        assert_eq!(reg.apply_at(rec, NOW).unwrap(), ApplyOutcome::AlreadyPresent);
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
        assert_eq!(r1.apply_at(late.clone(), NOW).unwrap(), ApplyOutcome::Inserted);
        assert_eq!(r1.apply_at(early.clone(), NOW).unwrap(), ApplyOutcome::Replaced);
        assert_eq!(r1.resolve("alex").as_deref(), Some(early_pk.as_str()));

        // Ordre 2 : early puis late → late doit être ignorée.
        let mut r2 = UsernameRegistry::new();
        assert_eq!(r2.apply_at(early, NOW).unwrap(), ApplyOutcome::Inserted);
        assert_eq!(r2.apply_at(late, NOW).unwrap(), ApplyOutcome::Kept);
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
        r1.apply_at(a.clone(), NOW).unwrap();
        r1.apply_at(b.clone(), NOW).unwrap();

        let mut r2 = UsernameRegistry::new();
        r2.apply_at(b, NOW).unwrap();
        r2.apply_at(a, NOW).unwrap();

        assert_eq!(r1.resolve("alex"), r2.resolve("alex"));
        assert_eq!(r1.resolve("alex").as_deref(), Some(winner_pk.as_str()));
    }

    #[test]
    fn rejects_forged_signature() {
        let mut reg = UsernameRegistry::new();
        let mut rec = record(1, "alex", 100);
        // Falsifie le pseudo après signature → signature invalide.
        rec.username = "mallory".to_string();
        assert_eq!(reg.apply_at(rec, NOW).unwrap_err(), UsernameError::InvalidSignature);
    }

    #[test]
    fn rejects_unbound_key_closes_pseudo_hijack() {
        // PQ-MIG-3B teeth (CRYPTO-ID-1 analog for @pseudo): claim the VICTIM's
        // address but reveal + sign with the ATTACKER's own ML-DSA key. The
        // revealed key does not hash to the victim's address (`lie` false) ⇒
        // rejected. Nobody can bind @pseudo → victim-address with their own key.
        let victim_addr = addr(2);
        let attacker = engine(3);
        let mut rec = UsernameRecord {
            username: "alex".into(),
            owner_pk: victim_addr, // the victim's address…
            owner_key: attacker.pq_identity_hex().unwrap(), // …but the attacker's key
            claimed_at: T0 + 100,
            signature: String::new(),
        };
        // A perfectly valid ML-DSA signature for owner_key — still rejected,
        // because owner_key does not bind to owner_pk.
        rec.signature = hex::encode(attacker.sign_pq_det(&signable_bytes(&rec)).unwrap());
        let mut reg = UsernameRegistry::new();
        assert_eq!(reg.apply_at(rec, NOW).unwrap_err(), UsernameError::InvalidOwner);
    }

    #[test]
    fn rejects_owner_mismatch() {
        // owner_pk + owner_key are a consistent (bound) pair for identity A, but
        // the signature was produced by a DIFFERENT key ⇒ ML-DSA verify fails.
        let a = engine(1);
        let b = engine(2);
        let mut rec = UsernameRecord {
            username: "alex".into(),
            owner_pk: a.pq_address_hex().unwrap(),
            owner_key: a.pq_identity_hex().unwrap(),
            claimed_at: T0 + 100,
            signature: String::new(),
        };
        // Signed by B over A's canonical bytes → invalid for owner_key (A).
        rec.signature = hex::encode(b.sign_pq_det(&signable_bytes(&rec)).unwrap());
        let mut reg = UsernameRegistry::new();
        assert_eq!(reg.apply_at(rec, NOW).unwrap_err(), UsernameError::InvalidSignature);
    }

    #[test]
    fn rejects_invalid_owner_hex() {
        let mut rec = record(1, "alex", 100);
        rec.owner_pk = "xyz".into();
        let mut reg = UsernameRegistry::new();
        assert_eq!(reg.apply_at(rec, NOW).unwrap_err(), UsernameError::InvalidOwner);
    }

    #[test]
    fn snapshot_restore_roundtrip() {
        let mut reg = UsernameRegistry::new();
        reg.apply_at(record(1, "alex", 100), NOW).unwrap();
        reg.apply_at(record(2, "bob", 110), NOW).unwrap();
        let pk_alex = reg.resolve("alex").unwrap();

        let snap = reg.snapshot();
        let restored = UsernameRegistry::restore(snap);
        assert_eq!(restored.count(), 2);
        assert_eq!(restored.resolve("alex").unwrap(), pk_alex);
        assert_eq!(restored.username_of(&pk_alex).as_deref(), Some("alex"));
    }

    #[test]
    fn connection_code_deterministic_and_distinct() {
        // PQ-MIG-3B: the connection code is now derived from the ML-DSA address
        // (the resolved `owner_pk`), so the "verify @pseudo + code" flow matches.
        let pk1 = addr(1);
        let pk2 = addr(2);
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
        reg.apply_at(record(1, "zoe", 200), NOW).unwrap();
        reg.apply_at(record(1, "alex", 100), NOW).unwrap();
        let pk = reg.resolve("alex").unwrap();
        assert_eq!(reg.username_of(&pk).as_deref(), Some("alex"));
    }

    // ── R2 (AUDIT-2026-08-13) — CLAIM-WINDOW-1 : le vol de @pseudo ────────────

    /// **R2 — `claimed_at = 0` ne franchit plus la porte.** C'était l'attaque :
    /// une date antérieure à tout le monde, choisie librement par le voleur et
    /// couverte par sa propre signature, gagnait « le plus ancien gagne ».
    #[test]
    fn r2_a_claim_predating_the_protocol_is_refused() {
        let mut reg = UsernameRegistry::new();
        // Le détenteur légitime, dans les règles.
        assert_eq!(reg.apply_at(record(1, "alex", 100), NOW).unwrap(), ApplyOutcome::Inserted);
        let victim = addr(1);

        // Le voleur : même pseudo, `claimed_at = 0`.
        let mut thief = UsernameRecord {
            username: "alex".into(),
            owner_pk: String::new(),
            owner_key: String::new(),
            claimed_at: 0,
            signature: String::new(),
        };
        sign_record(&engine(9), &mut thief);
        assert_eq!(
            reg.apply_at(thief, NOW).unwrap_err(),
            UsernameError::InvalidClaimTime,
            "R2 : une revendication antérieure au protocole est irrecevable"
        );
        assert_eq!(reg.resolve("alex").as_deref(), Some(victim.as_str()), "le pseudo n'a pas bougé");
    }

    /// **R2 — une date dans le futur est refusée aussi.** Sans borne haute, un
    /// attaquant pourrait « réserver » l'avenir pour perdre les départages qu'il
    /// veut perdre, ou saturer le registre de revendications inarbitables.
    #[test]
    fn r2_a_claim_in_the_future_is_refused() {
        let mut reg = UsernameRegistry::new();
        let far = NOW + CLAIM_MAX_FUTURE_SKEW_SECS + 1;
        let mut rec = UsernameRecord {
            username: "alex".into(),
            owner_pk: String::new(),
            owner_key: String::new(),
            claimed_at: far,
            signature: String::new(),
        };
        sign_record(&engine(1), &mut rec);
        assert_eq!(reg.apply_at(rec, NOW).unwrap_err(), UsernameError::InvalidClaimTime);
        // La dérive tolérée, elle, passe.
        let mut ok = UsernameRecord {
            username: "alex".into(),
            owner_pk: String::new(),
            owner_key: String::new(),
            claimed_at: NOW + CLAIM_MAX_FUTURE_SKEW_SECS,
            signature: String::new(),
        };
        sign_record(&engine(1), &mut ok);
        assert_eq!(reg.apply_at(ok, NOW).unwrap(), ApplyOutcome::Inserted);
    }

    /// **R2 — passé la fenêtre de contestation, un pseudo établi est gelé.**
    /// Même avec une date recevable mais plus ancienne, un challenger tardif ne
    /// déloge plus le détenteur : c'est la moitié du correctif que la seule borne
    /// de date ne donne pas (le voleur peut toujours viser le plancher).
    #[test]
    fn r2_an_established_name_cannot_be_taken_after_the_window() {
        let mut reg = UsernameRegistry::new();
        // Le détenteur revendique tard (T0+500) et nous l'observons à NOW.
        assert_eq!(reg.apply_at(record(1, "alex", 500), NOW).unwrap(), ApplyOutcome::Inserted);
        let victim = addr(1);

        // Un challenger avec une date PLUS ANCIENNE mais recevable, arrivé après
        // la fenêtre : refusé.
        let late = NOW + CLAIM_CONTEST_WINDOW_SECS + 1;
        assert_eq!(
            reg.apply_at(record(9, "alex", 1), late).unwrap(),
            ApplyOutcome::Kept,
            "R2 : hors fenêtre, le détenteur établi garde son pseudo"
        );
        assert_eq!(reg.resolve("alex").as_deref(), Some(victim.as_str()));
    }

    /// **R2 — la convergence honnête est préservée DANS la fenêtre.** Deux
    /// revendications concurrentes se départagent toujours de façon déterministe
    /// et indépendante de l'ordre d'arrivée : le correctif ne casse pas le
    /// mécanisme, il en borne l'exploitation.
    #[test]
    fn r2_concurrent_honest_claims_still_converge_inside_the_window() {
        let early = record(1, "alex", 100);
        let late = record(2, "alex", 200);
        let winner = early.owner_pk.clone();
        let inside = NOW + CLAIM_CONTEST_WINDOW_SECS - 1;

        let mut r1 = UsernameRegistry::new();
        r1.apply_at(late.clone(), NOW).unwrap();
        assert_eq!(r1.apply_at(early.clone(), inside).unwrap(), ApplyOutcome::Replaced);

        let mut r2 = UsernameRegistry::new();
        r2.apply_at(early, NOW).unwrap();
        assert_eq!(r2.apply_at(late, inside).unwrap(), ApplyOutcome::Kept);

        assert_eq!(r1.resolve("alex"), r2.resolve("alex"), "les deux nœuds convergent");
        assert_eq!(r1.resolve("alex").as_deref(), Some(winner.as_str()));
    }

    /// **R2 — un instantané d'avant le correctif ne rouvre pas la porte.** Sans
    /// date de première observation, le pseudo restauré est réputé définitif.
    #[test]
    fn r2_a_pre_fix_snapshot_restores_as_final() {
        let mut reg = UsernameRegistry::new();
        reg.apply_at(record(1, "alex", 500), NOW).unwrap();
        let victim = addr(1);

        // On simule l'ancienne forme : records présents, `first_seen` absent.
        let mut snap = reg.snapshot();
        snap.first_seen.clear();
        let mut restored = UsernameRegistry::restore(snap);

        assert_eq!(
            restored.apply_at(record(9, "alex", 1), NOW).unwrap(),
            ApplyOutcome::Kept,
            "R2 : sans première observation connue, le pseudo est gelé (défaut sûr)"
        );
        assert_eq!(restored.resolve("alex").as_deref(), Some(victim.as_str()));
    }
}
