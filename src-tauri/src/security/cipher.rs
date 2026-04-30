// security/cipher.rs — AES-256-GCM + Argon2id with zeroize
use aes_gcm::{aead::{Aead, AeadCore, KeyInit, OsRng}, Aes256Gcm, Nonce};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedPayload {
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
}

/// Derive a 256-bit key from password via Argon2id (GPU/ASIC resistant)
/// V6: Hardened params — 64 MiB memory, 3 iterations, 4 lanes
pub fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    let mut key = [0u8; 32];
    let params = argon2::Params::new(64 * 1024, 3, 4, Some(32))
        .map_err(|e| format!("Argon2id params: {}", e))?;
    argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| format!("Argon2id KDF failed: {}", e))?;
    Ok(key)
}

/// AES-256-GCM authenticated encryption
pub fn encrypt(plaintext: &[u8], key: &[u8; 32]) -> Result<EncryptedPayload, String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ct = cipher.encrypt(&nonce, plaintext).map_err(|e| e.to_string())?;
    Ok(EncryptedPayload { ciphertext: ct, nonce: nonce.to_vec() })
}

/// AES-256-GCM authenticated decryption.
/// Returns an opaque error on failure to avoid leaking timing/oracle hints.
pub fn decrypt(ciphertext: &[u8], key: &[u8; 32], nonce_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| "decryption failed".to_string())
}

/// Encrypt then zeroize the source plaintext
pub fn encrypt_and_wipe(plaintext: &mut Vec<u8>, key: &[u8; 32]) -> Result<EncryptedPayload, String> {
    let result = encrypt(plaintext, key)?;
    plaintext.zeroize();
    Ok(result)
}
