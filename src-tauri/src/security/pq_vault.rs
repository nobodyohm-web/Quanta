// security/pq_vault.rs — Post-Quantum Vault (ML-KEM / ML-DSA Ready)
// Defense-grade identity with classical Ed25519 + PQ abstraction layer.
// zeroize all sensitive material on drop.

use super::{CryptoEngine, cipher};
use rand::rngs::OsRng;
use rand_core::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

/// PQ-hardened identity (hybrid: Ed25519 + future ML-DSA-65)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantaIdentity {
    pub public_key_hex: String,
    pub display_name: String,
    pub created_at: String,
    pub is_initialized: bool,
    /// Post-quantum signature algorithm identifier
    pub pq_algorithm: String,
    /// Security level: "classical", "hybrid", "post-quantum"
    pub security_level: String,
    /// ML-DSA-65 public key (hex), dérivée de la graine Ed25519. `None` si la
    /// couche PQ n'a pas pu être dérivée (repli classique).
    #[serde(default)]
    pub pq_public_key_hex: Option<String>,
}

/// S/Kademlia node puzzle result for Sybil resistance
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodePuzzle {
    pub node_id: String,
    pub difficulty: u8,
    pub nonce: u64,
    pub proof_hash: String,
}

pub struct PQVault;

/// Bundle returned when creating a new identity:
/// (public identity, raw public key bytes, encrypted secret key, AES-GCM nonce,
/// **sel Argon2id**). Le sel est le 5ᵉ champ depuis MOY-2 : il est aléatoire, donc
/// il ne se recalcule plus depuis la clé publique — il DOIT être persisté avec le
/// ciphertext, sinon le coffre n'est plus ouvrable.
pub type CreatedIdentity = (QuantaIdentity, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>);

// PQ-MIG-3 §3: the ML-DSA primary vault below is now **wired into production**
// (`lib.rs` `create_identity`/`unlock_identity` persist + restore the encrypted
// root seed), so the PQ-MIG-1 `#[allow(dead_code)]` scaffolding is removed — the
// functions are live, and `cargo clippy -D warnings` proves they are reachable.

/// MOY-2 — coffre Ed25519 re-scellé : `(ciphertext, nonce, sel)`. Les trois
/// champs doivent être écrits **ensemble** : un ciphertext neuf avec l'ancien sel
/// est un coffre définitivement inouvrable.
pub type ResealedVault = (Vec<u8>, Vec<u8>, Vec<u8>);

/// PQ-MIG-1 — bundle returned when creating the **post-quantum primary** identity:
/// (ML-DSA-65 public key hex, encrypted 32-byte root seed, AES-GCM nonce, **sel
/// Argon2id**). Même raison qu'en [`CreatedIdentity`] : depuis MOY-2 le sel est
/// tiré d'`OsRng` et n'est plus reconstructible, il voyage donc avec le blob.
pub type CreatedPqIdentity = (String, Vec<u8>, Vec<u8>, Vec<u8>);

/// Domain tag binding the ML-DSA vault's Argon2id salt to its own key material —
/// separates it from the Ed25519 vault's salt (`blake3(ed_pk)`), so the two
/// at-rest secrets never share a derived key even under the same password.
/// **Conservé en lecture seule** : c'est la formule du sel *hérité* (MOY-2).
const PQ_VAULT_SALT_DOMAIN: &str = "QUANTA-PQ-MIG-1-vault-v1";

/// **MOY-2 (AUDIT-2026-08-13) — SALT-RANDOM-1 : longueur du sel Argon2id.**
///
/// 16 octets, la taille recommandée par la RFC 9106 (§3.1) et déjà la longueur
/// de fait du projet (les deux dérivations héritées tronquaient BLAKE3 à `[..16]`).
pub const VAULT_SALT_LEN: usize = 16;

