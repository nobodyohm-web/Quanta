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

pub struct AppState {
    pub crypto: Mutex<CryptoEngine>,
    pub db: Mutex<Option<Database>>,
    pub node: WillowNode,
    /// NET-15: Optional human-readable display name embedded in our outgoing
    /// Hello messages. The wallet's Ed25519 signature on the envelope already
    /// authenticates the name; sanitisation happens at receive time.
    pub display_name: tokio::sync::RwLock<Option<String>>,
    /// NET-16: Cached Tauri AppHandle, populated during `.setup()`. Used by
    /// background tasks (e.g. chain-sync handlers) to emit progress events
    /// to the frontend without holding a handle through every call site.
    pub app_handle: tokio::sync::RwLock<Option<tauri::AppHandle>>,
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
    let db = state.db.lock().await;
    db.as_ref().ok_or("DB not ready")?
        .store_keypair(&pk_bytes, &enc_sk, &nonce, &display_name)
        .await?;
    Ok(id)
}

#[tauri::command]
async fn unlock_identity(
    state: tauri::State<'_, Arc<AppState>>, password: String,
) -> Result<security::pq_vault::QuantaIdentity, String> {
    let db = state.db.lock().await;
    let kp = db.as_ref().ok_or("DB not ready")?.get_active_keypair().await?.ok_or("No identity")?;
    let mut engine = state.crypto.lock().await;
    PQVault::unlock_identity(
        &mut engine,
        &kp.public_key, &kp.encrypted_secret_key, &kp.nonce,
        &password,
        &kp.display_name,
        &kp.created_at,
    )
}

#[tauri::command]
async fn get_public_key(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
    state.crypto.lock().await.get_identity().map(|i| i.public_key_hex)
}

#[tauri::command]
async fn get_recovery_key(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
    let engine = state.crypto.lock().await;
    let secret = engine.get_secret_bytes()?;
    // Format: 8 groups of 8 hex chars (64 chars total = 32 bytes Ed25519 secret)
    let hex = hex::encode(&secret);
    let formatted: Vec<&str> = (0..8).map(|i| &hex[i*8..(i+1)*8]).collect();
    Ok(formatted.join("-"))
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
    let pk = state.crypto.lock().await.get_identity().map(|i| i.public_key_hex).unwrap_or_default();
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
    let my_pk = state.crypto.lock().await.get_identity().map(|i| i.public_key_hex).unwrap_or_default();
    let uqta = quanta_to_uqta(amount)?;
    state.node.reputation.write().await.transfer(&my_pk, &to_pk, uqta)
}

