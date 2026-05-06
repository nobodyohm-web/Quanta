//! Protocole de gossip P2P — échange d'état entre nœuds QUANTA
//!
//! Les messages gossip sont transportés via Iroh QUIC (existant).
//! Ce module définit les types de messages et la sérialisation.
//!
//! Flux de synchronisation (delta sync) :
//!   1. A → B : `Hello { heads: [...] }`
//!   2. B → A : `WantNodes { missing: [...] }` (nœuds que B ne connaît pas)
//!   3. A → B : `HaveNodes { nodes: [...] }` (les nœuds demandés)
//!   4. B insère les nœuds dans son DAG local

use crate::p2p::merkle_dag::DagNode;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

// ─── Protocol versioning (NET-5) ─────────────────────────────────────────────

/// NET-5: Current Torus wire protocol version. Embedded in every Hello.
/// Bump on any breaking change to message types or signing semantics.
///
/// Versioning policy:
/// - Receiving a Hello with `version > TORUS_PROTOCOL_VERSION` → log a warning
///   (peer is newer than us) but keep processing what we understand.
/// - Receiving a Hello with `version < TORUS_PROTOCOL_VERSION` → log a debug
///   line; we still accept legacy peers because all new fields are
///   `#[serde(default)]`.
/// - Unknown payload variants always deserialize as a parse error and are
///   silently dropped at the JSON layer, satisfying the "skip unknown
///   messages gracefully" requirement.
pub const TORUS_PROTOCOL_VERSION: u8 = 2;

// ─── Messages gossip ────────────────────────────────────────────────────────

/// Identifiant unique d'un message (BLAKE3 hex du contenu).
pub type MsgId = String;

/// Enveloppe signée d'un message gossip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipEnvelope {
    pub id: MsgId,
    /// Clé publique Ed25519 de l'émetteur
    pub sender: String,
    pub payload: GossipMessage,
    /// Signature Ed25519 du payload sérialisé
    pub signature: String,
    /// Horodatage RFC3339 (fenêtre ±90s pour anti-replay)
    pub timestamp: String,
    /// CRIT-1: Per-sender monotonic nonce for gossip-level anti-replay.
    /// Must be strictly increasing per sender. Default 0 for backward compat.
    #[serde(default)]
    pub nonce: u64,
}

/// Types de messages gossip.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum GossipMessage {
    /// Annonce initiale : "voici mes heads + ma consommation énergie + contributions".
    /// NET-5: `version` carries `TORUS_PROTOCOL_VERSION`. Peers on different versions
    /// keep talking — unknown fields fall back to defaults.
    Hello {
        heads: Vec<String>,
        node_id: String,
        #[serde(default = "default_protocol_version")]
        version: u8,
        watts: f64,      // watts CPU mesurés (V2 proportional mining)
        country: String, // code pays ISO (oracle énergie réseau)
        // STRUCT-6: Shapley contribution data — backward compat via #[serde(default)]
        #[serde(default)]
        tasks_completed: u64,
        #[serde(default)]
        blocks_verified: u64,
        #[serde(default)]
        uptime_minutes: u64,
        /// Chain height — so peers can detect if they need to sync.
        #[serde(default)]
        chain_height: u64,
        /// NET-2: Known peer EndpointIds for mesh discovery.
        /// Receiving nodes can auto-connect to peers they don't know.
        #[serde(default)]
        known_peer_ids: Vec<String>,
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
    Ping {
        nonce: u64,
    },
    Pong {
        nonce: u64,
    },
    /// Signalement d'un pair malveillant (votes Sybil, signature invalide…).
    ReportPeer {
        peer_id: String,
        reason: ReportReason,
    },
    /// D1: Propagation d'un bloc sealed. Les pairs le valident puis l'intègrent.
    NewBlock {
        block_json: String,
    },
    /// Late-joiner: request chain segments starting from a given block height.
    /// The receiving peer responds with a ChainSegment.
    RequestChain {
        from_height: u64,
        /// Maximum number of blocks to send back (prevents DoS).
        max_blocks: u64,
    },
    /// Response to RequestChain: a contiguous segment of the chain.
    ChainSegment {
        blocks_json: Vec<String>,
        /// Total chain height of the sender (so requester knows if more is needed).
        sender_height: u64,
    },
    /// Publication/mise à jour d'une page web P2P.
    PublishPage {
        page_json: String,
    },
    /// Requête de la page d'un wallet par clé publique.
    RequestPage {
        author_pk: String,
    },
    // ─── V3 Social Web ─────────────────────────────────────────────────
    /// V3 — Publication/mise à jour d'un enregistrement de domaine `*.torus`.
    /// Le receveur valide la signature du record avant insertion.
    PublishDomain {
        record_json: String,
    },
    /// V3 — Délégation d'un sous-domaine (signée par le parent).
    PublishSubdomain {
        grant_json: String,
    },
    /// V3 — Diffusion d'un site indexé (envoyé au moteur de recherche local).
    /// `doc_json` = `IndexedDoc` sérialisé (tokens déjà calculés côté émetteur).
    PublishSite {
        doc_json: String,
    },
    /// V3 — Action sociale signée (Vote / Follow / Tip / Boost).
    BroadcastSocialAction {
        action_json: String,
    },
    /// V3 — Signalement d'un contenu (cumulé jusqu'au seuil de jury).
    BroadcastReport {
        report_json: String,
    },
    /// V3 — Vote scellé d'un juré (commit phase).
    BroadcastJurorCommit {
        commit_json: String,
    },
    /// V3 — Révélation d'un vote de juré (reveal phase).
    BroadcastJurorReveal {
        reveal_json: String,
    },
    /// V3 — Diffusion d'un nœud forum (Forum / Thread / Comment) tagué par `kind`.
    PublishForumNode {
        kind: String, // "forum" | "thread" | "comment"
        node_json: String,
    },
    /// V3.3 — Diffusion d'un manifest de site multi-page (signé par l'auteur).
    /// Reçu → `page_store.publish_site()`. Vérifie sig + version + contraintes.
    PublishSiteManifest {
        manifest_json: String,
    },
}

