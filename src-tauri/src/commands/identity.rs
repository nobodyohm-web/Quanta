//! Identity & vault commands — creation, unlock (password + Touch ID),
//! recovery phrase, and address forms. The wallet-lifecycle helpers
//! (`create_wallet`/`unlock_wallet`/`restore_wallet`) are `pub(crate)`:
//! shared by these Tauri commands and the headless `quanta-node` daemon
//! (`crate::node_runtime`).

use crate::security;
use crate::security::pq_vault::PQVault;
use crate::AppState;
use std::sync::Arc;
use zeroize::Zeroize;

// ─── Identity (PQ Vault) ─────────────────────────────────────────

#[tauri::command]
pub async fn check_identity(state: tauri::State<'_, Arc<AppState>>) -> Result<bool, String> {
    let db = state.db.lock().await;
    Ok(db.as_ref().ok_or("DB not ready")?.get_active_keypair().await?.is_some())
}

#[tauri::command]
pub async fn create_identity(
    state: tauri::State<'_, Arc<AppState>>, display_name: String, password: String,
) -> Result<security::pq_vault::QuantaIdentity, String> {
    let display_name = display_name.trim().to_string();
    if display_name.is_empty() { return Err("Le nom d'affichage est requis".into()); }
    if password.len() < 8 { return Err("Mot de passe trop court (min. 8)".into()); }

    create_wallet(state.inner(), &display_name, &password).await
}

/// Create + persist a new wallet identity (Ed25519 + the **independent** ML-DSA
/// primary — the PQ tx-authority key the ledger binds, seeded separately via OsRng
/// so a quantum break of Ed25519 cannot yield it). Headless-callable: shared by the
/// `create_identity` command and the `quanta-node` daemon's persistent wallet.
pub(crate) async fn create_wallet(
    state: &Arc<AppState>,
    display_name: &str,
    password: &str,
) -> Result<security::pq_vault::QuantaIdentity, String> {
    let display_name = display_name.trim().to_string();
    if display_name.is_empty() {
        return Err("Le nom d'affichage est requis".into());
    }
    if password.len() < 8 {
        return Err("Mot de passe trop court (min. 8)".into());
    }
    let mut engine = state.crypto.lock().await;
    let (id, pk_bytes, enc_sk, nonce) = PQVault::create_identity(&mut engine, &display_name, password)?;
    let (pq_pk, pq_enc, pq_nonce) = PQVault::create_pq_identity(&mut engine, password)?;
    let db = state.db.lock().await;
    let dbref = db.as_ref().ok_or("DB not ready")?;
    dbref.store_keypair(&pk_bytes, &enc_sk, &nonce, &display_name).await?;
    dbref
        .save_state(PQ_IDENTITY_KEY, &pq_identity_blob(&pq_pk, &pq_enc, &pq_nonce))
        .await?;
    Ok(id)
}

/// Storage key for the persisted post-quantum (ML-DSA-65) primary identity bundle.
const PQ_IDENTITY_KEY: &str = "pq_identity_v1";

/// Serialize the encrypted ML-DSA primary bundle for the `state_snapshots` KV
/// (no `keypairs` schema migration; transparent to the frontend).
fn pq_identity_blob(pq_pk: &str, enc_seed: &[u8], nonce: &[u8]) -> String {
    serde_json::json!({
        "pq_public_key": pq_pk,
        "encrypted_seed": enc_seed,
        "nonce": nonce,
    })
    .to_string()
}

#[tauri::command]
pub async fn unlock_identity(
    state: tauri::State<'_, Arc<AppState>>, password: String,
) -> Result<security::pq_vault::QuantaIdentity, String> {
    state.unlock_guard.check().await?;
    match unlock_wallet(state.inner(), &password).await {
        Ok(id) => {
            state.unlock_guard.on_success().await;
            Ok(id)
        }
        Err(e) => {
            state.unlock_guard.on_failure().await;
            Err(e)
        }
    }
}