#[tauri::command]
async fn stake_atn(state: tauri::State<'_, Arc<AppState>>, amount: f64) -> Result<f64, String> {
    let my_pk = state.crypto.lock().await.get_identity().map(|i| i.public_key_hex).unwrap_or_default();
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

    // PoC score de l'identité courante (si présente)
    let pk = state.crypto.lock().await.get_identity().map(|i| i.public_key_hex).unwrap_or_default();
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
    // V7: Input validation
    if to.len() != 64 || hex::decode(&to).is_err() {
        return Err("Adresse destinataire invalide".into());
    }
    if amount <= 0.0 || amount > 1_000_000.0 {
        return Err("Montant invalide (0 < x ≤ 1 000 000)".into());
    }
    let uqta = quanta_to_uqta(amount)?;
    let crypto = state.crypto.lock().await;
    let from = crypto.get_identity()?.public_key_hex;
    let mut ledger = state.node.ledger.write().await;
    let (tx, burn_tx, burn_uqta) = ledger.transfer_with_burn(&from, &to, uqta, &crypto)?;
    let net_uqta = uqta - burn_uqta;
    drop(ledger);
    let _ = state.node.reputation.write().await.transfer(&from, &to, net_uqta);
    // Phase 3 — double-ledger : refléter le transfert net dans le PN-Counter CRDT (µQTA).
    {
        let mut cons = state.node.consensus.write().await;
        cons.ledger.debit(&from, &from, net_uqta);
        cons.ledger.credit(&from, &to, net_uqta);
    }

    // ── CRITICAL: Broadcast both legs (transfer + burn) so peers' ledgers
    //   stay aligned with ours. AUDIT-TX-2: a previous bug only sent the
    //   transfer leg, leaving every other node with a 1% balance gap.
    for tx_obj in std::iter::once(&tx).chain(burn_tx.as_ref()) {
        if let Ok(tx_json) = serde_json::to_string(tx_obj) {
            let msg = p2p::gossip::GossipMessage::BroadcastTx { tx_json };
            let timestamp = chrono::Utc::now().to_rfc3339();
            let nonce = state.node.gossip.read().await.next_outgoing_nonce();
            let signable = p2p::gossip::GossipRouter::signable_envelope_bytes(&from, nonce, &timestamp, &msg);
            let sig = crypto.sign(&signable).unwrap_or_default();
            if let Ok(env) = p2p::gossip::GossipRouter::build_signed_envelope(
                from.clone(), msg, nonce, timestamp, &sig,
            ) {
                state.node.gossip.write().await.mark_seen(&env.id);
                let _ = state.node.gossip_tx.send(env);
            }
        }
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

    let app_state = Arc::new(AppState {
        crypto: Mutex::new(CryptoEngine::new()),
        db: Mutex::new(None),
        node: WillowNode::new(),
        display_name: tokio::sync::RwLock::new(None),
        app_handle: tokio::sync::RwLock::new(None),
    });

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
                // ── Init Database ────────────────────────────────────
                let data_dir = dirs::data_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("quanta-protocol");
                // Auto-migrate from legacy DB name
                let legacy_path = data_dir.join("swe_titan.db");
                let db_path = data_dir.join("quanta.db");
                if legacy_path.exists() && !db_path.exists() {
                    let _ = std::fs::rename(&legacy_path, &db_path);
                    log::info!("◈ [Quanta] Migrated DB: swe_titan.db → quanta.db");
                }
                match storage::db::Database::new(&db_path).await {
                    Ok(database) => {
                        // Restore all persisted state
                        p2p::state_persistence::restore_state(&state, &database).await;
                        *state.db.lock().await = Some(database);
                        log::info!("◈ [Quanta] libSQL initialized at {:?}", db_path);
                    }
                    Err(e) => log::error!("◈ [Quanta] DB init failed: {}", e),
                }

                // ── Init Iroh P2P endpoint ───────────────────────────
                match state.node.init_endpoint().await {
                    Ok(()) => log::info!("◈ [Quanta] Iroh QUIC endpoint active"),
                    Err(e) => log::warn!("◈ [Quanta] P2P offline: {} (local mode)", e),
                }

                // ── Spawn background tasks ───────────────────────────
                p2p::gossip_tasks::spawn_outgoing_drain(state.clone());
                p2p::gossip_tasks::spawn_incoming_dispatch(state.clone());
                p2p::gossip_tasks::spawn_hello_broadcast(state.clone());
                p2p::gossip_tasks::spawn_ping_broadcast(state.clone());
                p2p::gossip_tasks::spawn_peer_cleanup(state.clone());
                p2p::gossip_tasks::spawn_auto_reconnect(state.clone());
                p2p::mining_loop::spawn(state.clone());
                p2p::state_persistence::spawn_persistence(state.clone());
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            check_identity, create_identity, unlock_identity, get_public_key, get_recovery_key,
            get_node_status, get_node_mode,
            get_peer_metrics, get_network_topology,
            set_display_name, get_display_name,
            get_security_audit,
            get_node_ticket, connect_peer,
            get_my_reputation, transfer_atn, stake_atn, get_trust_leaderboard,
            get_energy_stats, get_consensus_stats, get_network_health,
            get_ledger_stats, get_chain_overview, get_chain_history, get_recent_txs, get_balance, ledger_transfer,
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
