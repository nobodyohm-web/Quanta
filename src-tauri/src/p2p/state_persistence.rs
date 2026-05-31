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
//! ## V3.3 — Coverage des 6 moteurs sociaux
//!
//! En plus des 6 stores V2 (`ledger`, `reputation`, `dag`, `consensus`, `gossip`,
//! `pages`), on persiste désormais les 6 moteurs V3 (`domains`, `search`,
//! `social`, `moderation`, `forums`, `follow_graph`). Sans cette étape, redémarrer
//! le nœud faisait perdre l'état social — bloquant pour la réelle production.

use crate::p2p;
use crate::storage::db::Database;
use crate::AppState;
use std::sync::Arc;

/// How often the persistence loop runs.
const PERSIST_INTERVAL_SECS: u64 = 30;

/// Keys we snapshot, in stable order.
/// V2 (6) + V3 (6) = 12 stores persistés.
const KEYS: [&str; 12] = [
    "ledger",
    "reputation",
    "dag",
    "consensus",
    "gossip",
    "pages",
    // V3 social web
    "domains",
    "search",
    "social",
    "moderation",
    "forums",
    "follow_graph",
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
                ledger_json, rep_json, dag_json, cons_json, gos_json, pages_json,
                domains_json, search_json, social_json, mod_json, forums_json, follow_json,
            ) = tokio::join!(
                snapshot_ledger(&state),
                snapshot_reputation(&state),
                snapshot_dag(&state),
                snapshot_consensus(&state),
                snapshot_gossip(&state),
                snapshot_pages(&state),
                snapshot_domains(&state),
                snapshot_search(&state),
                snapshot_social(&state),
                snapshot_moderation(&state),
                snapshot_forums(&state),
                snapshot_follow_graph(&state),
            );
            let snapshots = [
                ledger_json, rep_json, dag_json, cons_json, gos_json, pages_json,
                domains_json, search_json, social_json, mod_json, forums_json, follow_json,
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
                let db_guard = state.db.lock().await;
                if let Some(db) = db_guard.as_ref() {
                    match db.save_states(&dirty).await {
                        Ok(()) => log::debug!(
                            "◈ [Quanta] persisted {} dirty keys in 1 tx{}",
                            dirty.len(),
                            if should_exit { " (final save)" } else { "" }
                        ),
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

async fn snapshot_dag(state: &AppState) -> String {
    let snap = state.node.dag.read().await.snapshot();
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

async fn snapshot_pages(state: &AppState) -> String {
    let snap = state.node.page_store.read().await.snapshot();
    serde_json::to_string(&snap).unwrap_or_default()
}

// ── V3 stores ────────────────────────────────────────────────────────────────

async fn snapshot_domains(state: &AppState) -> String {
    let snap = state.node.domains.read().await.snapshot();
    serde_json::to_string(&snap).unwrap_or_default()
}

async fn snapshot_search(state: &AppState) -> String {
    let snap = state.node.search.read().await.snapshot();
    serde_json::to_string(&snap).unwrap_or_default()
}

async fn snapshot_social(state: &AppState) -> String {
    let snap = state.node.social.read().await.snapshot();
    serde_json::to_string(&snap).unwrap_or_default()
}

async fn snapshot_moderation(state: &AppState) -> String {
    let snap = state.node.moderation.read().await.snapshot();
    serde_json::to_string(&snap).unwrap_or_default()
}

async fn snapshot_forums(state: &AppState) -> String {
    let snap = state.node.forums.read().await.snapshot();
    serde_json::to_string(&snap).unwrap_or_default()
}

async fn snapshot_follow_graph(state: &AppState) -> String {
    // FollowGraph est un type alias `HashMap<String, Vec<String>>`,
    // sérialisable directement.
    let g = state.node.follow_graph.read().await;
    serde_json::to_string(&*g).unwrap_or_default()
}

/// Restore all engine state from the database on startup.
pub async fn restore_state(state: &AppState, database: &Database) {
    // ── Ledger (with chain integrity verification) ────────────────
    if let Ok(Some(json)) = database.load_state("ledger").await {
        if let Ok(snap) = serde_json::from_str::<p2p::ledger::LedgerSnapshot>(&json) {
            let restored = p2p::ledger::Ledger::restore(snap);
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

    // ── Reputation ────────────────────────────────────────────────
    if let Ok(Some(json)) = database.load_state("reputation").await {
        if let Ok(snap) = serde_json::from_str::<p2p::reputation::ReputationSnapshot>(&json) {
            let user_count = snap.users.len();
            *state.node.reputation.write().await = p2p::reputation::ReputationEngine::restore(snap);
            log::info!("◈ [Quanta] Reputation restored ({} users)", user_count);
        }
    }

    // ── Merkle-DAG ────────────────────────────────────────────────
    if let Ok(Some(json)) = database.load_state("dag").await {
        if let Ok(snap) = serde_json::from_str::<p2p::merkle_dag::DagSnapshot>(&json) {
            let count = snap.nodes.len();
            *state.node.dag.write().await = p2p::merkle_dag::MerkleDAG::restore(snap);
            log::info!("◈ [Quanta] DAG restored ({} nodes)", count);
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

    // ── Pages P2P Web ─────────────────────────────────────────────
    if let Ok(Some(json)) = database.load_state("pages").await {
        if let Ok(snap) = serde_json::from_str::<p2p::page_store::PageStoreSnapshot>(&json) {
            let count = snap.pages.len();
            *state.node.page_store.write().await = p2p::page_store::PageStore::restore(snap);
            log::info!("◈ [Quanta] Pages restored ({} pages)", count);
        }
    }

    // ── V3 — Domains (Harberger registry) ─────────────────────────
    if let Ok(Some(json)) = database.load_state("domains").await {
        if let Ok(snap) = serde_json::from_str::<p2p::domains::DomainRegistrySnapshot>(&json) {
            let count = snap.records.len();
            *state.node.domains.write().await = p2p::domains::DomainRegistry::restore(snap);
            log::info!("◈ [V3] Domains restored ({} records)", count);
        }
    }

    // ── V3 — Search index ─────────────────────────────────────────
    if let Ok(Some(json)) = database.load_state("search").await {
        if let Ok(snap) = serde_json::from_str::<p2p::search::SearchIndexSnapshot>(&json) {
            let count = snap.docs.len();
            *state.node.search.write().await = p2p::search::SearchIndex::restore(snap);
            log::info!("◈ [V3] Search index restored ({} docs)", count);
        }
    }

    // ── V3 — Social (likes, follows, tips, boosts) ────────────────
    if let Ok(Some(json)) = database.load_state("social").await {
        if let Ok(snap) = serde_json::from_str::<p2p::social::SocialSnapshot>(&json) {
            let pages = snap.pages.len();
            let creators = snap.creators.len();
            *state.node.social.write().await = p2p::social::SocialState::restore(snap);
            log::info!("◈ [V3] Social restored ({} pages, {} creators)", pages, creators);
        }
    }

    // ── V3 — Moderation (cases + jury verdicts) ───────────────────
    if let Ok(Some(json)) = database.load_state("moderation").await {
        if let Ok(snap) = serde_json::from_str::<p2p::moderation::ModerationSnapshot>(&json) {
            let count = snap.cases.len();
            *state.node.moderation.write().await = p2p::moderation::ModerationEngine::restore(snap);
            log::info!("◈ [V3] Moderation restored ({} cases)", count);
        }
    }

    // ── V3 — Forums (DAG threads + comments) ──────────────────────
    if let Ok(Some(json)) = database.load_state("forums").await {
        if let Ok(snap) = serde_json::from_str::<p2p::forums::ForumsSnapshot>(&json) {
            let f = snap.forums.len();
            let t = snap.threads.len();
            let c = snap.comments.len();
            *state.node.forums.write().await = p2p::forums::ForumsEngine::restore(snap);
            log::info!("◈ [V3] Forums restored ({} forums, {} threads, {} comments)", f, t, c);
        }
    }

    // ── V3 — Follow graph (Web of Trust) ──────────────────────────
    if let Ok(Some(json)) = database.load_state("follow_graph").await {
        if let Ok(g) = serde_json::from_str::<p2p::trust_graph::FollowGraph>(&json) {
            let n = g.len();
            *state.node.follow_graph.write().await = g;
            log::info!("◈ [V3] Follow graph restored ({} accounts)", n);
        }
    }
}
