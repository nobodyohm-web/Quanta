// security/mod.rs — Post-Quantum Security Layer (QUANTA)
// ML-KEM (FIPS 203), ML-DSA (FIPS 204), AES-256-GCM, Argon2id, zeroize

pub mod pq_vault;
pub mod cipher;
pub mod crypto_agility;
pub mod hybrid_crypto;

use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use fips204::ml_dsa_65;
use fips204::traits::Signer as _;
use hybrid_crypto::derive_ml_dsa;
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
    /// Clé secrète ML-DSA-65 + clé publique hex, dérivées de la graine Ed25519.
    /// `fips204::PrivateKey` est zeroize-on-drop (feature activée par défaut).
    ml_dsa: Option<(ml_dsa_65::PrivateKey, String)>,
}

impl CryptoEngine {
    pub fn new() -> Self { Self { key_pair: None, ml_dsa: None } }

    pub fn generate_keypair(&mut self) -> PublicIdentity {
        let sk = SigningKey::generate(&mut OsRng);
        let vk = sk.verifying_key();
        let bytes = vk.to_bytes().to_vec();
        let hex_str = hex::encode(&bytes);
        // Dérive la paire ML-DSA-65 depuis la même graine (zéro secret en plus).
        let mut seed = sk.to_bytes();
        self.key_pair = Some(KeyPair { signing_key: sk, verifying_key: vk });
        self.ml_dsa = derive_ml_dsa(&seed);
        seed.zeroize();
        let pq_public_key_hex = self.ml_dsa.as_ref().map(|(_, h)| h.clone());
        PublicIdentity { public_key_hex: hex_str, public_key_bytes: bytes, pq_public_key_hex }
    }

    pub fn import_keypair(&mut self, sk_bytes: &[u8; 32]) -> Result<PublicIdentity, String> {
        let sk = SigningKey::from_bytes(sk_bytes);
        let vk = sk.verifying_key();
        let bytes = vk.to_bytes().to_vec();
        let hex_str = hex::encode(&bytes);
        self.key_pair = Some(KeyPair { signing_key: sk, verifying_key: vk });
        // Recalcule la paire ML-DSA-65 à partir de la graine restaurée.
        self.ml_dsa = derive_ml_dsa(sk_bytes);
        let pq_public_key_hex = self.ml_dsa.as_ref().map(|(_, h)| h.clone());
        Ok(PublicIdentity { public_key_hex: hex_str, public_key_bytes: bytes, pq_public_key_hex })
    }

    pub fn get_identity(&self) -> Result<PublicIdentity, String> {
        let kp = self.key_pair.as_ref().ok_or("No active keypair")?;
        let bytes = kp.verifying_key.to_bytes().to_vec();
        let pq_public_key_hex = self.ml_dsa.as_ref().map(|(_, h)| h.clone());
        Ok(PublicIdentity { public_key_hex: hex::encode(&bytes), public_key_bytes: bytes, pq_public_key_hex })
    }

    pub fn get_secret_bytes(&self) -> Result<Vec<u8>, String> {
        Ok(self.key_pair.as_ref().ok_or("No active keypair")?.signing_key.to_bytes().to_vec())
    }

    pub fn sign(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        let kp = self.key_pair.as_ref().ok_or("No active keypair")?;
        Ok(kp.signing_key.sign(data).to_bytes().to_vec())
    }

    /// Signature hybride : Ed25519 + ML-DSA-65 (FIPS 204).
    /// Renvoie `(classical_sig_bytes, quantum_sig_bytes, pq_pk_hex)`.
    /// Si aucune clé ML-DSA n'est dérivée, la couche quantum est vide et la
    /// vérification retombe en Ed25519 seul (rétro-compat).
    pub fn sign_hybrid(&self, data: &[u8]) -> Result<(Vec<u8>, Vec<u8>, String), String> {
        let kp = self.key_pair.as_ref().ok_or("No active keypair")?;
        let classical = kp.signing_key.sign(data).to_bytes().to_vec();
        match &self.ml_dsa {
            Some((sk, pk_hex)) => {
                let sig = sk
                    .try_sign_with_rng(&mut OsRng, data, &[])
                    .map_err(|_| "ML-DSA signing failed".to_string())?;
                Ok((classical, sig.to_vec(), pk_hex.clone()))
            }
            None => Ok((classical, Vec::new(), String::new())),
        }
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
