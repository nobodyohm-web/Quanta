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

/// The default on-disk data directory — the **same** location the desktop app
/// uses, so a daemon and the app on one machine share one chain/identity/DB.
pub fn default_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("quanta-protocol")
}

/// Boot the node: open libSQL (restoring any persisted state), start the Iroh QUIC
/// endpoint, and spawn all background tasks (gossip drain/dispatch, hello/ping,
/// peer cleanup, auto-reconnect, persistence, and — when `mine` is set — the mining
/// loop). Shared verbatim by the app and the daemon. Never panics: a failed DB or
/// endpoint degrades to local/offline mode, exactly like the app.
///
/// `mine` gates block production: the desktop app passes `true`; a watch/relay
/// daemon passes `false` (it syncs and serves queries without producing blocks).
pub async fn bootstrap(state: &Arc<AppState>, data_dir: PathBuf, mine: bool) {
    // ── Database ────────────────────────────────────────────────────────────
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        log::warn!("◈ [Quanta] data dir {:?} non créé: {}", data_dir, e);
    }
    // Auto-migrate from the legacy DB name (pre-rebrand), same as the app.
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

    // ── Iroh P2P endpoint ───────────────────────────────────────────────────
    match state.node.init_endpoint().await {
        Ok(()) => log::info!("◈ [Quanta] Iroh QUIC endpoint active"),
        Err(e) => log::warn!("◈ [Quanta] P2P offline: {} (local mode)", e),
    }

    // ── Background tasks ────────────────────────────────────────────────────
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
