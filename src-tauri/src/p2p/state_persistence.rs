//! State persistence — extracted from lib.rs for clarity.
//!
//! Periodically snapshots all engine state to SQLite (every 30s).
//! Also handles restoring state from DB on startup.
//!
//! ## Optimisations (vs. the naive sequential version)
//!
//! 1. **Parallel snapshots** — all engine snapshots are taken concurrently via
//!    `tokio::join!`. They acquire independent read locks so the contention cost
//!    is roughly `max(t_i)` instead of `sum(t_i)`.
//! 2. **Dirty-flag via hash** — each snapshot is hashed (BLAKE3); we only write
//!    the keys whose hash changed since the previous tick. On idle nodes this
//!    skips the DB round-trip entirely.
//! 3. **Single SQLite transaction** — the writes that survive the dirty-flag
//!    are committed atomically through `Database::save_states`, replacing N
//!    individual fsyncs with one.
//!
//! ## Crypto-core stores
//!
//! On persiste les stores du cœur crypto : `ledger`, `reputation`, `consensus`,
//! `gossip` et le registre d'identité `usernames`.

use crate::p2p;
use crate::storage::db::Database;
use crate::AppState;
use std::sync::Arc;

/// How often the persistence loop runs.
const PERSIST_INTERVAL_SECS: u64 = 30;

/// Keys we snapshot, in stable order.
const KEYS: [&str; 6] = [
    "ledger",
    "reputation",
    "consensus",
    "gossip",
    // Identité — registre de pseudos @handle
    "usernames",
    // C1 (AUDIT-2026-07-25) — mémo anti-slashing : le vote exact déjà signé pour
    // chaque époque cible. Une base de protection qu'un redémarrage oublie ne
    // protège de rien : un nœud qui reboote sur une frontière d'époque
    // re-dériverait son vote et pourrait s'auto-équivoquer contre celui qu'il a
    // émis avant l'arrêt — sanction : brûlage intégral de son enjeu.
    "finality_cast_memo",
];

/// Spawn the periodic persistence task (every 30 seconds).
/// On shutdown, performs one final save to ensure no data loss.
pub fn spawn_persistence(state: Arc<AppState>) {
    let token = state.node.shutdown.clone();
    tokio::spawn(async move {
        // last_hashes[i] is None until we've persisted KEYS[i] at least once.
        let mut last_hashes: [Option<[u8; 32]>; KEYS.len()] = [None; KEYS.len()];

        loop {
            let should_exit = tokio::select! {
                _ = token.cancelled() => true,
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(PERSIST_INTERVAL_SECS)) => false,
            };

            // ── 1. Snapshot all engines in parallel (independent read locks) ──
            let (
                ledger_json, rep_json, cons_json, gos_json, usernames_json, memo_json,
            ) = tokio::join!(
                snapshot_ledger(&state),
                snapshot_reputation(&state),
                snapshot_consensus(&state),
                snapshot_gossip(&state),
                snapshot_usernames(&state),
                snapshot_cast_memo(&state),
            );
            let snapshots = [
                ledger_json, rep_json, cons_json, gos_json, usernames_json, memo_json,
            ];

            // ── 2. Compute hashes; keep only the entries that changed ──
            let mut dirty: Vec<(&str, &str)> = Vec::new();
            for (i, json) in snapshots.iter().enumerate() {
                let h = *blake3::hash(json.as_bytes()).as_bytes();
                if last_hashes[i] != Some(h) {
                    last_hashes[i] = Some(h);
                    dirty.push((KEYS[i], json));
                }
            }

            if dirty.is_empty() && !should_exit {
                log::trace!("◈ [Quanta] persistence: nothing changed, skipping");
                continue;
            }

            // ── 3. One transaction covers every dirty key ──
            if !dirty.is_empty() {
                let bytes: usize = dirty.iter().map(|(_, j)| j.len()).sum();
                let save_t = std::time::Instant::now();
                let db_guard = state.db.lock().await;
                if let Some(db) = db_guard.as_ref() {
                    match db.save_states(&dirty).await {
                        Ok(()) => {
                            log::debug!(
                                "◈ [Quanta] persisted {} dirty keys in 1 tx{}",
                                dirty.len(),
                                if should_exit { " (final save)" } else { "" }
                            );
                            // Télémétrie moteur : l'écriture disque RÉELLE du
                            // snapshot (états modifiés, octets, durée mesurée) —
                            // le battement de cœur 30 s du nœud, best-effort.
                            if let Some(handle) = state.app_handle.read().await.as_ref() {
                                use tauri::Emitter;
                                let _ = handle.emit(
                                    "quanta://engine",
                                    serde_json::json!({
                                        "kind": "persist",
                                        "keys": dirty.len(),
                                        "bytes": bytes,
                                        "ms": save_t.elapsed().as_millis() as u64,
                                    }),
                                );
                            }
                        }
                        Err(e) => log::warn!("◈ [Quanta] persistence batch failed: {}", e),
                    }
                }
            }

            if should_exit {
                log::info!("◈ [Persistence] graceful shutdown complete");
                break;
            }
        }
    });
}

async fn snapshot_ledger(state: &AppState) -> String {
    let snap = state.node.ledger.read().await.snapshot();
    serde_json::to_string(&snap).unwrap_or_default()
}

async fn snapshot_reputation(state: &AppState) -> String {
    let snap = state.node.reputation.read().await.snapshot();
    serde_json::to_string(&snap).unwrap_or_default()
}

