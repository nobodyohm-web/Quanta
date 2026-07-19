//! QUANTA Protocol v1.0 — Energy-indexed Sovereign P2P Protocol.
//!
//! Architecture:
//! - `security/` — Ed25519 identity, PQ Vault, hybrid crypto
//! - `p2p/` — Gossip, ledger, reputation, consensus, mining, identity
//! - `storage/` — SQLite persistence layer
//! - `commands/` — the Tauri command surface, split by domain (identity, wallet,
//!   network, chain, diagnostics); `commands_v3` holds the `@pseudo` identity commands.
//! - `guardian` — the Rust-side freeze guardian (extracted from `setup()`)
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
mod commands;
mod guardian;

/// Shared read-model builders — pure functions that compute the view-models
/// consumed by BOTH `crate::commands` (desktop UI) and `crate::rpc` (exchange /
/// explorer integration), so the calculation lives once. See `views.rs`.
mod views;

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

use security::CryptoEngine;
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
                return Err(crate::commands::error::CmdError::RateLimited(secs).into());
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

/// Sign a `BroadcastTx` gossip envelope for `tx` with the ML-DSA-65 envelope
/// identity (PQ-ENVELOPE-1) and queue it for the network. Cross-cutting: shared
/// by every user-authored tx command (`commands::wallet`) **and** the JSON-RPC
/// send paths (`crate::rpc`), so all legs propagate identically (AUDIT-TX-2
/// taught us what happens when one leg is forgotten).
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

    // Un nœud doit vivre fenêtre cachée : sans ceci, App Nap étrangle le tick
    // de minage 60 s et les heartbeats gossip dès que la fenêtre est occluse.
    node_runtime::prevent_app_nap();

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
            // Freeze guardian (Rust-side layers the JS heartbeat cannot see).
            // See `crate::guardian`. Best-effort: never touches node state.
            guardian::spawn_freeze_guardian(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::diagnostics::ui_diag,
            commands::diagnostics::ui_beat,
            commands::diagnostics::was_guardian_reload,
            commands::identity::check_identity,
            commands::identity::create_identity,
            commands::identity::unlock_identity,
            commands::identity::get_public_key,
            commands::identity::get_recovery_key,
            commands::identity::get_recovery_phrase,
            commands::identity::restore_from_phrase,
            commands::identity::get_receive_address,
            commands::identity::validate_address,
            commands::identity::resolve_address,
            commands::identity::biometric_status,
            commands::identity::enable_biometric_unlock,
            commands::identity::disable_biometric_unlock,
            commands::identity::unlock_biometric,
            commands::network::get_node_status,
            commands::network::get_node_mode,
            commands::network::get_peer_metrics,
            commands::network::set_display_name,
            commands::network::get_display_name,
            commands::network::get_security_audit,
            commands::network::get_node_ticket,
            commands::network::connect_peer,
            commands::wallet::get_my_reputation,
            commands::wallet::get_balance,
            commands::wallet::ledger_transfer,
            commands::wallet::ledger_stake,
            commands::wallet::ledger_unstake,
            commands::wallet::get_wallet_overview,
            commands::chain::get_chain_overview,
            commands::chain::get_chain_history,
            commands::chain::get_recent_txs,
            commands::chain::get_finality_status,
            commands::chain::get_economy_stats,
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
