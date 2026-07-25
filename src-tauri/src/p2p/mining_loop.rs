//! Mining loop — extracted from lib.rs for clarity.
//!
//! Runs every 60 seconds:
//!   1. Collect peer contributions for Shapley distribution
//!   2. Mine QTA proportional to energy + contribution
//!   3. Broadcast to peers via gossip
//!   4. Periodically seal blocks
//!
//! ## Lock ordering
//!
//! Per-tick locks are taken in this order, each scoped tightly so writers
//! never overlap with subsequent readers:
//!
//!   crypto (lock) → peer_info (read) → reputation (write)
//!     → ledger (write) → consensus (write)
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

    // PQ-MIG-3B / PQ-ENVELOPE-1: this node has two distinct identities.
    //  • `pk`   — ML-DSA-65 **primary public key**: the gossip envelope
    //    authentication identity (signs the outgoing envelopes, PQ-ENVELOPE-1).
    //  • `addr` — ML-DSA **address** (value identity): the mining-reward target,
    //    the CRDT credit key, the PoS proposer/seal identity (the validator set
    //    is address-keyed via `validator_stakes()`), and — since REPUT-ID-1 —
    //    the **reputation actor** identity (uptime/energy keyed by address, the
    //    economic actor).
    let (pk, addr) = {
        let crypto = state.crypto.lock().await;
        let pk = crypto.pq_identity_hex().ok_or_else(
            || -> Box<dyn std::error::Error + Send + Sync> {
                "no ML-DSA primary identity".into()
            },
        )?;
        let addr = crypto.pq_address_hex().ok_or_else(
            || -> Box<dyn std::error::Error + Send + Sync> {
                "no ML-DSA primary identity (cannot derive value address)".into()
            },
        )?;
        (pk, addr)
    };

    // ── 1. Read inputs (peer contributions + validation count) ──
    let peer_contribs = collect_peer_contributions(state).await;
    let blocks_verified = state.node.blocks_validated
        .load(std::sync::atomic::Ordering::Relaxed);

    // ── 2. Compute mining reward via Shapley (reputation write) ──
    // TOKENOMICS v2 — émission décroissante : fraction de l'offre RESTANTE vers
    // le plafond dur (100M QUANTA). Lue depuis le ledger (consensus), donc tous
    // les nœuds calculent la même émission pour ce tick.
    let total_mined = state.node.ledger.read().await.stats().total_mined;
    let emission_this_tick = p2p::reputation::emission_for_tick(total_mined);
    // Garde-fou : ne jamais dépasser l'émission décroissante de ce tick.
    // REPUT-ID-1: the local reputation actor is keyed by the ML-DSA **address**
    // (the economic actor), not the transport key. `peer_contribs` stays
    // transport-keyed — peers are network entities identified by their transport
    // pubkey (`peer_info`); Shapley sums their contribution *values*, so mixing
    // the local address into that map is harmless (keys only need to be distinct).
    let (raw_uqta, kwh) = state.node.reputation.write().await
        .uptime_tick(&addr, blocks_verified, emission_this_tick, &peer_contribs);
    let uqta_mined = raw_uqta.min(emission_this_tick);

    // ── 3. Apply to ledger AND capture the freshly-built tx in one scope ──
    // PQ-MIG-3B: the reward credits the ML-DSA **address** (`tx.to = addr`).
    let mining_tx = state.node.ledger.write().await.mine_tx(&addr, uqta_mined, kwh);

    // ── 4. Mirror into CRDT dual-ledger (only if non-zero) ──────
    if uqta_mined > 0 {
        state.node.consensus.write().await
            .ledger.credit("network", &addr, uqta_mined);
        // Live UX: tell the frontend a reward just landed (toast + 3D surge).
        // Best-effort — nobody listening is fine.
        if let Some(handle) = state.app_handle.read().await.as_ref() {
            use tauri::Emitter;
            let _ = handle.emit(
                "quanta://mined",
                serde_json::json!({
                    "amount": uqta_mined as f64 / p2p::ledger::MICRO as f64,
                    "kwh": kwh,
                    // Matière unique pour le terminal : le montant EXACT en µQTA
                    // et le hash BLAKE3 réel de la tx de récompense.
                    "amount_micro": uqta_mined,
                    "tx_hash": mining_tx.hash.clone(),
                }),
            );
        }
    }

    // ── 5. Broadcast mining TX (uses captured tx, no ledger re-read) ──
    // The envelope is signed by the Ed25519 transport key (`pk`); the tx inside
    // already credits `addr`.
    broadcast_mining_tx(state, &pk, &mining_tx).await;

    *tick = tick.wrapping_add(1);

    // ── 6. PoS: Seal blocks only if we're the elected leader ────
    // Mining TXs accumulate in pending for ALL nodes, but only the
    // PoS-elected leader for this slot actually seals them into a block.
    if tick.is_multiple_of(SEAL_EVERY_N_TICKS) {
        pos_seal_if_leader(state, &addr, &pk).await;
    }

    // ── 7. LIVE-1: cast a finality vote when the tip is on an epoch boundary ──
    // A bonded validator attests `last-justified → current-epoch-checkpoint`;
    // the vote is gossiped so peers' fork-choice + finality can consume it. A
    // non-validator / non-boundary tick produces nothing. Additive: until LIVE-2
    // wires the head into proposal, votes only *observe* finality (no chain
    // divergence for peers that ignore them).
    cast_finality_vote_if_validator(state, &pk).await;

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
    let peers = state.node.peer_info.read().await;
    peers.iter()
        .filter(|(_, info)| info.elapsed() < std::time::Duration::from_secs(300))
        .map(|(pk, info)| {
            (pk.clone(), p2p::shapley::NodeContribution {
                node_id: pk.clone(),
                watts: info.watts,
                tasks_completed: info.tasks_completed,
                blocks_verified: info.blocks_verified,
                uptime_minutes: info.uptime_minutes,
                mode: p2p::shapley::NodeMode::Active,
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
///
/// PQ-MIG-3B: `addr` (the ML-DSA **address**) is the block's miner-reward target
/// (value identity); `pk` (the Ed25519 **transport** key) signs the carrying
/// gossip envelope.
/// LIVE-1 — cast this node's finality vote if it is a bonded validator sitting on
/// an epoch boundary. Builds the honest vote (last-justified → current-epoch
/// checkpoint) via the live gadget, signs it with the ML-DSA authority key, and
/// gossips it as `FinalityVote`. Also ingests it locally (a validator counts its
/// own attestation). `pk` (Ed25519) signs the carrying envelope; the vote itself
/// is ML-DSA-signed. No-ops when there is nothing honest to attest.
async fn cast_finality_vote_if_validator(state: &AppState, pk: &str) {
    let vote = {
        // LOCK ORDER (crypto → ledger → finality → gossip). `crypto` is taken FIRST
        // so this path can never deadlock against `transfer` (which holds crypto then
        // takes ledger). A previous ordering (ledger → finality → crypto) formed a
        // cross-cycle with `transfer` and could hang the node at an epoch boundary.
        let crypto = state.crypto.lock().await;
        let ledger = state.node.ledger.read().await;
        let mut fin = state.node.finality.write().await;
        // Keep the block tree current before reading state/anchors.
        fin.observe_chain(&ledger);
        p2p::finality_live::build_vote_to_cast(&ledger, &fin, &crypto)
    };
    let Some(vote) = vote else { return };

    // Ingest locally first (self-attestation counts toward the certificate).
    let (finalized, floor) = {
        let ledger = state.node.ledger.read().await;
        let mut fin = state.node.finality.write().await;
        // C1 (AUDIT-2026-07-25): commit the vote to the slashing-protection memo
        // BEFORE anything else can run. Recorded even if the broadcast below fails
        // — that is the correct behaviour: having signed this vote, we must never
        // sign a different one for the same target epoch, whether or not anyone
        // received it.
        fin.remember_cast(&vote);
        let out = fin.ingest_vote(vote.clone(), &ledger);
        (out.finalized, fin.finalized_floor())
    };
    // LIVE-2 — if our own attestation completed a certificate that finalized a
    // checkpoint, push the floor into the ledger (fresh write lock, no nesting).
    // HIGH-4: the setter only freezes if OUR block at that height matches the
    // finalized hash.
    if finalized {
        let (h, hash) = floor;
        state.node.ledger.write().await.set_finalized_floor(h, &hash);
    }

    let Ok(vote_json) = serde_json::to_string(&vote) else {
        return;
    };
    let msg = p2p::gossip::GossipMessage::FinalityVote { vote_json };
    if let Some(env) = sign_and_wrap(state, pk, msg).await {
        state.node.gossip.write().await.mark_seen(&env.id);
        let _ = state.node.gossip_tx.send(env);
        log::info!("◈ [Finality] cast vote for epoch {}", vote.target.epoch);
        // Télémétrie moteur : NOTRE vote de finalité réel (époque + checkpoint
        // cible réels, signé ML-DSA) — best-effort, hors chemin de sécurité.
        if let Some(handle) = state.app_handle.read().await.as_ref() {
            use tauri::Emitter;
            let _ = handle.emit(
                "quanta://engine",
                serde_json::json!({
                    "kind": "vote",
                    "epoch": vote.target.epoch,
                    "height": vote.target.height,
                    "hash": p2p::ledger::short(&vote.target.hash, 16),
                }),
            );
        }
    }
}

async fn seal_and_broadcast(state: &AppState, addr: &str, pk: &str) {
    let seal_t = std::time::Instant::now();
    let sealed = state.node.ledger.write().await.seal_if_pending(addr, 0.0);
    let seal_us = seal_t.elapsed().as_micros() as u64;
    if let Some(b) = sealed {
        log::info!("◈ [Ledger] Block #{} sealed ({} tx)", b.index, b.transactions.len());
        // Live UX: block-seal pulse for the 3D scenes + toast.
        if let Some(handle) = state.app_handle.read().await.as_ref() {
            use tauri::Emitter;
            // Symétrie avec le chemin distant : la scène Réseau affiche AUSSI
            // qui a scellé nos propres blocs (notre @pseudo s'il existe).
            let miner_name = state.node.usernames.read().await.username_of(addr);
            let _ = handle.emit(
                "quanta://block-sealed",
                serde_json::json!({
                    "index": b.index,
                    "txs": b.transactions.len(),
                    "mine": true,
                    "miner": crate::p2p::ledger::short(addr, 16),
                    "miner_name": miner_name,
                    // Le VRAI hash du bloc que NOUS venons de sceller + son
                    // parent — l'enchaînement prev ← hash devient visible.
                    "hash": b.hash.clone(),
                    "prev": b.prev_hash.clone(),
                    // Durée réelle du scellement (couverture COVER-2 + Merkle
                    // BLAKE3 + hash de bloc), mesurée autour de seal_if_pending.
                    "seal_us": seal_us,
                }),
            );
        }
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
///
/// PQ-MIG-3B: `addr` (the ML-DSA address) is the value identity used for BOTH
/// the proposer election (the validator set is address-keyed via
/// `validator_stakes()`) AND the sealed block's miner reward; `pk` (Ed25519)
/// signs the carrying gossip envelope.
async fn pos_seal_if_leader(state: &AppState, addr: &str, pk: &str) {
    // Source the election entropy from a *buried* block (LOOKBACK behind the
    // tip), not the freshly-sealed tip, so the immediate proposer can't grind
    // block contents to bias — or re-elect — themselves at the next slot.
    let (beacon, slot) = {
        let ledger = state.node.ledger.read().await;
        let height = ledger.chain_height();
        if height == 0 {
            // No chain — just seal (bootstrap)
            seal_and_broadcast(state, addr, pk).await;
            return;
        }
        let tip_index = height - 1;
        let buried_index = tip_index.saturating_sub(p2p::pos_consensus::LEADER_ENTROPY_LOOKBACK);
        match ledger.block_at(buried_index) {
            Some(b) => (p2p::pos_consensus::leader_beacon(&b.hash, height), height),
            None => {
                seal_and_broadcast(state, addr, pk).await;
                return;
            }
        }
    };

    // ONCHAIN-STAKE-1 §4: source the validator set's stake from the **on-chain**
    // stake state (`Ledger::validator_stakes`), NOT the node-local reputation
    // leaderboard. The stake is now a pure function of the chain, so every node
    // computes the same validator set and the same leader — closing the fork
    // vector. Reputation is off the security path entirely (ADR-002): it no longer
    // feeds the validator set, so the reputations map is empty.
    let stakes = {
        let ledger = state.node.ledger.read().await;
        ledger.validator_stakes()
    };
    let reputations: std::collections::HashMap<String, u64> = std::collections::HashMap::new();

    let validators = p2p::pos_consensus::build_validator_set(&stakes, &reputations);

    // If no eligible validators (nobody has staked enough), allow permissionless sealing
    // This is the bootstrap phase — before anyone has staked, everyone can propose
    let has_eligible = validators.iter().any(|v| v.stake >= p2p::pos_consensus::MIN_VALIDATOR_STAKE);
    if !has_eligible {
        log::debug!("◈ [PoS] No eligible validators — permissionless seal (bootstrap mode)");
        // Télémétrie : l'élection réelle de ce slot (mode bootstrap — personne
        // n'a encore staké, scellement permissionless).
        emit_engine(state, serde_json::json!({
            "kind": "elect", "slot": slot, "verdict": "bootstrap",
        })).await;
        seal_and_broadcast(state, addr, pk).await;
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

    // PQ-MIG-3B: the proposer identity matched against the address-keyed
    // validator set is this node's ML-DSA **address**, not its transport key.
    let (is_valid, is_primary) = p2p::pos_consensus::is_valid_proposer(
        addr, &beacon, slot, elapsed, &validators,
    );

    // Télémétrie : le VERDICT réel de l'élection pondérée par l'enjeu pour ce
    // slot — élu / fallback / observateur — avec la taille du set de validateurs.
    let verdict = if is_valid { if is_primary { "leader" } else { "fallback" } } else { "observer" };
    emit_engine(state, serde_json::json!({
        "kind": "elect", "slot": slot, "verdict": verdict, "validators": validators.len(),
    })).await;

    if is_valid {
        if is_primary {
            log::info!("◈ [PoS] We are the ELECTED LEADER for slot {} — sealing block", slot);
        } else {
            log::info!("◈ [PoS] We are a FALLBACK proposer for slot {} (elapsed {}s)", slot, elapsed);
        }
        seal_and_broadcast(state, addr, pk).await;
    } else {
        log::debug!(
            "◈ [PoS] Not our turn to propose at slot {} (elapsed {}s) — waiting for leader",
            slot, elapsed
        );
    }
}

/// Best-effort engine telemetry (`quanta://engine`) — local UI only, never on
/// the security or wire path. Nobody listening is fine.
async fn emit_engine(state: &AppState, payload: serde_json::Value) {
    if let Some(handle) = state.app_handle.read().await.as_ref() {
        use tauri::Emitter;
        let _ = handle.emit("quanta://engine", payload);
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
    let kind = match &msg {
        p2p::gossip::GossipMessage::BroadcastTx { .. } => "BroadcastTx",
        p2p::gossip::GossipMessage::NewBlock { .. } => "NewBlock",
        p2p::gossip::GossipMessage::FinalityVote { .. } => "FinalityVote",
        _ => "Gossip",
    };
    let sign_t = std::time::Instant::now();
    let sig = state.crypto.lock().await.sign_pq(&signable).unwrap_or_default();
    // Durée réelle de la signature ML-DSA-65 (FIPS 204) de cette enveloppe —
    // la preuve mesurée que la crypto post-quantique tourne à chaque envoi.
    emit_engine(state, serde_json::json!({
        "kind": "sign",
        "msg": kind,
        "us": sign_t.elapsed().as_micros() as u64,
        "bytes": signable.len(),
    })).await;
    p2p::gossip::GossipRouter::build_signed_envelope(
        pk.to_string(), msg, nonce, timestamp, &sig,
    ).ok()
}
