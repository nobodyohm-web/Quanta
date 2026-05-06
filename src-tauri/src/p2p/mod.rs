// p2p/mod.rs — Willow Protocol P2P Layer (QUANTA)

pub mod willow_node;
pub mod reputation;
pub mod ledger_types;
pub mod ledger;
// Phase 2B : Consensus Merkle-CRDT
pub mod merkle_dag;
pub mod consensus;
pub mod gossip;
// Phase 2C : Oracle énergie réelle
pub mod energy;
// Phase 2D : Anti-sybil PoC
pub mod sybil;
// Phase 3 : Shapley Value distribution
pub mod shapley;
// Phase 3 : Marketplace de calcul distribué
pub mod marketplace;
// Phase 4 : Dispatcher des messages gossip entrants
pub mod dispatcher;
// Mining loop (extracted from lib.rs)
pub mod mining_loop;
// State persistence and restoration (extracted from lib.rs)
pub mod state_persistence;
// Gossip background tasks (extracted from lib.rs)
pub mod gossip_tasks;
// P2P Web publishing — pages associées aux wallets
pub mod page_store;
// V3 Social Web — modules de la nouvelle vision Torus
pub mod domains;
pub mod search;
pub mod social;
pub mod moderation;
pub mod forums;
pub mod trust_graph;
// Simulation réseau
#[cfg(test)]
mod simulation;
// Phase B : Security testing framework (S1-S12 + property tests)
#[cfg(test)]
mod security_tests;
#[cfg(test)]
mod integration_test;

use serde::{Deserialize, Serialize};
use std::time::Instant;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatus {
    pub node_id: String,
    pub is_online: bool,
    pub peer_count: u32,
    pub active_subspaces: usize,
    pub protocol: String,
    pub puzzle_difficulty: u8,
}

// ─── B3: Peer Liveness Info ─────────────────────────────────────────────────

/// Metadata about a connected peer. Includes liveness tracking (last_seen)
/// for dead-peer cleanup (B3) and energy contribution data (watts, country).
/// STRUCT-6: now also carries Shapley contribution data (tasks/blocks/uptime).
pub struct PeerInfo {
    /// Real-time watts measured by this peer (for Shapley proportional mining)
    pub watts: f64,
    /// ISO country code (for energy oracle network average)
    pub country: String,
    /// STRUCT-6: tâches compute complétées par ce peer
    pub tasks_completed: u64,
    /// STRUCT-6: blocs DAG vérifiés par ce peer
    pub blocks_verified: u64,
    /// STRUCT-6: minutes d'uptime déclarées par ce peer
    pub uptime_minutes: u64,
    /// Last time we received a valid message from this peer.
    /// Updated on every valid Hello. Used by `cleanup_dead_peers()`.
    #[allow(dead_code)]
    last_seen: Instant,
}

impl PeerInfo {
    pub fn new(watts: f64, country: String) -> Self {
        Self {
            watts,
            country,
            tasks_completed: 0,
            blocks_verified: 0,
            uptime_minutes: 0,
            last_seen: Instant::now(),
        }
    }

    /// Update last_seen to now (peer is alive).
    pub fn touch(&mut self) {
        self.last_seen = Instant::now();
    }

    /// Duration since last valid message from this peer.
    pub fn elapsed(&self) -> std::time::Duration {
        self.last_seen.elapsed()
    }
}