/// Unlock the stored wallet identity (Ed25519 + restore/TOFU-establish the ML-DSA
/// primary). Headless-callable: shared by the `unlock_identity` command (which adds
/// the brute-force guard) and the daemon's persistent wallet (self-unlock).
pub(crate) async fn unlock_wallet(
    state: &Arc<AppState>,
    password: &str,
) -> Result<security::pq_vault::QuantaIdentity, String> {
    let db = state.db.lock().await;
    let dbref = db.as_ref().ok_or("DB not ready")?;
    let kp = dbref.get_active_keypair().await?.ok_or("No identity")?;
    let mut engine = state.crypto.lock().await;
    let id = PQVault::unlock_identity(
        &mut engine,
        &kp.public_key,
        &kp.encrypted_secret_key,
        &kp.nonce,
        password,
        &kp.display_name,
        &kp.created_at,
    )?;
    // PQ-MIG-3 §3: restore the independent ML-DSA primary. A legacy identity created
    // before PQ-MIG-3 has no bundle yet — TOFU-establish one at first unlock.
    match dbref.load_state(PQ_IDENTITY_KEY).await? {
        Some(json) => {
            let v: serde_json::Value =
                serde_json::from_str(&json).map_err(|_| "PQ identity corrompue".to_string())?;
            let pq_pk = v["pq_public_key"].as_str().ok_or("PQ identity invalide")?;
            let enc_seed: Vec<u8> = serde_json::from_value(v["encrypted_seed"].clone())
                .map_err(|_| "PQ identity invalide".to_string())?;
            let nonce: Vec<u8> = serde_json::from_value(v["nonce"].clone())
                .map_err(|_| "PQ identity invalide".to_string())?;
            PQVault::unlock_pq_identity(&mut engine, pq_pk, &enc_seed, &nonce, password)?;
        }
        None => {
            let (pq_pk, pq_enc, pq_nonce) = PQVault::create_pq_identity(&mut engine, password)?;
            dbref
                .save_state(PQ_IDENTITY_KEY, &pq_identity_blob(&pq_pk, &pq_enc, &pq_nonce))
                .await?;
        }
    }
    Ok(id)
}

/// RECOVER-1 — the wallet's **recovery phrase**: a 24-word BIP39 mnemonic of the
/// ML-DSA **fund** seed. Whoever holds this phrase controls the funds — it is the
/// true backup. (The legacy `get_recovery_key` exports the Ed25519 transport seed,
/// which does NOT recover funds; the phrase does.) Shown once at onboarding and
/// forced to be backed up + confirmed. Requires an unlocked identity.
#[tauri::command]
pub async fn get_recovery_phrase(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
    let engine = state.crypto.lock().await;
    // Self-wiping 32-byte ML-DSA root seed (the fund-controlling secret).
    let seed = engine.get_pq_seed_bytes()?;
    let mnemonic = bip39::Mnemonic::from_entropy(&seed[..])
        .map_err(|_| "Impossible de générer la phrase de récupération".to_string())?;
    Ok(mnemonic.to_string())
}

/// RECOVER-1 — restore a wallet from its 24-word recovery phrase, under a NEW
/// password + display name (e.g. a new device, or a forgotten password).
#[tauri::command]
pub async fn restore_from_phrase(
    state: tauri::State<'_, Arc<AppState>>,
    mnemonic: String,
    display_name: String,
    password: String,
) -> Result<security::pq_vault::QuantaIdentity, String> {
    restore_wallet(state.inner(), &mnemonic, &display_name, &password).await
}

/// RECOVER-1 — reconstruct the ML-DSA **fund authority** from a BIP39 phrase's seed
/// (deterministic keygen → the SAME address as the original wallet) and persist it
/// encrypted under `password`. The Ed25519 transport key is freshly generated (it is
/// regenerable and never the account authority). Headless-callable.
pub(crate) async fn restore_wallet(
    state: &Arc<AppState>,
    mnemonic: &str,
    display_name: &str,
    password: &str,
) -> Result<security::pq_vault::QuantaIdentity, String> {
    let display_name = display_name.trim().to_string();
    if display_name.is_empty() {
        return Err("Le nom d'affichage est requis".into());
    }
    if password.len() < 8 {
        return Err("Mot de passe trop court (min. 8)".into());
    }
    // Decode the phrase → the 32-byte fund seed (validates the checksum).
    let parsed = bip39::Mnemonic::parse_normalized(mnemonic.trim())
        .map_err(|_| "Phrase de récupération invalide".to_string())?;
    let mut entropy = parsed.to_entropy();
    if entropy.len() != 32 {
        entropy.zeroize();
        return Err("Phrase de récupération invalide (doit faire 24 mots)".into());
    }
    let mut seed: [u8; 32] = entropy[..32].try_into().map_err(|_| "Phrase invalide".to_string())?;
    entropy.zeroize();

    let mut engine = state.crypto.lock().await;
    // Fresh Ed25519 transport + the QuantaIdentity bundle (transport is regenerable).
    let (id, pk_bytes, enc_sk, nonce) = PQVault::create_identity(&mut engine, &display_name, password)?;
    // Reconstruct the ML-DSA fund authority from the phrase's seed, encrypted anew.
    let (pq_pk, pq_enc, pq_nonce) = PQVault::restore_pq_identity(&mut engine, &seed, password)?;
    seed.zeroize();
    let db = state.db.lock().await;
    let dbref = db.as_ref().ok_or("DB not ready")?;
    dbref.store_keypair(&pk_bytes, &enc_sk, &nonce, &display_name).await?;
    dbref
        .save_state(PQ_IDENTITY_KEY, &pq_identity_blob(&pq_pk, &pq_enc, &pq_nonce))
        .await?;
    Ok(id)
}

