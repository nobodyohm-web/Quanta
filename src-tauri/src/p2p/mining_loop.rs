//! Mining loop — extracted from lib.rs for clarity.
//!
//! Runs every 60 seconds:
//!   1. Collect peer contributions for Shapley distribution
//!   2. Mine QTA proportional to energy + contribution
//!   3. Record in Merkle-DAG
//!   4. Broadcast to peers via gossip
//!   5. Periodically seal blocks
//!
//! ## Lock ordering
//!
//! Per-tick locks are taken in this order, each scoped tightly so writers
//! never overlap with subsequent readers:
//!
//!   crypto (lock) → peer_info (read) → reputation (write)
//!     → ledger (write) → consensus (write) → dag (write)
//!     → gossip (write) → crypto (lock for sign)
//!
//! Breaking the order would risk deadlock with the dispatcher path
//! (`dispatch_incoming` → ledger.write or consensus.write).

use crate::p2p;
use crate::AppState;
use std::sync::Arc;
use std::time::Instant;

/// Mining interval (one tick per minute).
const MINE_INTERVAL_SECS: u64 = 60;

/// Wall-clock secs depuis l'epoch — utilisé pour les fenêtres glissantes
/// (anti-troll 30j, expiration des grants). On l'isole dans un helper testable.
fn tick_start_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
/// How many ticks between forced block seals (2 min = 2 ticks × 60s).
const SEAL_EVERY_N_TICKS: u32 = 2;
/// Threshold above which we log a slow-tick warning (lock contention indicator).
const SLOW_TICK_MS: u128 = 100;

/// Spawns the mining loop as a background task.
/// Returns immediately; the loop runs until shutdown is signalled.
pub fn spawn(state: Arc<AppState>) {
    let token = state.node.shutdown.clone();
    tokio::spawn(async move {
        let mut tick: u32 = 0;

        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    log::info!("◈ [Mining] graceful shutdown");
                    break;
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(MINE_INTERVAL_SECS)) => {
                    if let Err(e) = mine_tick(&state, &mut tick).await {
                        log::error!("◈ [Mining] tick error (recovering): {}", e);
                    }
                }
            }
        }
    });
}

/// Execute one mining tick. Returns Err on any failure; the caller logs and retries.
async fn mine_tick(state: &Arc<AppState>, tick: &mut u32) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tick_start = Instant::now();

    let pk = state.crypto.lock().await.get_identity()
        .map(|id| id.public_key_hex)
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;

    // ── 1. Read inputs (peer contributions + validation count) ──
    let peer_contribs = collect_peer_contributions(state).await;
    let blocks_verified = state.node.blocks_validated
        .load(std::sync::atomic::Ordering::Relaxed);

    // ── 2. Compute mining reward via Shapley (reputation write) ──
    let (raw_uqta, kwh) = state.node.reputation.write().await
        .uptime_tick(&pk, blocks_verified, &peer_contribs);

    // ── 2b. V3.3 — Anti-troll : multiplie par le facteur dérivé du compteur de
    //         reports modération validés sur 30j. Auteur clean → 1.0 ; troll → 0.0.
    let anti_troll = {
        let mod_engine = state.node.moderation.read().await;
        let n_reports = mod_engine.validated_count_30d(&pk, tick_start_secs());
        p2p::moderation::anti_troll_mining_factor(n_reports)
    };
    let after_troll = (raw_uqta as f64 * anti_troll) as u64;
    let uqta_mined = after_troll.min(p2p::reputation::EMISSION_PER_TICK);
    if anti_troll < 1.0 {
        log::warn!(
            "◈ [Anti-troll] mining réduit ×{:.2} ({} reports validés/30j)",
            anti_troll,
            state.node.moderation.read().await.validated_count_30d(&pk, tick_start_secs())
        );
    }

    // ── 3. Apply to ledger AND capture the freshly-built tx in one scope ──
    let mining_tx = state.node.ledger.write().await.mine_tx(&pk, uqta_mined, kwh);

    // ── 4. Mirror into CRDT dual-ledger (only if non-zero) ──────
    if uqta_mined > 0 {
        state.node.consensus.write().await
            .ledger.credit("network", &pk, uqta_mined);
    }

    // ── 5. Record in Merkle-DAG (single write scope, parents read inline) ──
    let payload = serde_json::to_vec(&serde_json::json!({
        "type": "mining",
        "pk": &pk,
        "atn": uqta_mined as f64 / p2p::ledger::MICRO as f64,
        "uqta": uqta_mined,
        "kwh": kwh,
        "model": "fixed_100h",
    })).unwrap_or_default();
    {
        let mut dag = state.node.dag.write().await;
        let parents = dag.heads();
        let parents = if parents.is_empty() { vec!["genesis".into()] } else { parents };
        let dag_node = p2p::merkle_dag::DagNode::new(parents, payload, pk.clone());
        let _ = dag.insert(dag_node);
    }

    // ── 6. Broadcast mining TX (uses captured tx, no ledger re-read) ──
    broadcast_mining_tx(state, &pk, &mining_tx).await;

    *tick = tick.wrapping_add(1);

    // ── 7. PoS: Seal blocks only if we're the elected leader ────
    // Mining TXs accumulate in pending for ALL nodes, but only the
    // PoS-elected leader for this slot actually seals them into a block.
    if tick.is_multiple_of(SEAL_EVERY_N_TICKS) {
        pos_seal_if_leader(state, &pk).await;
    }

    let elapsed = tick_start.elapsed();
    if elapsed.as_millis() > SLOW_TICK_MS {
        log::warn!(
            "◈ [Mining] slow tick: {}ms (threshold {}ms) — possible lock contention",
            elapsed.as_millis(),
            SLOW_TICK_MS
        );
    } else {
        log::trace!("◈ [Mining] tick #{} done in {}ms", tick, elapsed.as_millis());
    }

    Ok(())
}