/// NET-5: Default the embedded `Hello.version` field for legacy envelopes
/// that predate the protocol-version constant.
fn default_protocol_version() -> u8 {
    1
}

/// Raison d'un signalement de pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReportReason {
    InvalidSignature,
    SybilSuspected,
    MalformedMessage,
    TxReplay,
    RateLimitExceeded,
    Other(String),
}

// ─── Routeur gossip ─────────────────────────────────────────────────────────

/// Statistiques de gossip pour le monitoring.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct GossipStats {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub nodes_synced: u64,
    pub peers_reported: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    /// Messages dropped due to invalid signature
    pub dropped_signature: u64,
    /// Messages dropped due to rate limiting
    pub dropped_rate_limit: u64,
    /// Messages dropped due to nonce replay
    pub dropped_nonce: u64,
    /// P2P pages received from network
    pub pages_received: u64,
    /// P2P pages published locally
    pub pages_published: u64,
    /// V3 — Domaines publiés ou mis à jour via gossip
    #[serde(default)]
    pub domains_published: u64,
    /// V3 — Sites indexés reçus depuis le réseau
    #[serde(default)]
    pub sites_indexed: u64,
    /// V3 — Actions sociales (like/follow/tip/boost) appliquées
    #[serde(default)]
    pub social_actions_applied: u64,
    /// V3 — Reports modération acceptés
    #[serde(default)]
    pub reports_received: u64,
    /// V3 — Nœuds forum (forum/thread/comment) acceptés
    #[serde(default)]
    pub forum_nodes_received: u64,
    /// V3.3 — Manifests de site multi-page acceptés
    #[serde(default)]
    pub site_manifests_received: u64,
}

/// MOD-2: Maximum number of message IDs retained for deduplication.
/// Oldest entries are evicted once this limit is reached.
const MAX_SEEN_MESSAGES: usize = 100_000;

/// Routeur gossip — gère les messages entrants et sortants.
pub struct GossipRouter {
    /// Hashes des messages déjà traités (anti-replay)
    seen_messages: std::collections::HashSet<MsgId>,
    /// MOD-2: Insertion-order queue for bounded LRU eviction of seen_messages.
    seen_order: std::collections::VecDeque<MsgId>,
    pub stats: GossipStats,
    /// CRIT-A: Monotonic outgoing nonce — starts at 1, never 0.
    outgoing_nonce: AtomicU64,
}

impl GossipRouter {
    pub fn new() -> Self {
        Self {
            seen_messages: std::collections::HashSet::new(),
            seen_order: std::collections::VecDeque::new(),
            stats: GossipStats::default(),
            outgoing_nonce: AtomicU64::new(1),
        }
    }

    /// CRIT-A: Get and increment the outgoing nonce (atomic, no &mut needed).
    pub fn next_outgoing_nonce(&self) -> u64 {
        self.outgoing_nonce.fetch_add(1, Ordering::Relaxed)
    }

