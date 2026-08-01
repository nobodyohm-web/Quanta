//! Headless node runtime — the Tauri-agnostic boot path.
//!
//! The desktop app (`lib.rs::run`) and the headless **`quanta-node`** daemon
//! (`bin/quanta-node.rs`) share the exact same node core (P2P, ledger, consensus,
//! crypto). The only thing the app adds on top is a GUI and event emission; the
//! node itself has no Tauri dependency beyond the *optional* cached `AppHandle`
//! (which stays `None` headless, and every event emitter already guards on it).
//!
//! This module extracts the shared boot sequence — install the post-quantum TLS
//! provider, open the database (and restore persisted state), start the Iroh QUIC
//! endpoint, and spawn every background task — so both entry points call it and
//! can never drift apart.

use std::path::PathBuf;
use std::sync::Arc;

use crate::{p2p, storage, AppState};

/// Install `aws-lc-rs` as the **process-default** rustls provider — the only
/// provider carrying the hybrid post-quantum key exchange `X25519MLKEM768`
/// (PQ-TRANSPORT-1). Idempotent: a second call (or one from a linked lib) returns
/// `Err`, harmlessly. Both the app and the daemon must call this once at startup.
pub fn install_crypto_provider() {
    if rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .is_err()
    {
        log::debug!("◈ [PQ] fournisseur rustls aws-lc-rs déjà installé");
    } else {
        log::info!("◈ [PQ] transport post-quantique armé — échange de clés X25519MLKEM768 (aws-lc-rs)");
    }
}

/// Opt the process out of **App Nap** (macOS). Without this, macOS throttles
/// the timers of an app whose window is fully occluded or minimized — the 60 s
/// mining tick, the Hello/Ping heartbeats and the 30 s snapshot all fire late
/// or barely at all, so the node effectively stops mining while backgrounded.
/// `NSActivityBackground` disables App Nap ONLY: it does not keep the display
/// awake and does not prevent system sleep — closing the lid still sleeps the
/// machine (a laptop in a bag must not cook itself for the network).
#[cfg(target_os = "macos")]
pub fn prevent_app_nap() {
    use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};
    let reason = NSString::from_str("Quanta node: mining, gossip and block validation");
    let token = NSProcessInfo::processInfo()
        .beginActivityWithOptions_reason(NSActivityOptions::Background, &reason);
    // The activity ends when the token is released — leak it so it spans the
    // whole process lifetime.
    std::mem::forget(token);
    log::info!("◈ [Node] App Nap prevention active — the node keeps mining in the background");
}

#[cfg(not(target_os = "macos"))]
pub fn prevent_app_nap() {}

/// The default on-disk data directory — the **same** location the desktop app
/// uses, so a daemon and the app on one machine share one chain/identity/DB.
/// `QUANTA_DATA_DIR` overrides it (isolated probe/multi-node runs on one machine:
/// two processes must never share one live DB + node identity).
pub fn default_data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("QUANTA_DATA_DIR") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("quanta-protocol")
}

/// Open libSQL (creating the data dir + migrating the legacy name) and restore any
/// persisted state into `state`. Call before establishing an identity or starting
/// the network. Never panics — a DB failure degrades to in-memory/local mode.
pub async fn open_db(state: &Arc<AppState>, data_dir: PathBuf) {
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        log::warn!("◈ [Quanta] data dir {:?} non créé: {}", data_dir, e);
    }
    // Auto-migrate from the legacy DB name (pre-rebrand).
    let legacy_path = data_dir.join("swe_titan.db");
    let db_path = data_dir.join("quanta.db");
    if legacy_path.exists() && !db_path.exists() {
        let _ = std::fs::rename(&legacy_path, &db_path);
        log::info!("◈ [Quanta] Migrated DB: swe_titan.db → quanta.db");
    }
    match storage::db::Database::new(&db_path).await {
        Ok(database) => {
            p2p::state_persistence::restore_state(state, &database).await;
            *state.db.lock().await = Some(database);
            log::info!("◈ [Quanta] libSQL initialized at {:?}", db_path);
        }
        Err(e) => log::error!("◈ [Quanta] DB init failed: {}", e),
    }
}

/// Start the Iroh QUIC endpoint and spawn every background task (gossip drain/
/// dispatch, hello/ping liveness, peer cleanup, auto-reconnect, persistence, and —
/// when `mine` is set — the mining loop). Call after [`open_db`] (and, for the
/// daemon, after establishing an identity). Never panics.
pub async fn start_network(state: &Arc<AppState>, mine: bool) {
    match state.node.init_endpoint().await {
        Ok(()) => log::info!("◈ [Quanta] Iroh QUIC endpoint active"),
        Err(e) => log::warn!("◈ [Quanta] P2P offline: {} (local mode)", e),
    }
    p2p::gossip_tasks::spawn_outgoing_drain(state.clone());
    p2p::gossip_tasks::spawn_incoming_dispatch(state.clone());
    p2p::gossip_tasks::spawn_hello_broadcast(state.clone());
    p2p::gossip_tasks::spawn_ping_broadcast(state.clone());
    p2p::gossip_tasks::spawn_peer_cleanup(state.clone());
    p2p::gossip_tasks::spawn_auto_reconnect(state.clone());
    if mine {
        p2p::mining_loop::spawn(state.clone());
    }
    p2p::state_persistence::spawn_persistence(state.clone());
}

/// Full boot for the desktop app: open the DB, then start the network. (The app
/// establishes its wallet interactively via the UI, so no identity is set here —
/// the mining loop idles until the user unlocks.) `mine` gates block production.
pub async fn bootstrap(state: &Arc<AppState>, data_dir: PathBuf, mine: bool) {
    open_db(state, data_dir).await;
    start_network(state, mine).await;
}

/// Establish the node's signing identity.
///
/// - `Some(password)` → open a **persistent wallet** from the DB: unlock an existing
///   one, or create a new one (Ed25519 + ML-DSA primary). The node can then hold
///   funds, mine to a stable address, and sign its own sends. Requires [`open_db`]
///   to have run first.
/// - `None` → an **ephemeral in-memory** ML-DSA identity: the node participates in
///   gossip but holds no funds (a watch / relay node).
pub async fn establish_wallet(state: &Arc<AppState>, password: Option<&str>) -> Result<(), String> {
    match password {
        None => state.crypto.lock().await.generate_pq_identity().map(|_| ()),
        Some(pw) => {
            let has_wallet = {
                let db = state.db.lock().await;
                match db.as_ref() {
                    Some(d) => d.get_active_keypair().await.ok().flatten().is_some(),
                    None => return Err("DB not open (call open_db before establish_wallet)".into()),
                }
            };
            if has_wallet {
                crate::commands::identity::unlock_wallet(state, pw).await.map(|_| ())
            } else {
                crate::commands::identity::create_wallet(state, "quanta-node", pw).await.map(|_| ())
            }
        }
    }
}
