//! Gossip background tasks — extracted from lib.rs for clarity.
//!
//! Six async tasks:
//!   1. Outgoing gossip drain (priority queue → iroh-gossip broadcast)
//!   2. Incoming gossip dispatch (iroh-gossip events → dispatcher)
//!   3. NET-4: Heavy Hello broadcast (every 120s — full sync metadata)
//!   4. NET-4: Lightweight Ping broadcast (every 15s — liveness only)
//!   5. Dead-peer cleanup (every 30s)
//!   6. NET-1: Auto-reconnect loop (exponential backoff for lost peers)
//!
//! NET-4 — heartbeat split:
//! Hello carries the full state advertisement (heads, watts, country, contribs,
//! chain_height, known_peer_ids) and is expensive to sign + broadcast. Ping is
//! a 1-field payload sufficient to tell peers "I'm alive". Splitting them lets
//! us refresh liveness 8× more often without paying the heavy Hello cost each
//! time. Stale chain detection still happens via NewBlock broadcasts on seal —
//! Hello at 120s is just a lazy backstop for nodes that missed a NewBlock.

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
/// NET-1: Also handles NeighborUp/NeighborDown for auto-reconnection.
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
                    let id_str = id.to_string();
                    log::info!("◈ [Gossip] NeighborUp {}", &id_str[..id_str.len().min(16)]);
                    // NET-1: Mark peer as connected (resets backoff)
                    state.node.mark_peer_up(&id_str).await;
                    // HELLO-NEIGHBOR-1 — se présenter DÈS que le voisin existe.
                    //
                    // Sans ça, personne n'annonce rien avant le Hello périodique
                    // suivant : jusqu'à 120 s pendant lesquelles les deux nœuds sont
                    // mutuellement **invisibles** — `peer_info` vide (donc « 0 pair »
                    // affiché alors que le lien est établi), aucune comparaison de
                    // `chain_height`, donc aucune synchronisation déclenchée. Le
                    // Hello lancé juste après un dial ne comble pas le trou : il part
                    // AVANT que le voisin gossip ne soit prêt et se perd. Constaté en
                    // vivant — `NeighborUp` une seconde après le Hello, puis deux
                    // minutes de silence.
                    crate::p2p::gossip_tasks::trigger_hello_now(&state).await;
                }
                Ok(iroh_gossip::api::Event::NeighborDown(id)) => {
                    let id_str = id.to_string();
                    log::info!("◈ [Gossip] NeighborDown {}", &id_str[..id_str.len().min(16)]);
                    // NET-1: Mark peer as disconnected (triggers reconnection)
                    state.node.mark_peer_down(&id_str).await;
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

/// NET-4: Hello cadence — heavy metadata advertisement.
/// Every 120s instead of 60s — coupled with the lighter Ping at 15s,
/// total liveness signal is 8× more frequent for half the per-Hello cost.
pub const HELLO_INTERVAL_SECS: u64 = 120;

/// NET-4: Ping cadence — lightweight heartbeat (just nonce + envelope wrap).
pub const PING_INTERVAL_SECS: u64 = 15;

/// Spawn the periodic Hello broadcast (every 120s — see NET-4).
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

            // Wait HELLO_INTERVAL_SECS or shutdown
            tokio::select! {
                _ = token.cancelled() => {
                    log::info!("◈ [Gossip] Hello broadcast shutdown");
                    break;
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(HELLO_INTERVAL_SECS)) => {}
            }
        }
    });
}

/// NET-4: Spawn the lightweight Ping heartbeat task.
/// Every 15s, broadcast a `Ping { nonce }` envelope. Receiving peers
/// touch our `peer_info.last_seen` so dead-peer cleanup stays accurate
/// without forcing a full Hello round-trip.
pub fn spawn_ping_broadcast(state: Arc<AppState>) {
    let token = state.node.shutdown.clone();
    tokio::spawn(async move {
        // Stagger initial Ping by 7s (between Hello's 5s init and the first
        // 60-something-second tick) so the first heartbeat fires fast and
        // doesn't collide with the initial Hello.
        tokio::time::sleep(tokio::time::Duration::from_secs(7)).await;

        let mut nonce: u64 = 1;
        loop {
            if let Err(e) = broadcast_ping_once(&state, nonce).await {
                log::warn!("◈ [Gossip] Ping broadcast failed: {}", e);
            }
            // Wrap nonce safely — only used as a request/response correlator,
            // not anti-replay (gossip envelope nonce handles that).
            nonce = nonce.wrapping_add(1);

            tokio::select! {
                _ = token.cancelled() => {
                    log::info!("◈ [Gossip] Ping broadcast shutdown");
                    break;
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(PING_INTERVAL_SECS)) => {}
            }
        }
    });
}

