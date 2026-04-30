//! Protocole de gossip P2P — échange d'état entre nœuds SOVA
//!
//! Les messages gossip sont transportés via Iroh QUIC (existant).
//! Ce module définit les types de messages et la sérialisation.
//!
//! Flux de synchronisation (delta sync) :
//!   1. A → B : `Hello { heads: [...] }`
//!   2. B → A : `WantNodes { missing: [...] }` (nœuds que B ne connaît pas)
//!   3. A → B : `HaveNodes { nodes: [...] }` (les nœuds demandés)
//!   4. B insère les nœuds dans son DAG local

use serde::{Deserialize, Serialize};
use crate::p2p::merkle_dag::DagNode;

// ─── Messages gossip ────────────────────────────────────────────────────────

/// Identifiant unique d'un message (BLAKE3 hex du contenu).
pub type MsgId = String;

/// Enveloppe signée d'un message gossip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipEnvelope {
    pub id:        MsgId,
    /// Clé publique Ed25519 de l'émetteur
    pub sender:    String,
    pub payload:   GossipMessage,
    /// Signature Ed25519 du payload sérialisé
    pub signature: String,
    /// Horodatage RFC3339 (fenêtre ±5 min pour anti-replay)
    pub timestamp: String,
}

/// Types de messages gossip.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum GossipMessage {
    /// Annonce initiale : "voici mes heads + ma consommation énergie".
    Hello {
        heads:      Vec<String>,
        node_id:    String,
        version:    u8,
        watts:      f64,    // watts CPU mesurés (V2 proportional mining)
        country:    String, // code pays ISO (oracle énergie réseau)
    },
    /// "Donne-moi les nœuds avec ces IDs".
    WantNodes {
        ids: Vec<String>,
    },
    /// "Voici les nœuds que tu m'as demandés".
    HaveNodes {
        nodes: Vec<DagNode>,
    },
    /// Diffusion d'une nouvelle transaction ATN.
    BroadcastTx {
        tx_json: String,
    },
    /// Ping/pong pour mesurer la latence et signaler qu'on est en ligne.
    Ping { nonce: u64 },
    Pong { nonce: u64 },
    /// Signalement d'un pair malveillant (votes Sybil, signature invalide…).
    ReportPeer {
        peer_id: String,
        reason:  ReportReason,
    },
}

/// Raison d'un signalement de pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReportReason {
    InvalidSignature,
    SybilSuspected,
    MalformedMessage,
    TxReplay,
    Other(String),
}

// ─── Routeur gossip ─────────────────────────────────────────────────────────

/// Statistiques de gossip pour le monitoring.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct GossipStats {
    pub messages_sent:     u64,
    pub messages_received: u64,
    pub nodes_synced:      u64,
    pub peers_reported:    u64,
    pub bytes_sent:        u64,
    pub bytes_received:    u64,
}

/// Routeur gossip — gère les messages entrants et sortants.
pub struct GossipRouter {
    /// Hashes des messages déjà traités (anti-replay)
    seen_messages: std::collections::HashSet<MsgId>,
    pub stats: GossipStats,
}

impl GossipRouter {
    pub fn new() -> Self {
        Self {
            seen_messages: std::collections::HashSet::new(),
            stats: GossipStats::default(),
        }
    }

    /// Marque un message comme vu.
    /// Retourne `false` si le message était déjà connu (duplicate/replay).
    pub fn mark_seen(&mut self, msg_id: &str) -> bool {
        self.seen_messages.insert(msg_id.to_string())
    }

    /// Vérifie la fenêtre temporelle d'un message (±5 min).
    pub fn is_fresh(timestamp: &str) -> bool {
        let Ok(ts) = chrono::DateTime::parse_from_rfc3339(timestamp) else {
            return false;
        };
        let drift = (chrono::Utc::now().timestamp() - ts.timestamp()).unsigned_abs();
        drift <= 300
    }

    /// Construit un message `Hello` pour initier un sync (V2 : inclut watts + pays).
    pub fn build_hello(heads: Vec<String>, node_id: String, watts: f64, country: String) -> GossipMessage {
        GossipMessage::Hello { heads, node_id, version: 1, watts, country }
    }

    /// Calcule les IDs manquants par rapport à `our_known`.
    pub fn compute_want(
        their_heads: &[String],
        our_known: &std::collections::HashSet<String>,
    ) -> Vec<String> {
        their_heads.iter()
            .filter(|id| !our_known.contains(*id))
            .cloned()
            .collect()
    }

    /// Phase 3 — construit une enveloppe signée prête à être broadcastée.
    /// Le caller fournit `sig_bytes` (Ed25519 du payload sérialisé) ; cet appel
    /// reste synchrone pour rester utilisable depuis tout contexte.
    pub fn wrap_outgoing(
        sender: String,
        payload: GossipMessage,
        sig_bytes: &[u8],
    ) -> Result<GossipEnvelope, String> {
        let bytes = serde_json::to_vec(&payload).map_err(|e| e.to_string())?;
        let id = hex::encode(blake3::hash(&bytes).as_bytes());
        let timestamp = chrono::Utc::now().to_rfc3339();
        Ok(GossipEnvelope {
            id,
            sender,
            payload,
            signature: hex::encode(sig_bytes),
            timestamp,
        })
    }

    /// Phase 3 — sérialise la même charge utile que `wrap_outgoing` aurait signée,
    /// pour permettre au caller de produire la signature à la bonne granularité.
    pub fn payload_bytes(payload: &GossipMessage) -> Vec<u8> {
        serde_json::to_vec(payload).unwrap_or_default()
    }
}

impl Default for GossipRouter {
    fn default() -> Self { Self::new() }
}

// ─── Snapshot sérialisable ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipRouterSnapshot {
    pub seen_messages: std::collections::HashSet<MsgId>,
    pub stats: GossipStats,
}

impl GossipRouter {
    pub fn snapshot(&self) -> GossipRouterSnapshot {
        GossipRouterSnapshot {
            seen_messages: self.seen_messages.clone(),
            stats: self.stats.clone(),
        }
    }
    pub fn restore(snap: GossipRouterSnapshot) -> Self {
        Self {
            seen_messages: snap.seen_messages,
            stats: snap.stats,
        }
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_seen_deduplication() {
        let mut router = GossipRouter::new();
        assert!(router.mark_seen("msg_001"), "Premier message doit être accepté");
        assert!(!router.mark_seen("msg_001"), "Doublon doit être rejeté");
    }

    #[test]
    fn is_fresh_rejects_old() {
        let old_ts = "2020-01-01T00:00:00Z";
        assert!(!GossipRouter::is_fresh(old_ts), "Vieux timestamp doit être refusé");
    }

    #[test]
    fn compute_want_finds_missing() {
        let mut known = std::collections::HashSet::new();
        known.insert("abc".into());

        let theirs = vec!["abc".into(), "def".into(), "ghi".into()];
        let want = GossipRouter::compute_want(&theirs, &known);

        assert_eq!(want.len(), 2);
        assert!(want.contains(&"def".to_string()));
        assert!(want.contains(&"ghi".to_string()));
    }
}