/// Collect live peer contributions for Shapley distribution.
async fn collect_peer_contributions(
    state: &AppState,
) -> std::collections::HashMap<String, p2p::shapley::NodeContribution> {
    // V3 — snapshot social pour Shapley v2 social_utility (15%).
    let social = state.node.social.read().await.snapshot();
    let peers = state.node.peer_info.read().await;
    peers.iter()
        .filter(|(_, info)| info.elapsed() < std::time::Duration::from_secs(300))
        .map(|(pk, info)| {
            // V3.3 — vrais Σ √(amount) accumulés sur tous les votes positifs reçus
            // par cet auteur. Net = likes - dislikes, clampé à 0 (pas de score négatif).
            let weighted_likes = social
                .creators
                .get(pk)
                .map(|c| (c.weighted_likes_received - c.weighted_dislikes_received).max(0.0))
                .unwrap_or(0.0);
            (pk.clone(), p2p::shapley::NodeContribution {
                node_id: pk.clone(),
                watts: info.watts,
                tasks_completed: info.tasks_completed,
                blocks_verified: info.blocks_verified,
                uptime_minutes: info.uptime_minutes,
                mode: p2p::shapley::NodeMode::Active,
                weighted_likes,
            })
        })
        .collect()
}

/// Broadcast a mining TX captured directly from `mine_tx`.
async fn broadcast_mining_tx(
    state: &AppState,
    pk: &str,
    tx: &p2p::ledger::Transaction,
) {
    let tx_json = match serde_json::to_string(tx) {
        Ok(j) => j,
        Err(_) => return,
    };
    let msg = p2p::gossip::GossipMessage::BroadcastTx { tx_json };
    if let Some(env) = sign_and_wrap(state, pk, msg).await {
        state.node.gossip.write().await.mark_seen(&env.id);
        let _ = state.node.gossip_tx.send(env);
    }
}

/// Seal a block and broadcast it to peers.
async fn seal_and_broadcast(state: &AppState, pk: &str) {
    let sealed = state.node.ledger.write().await.seal_if_pending(pk, 0.0);
    if let Some(b) = sealed {
        log::info!("◈ [Ledger] Block #{} sealed ({} tx)", b.index, b.transactions.len());
        if let Ok(block_json) = serde_json::to_string(&b) {
            let msg = p2p::gossip::GossipMessage::NewBlock { block_json };
            if let Some(env) = sign_and_wrap(state, pk, msg).await {
                state.node.gossip.write().await.mark_seen(&env.id);
                let _ = state.node.gossip_tx.send(env);
            }
        }
    }
}