async fn snapshot_consensus(state: &AppState) -> String {
    let snap = state.node.consensus.read().await.snapshot();
    serde_json::to_string(&snap).unwrap_or_default()
}

async fn snapshot_gossip(state: &AppState) -> String {
    let snap = state.node.gossip.read().await.snapshot();
    serde_json::to_string(&snap).unwrap_or_default()
}

async fn snapshot_usernames(state: &AppState) -> String {
    let snap = state.node.usernames.read().await.snapshot();
    serde_json::to_string(&snap).unwrap_or_default()
}

/// C1 — the slashing-protection memo. Small (one vote per epoch we voted in) and
/// cheap to serialise, but the difference between a validator that survives a
/// restart at an epoch boundary and one that burns its whole stake.
async fn snapshot_cast_memo(state: &AppState) -> String {
    let votes = state.node.finality.read().await.cast_memo_votes();
    serde_json::to_string(&votes).unwrap_or_default()
}

/// Restore all engine state from the database on startup.
pub async fn restore_state(state: &AppState, database: &Database) {
    // ── Ledger (with chain integrity verification) ────────────────
    if let Ok(Some(json)) = database.load_state("ledger").await {
        if let Ok(snap) = serde_json::from_str::<p2p::ledger::LedgerSnapshot>(&json) {
            let restored = p2p::ledger::Ledger::restore(snap);
            // GENESIS-V4 guard: a snapshot from an earlier chain (v3 or before)
            // has a different genesis hash. Loading it would boot the node on an
            // incompatible chain that every v5 peer rejects. The genesis hash IS
            // the chain's identity (no separate version field needed): if the
            // restored genesis ≠ the v4 genesis this build produces, discard the
            // snapshot and start fresh from the v4 genesis (the default already in
            // `state.node.ledger`).
            let v4_genesis = p2p::ledger::Ledger::new()
                .chain
                .first()
                .map(|b| b.hash.clone());
            let restored_genesis = restored.chain.first().map(|b| b.hash.clone());
            if restored_genesis != v4_genesis {
                log::warn!(
                    "◈ [Quanta] Snapshot d'une genèse antérieure (pré-v4) — ignoré, démarrage sur la genèse v4"
                );
            } else {
                match restored.verify_chain() {
                    Ok((vb, vt)) => {
                        log::info!("◈ [Quanta] Ledger restored & verified ({} blocks, {} txs)", vb, vt);
                        *state.node.ledger.write().await = restored;
                    }
                    Err(e) => {
                        log::error!("◈ [Quanta] Ledger CORRUPTED — starting fresh: {}", e);
                    }
                }
            }
        }
    }

    // ── Reputation ────────────────────────────────────────────────
    if let Ok(Some(json)) = database.load_state("reputation").await {
        if let Ok(snap) = serde_json::from_str::<p2p::reputation::ReputationSnapshot>(&json) {
            let user_count = snap.users.len();
            *state.node.reputation.write().await = p2p::reputation::ReputationEngine::restore(snap);
            log::info!("◈ [Quanta] Reputation restored ({} users)", user_count);
        }
    }

    // ── CRDT Consensus ────────────────────────────────────────────
    if let Ok(Some(json)) = database.load_state("consensus").await {
        if let Ok(snap) = serde_json::from_str::<p2p::consensus::ConsensusSnapshot>(&json) {
            let n_accounts = snap.ledger.balances.len();
            *state.node.consensus.write().await = p2p::consensus::ConsensusEngine::restore(snap);
            log::info!("◈ [Quanta] CRDT consensus restored ({} accounts)", n_accounts);
        }
    }

    // ── Gossip Router ─────────────────────────────────────────────
    if let Ok(Some(json)) = database.load_state("gossip").await {
        if let Ok(snap) = serde_json::from_str::<p2p::gossip::GossipRouterSnapshot>(&json) {
            let seen = snap.seen_messages.len();
            *state.node.gossip.write().await = p2p::gossip::GossipRouter::restore(snap);
            log::info!("◈ [Quanta] Gossip router restored ({} seen msgs)", seen);
        }
    }

    // ── Identité — registre de pseudos @handle ────────────────────
    if let Ok(Some(json)) = database.load_state("usernames").await {
        if let Ok(snap) = serde_json::from_str::<p2p::username::UsernameRegistrySnapshot>(&json) {
            let count = snap.records.len();
            *state.node.usernames.write().await = p2p::username::UsernameRegistry::restore(snap);
            log::info!("◈ [identité] Pseudos restaurés ({} @handle)", count);
        }
    }

    // ── C1 (AUDIT-2026-07-25) — mémo anti-slashing ────────────────
    // Restauré AVANT que la boucle de minage ne puisse voter. Sans lui, un nœud
    // qui redémarre sur une frontière d'époque re-dérive son vote depuis l'état
    // vivant et peut en signer un DIFFÉRENT pour la même époque cible — une
    // double-signature prouvable par n'importe quel pair, sanctionnée par le
    // brûlage intégral de son enjeu bondé et en déverrouillage.
    if let Ok(Some(json)) = database.load_state("finality_cast_memo").await {
        if let Ok(votes) = serde_json::from_str::<Vec<crate::sm::finality_vote::Vote>>(&json) {
            let count = votes.len();
            state.node.finality.write().await.restore_cast_memo(votes);
            log::info!(
                "◈ [Finality] Mémo anti-slashing restauré ({} époque(s) déjà votée(s))",
                count
            );
        }
    }
}