// ─── Touch ID quick unlock (security/biometric.rs) ──────────────
//
// A random KEK sits in the macOS Keychain behind `.BIOMETRY_CURRENT_SET`
// (the OS demands the fingerprint at read time and invalidates the item if
// enrolled prints change). It wraps the two Argon2id-DERIVED vault keys —
// the password is never stored. Password unlock stays the fallback.

/// SQLite settings key of the KEK-wrapped vault keys.
const BIOMETRIC_WRAP_KEY: &str = "biometric_wrap_v1";

/// Whether the machine supports biometry (probed once per run) and whether
/// quick unlock is currently enabled for this wallet.
#[tauri::command]
pub async fn biometric_status(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    static SUPPORTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let supported = match SUPPORTED.get() {
        Some(v) => *v,
        None => {
            let v = tokio::task::spawn_blocking(security::biometric::probe_available)
                .await
                .unwrap_or(false);
            *SUPPORTED.get_or_init(|| v)
        }
    };
    let enabled = {
        let db = state.db.lock().await;
        match db.as_ref() {
            Some(d) => matches!(d.load_state(BIOMETRIC_WRAP_KEY).await, Ok(Some(s)) if !s.is_empty()),
            None => false,
        }
    };
    Ok(serde_json::json!({ "supported": supported, "enabled": enabled }))
}

/// Enable Touch ID quick unlock. Requires the password (deliberate re-auth:
/// enabling a new unlock factor is a security-sensitive act), which is used
/// ONLY to re-derive + verify the two vault keys before wrapping them.
#[tauri::command]
pub async fn enable_biometric_unlock(
    state: tauri::State<'_, Arc<AppState>>, password: String,
) -> Result<(), String> {
    let db = state.db.lock().await;
    let dbref = db.as_ref().ok_or("DB not ready")?;
    let kp = dbref.get_active_keypair().await?.ok_or("No identity")?;
    let pq_json = dbref
        .load_state(PQ_IDENTITY_KEY)
        .await?
        .ok_or("Identité PQ absente — déverrouillez d'abord une fois")?;
    let v: serde_json::Value =
        serde_json::from_str(&pq_json).map_err(|_| "PQ identity corrompue".to_string())?;
    let pq_pk = v["pq_public_key"].as_str().ok_or("PQ identity invalide")?;
    let pq_enc: Vec<u8> = serde_json::from_value(v["encrypted_seed"].clone())
        .map_err(|_| "PQ identity invalide".to_string())?;
    let pq_nonce: Vec<u8> = serde_json::from_value(v["nonce"].clone())
        .map_err(|_| "PQ identity invalide".to_string())?;

    // Derive both vault keys and PROVE the password is right (decrypt both
    // blobs; plaintexts wiped immediately). Opaque error on mismatch.
    let ed_key = PQVault::derive_ed_vault_key(&password, &kp.public_key)?;
    let pq_key = PQVault::derive_pq_vault_key(&password, pq_pk)?;
    let mut probe = security::cipher::decrypt(&kp.encrypted_secret_key, &ed_key, &kp.nonce)
        .map_err(|_| "Mot de passe invalide".to_string())?;
    probe.zeroize();
    let mut probe = security::cipher::decrypt(&pq_enc, &pq_key, &pq_nonce)
        .map_err(|_| "Mot de passe invalide".to_string())?;
    probe.zeroize();

    // Random KEK → Keychain (biometry-gated); KEK-wrapped derived keys → disk.
    let mut kek = [0u8; 32];
    {
        use rand::RngCore;
        rand::rngs::OsRng.fill_bytes(&mut kek);
    }
    let mut plain = Vec::with_capacity(64);
    plain.extend_from_slice(&ed_key[..]);
    plain.extend_from_slice(&pq_key[..]);
    let wrapped = security::cipher::encrypt_and_wipe(&mut plain, &kek)?;
    let kek_owned = kek;
    let store = tokio::task::spawn_blocking(move || {
        let r = security::biometric::store_kek(&kek_owned);
        let mut k = kek_owned;
        k.zeroize();
        r
    })
    .await
    .map_err(|_| "Tâche Keychain interrompue".to_string())?;
    kek.zeroize();
    store?;
    dbref
        .save_state(
            BIOMETRIC_WRAP_KEY,
            &serde_json::json!({ "wrapped": wrapped.ciphertext, "nonce": wrapped.nonce }).to_string(),
        )
        .await?;
    log::info!("◈ [Security] Touch ID quick unlock ENABLED (Keychain biometry-gated KEK)");
    Ok(())
}