/// PoS leader election check — only seal if we're the elected proposer.
///
/// 1. Build the validator set from staked amounts + reputation scores
/// 2. Compute the elected leader for the current slot (chain height)
/// 3. Only seal if we're the primary leader (or valid fallback after timeout)
///
/// If no validators have sufficient stake, falls back to the original
/// seal_and_broadcast (permissionless mode for bootstrap).
async fn pos_seal_if_leader(state: &AppState, pk: &str) {
    // Source the election entropy from a *buried* block (LOOKBACK behind the
    // tip), not the freshly-sealed tip, so the immediate proposer can't grind
    // block contents to bias — or re-elect — themselves at the next slot.
    let (beacon, slot) = {
        let ledger = state.node.ledger.read().await;
        let height = ledger.chain_height();
        if height == 0 {
            // No chain — just seal (bootstrap)
            seal_and_broadcast(state, pk).await;
            return;
        }
        let tip_index = height - 1;
        let buried_index = tip_index.saturating_sub(p2p::pos_consensus::LEADER_ENTROPY_LOOKBACK);
        match ledger.block_at(buried_index) {
            Some(b) => (p2p::pos_consensus::leader_beacon(&b.hash, height), height),
            None => {
                seal_and_broadcast(state, pk).await;
                return;
            }
        }
    };

    // Build validator set from reputation engine (staked + trust)
    let (stakes, reputations) = {
        let rep = state.node.reputation.read().await;
        let leaderboard = rep.get_leaderboard(100);
        let mut stakes = std::collections::HashMap::new();
        let mut reps = std::collections::HashMap::new();
        for u in leaderboard {
            stakes.insert(u.public_key.clone(), u.atn_staked);
            reps.insert(u.public_key.clone(), u.trust_score as u64);
        }
        (stakes, reps)
    };

    let validators = p2p::pos_consensus::build_validator_set(&stakes, &reputations);

    // If no eligible validators (nobody has staked enough), allow permissionless sealing
    // This is the bootstrap phase — before anyone has staked, everyone can propose
    let has_eligible = validators.iter().any(|v| v.stake >= p2p::pos_consensus::MIN_VALIDATOR_STAKE);
    if !has_eligible {
        log::debug!("◈ [PoS] No eligible validators — permissionless seal (bootstrap mode)");
        seal_and_broadcast(state, pk).await;
        return;
    }

    // Check if we're the elected leader
    // elapsed_secs = how long since the last block was sealed
    let tip_time = {
        let ledger = state.node.ledger.read().await;
        ledger.block_at(ledger.chain_height() - 1)
            .and_then(|b| chrono::DateTime::parse_from_rfc3339(&b.timestamp).ok())
            .map(|t| t.timestamp() as u64)
            .unwrap_or(0)
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let elapsed = now.saturating_sub(tip_time);

    let (is_valid, is_primary) = p2p::pos_consensus::is_valid_proposer(
        pk, &beacon, slot, elapsed, &validators,
    );

    if is_valid {
        if is_primary {
            log::info!("◈ [PoS] We are the ELECTED LEADER for slot {} — sealing block", slot);
        } else {
            log::info!("◈ [PoS] We are a FALLBACK proposer for slot {} (elapsed {}s)", slot, elapsed);
        }
        seal_and_broadcast(state, pk).await;
    } else {
        log::debug!(
            "◈ [PoS] Not our turn to propose at slot {} (elapsed {}s) — waiting for leader",
            slot, elapsed
        );
    }
}

/// Sign a gossip message and wrap it in an envelope.
async fn sign_and_wrap(
    state: &AppState,
    pk: &str,
    msg: p2p::gossip::GossipMessage,
) -> Option<p2p::gossip::GossipEnvelope> {
    let timestamp = chrono::Utc::now().to_rfc3339();
    let nonce = state.node.gossip.read().await.next_outgoing_nonce();
    let signable = p2p::gossip::GossipRouter::signable_envelope_bytes(pk, nonce, &timestamp, &msg);
    let sig = state.crypto.lock().await.sign(&signable).unwrap_or_default();
    p2p::gossip::GossipRouter::build_signed_envelope(
        pk.to_string(), msg, nonce, timestamp, &sig,
    ).ok()
}
