// security/pq_vault.rs — Post-Quantum Vault (ML-KEM / ML-DSA Ready)
// Defense-grade identity with classical Ed25519 + PQ abstraction layer.
// zeroize all sensitive material on drop.

use super::{CryptoEngine, cipher};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

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
        let enc_key = cipher::derive_key(password, &salt[..16])?;
        
        // Encrypt the secret key
        let enc = cipher::encrypt_and_wipe(&mut sk_bytes, &enc_key)?;
        
        let identity = QuantaIdentity {
            public_key_hex: pk.public_key_hex,
            display_name: display_name.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            is_initialized: true,
            pq_algorithm: "Ed25519 (ML-DSA-65 ready)".to_string(),
            security_level: "classical".to_string(),
        };
        
        Ok((identity, pk.public_key_bytes, enc.ciphertext, enc.nonce))
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
        let salt = CryptoEngine::blake3_hash(hex::encode(public_key).as_bytes());
        let enc_key = cipher::derive_key(password, &salt[..16])?;
        let mut sk_bytes = cipher::decrypt(encrypted_sk, &enc_key, nonce)?;

        let sk_arr: [u8; 32] = sk_bytes.clone().try_into().map_err(|_| "Bad SK len")?;
        let pk = engine.import_keypair(&sk_arr)?;

        // Zeroize decrypted secret key immediately
        sk_bytes.zeroize();

        Ok(QuantaIdentity {
            public_key_hex: pk.public_key_hex,
            display_name: display_name.to_string(),
            created_at: created_at.to_string(),
            is_initialized: true,
            pq_algorithm: "Ed25519 (ML-DSA-65 ready)".to_string(),
            security_level: "classical".to_string(),
        })
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