/// Disable Touch ID quick unlock: the Keychain KEK is deleted (the on-disk
/// wrap becomes undecryptable garbage, also cleared).
#[tauri::command]
pub async fn disable_biometric_unlock(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    tokio::task::spawn_blocking(security::biometric::delete_kek)
        .await
        .map_err(|_| "Tâche Keychain interrompue".to_string())?;
    let db = state.db.lock().await;
    if let Some(d) = db.as_ref() {
        d.save_state(BIOMETRIC_WRAP_KEY, "").await?;
    }
    log::info!("◈ [Security] Touch ID quick unlock disabled");
    Ok(())
}

/// Unlock the wallet with Touch ID: macOS shows the biometric sheet while we
/// read the KEK; the unwrapped derived keys decrypt both vault blobs. Ends in
/// exactly the same engine state as a password unlock.
#[tauri::command]
pub async fn unlock_biometric(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<security::pq_vault::QuantaIdentity, String> {
    state.unlock_guard.check().await?;
    let db = state.db.lock().await;
    let dbref = db.as_ref().ok_or("DB not ready")?;
    let kp = dbref.get_active_keypair().await?.ok_or("No identity")?;
    let wrap_json = dbref
        .load_state(BIOMETRIC_WRAP_KEY)
        .await?
        .filter(|s| !s.is_empty())
        .ok_or("Touch ID non activé")?;
    let w: serde_json::Value =
        serde_json::from_str(&wrap_json).map_err(|_| "Wrap biométrique corrompu".to_string())?;
    let wrapped: Vec<u8> = serde_json::from_value(w["wrapped"].clone())
        .map_err(|_| "Wrap biométrique invalide".to_string())?;
    let wnonce: Vec<u8> = serde_json::from_value(w["nonce"].clone())
        .map_err(|_| "Wrap biométrique invalide".to_string())?;
    let pq_json = dbref
        .load_state(PQ_IDENTITY_KEY)
        .await?
        .ok_or("Identité PQ absente")?;
    let v: serde_json::Value =
        serde_json::from_str(&pq_json).map_err(|_| "PQ identity corrompue".to_string())?;
    let pq_enc: Vec<u8> = serde_json::from_value(v["encrypted_seed"].clone())
        .map_err(|_| "PQ identity invalide".to_string())?;
    let pq_nonce: Vec<u8> = serde_json::from_value(v["nonce"].clone())
        .map_err(|_| "PQ identity invalide".to_string())?;

    // The Touch ID moment — blocking while the system sheet is up.
    let kek = tokio::task::spawn_blocking(security::biometric::read_kek)
        .await
        .map_err(|_| "Tâche Keychain interrompue".to_string())?;
    let kek = match kek {
        Ok(k) => k,
        Err(e) => {
            state.unlock_guard.on_failure().await;
            return Err(e);
        }
    };
    let kek_arr: [u8; 32] = kek[..]
        .try_into()
        .map_err(|_| "KEK invalide".to_string())?;
    let mut keys = match security::cipher::decrypt(&wrapped, &kek_arr, &wnonce) {
        Ok(k) => k,
        Err(_) => {
            state.unlock_guard.on_failure().await;
            return Err("Déverrouillage refusé".to_string());
        }
    };
    if keys.len() != 64 {
        keys.zeroize();
        state.unlock_guard.on_failure().await;
        return Err("Déverrouillage refusé".to_string());
    }
    let mut ed_key = [0u8; 32];
    let mut pq_key = [0u8; 32];
    ed_key.copy_from_slice(&keys[..32]);
    pq_key.copy_from_slice(&keys[32..]);
    keys.zeroize();

    let mut engine = state.crypto.lock().await;
    let unlocked = PQVault::unlock_identity_with_key(
        &mut engine,
        &kp.encrypted_secret_key,
        &kp.nonce,
        &ed_key,
        &kp.display_name,
        &kp.created_at,
    )
    .and_then(|id| {
        PQVault::unlock_pq_identity_with_key(&mut engine, &pq_enc, &pq_nonce, &pq_key)
            .map(|_| id)
    });
    ed_key.zeroize();
    pq_key.zeroize();
    match unlocked {
        Ok(id) => {
            state.unlock_guard.on_success().await;
            log::info!("◈ [Security] Wallet unlocked via Touch ID");
            Ok(id)
        }
        Err(_) => {
            state.unlock_guard.on_failure().await;
            // Opaque: don't reveal which layer failed.
            Err("Déverrouillage refusé".to_string())
        }
    }
}

// ─── Public key / address forms ─────────────────────────────────

#[tauri::command]
pub async fn get_public_key(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
    // PQ-MIG-3B: the wallet identity exposed to the UI is the ML-DSA **address**
    // (the value identity: balance key, receive address, `from`/`to`, @pseudo
    // target). The Ed25519 transport key is internal plumbing and never the
    // user's account. (Command name kept for wire/UI compat, per CLAUDE.md.)
    state
        .crypto
        .lock()
        .await
        .pq_address_hex()
        .ok_or_else(|| "Identité ML-DSA absente".to_string())
}

#[tauri::command]
pub async fn get_recovery_key(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
    let engine = state.crypto.lock().await;
    // ZEROIZE-SWEEP: `secret` is a self-wiping `Zeroizing<Vec<u8>>`; wrap the
    // full-secret hex `String` in `Zeroizing` too so neither intermediate
    // lingers on the heap after this command returns. Only the formatted
    // recovery key the user explicitly asked to see leaves the function.
    let secret = engine.get_secret_bytes()?;
    // Format: 8 groups of 8 hex chars (64 chars total = 32 bytes Ed25519 secret)
    let hex = zeroize::Zeroizing::new(hex::encode(&secret));
    let hs: &str = &hex;
    let formatted: Vec<&str> = (0..8).map(|i| &hs[i * 8..(i + 1) * 8]).collect();
    Ok(formatted.join("-"))
}

/// The user's **public receive address** in canonical `qta1…` (Bech32m) form — the
/// checksummed address to share, put in a QR, or hand to an exchange. Unlike the raw
/// hex (`get_public_key`), a single mistyped character fails the checksum instead of
/// silently pointing at another account. The hex form stays the on-chain identity.
/// See [`crate::security::address`].
#[tauri::command]
pub async fn get_receive_address(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
    state
        .crypto
        .lock()
        .await
        .pq_address_bech32()
        .ok_or_else(|| "Identité ML-DSA absente".to_string())
}

/// Exchange-grade address validation: `true` iff `address` is a well-formed Quanta
/// `qta1…` address (Bech32m checksum + length). This is the `validateaddress`
/// primitive a wallet send-form and an exchange integration both need.
#[tauri::command]
pub fn validate_address(address: String) -> bool {
    crate::security::address::is_valid(&address)
}

/// Normalize any accepted address form — `qta1…` Bech32m **or** canonical 64-hex —
/// into both representations, or an opaque error. Lets a UI accept either and always
/// display the checksummed public form.
#[tauri::command]
pub fn resolve_address(address: String) -> Result<serde_json::Value, String> {
    let bytes = crate::security::address::parse(&address)?;
    Ok(serde_json::json!({
        "bech32": crate::security::address::encode(&bytes),
        "hex": hex::encode(bytes),
    }))
}
