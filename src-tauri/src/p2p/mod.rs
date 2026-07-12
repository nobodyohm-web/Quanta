// p2p/mod.rs — Willow Protocol P2P Layer (QUANTA)

pub mod willow_node;
pub mod reputation;
pub mod ledger_types;
pub mod ledger;
pub mod consensus;
pub mod gossip;
// NET-3 : Priority-aware outgoing gossip channel
pub mod gossip_priority;
// Phase 2C : Oracle énergie réelle
pub mod energy;
// Phase 2D : Anti-sybil PoC
pub mod sybil;
// Phase 3 : Shapley Value distribution
pub mod shapley;
// Phase 4 : Dispatcher des messages gossip entrants
pub mod dispatcher;
// Mining loop (extracted from lib.rs)
pub mod mining_loop;
// State persistence and restoration (extracted from lib.rs)
pub mod state_persistence;
// Gossip background tasks (extracted from lib.rs)
pub mod gossip_tasks;
// Phase 5 : Proof-of-Stake consensus with VRF leader election
pub mod pos_consensus;
// LIVE-1 — câblage vivant du gadget de finalité (gossip des votes → LatestVotes/FinalityState)
pub mod finality_live;
// LIVE-4 — réconciliation de fork profonde en vivant (l'appelant réseau de GADGET-5B)
pub mod fork_heal;
// Identité — registre P2P de pseudos uniques @handle (adresse de wallet lisible)
pub mod username;
// Simulation réseau
#[cfg(test)]
mod simulation;
// Phase B : Security testing framework (S1-S12 + property tests)
#[cfg(test)]
mod security_tests;
#[cfg(test)]
mod integration_test;
#[cfg(test)]
mod integration_tests;

