//! QUANTA Protocol v1.0 — Energy-indexed Sovereign P2P Protocol.
//!
//! Architecture:
//! - `security/` — Ed25519 identity, PQ Vault, hybrid crypto
//! - `p2p/` — Gossip, ledger, reputation, consensus, mining, identity
//! - `storage/` — SQLite persistence layer
//!
//! Background tasks (all respect `CancellationToken` for graceful shutdown):
//! - Mining loop (60s interval)
//! - Gossip outgoing drain (mpsc → iroh)
//! - Gossip incoming dispatch (iroh → dispatcher)
//! - Peer cleanup (30s dead-peer TTL)
//! - State persistence (30s snapshot to SQLite)

mod security;
mod p2p;
mod storage;
mod commands_v3;

/// Tauri-agnostic node boot path (DB, endpoint, background tasks), shared by the
/// desktop app and the headless `quanta-node` daemon. See `node_runtime.rs`.
pub mod node_runtime;

/// JSON-RPC 2.0 over HTTP — the integration surface (wallets, explorers, exchange
/// deposit monitoring). Served by the daemon. See `rpc.rs`.
pub mod rpc;

/// Deterministic, sans-IO state-machine core (Phase 0 simulation harness, task
/// T0.1). Boundary types (`Event`/`Effect`) + determinism abstractions
/// (`Clock`/`Rng`). See `QUANTA_T0_DST_HARNESS.md`. Public so simulation/fuzz
/// shells (test code) can drive it.
pub mod sm;

/// Re-exported for fuzz harnesses (`src-tauri/fuzz/`). Not part of the stable
/// API: it exposes the stateless gossip-envelope parser/validator so a fuzzer
/// can hammer it with arbitrary untrusted bytes. See `fuzz/README.md`.
#[doc(hidden)]
pub use p2p::dispatcher::try_process_raw_gossip as fuzz_parse_gossip;

use security::{CryptoEngine, pq_vault::PQVault};
use p2p::willow_node::WillowNode;
use storage::db::Database;
use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::Manager;

/// Local brute-force throttle on vault unlocks (password AND Touch ID paths).
/// Exponential backoff: after `n ≥ 3` consecutive failures the next attempt is
/// only accepted `min(2^(n−3), 60)` seconds later. In-memory by design — a
/// reboot resets it, but a reboot also costs the attacker far more than the
/// wait, and the real wall is Argon2id (64 MiB, 3 iters) per guess.
#[derive(Default)]
pub struct UnlockGuard {
    failures: std::sync::atomic::AtomicU32,
    not_before: Mutex<Option<std::time::Instant>>,
}

impl UnlockGuard {
    /// Err(message) while the backoff window is still open.
    pub async fn check(&self) -> Result<(), String> {
        if let Some(t) = *self.not_before.lock().await {
            let now = std::time::Instant::now();
            if now < t {
                let secs = (t - now).as_secs().max(1);
                return Err(format!(
                    "Trop de tentatives — réessayez dans {} s",
                    secs
                ));
            }
        }
        Ok(())
    }

    pub async fn on_failure(&self) {
        use std::sync::atomic::Ordering;
        let n = self.failures.fetch_add(1, Ordering::SeqCst) + 1;
        if n >= 3 {
            let delay = 2u64.saturating_pow(n.saturating_sub(3)).min(60);
            *self.not_before.lock().await =
                Some(std::time::Instant::now() + std::time::Duration::from_secs(delay));
        }
    }

    pub async fn on_success(&self) {
        use std::sync::atomic::Ordering;
        self.failures.store(0, Ordering::SeqCst);
        *self.not_before.lock().await = None;
    }
}

pub struct AppState {
    pub crypto: Mutex<CryptoEngine>,
    pub db: Mutex<Option<Database>>,
    pub node: WillowNode,
    /// Brute-force throttle shared by every unlock path.
    pub unlock_guard: UnlockGuard,
    /// NET-15: Optional human-readable display name embedded in our outgoing
    /// Hello messages. The wallet's Ed25519 signature on the envelope already
    /// authenticates the name; sanitisation happens at receive time.
    pub display_name: tokio::sync::RwLock<Option<String>>,
    /// NET-16: Cached Tauri AppHandle, populated during `.setup()`. Used by
    /// background tasks (e.g. chain-sync handlers) to emit progress events
    /// to the frontend without holding a handle through every call site.
    pub app_handle: tokio::sync::RwLock<Option<tauri::AppHandle>>,
}