/// Build + sign + broadcast a single Ping envelope. Cheap relative to Hello
/// (1 u64 payload vs Hello's heads/peers/watts/country/contribs).
///
/// NET-9: Records the Ping nonce + send instant in `pending_pings` so the
/// matching Pong handler can compute RTT. Also bumps `pings_sent` on every
/// currently-known peer so the loss-rate denominator stays accurate.
async fn broadcast_ping_once(state: &AppState, nonce: u64) -> Result<(), String> {
    // PQ-ENVELOPE-1: the envelope sender + signature identity is the ML-DSA-65
    // primary public key (post-quantum), no longer the Ed25519 transport key.
    let pk = state.crypto.lock().await.pq_identity_hex()
        .ok_or_else(|| "no ML-DSA primary identity".to_string())?;

    let msg = crate::p2p::gossip::GossipMessage::Ping { nonce };

    let timestamp = chrono::Utc::now().to_rfc3339();
    let env_nonce = state.node.gossip.read().await.next_outgoing_nonce();
    let signable = crate::p2p::gossip::GossipRouter::signable_envelope_bytes(
        &pk, env_nonce, &timestamp, &msg
    );
    let sig = state.crypto.lock().await.sign_pq(&signable).unwrap_or_default();
    if let Ok(env) = crate::p2p::gossip::GossipRouter::build_signed_envelope(
        pk, msg, env_nonce, timestamp, &sig
    ) {
        state.node.gossip.write().await.mark_seen(&env.id);

        // NET-9: Record the Ping for RTT measurement.
        let send_instant = std::time::Instant::now();
        {
            let mut pending = state.node.pending_pings.write().await;
            pending.insert(nonce, send_instant);
            // Hard cap — drop oldest by sweeping if over.
            if pending.len() > crate::p2p::willow_node::MAX_PENDING_PINGS {
                let cutoff = std::time::Instant::now()
                    - std::time::Duration::from_secs(600);
                pending.retain(|_, sent_at| *sent_at > cutoff);
            }
        }
        // Bump pings_sent for every known peer (denominator of loss ratio).
        {
            let mut info = state.node.peer_info.write().await;
            for entry in info.values_mut() {
                entry.pings_sent = entry.pings_sent.saturating_add(1);
            }
        }

        let _ = state.node.gossip_tx.send(env);
    }
    Ok(())
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
    // PQ-ENVELOPE-1: `pk` = ML-DSA-65 primary public key — the envelope
    // authentication identity (signs the envelope, keys `peer_info`, and is the
    // Hello `node_id` label; connections still use the Iroh endpoint_id, untouched).
    // `addr` = ML-DSA **address** — the reputation actor identity (REPUT-ID-1),
    // under which the mining loop accrues this node's uptime.
    let (pk, addr) = {
        let crypto = state.crypto.lock().await;
        let pk = crypto.pq_identity_hex().ok_or_else(|| "no ML-DSA primary identity".to_string())?;
        let addr = crypto.pq_address_hex().unwrap_or_default();
        (pk, addr)
    };

    let country = p2p::energy::EnergyOracle::detect_country().to_string();
    let watts = p2p::energy::estimate_watts();

    // PEER-SELF-1 — le nœud ne s'inscrit **PAS** dans `peer_info`.
    //
    // Il le faisait, et `peer_info` est la carte des PAIRS : sa taille est lue
    // comme « nombre de pairs » à quatre endroits, qui mentaient tous d'un
    // cran — mais surtout, deux d'entre eux en tiraient des conséquences :
    //
    //  • `willow_node::status` → `getinfo.peers` et `get_node_status` : un nœud
    //    totalement seul affichait **« 1 pair »**, donc « connecté au réseau »
    //    alors qu'il ne parlait à personne. Le même compteur alimente l'écran
    //    Réseau (`get_peer_metrics`), qui listait donc l'utilisateur lui-même.
    //  • `rendezvous.rs` → la **cadence d'amorçage adaptative** : `live_peers`
    //    n'était jamais nul, donc le nœud passait immédiatement en croisière
    //    25 min au lieu de réessayer toutes les 30 s tant qu'il est seul. Une
    //    seule fenêtre de découverte manquée (typiquement la première, avant
    //    d'avoir publié son propre carnet) devenait une panne de 25 minutes —
    //    exactement l'angle mort que RDV-1 était censé fermer. Constaté en
    //    vivant : deux nœuds frais sur la même DHT ne se sont jamais trouvés.
    //  • `dispatcher.rs` → le plafond de débit adaptatif (NET-13), décalé d'un.
    //  • `mining_loop::collect_peer_contributions` → Shapley comptait le nœud
    //    **deux fois** (ici sous sa clé de transport, et via `my_contrib` sous
    //    son adresse). Sans conséquence monétaire depuis MINT-EXACT-1, mais
    //    c'était une dilution réelle du partage avant.
    //
    // Le pays est enregistré séparément juste en dessous (`peer_country_reports`),
    // donc l'oracle énergie ne perd rien.

    // Register country for energy oracle
    *state.node.peer_country_reports.write().await
        .entry(country.to_string()).or_insert(0) += 1;

    // Crypto-core: no DAG / marketplace — heads stay empty and tasks_completed 0.
    let heads: Vec<String> = Vec::new();
    let tasks_completed = 0u64;
    // REPUT-ID-1: read the local uptime under the **address** (reputation actor).
    let uptime_min = state.node.reputation.read().await
        .get_user(&addr).map(|u| u.uptime_minutes).unwrap_or(0);
    let chain_height = state.node.ledger.read().await.chain_height();

    // NET-2: Collect known peer EndpointIds for mesh discovery
    let known_peer_ids: Vec<String> = {
        let kp = state.node.known_peers.read().await;
        kp.values()
            .filter(|p| p.connected) // Only share peers that are currently connected
            .map(|p| p.endpoint_id.clone())
            .collect()
    };

    // NET-15: read optional display_name from settings (None for now — wired
    // up so the frontend can populate it later via a Tauri command).
    let display_name = state.display_name.read().await.clone();

    let msg = p2p::gossip::GossipRouter::build_hello(
        heads, pk.clone(), watts, country.clone(),
        tasks_completed, 0, uptime_min, chain_height,
        known_peer_ids, display_name,
    );

    // Sign and broadcast
    let timestamp = chrono::Utc::now().to_rfc3339();
    let nonce = state.node.gossip.read().await.next_outgoing_nonce();
    let signable = p2p::gossip::GossipRouter::signable_envelope_bytes(&pk, nonce, &timestamp, &msg);
    let sig = state.crypto.lock().await.sign_pq(&signable).unwrap_or_default();
    if let Ok(env) = p2p::gossip::GossipRouter::build_signed_envelope(pk, msg, nonce, timestamp, &sig) {
        state.node.gossip.write().await.mark_seen(&env.id);
        let _ = state.node.gossip_tx.send(env);
    }
    log::info!("◈ [Gossip] Hello broadcast (country={}, watts={:.0}W)", country, watts);
    Ok(())
}

