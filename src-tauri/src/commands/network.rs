//! Network commands — node status, per-peer metrics, display name, contribution
//! mode, connection ticket, peer connect, and the security-posture snapshot.

use crate::p2p;
use crate::security;
use crate::AppState;
use std::sync::Arc;

#[tauri::command]
pub async fn get_node_status(state: tauri::State<'_, Arc<AppState>>) -> Result<p2p::NodeStatus, String> {
    Ok(state.node.get_status().await)
}

/// NET-9 + NET-10: Snapshot of per-peer network metrics for the frontend.
/// Returns a sorted list (best quality first) of peers with their RTT, byte
/// counts, message counts, loss ratio and 0-100 quality score.
#[tauri::command]
pub async fn get_peer_metrics(
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
pub async fn set_display_name(
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
pub async fn get_display_name(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Option<String>, String> {
    Ok(state.display_name.read().await.clone())
}

/// Returns the current node contribution mode based on real-time CPU watts.
#[tauri::command]
pub async fn get_node_mode() -> Result<serde_json::Value, String> {
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

#[tauri::command]
pub async fn get_security_audit(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let cbom = security::crypto_agility::CryptoBOM::current();
    let node = state.node.get_status().await;
    let has_id = state.crypto.lock().await.get_identity().is_ok();
    Ok(serde_json::json!({
        "pq_pending": cbom.pq_migration_count(),
        "signing": cbom.signing,
        "key_exchange": cbom.key_exchange,
        "transport_auth": cbom.transport_auth,
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

/// Phase 4 — connecte ce nœud à un peer via son EndpointId Iroh.
/// Le peer doit aussi être abonné au topic QUANTA pour que le sync démarre.
/// After successful connection, immediately broadcasts a Hello so chain sync begins.
#[tauri::command]
pub async fn connect_peer(state: tauri::State<'_, Arc<AppState>>, peer_id: String) -> Result<(), String> {
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
