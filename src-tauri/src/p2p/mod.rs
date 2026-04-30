// p2p/mod.rs — Willow Protocol P2P Layer (SOVA)

pub mod willow_node;
pub mod reputation;
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
// Simulation réseau
#[cfg(test)]
mod simulation;

use serde::{Deserialize, Serialize};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatus {
    pub node_id: String,
    pub is_online: bool,
    pub peer_count: u32,
    pub active_subspaces: usize,
    pub protocol: String,
    pub puzzle_difficulty: u8,
}