use serde::{Deserialize, Serialize};
use std::time::Instant;

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod peer_info_tests {
    use super::PeerInfo;

    #[test]
    fn quality_score_is_none_until_first_pong() {
        let p = PeerInfo::new(10.0, "FR".into());
        assert_eq!(p.quality_score(), None);
    }

    #[test]
    fn quality_score_high_for_low_rtt_full_response_rate() {
        let mut p = PeerInfo::new(10.0, "FR".into());
        p.pings_sent = 100;
        // record_rtt also bumps pongs_received
        for _ in 0..100 {
            p.record_rtt(20);
        }
        // simulate >10 min uptime by backdating first_seen
        p.first_seen = std::time::Instant::now() - std::time::Duration::from_secs(900);
        let score = p.quality_score().expect("score");
        // 50 (latency) + 30 (loss=0) + 20 (uptime) = 100
        assert!(score >= 99, "expected near-perfect score, got {}", score);
    }

    #[test]
    fn quality_score_drops_with_high_loss() {
        let mut p = PeerInfo::new(10.0, "FR".into());
        p.pings_sent = 100;
        for _ in 0..50 {  // only half respond
            p.record_rtt(20);
        }
        p.first_seen = std::time::Instant::now() - std::time::Duration::from_secs(900);
        let score = p.quality_score().expect("score");
        // 50 (lat) + 15 (loss=0.5 → 30*0.5) + 20 (up) = 85
        assert!((80..=90).contains(&score),
            "expected mid-80s for 50% loss, got {}", score);
    }

    #[test]
    fn quality_score_drops_with_high_rtt() {
        let mut p = PeerInfo::new(10.0, "FR".into());
        p.pings_sent = 10;
        for _ in 0..10 {
            p.record_rtt(1000);
        }
        p.first_seen = std::time::Instant::now() - std::time::Duration::from_secs(900);
        let score = p.quality_score().expect("score");
        // 0 (latency at 1000ms) + 30 (loss=0) + 20 (uptime) = 50
        assert!((45..=55).contains(&score),
            "expected ~50 for 1000ms RTT, got {}", score);
    }

    #[test]
    fn smoothed_rtt_uses_ewma() {
        let mut p = PeerInfo::new(10.0, "FR".into());
        p.record_rtt(100);
        assert_eq!(p.smoothed_rtt_ms, Some(100));
        p.record_rtt(200);
        // (7*100 + 200) / 8 = 112
        assert_eq!(p.smoothed_rtt_ms, Some(112));
        p.record_rtt(200);
        // (7*112 + 200) / 8 = 123
        assert_eq!(p.smoothed_rtt_ms, Some(123));
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatus {
    pub node_id: String,
    /// Shareable Iroh EndpointId — the other user pastes this to connect.
    pub peer_id: String,
    pub is_online: bool,
    pub peer_count: u32,
    /// NET-1: Total number of peers we've ever connected to.
    pub known_peers: u32,
    /// NET-1: Currently connected known peers.
    pub connected_peers: u32,
    pub active_subspaces: usize,
    pub protocol: String,
    pub puzzle_difficulty: u8,
}

// ─── B3: Peer Liveness Info ─────────────────────────────────────────────────

/// Metadata about a connected peer. Includes liveness tracking (last_seen)
/// for dead-peer cleanup (B3) and energy contribution data (watts, country).
/// STRUCT-6: now also carries Shapley contribution data (tasks/blocks/uptime).
/// NET-9: now also carries network metrics (RTT, byte counters, msg counters).
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
    /// NET-9: Most recent measured RTT (ms). Set on each Pong from this peer.
    pub last_rtt_ms: Option<u64>,
    /// NET-9: Smoothed RTT via EWMA (alpha = 1/8). Less noisy for quality
    /// scoring than the raw last_rtt_ms.
    pub smoothed_rtt_ms: Option<u64>,
    /// NET-9: Total bytes received attributed to this peer (envelope size).
    pub bytes_in: u64,
    /// NET-9: Total messages received from this peer (any payload).
    pub messages_in: u64,
    /// NET-9: Pings sent to this peer's expected receiving (broadcast — increments
    /// every time we send a global Ping while this peer is in the live set).
    pub pings_sent: u64,
    /// NET-9: Pongs received back from this peer. `pings_sent - pongs_received`
    /// approximates message-loss rate for quality scoring.
    pub pongs_received: u64,
    /// NET-9: First time we ever observed this peer (for uptime ratio).
    pub first_seen: Instant,
    /// NET-15: Optional sanitised display_name advertised by this peer.
    pub display_name: Option<String>,
}

impl PeerInfo {
    pub fn new(watts: f64, country: String) -> Self {
        let now = Instant::now();
        Self {
            watts,
            country,
            tasks_completed: 0,
            blocks_verified: 0,
            uptime_minutes: 0,
            last_seen: now,
            last_rtt_ms: None,
            smoothed_rtt_ms: None,
            bytes_in: 0,
            messages_in: 0,
            pings_sent: 0,
            pongs_received: 0,
            first_seen: now,
            display_name: None,
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

    /// NET-9: Record a fresh RTT sample, updating both the raw and smoothed
    /// fields. Smoothed uses TCP-style EWMA: `srtt = (7*srtt + sample) / 8`.
    pub fn record_rtt(&mut self, sample_ms: u64) {
        self.last_rtt_ms = Some(sample_ms);
        self.smoothed_rtt_ms = Some(match self.smoothed_rtt_ms {
            None => sample_ms,
            Some(prev) => ((prev.saturating_mul(7)).saturating_add(sample_ms)) / 8,
        });
        self.pongs_received = self.pongs_received.saturating_add(1);
    }

    /// NET-10: Connection-quality score 0..=100.
    ///
    /// Components (each capped to a sub-band, summed):
    /// - 50 pts: latency (50 ms RTT → 50 pts; 1000 ms → 0 pts; linear)
    /// - 30 pts: pong / ping ratio (1.0 → 30 pts; 0.0 → 0)
    /// - 20 pts: uptime ratio (>=10 min observed → 20 pts; <60s → 0)
    ///
    /// Returns `None` until at least one Pong has been measured (no signal).
    pub fn quality_score(&self) -> Option<u8> {
        let rtt = self.smoothed_rtt_ms.or(self.last_rtt_ms)?;
        let latency_pts = if rtt <= 50 {
            50.0
        } else if rtt >= 1000 {
            0.0
        } else {
            // Linear from (50ms, 50pts) to (1000ms, 0pts).
            let ratio = (1000.0 - rtt as f64) / 950.0;
            50.0 * ratio
        };
        let loss_pts = if self.pings_sent == 0 {
            30.0
        } else {
            let ratio = (self.pongs_received as f64 / self.pings_sent as f64).min(1.0);
            30.0 * ratio
        };
        let uptime_secs = self.first_seen.elapsed().as_secs();
        let uptime_pts = if uptime_secs >= 600 {
            20.0
        } else if uptime_secs < 60 {
            0.0
        } else {
            20.0 * ((uptime_secs - 60) as f64 / 540.0)
        };
        Some((latency_pts + loss_pts + uptime_pts).round().clamp(0.0, 100.0) as u8)
    }
}