/// **MOY-2 — le sel n'était pas un sel.**
///
/// Il valait `BLAKE3(clé_publique)[..16]` : une fonction **pure de données
/// publiques**. La clé publique voyage dans chaque transaction et chaque
/// enveloppe gossip, donc l'attaquant connaissait le sel de sa victime **avant**
/// de voler le fichier de coffre — il pouvait lancer son dictionnaire à l'avance,
/// et la table restait valide après un changement de mot de passe puisque le sel
/// ne dépendait pas de l'époque. Le coût unitaire d'Argon2id (88 ms mesurés) ne
/// change pas ; c'est la *fenêtre* d'attaque qui s'ouvrait.
///
/// Un sel tiré d'`OsRng` rend ce précalcul impossible par construction : il
/// n'existe qu'après la création du coffre, et il change à chaque re-chiffrement.
pub fn random_salt() -> [u8; VAULT_SALT_LEN] {
    let mut salt = [0u8; VAULT_SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    salt
}

/// MOY-2 — normalise un sel relu du stockage : une valeur **vide** signifie
/// « coffre écrit avant SALT-RANDOM-1 », donc sel hérité (dérivé de la clé
/// publique). C'est l'unique discriminant de format, et il est en lecture seule :
/// aucun chemin d'écriture ne produit plus de sel vide.
pub fn stored_salt(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.is_empty() { None } else { Some(bytes) }
}

/// **MOY-4 (AUDIT-2026-08-13) — VAULT-BIND-1 : refus d'une identité substituée.**
///
/// Message unique des deux couches. Il ne dit PAS « mauvais mot de passe » : le
/// déchiffrement a réussi, c'est le contenu du coffre qui n'est pas celui de
/// l'identité enregistrée. Continuer à saisir le mot de passe n'y changerait rien
/// et brûlerait le backoff ; la seule sortie est la phrase de 24 mots.
const VAULT_IDENTITY_MISMATCH: &str = "Coffre incohérent : la clé déchiffrée n'est pas celle de \
     l'identité enregistrée. Ne ressaisissez pas votre mot de passe — restaurez le portefeuille \
     depuis votre phrase de 24 mots.";

impl PQVault {
    /// Create a new sovereign identity with Ed25519 + PQ preparation
    ///
    /// MOY-2 : le sel Argon2id est désormais tiré d'`OsRng` et **remonte à
    /// l'appelant** — c'est lui qui doit le persister à côté du ciphertext, sans
    /// quoi le coffre devient inouvrable (le sel ne se recalcule plus).
    pub fn create_identity(
        engine: &mut CryptoEngine,
        display_name: &str,
        password: &str,
    ) -> Result<CreatedIdentity, String> {
        let pk = engine.generate_keypair();
        let mut sk_bytes = engine.get_secret_bytes()?;
        
        // Derive encryption key from password (Argon2id), sel aléatoire (MOY-2).
        let salt = random_salt();
        // ZEROIZE-SWEEP: the Argon2id-derived AES key wipes on drop.
        let enc_key = Self::derive_ed_vault_key(password, &pk.public_key_bytes, Some(&salt))?;

        // Encrypt the secret key (encrypt_and_wipe erases `sk_bytes`)
        let enc = cipher::encrypt_and_wipe(&mut sk_bytes, &enc_key)?;
        
        let security_level = if pk.pq_public_key_hex.is_some() { "hybrid" } else { "classical" };
        let identity = QuantaIdentity {
            public_key_hex: pk.public_key_hex,
            display_name: display_name.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            is_initialized: true,
            pq_algorithm: "Ed25519 + ML-DSA-65 (FIPS 204)".to_string(),
            security_level: security_level.to_string(),
            pq_public_key_hex: pk.pq_public_key_hex,
        };

        Ok((identity, pk.public_key_bytes, enc.ciphertext, enc.nonce, salt.to_vec()))
    }

    /// Argon2id-derived AES key of the **Ed25519** vault blob. Exposed so the
    /// Touch ID quick-unlock can wrap this derived key under a Keychain-gated
    /// KEK — the password itself is never stored.
    ///
    /// MOY-2 — `salt` :
    /// - `Some(sel)` : le sel aléatoire persisté avec le coffre (format courant) ;
    /// - `None` : coffre écrit **avant** SALT-RANDOM-1, dont le sel valait
    ///   `BLAKE3(hex(pk))[..16]`. Ce chemin existe uniquement pour **relire** un
    ///   coffre existant et le re-chiffrer ensuite — jamais pour en écrire un.
    ///
    /// Un sel persisté plus court que [`VAULT_SALT_LEN`] est refusé : la ligne de
    /// coffre est modifiable par tout processus local (A6 la restreint, ne la
    /// signe pas), et un sel tronqué à zéro octet ramènerait la dérivation à une
    /// constante partagée par toutes les installations.
    pub fn derive_ed_vault_key(
        password: &str,
        public_key: &[u8],
        salt: Option<&[u8]>,
    ) -> Result<Zeroizing<[u8; 32]>, String> {
        match salt {
            Some(s) => {
                if s.len() < VAULT_SALT_LEN {
                    return Err("Sel de coffre invalide".to_string());
                }
                Ok(Zeroizing::new(cipher::derive_key(password, s)?))
            }
            None => {
                let legacy = CryptoEngine::blake3_hash(hex::encode(public_key).as_bytes());
                Ok(Zeroizing::new(cipher::derive_key(password, &legacy[..VAULT_SALT_LEN])?))
            }
        }
    }

    /// MOY-2 — re-chiffre le coffre **Ed25519 déjà déverrouillé** du moteur sous un
    /// sel neuf. C'est la seconde moitié de la migration : sans elle, un coffre
    /// hérité resterait éternellement sur son sel prévisible.
    pub fn reencrypt_ed_vault(
        engine: &CryptoEngine,
        password: &str,
    ) -> Result<ResealedVault, String> {
        let identity = engine.get_identity()?;
        let mut sk_bytes = engine.get_secret_bytes()?;
        let salt = random_salt();
        let enc_key = Self::derive_ed_vault_key(password, &identity.public_key_bytes, Some(&salt))?;
        let enc = cipher::encrypt_and_wipe(&mut sk_bytes, &enc_key)?;
        Ok((enc.ciphertext, enc.nonce, salt.to_vec()))
    }

    /// Unlock identity from encrypted storage. `salt` : voir
    /// [`Self::derive_ed_vault_key`] (`None` == coffre hérité, MOY-2).
    ///
    /// Huit paramètres, un de plus que le seuil de clippy : ce sont exactement les
    /// colonnes que la base stocke pour ce coffre. Les regrouper obligerait
    /// `security/` à connaître la forme de `storage/`, c'est-à-dire à inverser la
    /// dépendance pour économiser un argument.
    #[allow(clippy::too_many_arguments)]
    pub fn unlock_identity(
        engine: &mut CryptoEngine,
        public_key: &[u8],
        encrypted_sk: &[u8],
        nonce: &[u8],
        salt: Option<&[u8]>,
        password: &str,
        display_name: &str,
        created_at: &str,
    ) -> Result<QuantaIdentity, String> {
        let enc_key = Self::derive_ed_vault_key(password, public_key, salt)?;
        Self::unlock_identity_with_key(
            engine, public_key, encrypted_sk, nonce, &enc_key, display_name, created_at,
        )
    }

    /// Decryption core of [`Self::unlock_identity`] taking the **derived** AES
    /// key directly (Touch ID path: the key comes unwrapped from the
    /// Keychain-gated KEK instead of Argon2id). Identical hygiene.
    ///
    /// MOY-4 — `expected_public_key` est la clé publique **enregistrée** : la clé
    /// reconstruite doit lui être égale. Sans cette comparaison, un attaquant
    /// ayant l'écriture sur `quanta.db` remplaçait le triplet
    /// `(public_key, ciphertext, nonce)` par le sien et l'application ouvrait
    /// l'identité substituée sans le moindre signal.
    pub fn unlock_identity_with_key(
        engine: &mut CryptoEngine,
        expected_public_key: &[u8],
        encrypted_sk: &[u8],
        nonce: &[u8],
        enc_key: &[u8; 32],
        display_name: &str,
        created_at: &str,
    ) -> Result<QuantaIdentity, String> {
        let mut sk_bytes = cipher::decrypt(encrypted_sk, enc_key, nonce)?;

        // ZEROIZE-SWEEP: build the [u8;32] directly from the slice (no un-wiped
        // `.clone()` Vec), then erase BOTH the decrypted Vec and the stack copy.
        // Previously `sk_bytes.clone().try_into()` left an un-wiped clone and
        // `sk_arr` behind — a brother of the fixed Ed25519 export hole.
        let mut sk_arr: [u8; 32] = sk_bytes
            .as_slice()
            .try_into()
            .map_err(|_| "Bad SK len")?;
        let pk = engine.import_keypair(&sk_arr)?;
        sk_arr.zeroize();
        sk_bytes.zeroize();

        // MOY-4 : le déchiffrement a pu réussir sur un coffre substitué (chiffré
        // sous le même mot de passe par un tiers). On verrouille le moteur avant
        // de rendre l'erreur : une identité qui n'est pas celle enregistrée ne
        // doit pas rester chargée, sinon l'UI l'afficherait comme la nôtre.
        if pk.public_key_bytes != expected_public_key {
            engine.lock();
            return Err(VAULT_IDENTITY_MISMATCH.to_string());
        }

        let security_level = if pk.pq_public_key_hex.is_some() { "hybrid" } else { "classical" };
        Ok(QuantaIdentity {
            public_key_hex: pk.public_key_hex,
            display_name: display_name.to_string(),
            created_at: created_at.to_string(),
            is_initialized: true,
            pq_algorithm: "Ed25519 + ML-DSA-65 (FIPS 204)".to_string(),
            security_level: security_level.to_string(),
            pq_public_key_hex: pk.pq_public_key_hex,
        })
    }

    // ── PQ-MIG-1 : vault de l'identité primaire post-quantique (ML-DSA-65) ─────
    //
    // Additif : `create_identity` / `unlock_identity` (Ed25519) restent intacts
    // pour les consommateurs existants (coexistence). Ces deux fonctions stockent
    // et rechargent la **graine racine** ML-DSA de 32 octets — la clé secrète
    // ML-DSA est intégralement déterminée par cette graine (keygen déterministe),
    // donc 32 octets suffisent et le rechargement redonne la clé publique
    // identique. Même posture zeroize/erreur-opaque que le vault Ed25519.

    /// PQ-MIG-1 — crée l'identité **primaire** post-quantique. Génère une racine
    /// ML-DSA-65 indépendante dans le moteur, puis chiffre sa graine de 32 octets
    /// au repos (Argon2id → AES-256-GCM) en effaçant la graine en clair. Renvoie
    /// la clé publique ML-DSA (hex) + le bundle de graine chiffrée.
    pub fn create_pq_identity(
        engine: &mut CryptoEngine,
        password: &str,
    ) -> Result<CreatedPqIdentity, String> {
        let pq_pk_hex = engine.generate_pq_identity()?;
        Self::seal_pq_seed(engine, &pq_pk_hex, password)
    }

    /// MOY-2 — chiffre la graine ML-DSA **courante** du moteur sous un sel neuf.
    /// Facteur commun de la création, de la restauration et de la migration de
    /// format : un seul endroit produit un blob de coffre PQ, donc un seul endroit
    /// peut se tromper de sel.
    fn seal_pq_seed(
        engine: &CryptoEngine,
        pq_pk_hex: &str,
        password: &str,
    ) -> Result<CreatedPqIdentity, String> {
        // Graine racine auto-effaçante (Zeroizing) extraite du moteur.
        let mut seed = engine.get_pq_seed_bytes()?;
        let salt = random_salt();
        // ZEROIZE : la clé AES dérivée d'Argon2id s'efface au drop.
        let enc_key = Self::derive_pq_vault_key(password, pq_pk_hex, Some(&salt))?;
        // encrypt_and_wipe efface `seed` après chiffrement.
        let enc = cipher::encrypt_and_wipe(&mut seed, &enc_key)?;
        Ok((pq_pk_hex.to_string(), enc.ciphertext, enc.nonce, salt.to_vec()))
    }

    /// MOY-2 — re-chiffre le coffre de fonds **déjà déverrouillé** sous un sel
    /// neuf (migration d'un coffre hérité). Pendant du [`Self::reencrypt_ed_vault`].
    pub fn reencrypt_pq_vault(
        engine: &CryptoEngine,
        password: &str,
    ) -> Result<CreatedPqIdentity, String> {
        let pq_pk_hex = engine
            .pq_identity_hex()
            .ok_or_else(|| "No ML-DSA primary identity".to_string())?;
        Self::seal_pq_seed(engine, &pq_pk_hex, password)
    }

    /// RECOVER-1 — restore the primary PQ identity from a **known 32-byte seed** (the
    /// one a BIP39 recovery phrase carries), then re-encrypt it at rest under the new
    /// password. Mirror of [`Self::create_pq_identity`] but **seeded**: ML-DSA keygen
    /// is deterministic, so this reconstructs the SAME public key / fund address as
    /// the original wallet. The in-clear seed copy is wiped after encryption.
    pub fn restore_pq_identity(
        engine: &mut CryptoEngine,
        seed: &[u8; 32],
        password: &str,
    ) -> Result<CreatedPqIdentity, String> {
        let pq_pk_hex = engine.import_pq_identity(seed)?;
        Self::seal_pq_seed(engine, &pq_pk_hex, password)
    }

    /// Argon2id-derived AES key of the **ML-DSA seed** vault blob. Same rationale
    /// as [`Self::derive_ed_vault_key`].
    ///
    /// MOY-2 — `salt` : voir [`Self::derive_ed_vault_key`]. `None` == blob écrit
    /// avant SALT-RANDOM-1, dont le sel valait
    /// `BLAKE3("QUANTA-PQ-MIG-1-vault-v1:" ‖ pk_ML-DSA)[..16]` — c'est-à-dire une
    /// fonction de la clé publique **que chaque transaction signée révèle**.
    pub fn derive_pq_vault_key(
        password: &str,
        pq_public_key_hex: &str,
        salt: Option<&[u8]>,
    ) -> Result<Zeroizing<[u8; 32]>, String> {
        match salt {
            Some(s) => {
                if s.len() < VAULT_SALT_LEN {
                    return Err("Sel de coffre invalide".to_string());
                }
                Ok(Zeroizing::new(cipher::derive_key(password, s)?))
            }
            None => {
                let legacy = CryptoEngine::blake3_hash(
                    format!("{PQ_VAULT_SALT_DOMAIN}:{pq_public_key_hex}").as_bytes(),
                );
                Ok(Zeroizing::new(cipher::derive_key(password, &legacy[..VAULT_SALT_LEN])?))
            }
        }
    }

    /// PQ-MIG-1 — déverrouille l'identité primaire post-quantique : déchiffre la
    /// graine racine ML-DSA de 32 octets et rétablit la paire ML-DSA-65 dans le
    /// moteur. Round-trip vers la clé publique **identique** (keygen
    /// déterministe). Mauvais mot de passe ⇒ échec opaque. La graine déchiffrée
    /// est effacée après usage. Renvoie la clé publique ML-DSA (hex).
    pub fn unlock_pq_identity(
        engine: &mut CryptoEngine,
        pq_public_key_hex: &str,
        encrypted_seed: &[u8],
        nonce: &[u8],
        salt: Option<&[u8]>,
        password: &str,
    ) -> Result<String, String> {
        let enc_key = Self::derive_pq_vault_key(password, pq_public_key_hex, salt)?;
        Self::unlock_pq_identity_with_key(engine, pq_public_key_hex, encrypted_seed, nonce, &enc_key)
    }

    /// Decryption core of [`Self::unlock_pq_identity`] taking the derived AES
    /// key directly (Touch ID path). Identical zeroize/opaque-error posture.
    ///
    /// MOY-4 — même liaison que côté Ed25519, sur la clé qui contrôle **les
    /// fonds** : la clé publique reconstruite depuis la graine déchiffrée doit
    /// être celle annoncée par le blob. Un blob qui ment sur sa propre clé
    /// publique est un coffre substitué, pas un coffre à ouvrir.
    pub fn unlock_pq_identity_with_key(
        engine: &mut CryptoEngine,
        expected_pq_public_key_hex: &str,
        encrypted_seed: &[u8],
        nonce: &[u8],
        enc_key: &[u8; 32],
    ) -> Result<String, String> {
        let mut seed_bytes = cipher::decrypt(encrypted_seed, enc_key, nonce)?;
        // Construit le [u8;32] depuis la tranche (pas de clone non effacé), puis
        // efface les DEUX copies — même hygiène que le vault Ed25519.
        let mut seed_arr: [u8; 32] = seed_bytes
            .as_slice()
            .try_into()
            .map_err(|_| "Bad seed len")?;
        let pk_hex = engine.import_pq_identity(&seed_arr)?;
        seed_arr.zeroize();
        seed_bytes.zeroize();
        if !pk_hex.eq_ignore_ascii_case(expected_pq_public_key_hex) {
            engine.lock();
            return Err(VAULT_IDENTITY_MISMATCH.to_string());
        }
        Ok(pk_hex)
    }

    /// S/Kademlia crypto puzzle: prevent Sybil attacks by requiring
    /// computational proof tied to node ID (cost: ~500ms CPU)
    #[allow(dead_code)]
    pub fn solve_node_puzzle(node_id: &str, difficulty: u8) -> NodePuzzle {
        let target = difficulty;
        let mut nonce: u64 = 0;
        loop {
            let input = format!("{}:{}", node_id, nonce);
            let hash = hex::encode(CryptoEngine::blake3_hash(input.as_bytes()));
            let leading = hash.chars().take_while(|c| *c == '0').count() as u8;
            if leading >= target {
                return NodePuzzle {
                    node_id: node_id.to_string(),
                    difficulty, nonce, proof_hash: hash,
                };
            }
            nonce += 1;
        }
    }

    /// Verify a node puzzle solution
    #[allow(dead_code)]
    pub fn verify_puzzle(puzzle: &NodePuzzle) -> bool {
        let input = format!("{}:{}", puzzle.node_id, puzzle.nonce);
        let hash = hex::encode(CryptoEngine::blake3_hash(input.as_bytes()));
        let leading = hash.chars().take_while(|c| *c == '0').count() as u8;
        leading >= puzzle.difficulty && hash == puzzle.proof_hash
    }
}

#[cfg(test)]
mod audit_20260813_vault {
    //! **MOY-2 / MOY-4 (AUDIT-2026-08-13)** au niveau du coffre lui-même :
    //! le sel Argon2id est aléatoire et persisté (et l'ancien format se relit
    //! toujours), et un coffre substitué est refusé au lieu d'être ouvert.
    use super::*;

    const PW: &str = "correct horse battery staple";

    /// **MOY-2** — un coffre écrit avant SALT-RANDOM-1 (sel dérivé de la clé
    /// publique) doit continuer à s'ouvrir : c'est la moitié « lecture » de la
    /// migration, et sans elle tous les portefeuilles existants seraient perdus.
    #[test]
    fn moy2_a_legacy_salt_vault_still_opens_and_a_new_one_does_not_use_it() {
        // Coffre hérité : chiffré sous la dérivation publique d'autrefois.
        let mut engine = CryptoEngine::new();
        let pk = engine.generate_keypair();
        let mut sk = engine.get_secret_bytes().expect("sk").to_vec();
        let legacy_key =
            PQVault::derive_ed_vault_key(PW, &pk.public_key_bytes, None).expect("legacy key");
        let enc = cipher::encrypt_and_wipe(&mut sk, &legacy_key).expect("encrypt");

        let mut reader = CryptoEngine::new();
        let opened = PQVault::unlock_identity(
            &mut reader,
            &pk.public_key_bytes,
            &enc.ciphertext,
            &enc.nonce,
            None,
            PW,
            "alice",
            "2026-01-01T00:00:00+00:00",
        )
        .expect("un coffre hérité DOIT rester ouvrable");
        assert_eq!(opened.public_key_hex, pk.public_key_hex);

        // Et la dérivation héritée n'est PAS celle d'un coffre neuf : le sel
        // aléatoire donne une autre clé, donc l'ancien ciphertext ne s'ouvre plus
        // avec — c'est bien un changement de format, pas un alias.
        let fresh_salt = random_salt();
        let fresh_key =
            PQVault::derive_ed_vault_key(PW, &pk.public_key_bytes, Some(&fresh_salt)).expect("key");
        assert_ne!(&legacy_key[..], &fresh_key[..]);
        assert!(cipher::decrypt(&enc.ciphertext, &fresh_key, &enc.nonce).is_err());
    }

    /// MOY-2 — deux sels tirés coup sur coup ne coïncident pas, et font 16 octets.
    /// (Anti-vacuité : un `random_salt` qui renverrait des zéros passerait tout
    /// le reste des tests sans rien protéger.)
    #[test]
    fn moy2_random_salt_is_sixteen_fresh_bytes() {
        let a = random_salt();
        let b = random_salt();
        assert_eq!(a.len(), VAULT_SALT_LEN);
        assert_eq!(VAULT_SALT_LEN, 16);
        assert_ne!(a, b, "le sel doit venir d'OsRng, pas d'une constante");
        assert_ne!(a, [0u8; VAULT_SALT_LEN]);
    }

    /// **MOY-4** — le coffre Ed25519 était déchiffré puis **rendu tel quel** :
    /// personne ne comparait la clé reconstruite à la clé publique enregistrée.
    /// Un attaquant ayant l'écriture sur `quanta.db` substituait le triplet
    /// complet, chiffré sous un mot de passe qu'il choisit ; si la victime le
    /// saisissait (mot de passe imposé, sauvegarde falsifiée), l'application
    /// ouvrait l'identité de l'attaquant sans un signal.
    #[test]
    fn moy4_a_substituted_ed25519_vault_is_refused() {
        let victim = {
            let mut e = CryptoEngine::new();
            e.generate_keypair()
        };
        // Coffre de l'attaquant, parfaitement valide — mais d'une autre clé.
        let mut rogue_engine = CryptoEngine::new();
        let rogue = rogue_engine.generate_keypair();
        let mut rogue_sk = rogue_engine.get_secret_bytes().expect("sk").to_vec();
        let salt = random_salt();
        let key = PQVault::derive_ed_vault_key(PW, &victim.public_key_bytes, Some(&salt))
            .expect("key");
        let enc = cipher::encrypt_and_wipe(&mut rogue_sk, &key).expect("encrypt");

        let mut engine = CryptoEngine::new();
        let out = PQVault::unlock_identity(
            &mut engine,
            &victim.public_key_bytes, // ce que la base annonce
            &enc.ciphertext,          // ce qu'elle contient réellement
            &enc.nonce,
            Some(&salt),
            PW,
            "alice",
            "2026-01-01T00:00:00+00:00",
        );
        assert!(out.is_err(), "la clé déchiffrée doit être liée à l'identité enregistrée");
        assert_ne!(
            engine.get_identity().map(|i| i.public_key_hex).ok(),
            Some(rogue.public_key_hex),
            "l'identité substituée ne doit pas rester chargée dans le moteur"
        );
    }

    /// **MOY-4** — même défaut sur la couche qui contrôle **les fonds** : le blob
    /// annonce une clé publique ML-DSA et en scelle une autre. Seule la clé
    /// effectivement déchiffrée fait autorité.
    #[test]
    fn moy4_a_fund_vault_lying_about_its_public_key_is_refused() {
        let mut victim = CryptoEngine::new();
        let victim_pk = victim.generate_pq_identity().expect("pq");

        let mut rogue = CryptoEngine::new();
        rogue.import_pq_identity(&[0x42u8; 32]).expect("rogue");
        let mut seed = rogue.get_pq_seed_bytes().expect("seed").to_vec();
        let salt = random_salt();
        // Scellé sous la clé de coffre de la victime, mais contenant l'autre graine.
        let key = PQVault::derive_pq_vault_key(PW, &victim_pk, Some(&salt)).expect("key");
        let enc = cipher::encrypt_and_wipe(&mut seed, &key).expect("encrypt");

        let mut engine = CryptoEngine::new();
        let out = PQVault::unlock_pq_identity(
            &mut engine,
            &victim_pk,
            &enc.ciphertext,
            &enc.nonce,
            Some(&salt),
            PW,
        );
        assert!(out.is_err(), "un blob qui ment sur sa clé publique doit être refusé");
        assert!(
            !engine.is_unlocked(),
            "aucune autorité de dépense ne doit survivre au refus"
        );
    }

    /// Anti-vacuité des deux tests ci-dessus : le chemin honnête, lui, passe.
    #[test]
    fn moy4_the_honest_vault_still_opens() {
        let mut engine = CryptoEngine::new();
        let (identity, pk_bytes, ct, nonce, salt) =
            PQVault::create_identity(&mut engine, "alice", PW).expect("create");
        let (pq_pk, pq_ct, pq_nonce, pq_salt) =
            PQVault::create_pq_identity(&mut engine, PW).expect("create pq");

        let mut reader = CryptoEngine::new();
        let opened = PQVault::unlock_identity(
            &mut reader,
            &pk_bytes,
            &ct,
            &nonce,
            Some(&salt),
            PW,
            "alice",
            &identity.created_at,
        )
        .expect("unlock");
        assert_eq!(opened.public_key_hex, identity.public_key_hex);
        let reloaded =
            PQVault::unlock_pq_identity(&mut reader, &pq_pk, &pq_ct, &pq_nonce, Some(&pq_salt), PW)
                .expect("unlock pq");
        assert_eq!(reloaded, pq_pk);
    }
}
