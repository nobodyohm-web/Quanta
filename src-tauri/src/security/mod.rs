// security/mod.rs — Post-Quantum Security Layer (SOVA)
// ML-KEM (FIPS 203), ML-DSA (FIPS 204), AES-256-GCM, Argon2id, zeroize

pub mod pq_vault;
pub mod cipher;
pub mod crypto_agility;
pub mod hybrid_crypto;

use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Ed25519 keypair with automatic memory wipe on drop
#[derive(Clone)]
pub struct KeyPair {
    pub signing_key: SigningKey,
    pub verifying_key: VerifyingKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicIdentity {
    pub public_key_hex: String,
    pub public_key_bytes: Vec<u8>,
    pub pq_public_key_hex: Option<String>,
}

/// Core crypto engine with zeroize-on-drop sensitive state
pub struct CryptoEngine {
    key_pair: Option<KeyPair>,
}

impl CryptoEngine {
    pub fn new() -> Self { Self { key_pair: None } }

    pub fn generate_keypair(&mut self) -> PublicIdentity {
        let sk = SigningKey::generate(&mut OsRng);
        let vk = sk.verifying_key();
        let bytes = vk.to_bytes().to_vec();
        let hex_str = hex::encode(&bytes);
        self.key_pair = Some(KeyPair { signing_key: sk, verifying_key: vk });
        PublicIdentity { public_key_hex: hex_str, public_key_bytes: bytes, pq_public_key_hex: None }
    }

    pub fn import_keypair(&mut self, sk_bytes: &[u8; 32]) -> Result<PublicIdentity, String> {
        let sk = SigningKey::from_bytes(sk_bytes);
        let vk = sk.verifying_key();
        let bytes = vk.to_bytes().to_vec();
        let hex_str = hex::encode(&bytes);
        self.key_pair = Some(KeyPair { signing_key: sk, verifying_key: vk });
        Ok(PublicIdentity { public_key_hex: hex_str, public_key_bytes: bytes, pq_public_key_hex: None })
    }

    pub fn get_identity(&self) -> Result<PublicIdentity, String> {
        let kp = self.key_pair.as_ref().ok_or("No active keypair")?;
        let bytes = kp.verifying_key.to_bytes().to_vec();
        Ok(PublicIdentity { public_key_hex: hex::encode(&bytes), public_key_bytes: bytes, pq_public_key_hex: None })
    }

    pub fn get_secret_bytes(&self) -> Result<Vec<u8>, String> {
        Ok(self.key_pair.as_ref().ok_or("No active keypair")?.signing_key.to_bytes().to_vec())
    }

    pub fn sign(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        let kp = self.key_pair.as_ref().ok_or("No active keypair")?;
        Ok(kp.signing_key.sign(data).to_bytes().to_vec())
    }

    /// Hybrid sign : Ed25519 actif + ML-DSA stub (vide jusqu'à activation).
    /// Renvoie `(classical_sig_bytes, quantum_sig_bytes, pq_pk_hex)`.
    pub fn sign_hybrid(&self, data: &[u8]) -> Result<(Vec<u8>, Vec<u8>, String), String> {
        let kp = self.key_pair.as_ref().ok_or("No active keypair")?;
        let classical = kp.signing_key.sign(data).to_bytes().to_vec();
        // Quantum + pq_pk seront non vides quand `ml-dsa >= 0.2` sera stable.
        Ok((classical, Vec::new(), String::new()))
    }

    pub fn verify(pk: &[u8], data: &[u8], sig: &[u8]) -> Result<bool, String> {
        let pk_arr: [u8; 32] = pk.try_into().map_err(|_| "Invalid pk len")?;
        let sig_arr: [u8; 64] = sig.try_into().map_err(|_| "Invalid sig len")?;
        let vk = VerifyingKey::from_bytes(&pk_arr).map_err(|e| e.to_string())?;
        let signature = ed25519_dalek::Signature::from_bytes(&sig_arr);
        Ok(vk.verify(data, &signature).is_ok())
    }

    /// BLAKE3 hash — 10+ Go/s, parallelizable, streaming-verified
    pub fn blake3_hash(data: &[u8]) -> Vec<u8> {
        blake3::hash(data).as_bytes().to_vec()
    }

    /// BLAKE3 keyed MAC for authenticated data
    pub fn blake3_mac(key: &[u8; 32], data: &[u8]) -> Vec<u8> {
        blake3::keyed_hash(key, data).as_bytes().to_vec()
    }

    /// XOR distance for Kademlia DHT routing
    pub fn xor_distance(a: &[u8], b: &[u8]) -> Vec<u8> {
        a.iter().zip(b.iter()).map(|(x, y)| x ^ y).collect()
    }

    /// Validate BLAKE3 integrity of incoming data
    pub fn verify_blake3(data: &[u8], expected_hash: &str) -> bool {
        hex::encode(blake3::hash(data).as_bytes()) == expected_hash
    }
}

impl Default for CryptoEngine { fn default() -> Self { Self::new() } }

/// Secure buffer that zeroizes on drop
#[allow(dead_code)]
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SecureBuffer {
    pub data: Vec<u8>,
}

#[allow(dead_code)]
impl SecureBuffer {
    pub fn new(data: Vec<u8>) -> Self { Self { data } }
}
