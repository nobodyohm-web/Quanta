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
    // AUDIT-2026-07-25: `Nonce::from_slice` PANICS on a length other than 12, and
    // the nonce comes from stored/attacker-influenceable bytes. A panic inside a
    // Tauri command kills the command (and inside a spawned task, the task) —
    // while this function documents an opaque error. Check the length instead, and
    // return the same opaque error so no oracle is introduced.
    if nonce_bytes.len() != 12 {
        return Err("decryption failed".to_string());
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// AUDIT-2026-07-25 — `Nonce::from_slice` panics on any length other than 12,
    /// and the nonce arrives from stored bytes. This function documents an opaque
    /// error, and a panic inside a Tauri command kills the command outright, so a
    /// malformed nonce must be an `Err`, never an abort.
    #[test]
    fn malformed_nonce_returns_an_error_instead_of_panicking() {
        let key = [7u8; 32];
        for bad in [vec![], vec![0u8; 11], vec![0u8; 13], vec![0u8; 64]] {
            let out = decrypt(b"whatever", &key, &bad);
            assert!(out.is_err(), "nonce of {} bytes must be rejected", bad.len());
        }
    }

    /// The opaque-error contract: a wrong nonce and a wrong key must be
    /// indistinguishable to the caller, so no decryption oracle is offered.
    #[test]
    fn failures_are_indistinguishable() {
        let key = [7u8; 32];
        let payload = encrypt(b"secret", &key).expect("encrypt");
        let wrong_key = decrypt(&payload.ciphertext, &[9u8; 32], &payload.nonce);
        let wrong_nonce = decrypt(&payload.ciphertext, &key, &[0u8; 11]);
        assert_eq!(wrong_key.unwrap_err(), wrong_nonce.unwrap_err());
    }

    /// Round-trip sanity, so the guard above cannot pass by breaking decryption.
    #[test]
    fn roundtrip_still_works() {
        let key = [3u8; 32];
        let payload = encrypt(b"quanta", &key).expect("encrypt");
        let out = decrypt(&payload.ciphertext, &key, &payload.nonce).expect("decrypt");
        assert_eq!(out, b"quanta");
    }
}
