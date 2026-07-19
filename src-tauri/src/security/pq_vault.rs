// security/pq_vault.rs — Post-Quantum Vault (ML-KEM / ML-DSA Ready)
// Defense-grade identity with classical Ed25519 + PQ abstraction layer.
// zeroize all sensitive material on drop.

use super::{CryptoEngine, cipher};
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
/// (public identity, raw public key bytes, encrypted secret key, AES-GCM nonce).
pub type CreatedIdentity = (QuantaIdentity, Vec<u8>, Vec<u8>, Vec<u8>);

// PQ-MIG-3 §3: the ML-DSA primary vault below is now **wired into production**
// (`lib.rs` `create_identity`/`unlock_identity` persist + restore the encrypted
// root seed), so the PQ-MIG-1 `#[allow(dead_code)]` scaffolding is removed — the
// functions are live, and `cargo clippy -D warnings` proves they are reachable.

/// PQ-MIG-1 — bundle returned when creating the **post-quantum primary** identity:
/// (ML-DSA-65 public key hex, encrypted 32-byte root seed, AES-GCM nonce).
pub type CreatedPqIdentity = (String, Vec<u8>, Vec<u8>);

/// Domain tag binding the ML-DSA vault's Argon2id salt to its own key material —
/// separates it from the Ed25519 vault's salt (`blake3(ed_pk)`), so the two
/// at-rest secrets never share a derived key even under the same password.
const PQ_VAULT_SALT_DOMAIN: &str = "QUANTA-PQ-MIG-1-vault-v1";

impl PQVault {
    /// Create a new sovereign identity with Ed25519 + PQ preparation
    pub fn create_identity(
        engine: &mut CryptoEngine,
        display_name: &str,
        password: &str,
    ) -> Result<CreatedIdentity, String> {
        let pk = engine.generate_keypair();
        let mut sk_bytes = engine.get_secret_bytes()?;
        
        // Derive encryption key from password (Argon2id)
        let salt = CryptoEngine::blake3_hash(pk.public_key_hex.as_bytes());
        // ZEROIZE-SWEEP: the Argon2id-derived AES key wipes on drop.
        let enc_key = Zeroizing::new(cipher::derive_key(password, &salt[..16])?);

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

        Ok((identity, pk.public_key_bytes, enc.ciphertext, enc.nonce))
    }

    /// Argon2id-derived AES key of the **Ed25519** vault blob (salt =
    /// `blake3(ed_pk_hex)[..16]`, exactly what create/unlock use). Exposed so
    /// the Touch ID quick-unlock can wrap this derived key under a
    /// Keychain-gated KEK — the password itself is never stored.
    pub fn derive_ed_vault_key(
        password: &str,
        public_key: &[u8],
    ) -> Result<Zeroizing<[u8; 32]>, String> {
        let salt = CryptoEngine::blake3_hash(hex::encode(public_key).as_bytes());
        Ok(Zeroizing::new(cipher::derive_key(password, &salt[..16])?))
    }

    /// Unlock identity from encrypted storage
    pub fn unlock_identity(
        engine: &mut CryptoEngine,
        public_key: &[u8],
        encrypted_sk: &[u8],
        nonce: &[u8],
        password: &str,
        display_name: &str,
        created_at: &str,
    ) -> Result<QuantaIdentity, String> {
        let enc_key = Self::derive_ed_vault_key(password, public_key)?;
        Self::unlock_identity_with_key(
            engine, encrypted_sk, nonce, &enc_key, display_name, created_at,
        )
    }

    /// Decryption core of [`Self::unlock_identity`] taking the **derived** AES
    /// key directly (Touch ID path: the key comes unwrapped from the
    /// Keychain-gated KEK instead of Argon2id). Identical hygiene.
    pub fn unlock_identity_with_key(
        engine: &mut CryptoEngine,
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
        // Graine racine auto-effaçante (Zeroizing) extraite du moteur.
        let mut seed = engine.get_pq_seed_bytes()?;
        let salt = CryptoEngine::blake3_hash(
            format!("{PQ_VAULT_SALT_DOMAIN}:{pq_pk_hex}").as_bytes(),
        );
        // ZEROIZE : la clé AES dérivée d'Argon2id s'efface au drop.
        let enc_key = Zeroizing::new(cipher::derive_key(password, &salt[..16])?);
        // encrypt_and_wipe efface `seed` après chiffrement.
        let enc = cipher::encrypt_and_wipe(&mut seed, &enc_key)?;
        Ok((pq_pk_hex, enc.ciphertext, enc.nonce))
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
        let mut seed_copy = engine.get_pq_seed_bytes()?;
        let salt = CryptoEngine::blake3_hash(
            format!("{PQ_VAULT_SALT_DOMAIN}:{pq_pk_hex}").as_bytes(),
        );
        let enc_key = Zeroizing::new(cipher::derive_key(password, &salt[..16])?);
        let enc = cipher::encrypt_and_wipe(&mut seed_copy, &enc_key)?;
        Ok((pq_pk_hex, enc.ciphertext, enc.nonce))
    }

    /// PQ-MIG-1 — déverrouille l'identité primaire post-quantique : déchiffre la
    /// graine racine ML-DSA de 32 octets et rétablit la paire ML-DSA-65 dans le
    /// moteur. Round-trip vers la clé publique **identique** (keygen
    /// déterministe). Mauvais mot de passe ⇒ échec opaque. La graine déchiffrée
    /// est effacée après usage. Renvoie la clé publique ML-DSA (hex).
    /// Argon2id-derived AES key of the **ML-DSA seed** vault blob
    /// (domain-separated salt — see [`PQ_VAULT_SALT_DOMAIN`]). Same rationale
    /// as [`Self::derive_ed_vault_key`].
    pub fn derive_pq_vault_key(
        password: &str,
        pq_public_key_hex: &str,
    ) -> Result<Zeroizing<[u8; 32]>, String> {
        let salt = CryptoEngine::blake3_hash(
            format!("{PQ_VAULT_SALT_DOMAIN}:{pq_public_key_hex}").as_bytes(),
        );
        Ok(Zeroizing::new(cipher::derive_key(password, &salt[..16])?))
    }

    pub fn unlock_pq_identity(
        engine: &mut CryptoEngine,
        pq_public_key_hex: &str,
        encrypted_seed: &[u8],
        nonce: &[u8],
        password: &str,
    ) -> Result<String, String> {
        let enc_key = Self::derive_pq_vault_key(password, pq_public_key_hex)?;
        Self::unlock_pq_identity_with_key(engine, encrypted_seed, nonce, &enc_key)
    }

    /// Decryption core of [`Self::unlock_pq_identity`] taking the derived AES
    /// key directly (Touch ID path). Identical zeroize/opaque-error posture.
    pub fn unlock_pq_identity_with_key(
        engine: &mut CryptoEngine,
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