impl AppState {
    /// Construct a fresh node state. `app_handle` starts `None` — the desktop app
    /// fills it during Tauri `.setup()`; the headless daemon leaves it `None` (and
    /// every event emitter already guards on it), so the same core runs both ways.
    pub fn new() -> Self {
        Self {
            crypto: Mutex::new(CryptoEngine::new()),
            db: Mutex::new(None),
            node: WillowNode::new(),
            unlock_guard: UnlockGuard::default(),
            display_name: tokio::sync::RwLock::new(None),
            app_handle: tokio::sync::RwLock::new(None),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Gossip stats ───────────────────────────────────────────────

#[tauri::command]
async fn get_gossip_stats(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let g = state.node.gossip.read().await;
    Ok(serde_json::json!({
        "messages_sent": g.stats.messages_sent,
        "messages_received": g.stats.messages_received,
        "bytes_sent": g.stats.bytes_sent,
        "bytes_received": g.stats.bytes_received,
        "nodes_synced": g.stats.nodes_synced,
        "peers_reported": g.stats.peers_reported,
        "dropped_signature": g.stats.dropped_signature,
        "dropped_rate_limit": g.stats.dropped_rate_limit,
        "dropped_nonce": g.stats.dropped_nonce,
    }))
}

// ─── Identity (PQ Vault) ─────────────────────────────────────────

#[tauri::command]
async fn check_identity(state: tauri::State<'_, Arc<AppState>>) -> Result<bool, String> {
    let db = state.db.lock().await;
    Ok(db.as_ref().ok_or("DB not ready")?.get_active_keypair().await?.is_some())
}

#[tauri::command]
async fn create_identity(
    state: tauri::State<'_, Arc<AppState>>, display_name: String, password: String,
) -> Result<security::pq_vault::QuantaIdentity, String> {
    let display_name = display_name.trim().to_string();
    if display_name.is_empty() { return Err("Le nom d'affichage est requis".into()); }
    if password.len() < 8 { return Err("Mot de passe trop court (min. 8)".into()); }

    let mut engine = state.crypto.lock().await;
    let (id, pk_bytes, enc_sk, nonce) = PQVault::create_identity(&mut engine, &display_name, &password)?;
    // PQ-MIG-3 §3: establish + persist the **independent** ML-DSA primary — the
    // post-quantum tx-authority key the ledger binds. Its 32-byte root seed is
    // generated separately (OsRng, not derived from the Ed25519 seed) and stored
    // encrypted alongside the identity, so a quantum break of Ed25519 cannot yield it.
    let (pq_pk, pq_enc, pq_nonce) = PQVault::create_pq_identity(&mut engine, &password)?;
    let db = state.db.lock().await;
    let dbref = db.as_ref().ok_or("DB not ready")?;
    dbref.store_keypair(&pk_bytes, &enc_sk, &nonce, &display_name).await?;
    dbref.save_state(PQ_IDENTITY_KEY, &pq_identity_blob(&pq_pk, &pq_enc, &pq_nonce)).await?;
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
async fn unlock_identity(
    state: tauri::State<'_, Arc<AppState>>, password: String,
) -> Result<security::pq_vault::QuantaIdentity, String> {
    state.unlock_guard.check().await?;
    let db = state.db.lock().await;
    let dbref = db.as_ref().ok_or("DB not ready")?;
    let kp = dbref.get_active_keypair().await?.ok_or("No identity")?;
    let mut engine = state.crypto.lock().await;
    let id = match PQVault::unlock_identity(
        &mut engine,
        &kp.public_key, &kp.encrypted_secret_key, &kp.nonce,
        &password,
        &kp.display_name,
        &kp.created_at,
    ) {
        Ok(id) => id,
        Err(e) => {
            state.unlock_guard.on_failure().await;
            return Err(e);
        }
    };
    // PQ-MIG-3 §3: restore the independent ML-DSA primary (the tx-authority key).
    // A legacy identity created before PQ-MIG-3 has no stored bundle yet — establish
    // and persist one now (TOFU at first unlock), so every unlocked wallet can sign
    // post-quantum-authoritative transactions.
    match dbref.load_state(PQ_IDENTITY_KEY).await? {
        Some(json) => {
            let v: serde_json::Value =
                serde_json::from_str(&json).map_err(|_| "PQ identity corrompue".to_string())?;
            let pq_pk = v["pq_public_key"].as_str().ok_or("PQ identity invalide")?;
            let enc_seed: Vec<u8> = serde_json::from_value(v["encrypted_seed"].clone())
                .map_err(|_| "PQ identity invalide".to_string())?;
            let nonce: Vec<u8> = serde_json::from_value(v["nonce"].clone())
                .map_err(|_| "PQ identity invalide".to_string())?;
            PQVault::unlock_pq_identity(&mut engine, pq_pk, &enc_seed, &nonce, &password)?;
        }
        None => {
            let (pq_pk, pq_enc, pq_nonce) = PQVault::create_pq_identity(&mut engine, &password)?;
            dbref
                .save_state(PQ_IDENTITY_KEY, &pq_identity_blob(&pq_pk, &pq_enc, &pq_nonce))
                .await?;
        }
    }
    state.unlock_guard.on_success().await;
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
async fn biometric_status(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
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
async fn enable_biometric_unlock(
    state: tauri::State<'_, Arc<AppState>>, password: String,
) -> Result<(), String> {
    use zeroize::Zeroize;
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
async fn disable_biometric_unlock(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
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
async fn unlock_biometric(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<security::pq_vault::QuantaIdentity, String> {
    use zeroize::Zeroize;
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

#[tauri::command]
async fn get_public_key(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
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
async fn get_recovery_key(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
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
async fn get_receive_address(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
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
fn validate_address(address: String) -> bool {
    crate::security::address::is_valid(&address)
}

/// Normalize any accepted address form — `qta1…` Bech32m **or** canonical 64-hex —
/// into both representations, or an opaque error. Lets a UI accept either and always
/// display the checksummed public form.
#[tauri::command]
fn resolve_address(address: String) -> Result<serde_json::Value, String> {
    let bytes = crate::security::address::parse(&address)?;
    Ok(serde_json::json!({
        "bech32": crate::security::address::encode(&bytes),
        "hex": hex::encode(bytes),
    }))
}

// ─── P2P (Willow Node) ─────────────────────────────────────────

#[tauri::command]
async fn get_node_status(state: tauri::State<'_, Arc<AppState>>) -> Result<p2p::NodeStatus, String> {
    Ok(state.node.get_status().await)
}

/// NET-9 + NET-10: Snapshot of per-peer network metrics for the frontend.
/// Returns a sorted list (best quality first) of peers with their RTT, byte
/// counts, message counts, loss ratio and 0-100 quality score.
#[tauri::command]
async fn get_peer_metrics(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<serde_json::Value>, String> {
    let info = state.node.peer_info.read().await;
    let mut rows: Vec<serde_json::Value> = info.iter().map(|(pk, p)| {
        let loss_ratio = if p.pings_sent == 0 {
            0.0
        } else {
            1.0 - (p.pongs_received as f64 / p.pings_sent as f64).min(1.0)
        };
        serde_json::json!({
            "public_key": pk,
            "display_name": p.display_name,
            "country": p.country,
            "watts": p.watts,
            "last_rtt_ms": p.last_rtt_ms,
            "smoothed_rtt_ms": p.smoothed_rtt_ms,
            "bytes_in": p.bytes_in,
            "messages_in": p.messages_in,
            "pings_sent": p.pings_sent,
            "pongs_received": p.pongs_received,
            "loss_ratio": loss_ratio,
            "uptime_secs": p.first_seen.elapsed().as_secs(),
            "quality_score": p.quality_score(),
            "last_seen_secs_ago": p.elapsed().as_secs(),
        })
    }).collect();
    // Best quality first; peers with no score (no Pong yet) sort last.
    rows.sort_by(|a, b| {
        let qa = a.get("quality_score").and_then(|v| v.as_u64()).unwrap_or(0);
        let qb = b.get("quality_score").and_then(|v| v.as_u64()).unwrap_or(0);
        qb.cmp(&qa)
    });
    Ok(rows)
}

/// NET-15: Set our outgoing display name. Sanitised before storage; subsequent
/// Hello broadcasts will carry it. Pass `None` (or an empty string) to clear.
#[tauri::command]
async fn set_display_name(
    state: tauri::State<'_, Arc<AppState>>,
    name: Option<String>,
) -> Result<Option<String>, String> {
    let sanitised = name
        .as_deref()
        .and_then(p2p::gossip::sanitize_display_name);
    *state.display_name.write().await = sanitised.clone();
    Ok(sanitised)
}

/// NET-15: Read the currently advertised display name.
#[tauri::command]
async fn get_display_name(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Option<String>, String> {
    Ok(state.display_name.read().await.clone())
}

/// NET-11: 2-hop network topology view. Returns the local node + each known
/// peer + that peer's last-advertised known_peer_ids list (collected from
/// the most recent Hello). Useful for visualizing the mesh in the frontend.
#[tauri::command]
async fn get_network_topology(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<serde_json::Value, String> {
    let our_id = state.node.get_ticket().await.unwrap_or_default();
    let kp = state.node.known_peers.read().await;
    let info = state.node.peer_info.read().await;

    let peers: Vec<serde_json::Value> = kp.values().map(|p| {
        let pi = info.values().find(|_| false); // placeholder — we key peer_info by pubkey, known_peers by endpoint_id
        // We don't have a stable mapping endpoint_id ↔ pubkey here, so we
        // expose what we can: connection state + reconnect attempts.
        let _ = pi;
        serde_json::json!({
            "endpoint_id": p.endpoint_id,
            "connected": p.connected,
            "reconnect_attempts": p.reconnect_attempts,
            "last_connected_secs_ago": p.last_connected.elapsed().as_secs(),
        })
    }).collect();

    Ok(serde_json::json!({
        "self": our_id,
        "direct_peers": peers,
        "peer_pubkey_metrics": info.iter().map(|(pk, p)| serde_json::json!({
            "pk": pk,
            "country": p.country,
            "watts": p.watts,
            "rtt_ms": p.smoothed_rtt_ms,
            "quality": p.quality_score(),
        })).collect::<Vec<_>>(),
    }))
}

/// Returns the current node contribution mode based on real-time CPU watts.
#[tauri::command]
async fn get_node_mode() -> Result<serde_json::Value, String> {
    let watts = p2p::energy::estimate_watts();
    let mode = if watts < 5.0 {
        p2p::shapley::NodeMode::Guardian
    } else {
        p2p::shapley::NodeMode::Active
    };
    Ok(serde_json::json!({
        "mode": mode,
        "watts": watts,
        "label": match mode {
            p2p::shapley::NodeMode::Guardian => "Guardian 🛡️",
            p2p::shapley::NodeMode::Active => "Active ⚡",
            p2p::shapley::NodeMode::Research => "Research 🔬",
        },
    }))
}

// ─── Security Audit (CBOM + Monitoring) ─────────────────────────

#[tauri::command]
async fn get_security_audit(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let cbom = security::crypto_agility::CryptoBOM::current();
    let node = state.node.get_status().await;
    let has_id = state.crypto.lock().await.get_identity().is_ok();
    Ok(serde_json::json!({
        "grade": cbom.security_grade(),
        "pq_pending": cbom.pq_migration_count(),
        "signing": cbom.signing,
        "key_exchange": cbom.key_exchange,
        "hashing": cbom.hashing,
        "symmetric": cbom.symmetric,
        "kdf": cbom.kdf,
        "node_online": node.is_online,
        "active_subspaces": node.active_subspaces,
        "peer_count": node.peer_count,
        "identity_active": has_id,
        "protocol": node.protocol,
    }))
}

// ─── Proof of Attention Economy (REMOVED — V2 pure crypto) ──────

#[tauri::command]
async fn get_node_ticket(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
    state.node.get_ticket().await.ok_or_else(|| "No ticket available (offline)".into())
}

/// Phase 4 — connecte ce nœud à un peer via son EndpointId Iroh.
/// Le peer doit aussi être abonné au topic QUANTA pour que le sync démarre.
/// After successful connection, immediately broadcasts a Hello so chain sync begins.
#[tauri::command]
async fn connect_peer(state: tauri::State<'_, Arc<AppState>>, peer_id: String) -> Result<(), String> {
    let peer_id = peer_id.trim().to_string();
    if peer_id.is_empty() { return Err("EndpointId vide".into()); }
    state.node.connect_peer(&peer_id).await?;
    // Trigger immediate Hello so the peer sees us and sync starts now
    let state_clone = state.inner().clone();
    tokio::spawn(async move {
        // Small delay to let gossip topology settle
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        p2p::gossip_tasks::trigger_hello_now(&state_clone).await;
    });
    Ok(())
}

// ─── Reputation & Social (REMOVED — V2 pure crypto) ─────────────

#[tauri::command]
async fn get_my_reputation(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    // REPUT-ID-1: reputation is keyed by the ML-DSA **address** (economic actor),
    // matching the mining loop's `uptime_tick(&addr, …)` — not the transport key.
    let pk = state.crypto.lock().await.pq_address_hex().unwrap_or_default();
    let rep = state.node.reputation.read().await;
    let micro = p2p::ledger::MICRO as f64;
    // Convert µQTA → QUANTA for frontend display
    match rep.get_user(&pk) {
        Some(user) => Ok(serde_json::json!({
            "public_key": user.public_key,
            "trust_score": user.trust_score,
            "status": user.status,
            "atn_earned": user.atn_earned as f64 / micro,
            "atn_balance": user.atn_balance as f64 / micro,
            "atn_staked": user.atn_staked as f64 / micro,
            "uptime_minutes": user.uptime_minutes,
            "energy_kwh": user.energy_kwh,
            "energy_atn_mined": user.energy_atn_mined as f64 / micro,
            "joined_at": user.joined_at,
        })),
        None => Ok(serde_json::json!({
            "public_key": pk,
            "trust_score": 0.0,
            "status": "New",
            "atn_earned": 0.0,
            "atn_balance": 0.0,
            "atn_staked": 0.0,
            "uptime_minutes": 0,
            "energy_kwh": 0.0,
            "energy_atn_mined": 0.0,
            "joined_at": "",
        })),
    }
}

#[tauri::command]
async fn transfer_atn(state: tauri::State<'_, Arc<AppState>>, to_pk: String, amount: f64) -> Result<(), String> {
    // REPUT-ID-1: local actor = ML-DSA address (coherent with mining/ledger_transfer).
    let my_pk = state.crypto.lock().await.pq_address_hex().unwrap_or_default();
    let uqta = quanta_to_uqta(amount)?;
    state.node.reputation.write().await.transfer(&my_pk, &to_pk, uqta)
}

#[tauri::command]
async fn stake_atn(state: tauri::State<'_, Arc<AppState>>, amount: f64) -> Result<f64, String> {
    // REPUT-ID-1: local actor = ML-DSA address (coherent with mining/ledger_transfer).
    let my_pk = state.crypto.lock().await.pq_address_hex().unwrap_or_default();
    let uqta = quanta_to_uqta(amount)?;
    let staked_uqta = state.node.reputation.write().await.stake(&my_pk, uqta)?;
    Ok(staked_uqta as f64 / p2p::ledger::MICRO as f64)
}

/// STRUCT-2: Convert a frontend-supplied QUANTA value (f64) to µQTA (u64) safely.
/// Rejects negatives, NaN, infinities, and values that would overflow u64.
fn quanta_to_uqta(amount: f64) -> Result<u64, String> {
    if !amount.is_finite() || amount < 0.0 {
        return Err("Montant invalide".into());
    }
    let uqta_f = amount * p2p::ledger::MICRO as f64;
    if uqta_f >= u64::MAX as f64 {
        return Err("Montant trop grand".into());
    }
    Ok(uqta_f.round() as u64)
}

#[tauri::command]
async fn get_trust_leaderboard(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!(state.node.reputation.read().await.get_leaderboard(20)))
}

#[tauri::command]
async fn get_energy_stats(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let rep = state.node.reputation.read().await;
    let (kwh, mined_uqta, uptime) = rep.network_energy_stats();
    drop(rep);
    // Énergie réelle uniquement (kWh mesurés). Aucune conversion en euros :
    // QUANTA n'a pas de prix de marché, donc l'app n'en affiche aucun.
    Ok(serde_json::json!({
        "kwh_consumed": kwh,
        "atn_mined": mined_uqta as f64 / p2p::ledger::MICRO as f64,
        "uptime_minutes": uptime,
    }))
}

/// État du consensus CRDT pour le frontend.
#[tauri::command]
async fn get_consensus_stats(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let cons = state.node.consensus.read().await;
    let crdt_accounts = cons.ledger.account_count();
    drop(cons);
    let gossip = state.node.gossip.read().await;
    let stats = gossip.stats.clone();
    drop(gossip);
    Ok(serde_json::json!({
        "crdt_accounts": crdt_accounts,
        "gossip_messages_sent": stats.messages_sent,
        "gossip_messages_received": stats.messages_received,
        "gossip_nodes_synced": stats.nodes_synced,
    }))
}

/// Phase 3 — santé globale du réseau (économie + énergie + sécurité).
#[tauri::command]
async fn get_network_health(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let stats = state.node.ledger.read().await.stats();
    let total_quanta_supply = stats.total_mined as f64 / p2p::ledger::MICRO as f64;

    // PoC score de l'identité courante (si présente). REPUT-ID-1 : la réputation
    // est keyée par l'**adresse** ML-DSA (acteur économique), pas le transport.
    let pk = state.crypto.lock().await.pq_address_hex().unwrap_or_default();
    let (poc_score, mining_multiplier) = if pk.is_empty() {
        (0.0_f64, 0.1_f64)
    } else {
        let rep = state.node.reputation.read().await;
        match rep.get_user(&pk) {
            Some(u) => {
                let s = p2p::sybil::SybilGuard::poc_score(u);
                (s, p2p::sybil::SybilGuard::mining_multiplier(s))
            }
            None => (0.0, 0.1),
        }
    };

    // Prix énergie : local si pas de peers, moyenne réseau sinon
    let reports = state.node.peer_country_reports.read().await.clone();
    let oracle = state.node.energy_oracle.read().await;
    let local_country = p2p::energy::EnergyOracle::detect_country();
    let energy_price_local_eur = if reports.is_empty() {
        oracle.price_for(local_country)
    } else {
        let pairs: Vec<(String, u64)> = reports.into_iter().collect();
        oracle.network_weighted_average(&pairs)
    };
    drop(oracle);

    let node = state.node.get_status().await;

    Ok(serde_json::json!({
        "total_quanta_supply": total_quanta_supply,
        "emission_model": "fixed",
        "emission_per_hour": 100.0,
        "energy_price_local_eur": energy_price_local_eur,
        "country": local_country,
        "peer_country_reports": state.node.peer_country_reports.read().await.clone(),
        "poc_score": poc_score,
        "mining_multiplier": mining_multiplier,
        "connected_peers": node.peer_count,
        "is_online": node.is_online,
    }))
}

// ─── ATN Protocol (Native Ledger) ───────────────────────────────

#[tauri::command]
async fn get_ledger_stats(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let ledger = state.node.ledger.read().await;
    Ok(serde_json::json!(ledger.stats()))
}

/// Vue d'ensemble de la chaîne pour la visualisation live : offre/émission
/// globales + les N derniers blocs (du plus récent au plus ancien) avec les
/// QUANTA créés par chacun. Lecture seule — sûr à interroger à haute fréquence.
#[tauri::command]
async fn get_chain_overview(
    state: tauri::State<'_, Arc<AppState>>,
    limit: Option<u64>,
) -> Result<serde_json::Value, String> {
    use p2p::ledger_types::TxType;
    let ledger = state.node.ledger.read().await;
    let micro = p2p::ledger::MICRO as f64;
    let stats = ledger.stats();
    let height = ledger.chain_height();
    let lim = limit.unwrap_or(14).min(60);
    let start = height.saturating_sub(lim);

    let mut blocks: Vec<serde_json::Value> = Vec::new();
    for i in (start..height).rev() {
        if let Some(b) = ledger.block_at(i) {
            let minted: u64 = b
                .transactions
                .iter()
                .filter(|t| matches!(t.tx_type, TxType::Mining))
                .map(|t| t.amount)
                .sum();
            blocks.push(serde_json::json!({
                "index": b.index,
                "timestamp": b.timestamp,
                "hash": b.hash,
                "miner": b.miner,
                "tx_count": b.transactions.len(),
                "minted_qta": minted as f64 / micro,
                "energy_kwh": b.energy_kwh,
            }));
        }
    }

    let max_supply = p2p::reputation::MAX_SUPPLY_MICRO;
    let next_emission = p2p::reputation::emission_for_tick(stats.total_mined);
    Ok(serde_json::json!({
        "height": height,
        "total_supply_qta": ledger.total_supply() as f64 / micro,
        "total_mined_qta": stats.total_mined as f64 / micro,
        "total_burned_qta": ledger.total_burned() as f64 / micro,
        "max_supply_qta": max_supply as f64 / micro,
        "remaining_qta": max_supply.saturating_sub(stats.total_mined) as f64 / micro,
        "pct_to_cap": (stats.total_mined as f64 / max_supply as f64) * 100.0,
        "emission_next_tick_qta": next_emission as f64 / micro,
        "pending": stats.pending,
        "holders": stats.holders,
        "total_energy_kwh": stats.total_energy,
        "blocks": blocks,
    }))
}

/// Histoire COMPLÈTE de la chaîne, depuis la genèse, pour la visualiser d'un
/// coup d'œil : les blocs récents restent individuels (petits), les plus
/// anciens sont **agrégés** en gros blocs (level-of-detail). Le nombre de gros
/// blocs est borné pour rester lisible même sur une longue chaîne.
#[tauri::command]
async fn get_chain_history(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<serde_json::Value, String> {
    use p2p::ledger_types::TxType;
    let ledger = state.node.ledger.read().await;
    let micro = p2p::ledger::MICRO as f64;
    let height = ledger.chain_height();

    const RECENT: u64 = 24; // derniers blocs montrés individuellement
    const MAX_BUCKETS: u64 = 48; // borne le nombre de gros blocs
    let split = height.saturating_sub(RECENT);
    // Taille de bucket dynamique : ≥10, et assez grande pour ne pas dépasser MAX_BUCKETS.
    let bucket_size = if split == 0 { 10 } else { (split.div_ceil(MAX_BUCKETS)).max(10) };

    let summarize = |from: u64, to: u64| -> serde_json::Value {
        let mut minted = 0u64;
        let mut tx = 0usize;
        let mut energy = 0f64;
        let mut first_ts = String::new();
        let mut last_ts = String::new();
        for i in from..to {
            if let Some(b) = ledger.block_at(i) {
                if first_ts.is_empty() {
                    first_ts = b.timestamp.clone();
                }
                last_ts = b.timestamp.clone();
                tx += b.transactions.len();
                energy += b.energy_kwh;
                minted += b
                    .transactions
                    .iter()
                    .filter(|t| matches!(t.tx_type, TxType::Mining))
                    .map(|t| t.amount)
                    .sum::<u64>();
            }
        }
        serde_json::json!({
            "from": from, "to": to.saturating_sub(1), "count": to - from,
            "minted_qta": minted as f64 / micro, "tx_count": tx,
            "energy_kwh": energy, "first_ts": first_ts, "last_ts": last_ts,
        })
    };

    let mut buckets: Vec<serde_json::Value> = Vec::new();
    let mut i = 0u64;
    while i < split {
        let end = (i + bucket_size).min(split);
        buckets.push(summarize(i, end));
        i = end;
    }

    let mut recent: Vec<serde_json::Value> = Vec::new();
    for j in split..height {
        if let Some(b) = ledger.block_at(j) {
            let minted: u64 = b
                .transactions
                .iter()
                .filter(|t| matches!(t.tx_type, TxType::Mining))
                .map(|t| t.amount)
                .sum();
            recent.push(serde_json::json!({
                "index": b.index, "minted_qta": minted as f64 / micro,
                "tx_count": b.transactions.len(), "energy_kwh": b.energy_kwh,
                "timestamp": b.timestamp, "hash": b.hash,
            }));
        }
    }

    Ok(serde_json::json!({
        "height": height,
        "bucket_size": bucket_size,
        "recent_individual": RECENT,
        "buckets": buckets,
        "recent": recent,
    }))
}

#[tauri::command]
async fn get_recent_txs(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let ledger = state.node.ledger.read().await;
    let micro = p2p::ledger::MICRO as f64;
    // Convert µQTA amounts to QUANTA for frontend display
    let txs: Vec<serde_json::Value> = ledger.recent_txs(50).iter().map(|tx| {
        serde_json::json!({
            "id": tx.id,
            "from": tx.from,
            "to": tx.to,
            "amount": tx.amount as f64 / micro,
            "tx_type": tx.tx_type,
            "timestamp": tx.timestamp,
        })
    }).collect();
    Ok(serde_json::json!(txs))
}

#[tauri::command]
async fn get_balance(state: tauri::State<'_, Arc<AppState>>, pk: String) -> Result<f64, String> {
    let ledger = state.node.ledger.read().await;
    // STRUCT-2: balance is u64 µQTA — convert to QUANTA (f64) for the frontend.
    Ok(ledger.balance_of(&pk) as f64 / p2p::ledger::MICRO as f64)
}

#[tauri::command]
async fn ledger_transfer(state: tauri::State<'_, Arc<AppState>>, to: String, amount: f64) -> Result<serde_json::Value, String> {
    // V7: Input validation. Accept EITHER the public `qta1…` (Bech32m, checksummed)
    // form OR the canonical 64-hex form, and normalize to the on-chain hex the
    // ledger keys on. The bech32 path is checksum-validated, so a mistyped receive
    // address is rejected here instead of sending to a valid-looking wrong account.
    let to = crate::security::address::parse(&to)
        .map(hex::encode)
        .map_err(|_| "Adresse destinataire invalide".to_string())?;
    if amount <= 0.0 || amount > 1_000_000.0 {
        return Err("Montant invalide (0 < x ≤ 1 000 000)".into());
    }
    let uqta = quanta_to_uqta(amount)?;
    let crypto = state.crypto.lock().await;
    // PQ-MIG-3B: the account identity (`from`, balance key, and the CRDT/reputation
    // mirror key) is the ML-DSA **address**. PQ-ENVELOPE-1: the gossip envelope
    // sender is the ML-DSA **primary public key**. `to` is already an address (64-hex).
    let from = crypto
        .pq_address_hex()
        .ok_or("Identité ML-DSA absente (impossible de dériver l'adresse de valeur)")?;
    let sender_pk = crypto
        .pq_identity_hex()
        .ok_or("Identité ML-DSA absente")?;
    let mut ledger = state.node.ledger.write().await;
    let (tx, burn_tx, burn_uqta) = ledger.transfer_with_burn(&from, &to, uqta, &crypto)?;
    let net_uqta = uqta - burn_uqta;
    drop(ledger);
    // REPUT-ID-1: the reputation mirror is keyed by the ML-DSA **address** (the
    // economic actor) — both legs. The mining loop now fills it under `addr`
    // (`uptime_tick(&addr, …)`), so a received credit lands in the SAME bucket
    // the recipient mines into. Off the security path (ADR-002), but coherent.
    let _ = state.node.reputation.write().await.transfer(&from, &to, net_uqta);
    // Phase 3 — double-ledger : refléter le transfert net dans le PN-Counter CRDT
    // (µQTA), en **espace de valeur** (adresse ML-DSA), miroir du ledger on-chain.
    {
        let mut cons = state.node.consensus.write().await;
        cons.ledger.debit(&from, &from, net_uqta);
        cons.ledger.credit(&from, &to, net_uqta);
    }

    // ── CRITICAL: Broadcast both legs (transfer + burn) so peers' ledgers
    //   stay aligned with ours. AUDIT-TX-2: a previous bug only sent the
    //   transfer leg, leaving every other node with a 1% balance gap.
    for tx_obj in std::iter::once(&tx).chain(burn_tx.as_ref()) {
        broadcast_signed_tx(&state, &crypto, &sender_pk, tx_obj).await;
    }
    log::info!("◈ [Transfer] Broadcast {} QUANTA (+{:.6} burn) → {}",
        amount, burn_uqta as f64 / p2p::ledger::MICRO as f64, &to[..12]);

    let micro = p2p::ledger::MICRO as f64;
    Ok(serde_json::json!({
        "tx": tx,
        "burn_amount": burn_uqta as f64 / micro,
        "net_amount": net_uqta as f64 / micro,
    }))
}

/// Sign a `BroadcastTx` gossip envelope for `tx` with the ML-DSA-65 envelope
/// identity (PQ-ENVELOPE-1) and queue it for the network. Shared by every
/// user-authored tx command (transfer, burn leg, stake, unstake) so all legs
/// propagate identically (AUDIT-TX-2 taught us what happens when one leg is
/// forgotten).
async fn broadcast_signed_tx(
    state: &Arc<AppState>,
    crypto: &CryptoEngine,
    sender_pk: &str,
    tx: &p2p::ledger_types::Transaction,
) {
    let Ok(tx_json) = serde_json::to_string(tx) else { return };
    let msg = p2p::gossip::GossipMessage::BroadcastTx { tx_json };
    let timestamp = chrono::Utc::now().to_rfc3339();
    let nonce = state.node.gossip.read().await.next_outgoing_nonce();
    let signable =
        p2p::gossip::GossipRouter::signable_envelope_bytes(sender_pk, nonce, &timestamp, &msg);
    let sig = crypto.sign_pq(&signable).unwrap_or_default();
    if let Ok(env) = p2p::gossip::GossipRouter::build_signed_envelope(
        sender_pk.to_string(), msg, nonce, timestamp, &sig,
    ) {
        state.node.gossip.write().await.mark_seen(&env.id);
        let _ = state.node.gossip_tx.send(env);
    }
}

/// ONCHAIN-STAKE-1 — real on-chain staking from the UI: builds the signed
/// `Stake` tx (spendable → bonded **at seal**) and broadcasts it, so every node
/// derives the same stake state from the chain. Consensus weight (leader
/// election + finality votes) comes from exactly these txs — this is what
/// makes the user a validator. The legacy `stake_atn` (reputation mirror only,
/// no consensus effect) is superseded by this in the wallet UI.
#[tauri::command]
async fn ledger_stake(state: tauri::State<'_, Arc<AppState>>, amount: f64) -> Result<serde_json::Value, String> {
    if amount <= 0.0 || amount > 1_000_000.0 {
        return Err("Montant invalide (0 < x ≤ 1 000 000)".into());
    }
    let uqta = quanta_to_uqta(amount)?;
    let crypto = state.crypto.lock().await;
    let from = crypto
        .pq_address_hex()
        .ok_or("Identité ML-DSA absente (impossible de dériver l'adresse de valeur)")?;
    let sender_pk = crypto.pq_identity_hex().ok_or("Identité ML-DSA absente")?;
    let mut ledger = state.node.ledger.write().await;
    let tx = ledger.stake_tx(&from, uqta, &crypto)?;
    drop(ledger);
    broadcast_signed_tx(&state, &crypto, &sender_pk, &tx).await;
    log::info!("◈ [Stake] Broadcast {} QUANTA → bonded at next seal", amount);
    Ok(serde_json::json!({ "tx": tx }))
}

/// ONCHAIN-STAKE-1 §3 — begin unlocking bonded stake. On seal the amount moves
/// into an unbonding entry that matures at `height + UNBONDING_PERIOD_BLOCKS`
/// (it stays slashable until then — Casper semantics, LIVE-3B). Broadcast like
/// any user tx.
#[tauri::command]
async fn ledger_unstake(state: tauri::State<'_, Arc<AppState>>, amount: f64) -> Result<serde_json::Value, String> {
    if amount <= 0.0 || amount > 1_000_000.0 {
        return Err("Montant invalide (0 < x ≤ 1 000 000)".into());
    }
    let uqta = quanta_to_uqta(amount)?;
    let crypto = state.crypto.lock().await;
    let from = crypto
        .pq_address_hex()
        .ok_or("Identité ML-DSA absente (impossible de dériver l'adresse de valeur)")?;
    let sender_pk = crypto.pq_identity_hex().ok_or("Identité ML-DSA absente")?;
    let mut ledger = state.node.ledger.write().await;
    let tx = ledger.unstake_tx(&from, uqta, &crypto)?;
    drop(ledger);
    broadcast_signed_tx(&state, &crypto, &sender_pk, &tx).await;
    log::info!("◈ [Unstake] Broadcast {} QUANTA → unbonding at next seal", amount);
    Ok(serde_json::json!({ "tx": tx }))
}

/// The wallet's single source of truth — everything the UI needs to show the
/// three-way money split, straight from the **ledger** (chain state), not the
/// reputation mirror: spendable, bonded stake, unbonding entries with their
/// unlock heights, plus pending (unsealed) stake movements for instant feedback.
/// All amounts converted µQTA → QUANTA for display.
#[tauri::command]
async fn get_wallet_overview(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    // `address` = canonical on-chain hex (keys the ledger, tx `from`/`to`).
    // `address_bech32` = the public checksummed `qta1…` form to show/share/QR.
    let (addr, addr_bech32) = {
        let c = state.crypto.lock().await;
        (
            c.pq_address_hex().unwrap_or_default(),
            c.pq_address_bech32().unwrap_or_default(),
        )
    };
    let earned_uqta = {
        let rep = state.node.reputation.read().await;
        rep.get_user(&addr).map(|u| u.atn_earned).unwrap_or(0)
    };
    let ledger = state.node.ledger.read().await;
    let micro = p2p::ledger::MICRO as f64;
    let height = ledger.chain_height();
    let spendable = ledger.balance_of(&addr);
    let staked = ledger.staked_of(&addr);
    let unbonding = ledger.unbonding_of(&addr);
    let entries: Vec<serde_json::Value> = ledger
        .unbonding_entries_of(&addr)
        .into_iter()
        .map(|(amount, unlock_height)| {
            serde_json::json!({
                "amount": amount as f64 / micro,
                "unlock_height": unlock_height,
                "blocks_remaining": unlock_height.saturating_sub(height),
            })
        })
        .collect();
    // Pending (mempool) movements from this account: visible instantly in the
    // UI as "in preparation" even though they only take effect at seal.
    use p2p::ledger_types::TxType;
    let (mut pending_stake, mut pending_unstake) = (0u64, 0u64);
    for tx in ledger.pending_txs() {
        if tx.from == addr {
            match tx.tx_type {
                TxType::Stake => pending_stake += tx.amount,
                TxType::Unstake => pending_unstake += tx.amount,
                _ => {}
            }
        }
    }
    Ok(serde_json::json!({
        "address": addr,
        "address_bech32": addr_bech32,
        "height": height,
        "spendable": spendable as f64 / micro,
        "staked": staked as f64 / micro,
        "unbonding": unbonding as f64 / micro,
        "unbonding_entries": entries,
        "pending_stake": pending_stake as f64 / micro,
        "pending_unstake": pending_unstake as f64 / micro,
        "earned": earned_uqta as f64 / micro,
        "min_validator_stake": p2p::pos_consensus::MIN_VALIDATOR_STAKE as f64 / micro,
        "unbonding_period_blocks": p2p::ledger::UNBONDING_PERIOD_BLOCKS,
    }))
}

/// Finality-gadget status for the mining screen: how far the chain is into the
/// current epoch, where the irreversibility floor sits, and the live validator
/// set (count + total bonded weight + this node's bonded weight). Read-only,
/// cheap, safe to poll.
#[tauri::command]
async fn get_finality_status(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    use sm::finality::EPOCH_LENGTH_BLOCKS;
    let addr = state.crypto.lock().await.pq_address_hex().unwrap_or_default();
    let ledger = state.node.ledger.read().await;
    let micro = p2p::ledger::MICRO as f64;
    let height = ledger.chain_height();
    let floor = ledger.finalized_floor_index();
    let stakes = ledger.validator_stakes();
    let total_staked: u64 = stakes.values().sum();
    let eligible = stakes
        .values()
        .filter(|&&s| s >= p2p::pos_consensus::MIN_VALIDATOR_STAKE)
        .count();
    let my_stake = stakes.get(&addr).copied().unwrap_or(0);
    let blocks_into_epoch = height % EPOCH_LENGTH_BLOCKS;
    Ok(serde_json::json!({
        "height": height,
        "finalized_floor": floor,
        "epoch": height / EPOCH_LENGTH_BLOCKS,
        "epoch_length": EPOCH_LENGTH_BLOCKS,
        "blocks_into_epoch": blocks_into_epoch,
        "next_checkpoint": height - blocks_into_epoch + EPOCH_LENGTH_BLOCKS,
        "validators": eligible,
        "total_staked": total_staked as f64 / micro,
        "my_stake": my_stake as f64 / micro,
        "i_am_validator": my_stake >= p2p::pos_consensus::MIN_VALIDATOR_STAKE,
        "quorum_num": 2,
        "quorum_den": 3,
    }))
}

#[tauri::command]
async fn verify_ledger(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let ledger = state.node.ledger.read().await;
    let (blocks, txs) = ledger.verify_chain()?;
    Ok(serde_json::json!({
        "verified": true,
        "blocks_verified": blocks,
        "txs_verified": txs,
    }))
}

/// V2 — Snapshot of the QUANTA economy: supply, emission rate, burn total (in QUANTA, f64 for UI).
#[tauri::command]
async fn get_economy_stats(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let stats = state.node.ledger.read().await.stats();
    let burned_uqta = state.node.ledger.read().await.total_burned();
    let total_mined_uqta = stats.total_mined;
    let circulating_uqta = total_mined_uqta.saturating_sub(burned_uqta);
    let micro = p2p::ledger::MICRO as f64;
    // Taux d'émission RÉEL au point actuel de la chaîne : décroît à mesure que
    // `total_mined` s'approche du plafond. 1 tick = 60 s ⇒ /min ; ×60 ⇒ /h.
    // Aucune valeur fixe codée en dur : on interroge la même fonction que le minage.
    // Aucune conversion en euros : QUANTA n'a pas de prix de marché.
    let emission_per_min_uqta = p2p::reputation::emission_for_tick(total_mined_uqta);
    let emission_per_minute = emission_per_min_uqta as f64 / micro;
    let emission_per_hour = emission_per_minute * 60.0;
    Ok(serde_json::json!({
        "total_mined": total_mined_uqta as f64 / micro,
        "total_burned": burned_uqta as f64 / micro,
        "circulating": circulating_uqta as f64 / micro,
        "emission_model": "decaying_capped",
        "emission_per_hour": emission_per_hour,
        "emission_per_minute": emission_per_minute,
        "max_supply": p2p::reputation::MAX_SUPPLY_MICRO as f64 / micro,
    }))
}

// ─── App Entry ──────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    // PQ-TRANSPORT-1 (2026-07-18) — installe `aws-lc-rs` comme fournisseur
    // cryptographique rustls **par défaut du process**. C'est le seul fournisseur
    // qui porte le groupe d'échange hybride post-quantique `X25519MLKEM768`
    // (activé par la feature `prefer-post-quantum`). iroh configure son propre
    // fournisseur explicitement, mais tout autre chemin TLS 1.3 (ex. HTTPS vers
    // un relay) qui s'appuie sur le fournisseur « par défaut » serait ambigu si
    // `ring` coexiste dans le graphe → panique runtime. L'installer ici, une
    // fois, lève cette ambiguïté de façon déterministe. Idempotent : renvoie
    // `Err` si déjà installé (ex. par une lib) — sans conséquence.
    // PQ-TRANSPORT-1: install aws-lc-rs (X25519MLKEM768) as the process default.
    node_runtime::install_crypto_provider();

    let app_state = Arc::new(AppState::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(app_state.clone())
        .setup(move |app| {
            let state = app_state.clone();
            // NET-16: Cache the AppHandle so background tasks can emit events.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                *state.app_handle.write().await = Some(handle);
                // Shared boot path (DB + endpoint + all background tasks). The app
                // mines; the daemon (bin/quanta-node) calls the same fn with mine=false.
                node_runtime::bootstrap(&state, node_runtime::default_data_dir(), true).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            check_identity, create_identity, unlock_identity, get_public_key, get_recovery_key,
            get_receive_address, validate_address, resolve_address,
            biometric_status, enable_biometric_unlock, disable_biometric_unlock, unlock_biometric,
            get_node_status, get_node_mode,
            get_peer_metrics, get_network_topology,
            set_display_name, get_display_name,
            get_security_audit,
            get_node_ticket, connect_peer,
            get_my_reputation, transfer_atn, stake_atn, get_trust_leaderboard,
            get_energy_stats, get_consensus_stats, get_network_health,
            get_ledger_stats, get_chain_overview, get_chain_history, get_recent_txs, get_balance, ledger_transfer,
            ledger_stake, ledger_unstake, get_wallet_overview, get_finality_status,
            verify_ledger,
            get_economy_stats,
            get_gossip_stats,
            // Identité — pseudos uniques @handle (adresse de wallet lisible)
            commands_v3::claim_username,
            commands_v3::resolve_username,
            commands_v3::is_username_available,
            commands_v3::get_my_username,
            commands_v3::username_of_pk,
            commands_v3::get_my_connection_code,
            commands_v3::verify_connection,
        ])
        .build(tauri::generate_context!())
        .expect("QUANTA Protocol v1.0 failed to build")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                log::info!("◈ [Quanta] App exit — signaling graceful shutdown");
                let state: tauri::State<'_, Arc<AppState>> = app_handle.state();
                state.node.shutdown.cancel();
            }
        });
}