/// Spawn the dead-peer cleanup task (every 30s).
/// NET-12: Also runs the eclipse-attack heuristic on every cleanup tick.
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
                    // NET-14: Prune expired or excess pending transactions.
                    state.node.ledger.write().await.prune_mempool();
                    if let Some(prefix) = state.node.check_eclipse_risk().await {
                        log::warn!(
                            "◈ [NET-12] ECLIPSE WARNING: >{}% of peers share pubkey prefix '{}…' \
                             — possible Sybil/eclipse attempt; consider reseeding peers manually",
                            (crate::p2p::willow_node::ECLIPSE_THRESHOLD * 100.0) as u32,
                            prefix
                        );
                    }
                }
            }
        }
    });
}

/// NET-1: Spawn the auto-reconnection task.
/// Checks every 5s for disconnected known peers and attempts reconnection
/// with exponential backoff (1s → 2s → 4s → 8s → ... → 60s max, 10 attempts max).
pub fn spawn_auto_reconnect(state: Arc<AppState>) {
    let token = state.node.shutdown.clone();
    tokio::spawn(async move {
        // Wait for endpoint to be ready
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    log::info!("◈ [NET-1] auto-reconnect shutdown");
                    break;
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => {
                    let peers = state.node.peers_needing_reconnect().await;
                    for (endpoint_id, backoff) in peers {
                        log::info!(
                            "◈ [NET-1] Reconnecting to {} (backoff {:?})",
                            &endpoint_id[..endpoint_id.len().min(16)],
                            backoff
                        );
                        // Wait the backoff delay before attempting
                        tokio::time::sleep(backoff).await;

                        match state.node.try_reconnect(&endpoint_id).await {
                            Ok(true) => {
                                // Trigger immediate Hello after reconnection
                                trigger_hello_now(&state).await;
                            }
                            Ok(false) => {} // Skipped (already connected or exhausted)
                            Err(_) => {} // Logged inside try_reconnect
                        }
                    }
                }
            }
        }
    });
}