    /// Marque un message comme vu.
    /// Retourne `false` si le message était déjà connu (duplicate/replay).
    /// MOD-2: Evicts oldest entries when the seen set exceeds MAX_SEEN_MESSAGES.
    pub fn mark_seen(&mut self, msg_id: &str) -> bool {
        let is_new = self.seen_messages.insert(msg_id.to_string());
        if is_new {
            self.seen_order.push_back(msg_id.to_string());
            while self.seen_order.len() > MAX_SEEN_MESSAGES {
                if let Some(old) = self.seen_order.pop_front() {
                    self.seen_messages.remove(&old);
                }
            }
        }
        is_new
    }

    /// Number of unique message IDs currently tracked (for tests + monitoring).
    pub fn seen_messages_count(&self) -> usize {
        self.seen_messages.len()
    }

    /// Vérifie la fenêtre temporelle d'un message (±90s).
    /// Tightened from ±5min to reduce replay attack window
    /// while tolerating reasonable clock drift.
    pub fn is_fresh(timestamp: &str) -> bool {
        let Ok(ts) = chrono::DateTime::parse_from_rfc3339(timestamp) else {
            return false;
        };
        let drift = (chrono::Utc::now().timestamp() - ts.timestamp()).unsigned_abs();
        drift <= 90
    }

    /// Construit un message `Hello` pour initier un sync (V2: watts + pays + STRUCT-6 contribs).
    #[allow(clippy::too_many_arguments)]
    pub fn build_hello(
        heads: Vec<String>,
        node_id: String,
        watts: f64,
        country: String,
        tasks_completed: u64,
        blocks_verified: u64,
        uptime_minutes: u64,
        chain_height: u64,
        known_peer_ids: Vec<String>,
    ) -> GossipMessage {
        GossipMessage::Hello {
            heads,
            node_id,
            version: TORUS_PROTOCOL_VERSION,
            watts,
            country,
            tasks_completed,
            blocks_verified,
            uptime_minutes,
            chain_height,
            known_peer_ids,
        }
    }

    /// Calcule les IDs manquants par rapport à `our_known`.
    pub fn compute_want(
        their_heads: &[String],
        our_known: &std::collections::HashSet<String>,
    ) -> Vec<String> {
        their_heads
            .iter()
            .filter(|id| !our_known.contains(*id))
            .cloned()
            .collect()
    }

    /// STRUCT-1: Produce the canonical bytes that MUST be signed for an envelope.
    ///
    /// Covers: sender + nonce + timestamp + payload — so that none of these
    /// fields can be tampered with after signing.
    ///
    /// This is the ONLY function callers should use to produce signable data.
    pub fn signable_envelope_bytes(
        sender: &str,
        nonce: u64,
        timestamp: &str,
        payload: &GossipMessage,
    ) -> Vec<u8> {
        // Canonical format: JSON array [sender, nonce, timestamp, payload]
        // Using serde_json ensures deterministic serialization.
        let canonical = serde_json::json!([sender, nonce, timestamp, payload]);
        serde_json::to_vec(&canonical).unwrap_or_default()
    }

    /// Legacy helper — returns just the payload bytes (for backward compat).
    /// NEW CODE SHOULD USE `signable_envelope_bytes()` INSTEAD.
    #[deprecated(note = "Use signable_envelope_bytes() for STRUCT-1 compliant signing")]
    pub fn payload_bytes(payload: &GossipMessage) -> Vec<u8> {
        serde_json::to_vec(payload).unwrap_or_default()
    }

    /// Phase 3 — construit une enveloppe signée prête à être broadcastée.
    ///
    /// STRUCT-1: The caller must sign the bytes produced by `signable_envelope_bytes()`
    /// which covers sender + nonce + timestamp + payload.
    ///
    /// The `sig_bytes` MUST be the Ed25519 signature of `signable_envelope_bytes(sender, nonce, &timestamp, &payload)`.
    pub fn wrap_outgoing(
        sender: String,
        payload: GossipMessage,
        sig_bytes: &[u8],
    ) -> Result<GossipEnvelope, String> {
        let payload_json = serde_json::to_vec(&payload).map_err(|e| e.to_string())?;
        let id = hex::encode(blake3::hash(&payload_json).as_bytes());
        let timestamp = chrono::Utc::now().to_rfc3339();
        Ok(GossipEnvelope {
            id,
            sender,
            payload,
            signature: hex::encode(sig_bytes),
            timestamp,
            nonce: 0,
        })
    }

