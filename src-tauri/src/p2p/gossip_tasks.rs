//! Gossip background tasks — extracted from lib.rs for clarity.
//!
//! Three async tasks:
//!   1. Outgoing gossip drain (mpsc → iroh-gossip broadcast)
//!   2. Incoming gossip dispatch (iroh-gossip events → dispatcher)
//!   3. Initial Hello broadcast (country + watts announcement)

use crate::p2p;
use crate::AppState;
use std::sync::Arc;

/// Spawn the outgoing gossip drain task.
/// Reads from the internal mpsc channel and broadcasts via iroh-gossip.
pub fn spawn_outgoing_drain(state: Arc<AppState>) {
    let token = state.node.shutdown.clone();
    tokio::spawn(async move {
        let Some(mut rx) = state.node.take_gossip_receiver().await else { return; };
        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    log::info!("◈ [Gossip] outgoing drain shutdown");
                    break;
                }
                msg = rx.recv() => {
                    let Some(env) = msg else { break; };
                    let bytes = serde_json::to_vec(&env).unwrap_or_default();
                    let len = bytes.len();
                    let topic_sender = state.node.gossip_topic_sender.read().await.clone();
                    if let Some(sender) = topic_sender {
                        if let Err(e) = sender.broadcast(bytes.into()).await {
                            log::warn!("◈ [Gossip] broadcast failed: {}", e);
                            continue;
                        }
                    }
                    let mut g = state.node.gossip.write().await;
                    g.stats.messages_sent += 1;
                    g.stats.bytes_sent += len as u64;
                    log::debug!("◈ [Gossip] outgoing {} bytes id={}", len, &env.id[..env.id.len().min(12)]);
                }
            }
        }
    });
}

/// Spawn the incoming gossip dispatch task.
/// Reads iroh-gossip events and routes them through the dispatcher.
pub fn spawn_incoming_dispatch(state: Arc<AppState>) {
    tokio::spawn(async move {
        use futures_util::StreamExt as _;
        let Some(mut rx) = state.node.take_gossip_topic_receiver().await else { return; };
        while let Some(event) = rx.next().await {
            match event {
                Ok(iroh_gossip::api::Event::Received(msg)) => {
                    p2p::dispatcher::dispatch_incoming(&state, &msg.content).await;
                }
                Ok(iroh_gossip::api::Event::NeighborUp(id)) => {
                    log::info!("◈ [Gossip] NeighborUp {}", id);
                }
                Ok(iroh_gossip::api::Event::NeighborDown(id)) => {
                    log::info!("◈ [Gossip] NeighborDown {}", id);
                }
                Ok(iroh_gossip::api::Event::Lagged) => {
                    log::warn!("◈ [Gossip] receiver lagged — messages dropped");
                }
                Err(e) => {
                    log::warn!("◈ [Gossip] event error: {}", e);
                }
            }
        }
    });
}

/// Spawn the periodic Hello broadcast (every 60s).
/// Announces this node's country, watts, and contribution data to the network.
/// The first broadcast fires after a 5s delay to let the endpoint initialize.
pub fn spawn_hello_broadcast(state: Arc<AppState>) {
    let token = state.node.shutdown.clone();
    tokio::spawn(async move {
        // Initial delay to let Iroh endpoint initialize
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

        loop {
            // Build and broadcast Hello
            if let Err(e) = broadcast_hello_once(&state).await {
                log::warn!("◈ [Gossip] Hello broadcast failed: {}", e);
            }

            // Wait 60s or shutdown
            tokio::select! {
                _ = token.cancelled() => {
                    log::info!("◈ [Gossip] Hello broadcast shutdown");
                    break;
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(60)) => {}
            }
        }
    });
}

/// Immediately broadcast a Hello. Called after connect_peer so the new peer
/// discovers us and can trigger chain sync without waiting for the 60s cycle.
pub async fn trigger_hello_now(state: &AppState) {
    if let Err(e) = broadcast_hello_once(state).await {
        log::warn!("◈ [Gossip] immediate Hello failed: {}", e);
    }
}

/// Build and send one Hello envelope. Extracted for reuse.
async fn broadcast_hello_once(state: &AppState) -> Result<(), String> {
    let pk = state.crypto.lock().await.get_identity()
        .map(|i| i.public_key_hex)
        .map_err(|e| e.to_string())?;

    let country = p2p::energy::EnergyOracle::detect_country().to_string();
    let watts = p2p::energy::estimate_watts();

    // Update peer_info with liveness data
    {
        let mut info = state.node.peer_info.write().await;
        let entry = info.entry(pk.to_string()).or_insert_with(|| {
            p2p::PeerInfo::new(watts, country.to_string())
        });
        entry.watts = watts;
        entry.country = country.to_string();
        entry.touch();
    }

    // Register country for energy oracle
    *state.node.peer_country_reports.write().await
        .entry(country.to_string()).or_insert(0) += 1;

    let heads = state.node.dag.read().await.heads();
    let tasks_completed = state.node.marketplace.read().await.completed_by(&pk);
    let uptime_min = state.node.reputation.read().await
        .get_user(&pk).map(|u| u.uptime_minutes).unwrap_or(0);
    let chain_height = state.node.ledger.read().await.chain_height();

    let msg = p2p::gossip::GossipRouter::build_hello(
        heads, pk.clone(), watts, country.clone(),
        tasks_completed, 0, uptime_min, chain_height,
    );

    // Sign and broadcast
    let timestamp = chrono::Utc::now().to_rfc3339();
    let nonce = state.node.gossip.read().await.next_outgoing_nonce();
    let signable = p2p::gossip::GossipRouter::signable_envelope_bytes(&pk, nonce, &timestamp, &msg);
    let sig = state.crypto.lock().await.sign(&signable).unwrap_or_default();
    if let Ok(env) = p2p::gossip::GossipRouter::build_signed_envelope(pk, msg, nonce, timestamp, &sig) {
        state.node.gossip.write().await.mark_seen(&env.id);
        let _ = state.node.gossip_tx.send(env);
    }
    log::info!("◈ [Gossip] Hello broadcast (country={}, watts={:.0}W)", country, watts);
    Ok(())
}

/// Spawn the dead-peer cleanup task (every 30s).
pub fn spawn_peer_cleanup(state: Arc<AppState>) {
    let token = state.node.shutdown.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    log::info!("◈ [Gossip] peer cleanup shutdown");
                    break;
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(30)) => {
                    state.node.cleanup_dead_peers().await;
                }
            }
        }
    });
}