    /// STRUCT-1: Build + sign a complete envelope in one step.
    ///
    /// This is the recommended production API. It:
    /// 1. Accepts the pre-computed timestamp (MUST match what was signed)
    /// 2. Takes the pre-computed signature of `signable_envelope_bytes(sender, nonce, timestamp, payload)`
    /// 3. Returns the complete envelope
    ///
    /// **Critical**: The `timestamp` MUST be the same value used in `signable_envelope_bytes()`
    /// when computing the signature. Otherwise verification will fail.
    pub fn build_signed_envelope(
        sender: String,
        payload: GossipMessage,
        nonce: u64,
        timestamp: String,
        sig_bytes: &[u8],
    ) -> Result<GossipEnvelope, String> {
        let payload_json = serde_json::to_vec(&payload).map_err(|e| e.to_string())?;
        let id = hex::encode(blake3::hash(&payload_json).as_bytes());
        Ok(GossipEnvelope {
            id,
            sender,
            payload,
            signature: hex::encode(sig_bytes),
            timestamp,
            nonce,
        })
    }

    /// CRIT-1: wrap_outgoing with explicit nonce for anti-replay.
    pub fn wrap_outgoing_with_nonce(
        sender: String,
        payload: GossipMessage,
        sig_bytes: &[u8],
        nonce: u64,
    ) -> Result<GossipEnvelope, String> {
        let mut env = Self::wrap_outgoing(sender, payload, sig_bytes)?;
        env.nonce = nonce;
        Ok(env)
    }
}

impl Default for GossipRouter {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Snapshot sérialisable ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipRouterSnapshot {
    pub seen_messages: std::collections::HashSet<MsgId>,
    /// MOD-2: Eviction order — persisted so bounds hold across restarts.
    #[serde(default)]
    pub seen_order: std::collections::VecDeque<MsgId>,
    pub stats: GossipStats,
    /// CRIT-A: Persisted outgoing nonce so it never resets on restart.
    #[serde(default = "default_nonce")]
    pub outgoing_nonce: u64,
}

fn default_nonce() -> u64 { 1 }

impl GossipRouter {
    pub fn snapshot(&self) -> GossipRouterSnapshot {
        GossipRouterSnapshot {
            seen_messages: self.seen_messages.clone(),
            seen_order: self.seen_order.clone(),
            stats: self.stats.clone(),
            outgoing_nonce: self.outgoing_nonce.load(Ordering::Relaxed),
        }
    }
    pub fn restore(snap: GossipRouterSnapshot) -> Self {
        Self {
            seen_messages: snap.seen_messages,
            seen_order: snap.seen_order,
            stats: snap.stats,
            outgoing_nonce: AtomicU64::new(snap.outgoing_nonce.max(1)),
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
        assert!(
            router.mark_seen("msg_001"),
            "Premier message doit être accepté"
        );
        assert!(!router.mark_seen("msg_001"), "Doublon doit être rejeté");
    }

    #[test]
    fn is_fresh_rejects_old() {
        let old_ts = "2020-01-01T00:00:00Z";
        assert!(
            !GossipRouter::is_fresh(old_ts),
            "Vieux timestamp doit être refusé"
        );
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

    #[test]
    fn build_hello_carries_current_protocol_version() {
        // NET-5: every freshly built Hello must announce TORUS_PROTOCOL_VERSION
        // so peers can reason about compat.
        let hello = GossipRouter::build_hello(
            vec![], "n".into(), 10.0, "FR".into(), 0, 0, 0, 0, vec![]
        );
        match hello {
            GossipMessage::Hello { version, .. } => {
                assert_eq!(version, TORUS_PROTOCOL_VERSION);
            }
            _ => panic!("expected Hello variant"),
        }
    }

    #[test]
    fn legacy_hello_without_version_field_defaults_to_v1() {
        // NET-5: A legacy peer that sent Hello without a `version` field
        // (pre-NET-5 wire format) must deserialize cleanly with version=1.
        let legacy_json = serde_json::json!({
            "type": "Hello",
            "data": {
                "heads": [],
                "node_id": "legacy-node",
                "watts": 5.0,
                "country": "FR"
                // no `version`, no other newer fields
            }
        });
        let parsed: GossipMessage = serde_json::from_value(legacy_json)
            .expect("legacy Hello must deserialize via #[serde(default)]");
        match parsed {
            GossipMessage::Hello { version, .. } => assert_eq!(version, 1),
            _ => panic!("expected Hello"),
        }
    }

    #[test]
    fn unknown_message_variant_fails_to_deserialize_gracefully() {
        // NET-5: An unknown payload variant must fail at JSON layer (no panic),
        // letting the dispatcher silently drop it.
        let unknown = serde_json::json!({
            "type": "FromTheFuture",
            "data": { "foo": 42 }
        });
        let res: Result<GossipMessage, _> = serde_json::from_value(unknown);
        assert!(res.is_err(), "unknown variants must fail safely, not crash");
    }
}
