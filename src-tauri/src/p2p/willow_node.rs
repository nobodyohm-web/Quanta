// p2p/willow_node.rs — Real Iroh P2P Node
// Creates a QUIC endpoint, serves content, enables peer connections via tickets.
// B3: PeerInfo with TTL-based dead peer cleanup.
// NET-1: Auto-reconnection with exponential backoff for lost peers.

use super::*;
use super::dispatcher::NonceTracker;
use super::reputation::ReputationEngine;
use super::ledger::Ledger;
use super::consensus::ConsensusEngine;
use super::merkle_dag::MerkleDAG;
use super::gossip::GossipRouter;
use super::gossip_priority::{priority_channel, PrioritySender, PriorityReceiver};
use super::energy::EnergyOracle;
use super::marketplace::Marketplace;
use super::page_store::PageStore;
// V3 social web engines
use super::domains::DomainRegistry;
use super::search::SearchIndex;
use super::social::SocialState;
use super::moderation::ModerationEngine;
use super::forums::ForumsEngine;
use super::trust_graph::FollowGraph;
use iroh::protocol::Router;
use iroh_gossip::{
    api::{GossipReceiver, GossipSender},
    net::{Gossip, GOSSIP_ALPN},
    proto::TopicId,
};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicU64;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

// ─── NET-7: Incremental DAG sync state ──────────────────────────────────────

/// Per-peer cache so we only re-ask for DAG nodes when the peer's head set
/// genuinely changed (or the cache is stale). Avoids redundant WantNodes
/// chatter every time we receive a periodic Hello.
#[derive(Debug, Clone)]
pub struct DagSyncState {
    /// Hash set of the heads we last saw from this peer.
    pub last_their_heads: HashSet<String>,
    /// When we last asked this peer for missing nodes.
    pub last_asked: Instant,
}

/// Window after which we re-issue a WantNodes even if heads haven't changed.
/// 90s is comfortably > Hello interval (120s would skip an entire cycle), so
/// we still get a periodic re-ask as a backstop against lost messages.
pub const DAG_SYNC_REASK_WINDOW: Duration = Duration::from_secs(90);

/// B3: Maximum time without a Hello before a peer is considered dead.
/// Conservative 5-minute TTL. Solana uses 15s; we use 5m for our scale.
const PEER_TTL: Duration = Duration::from_secs(300);

/// B3: How often to run the dead-peer cleanup task.
#[allow(dead_code)] // Documented constant; cleanup interval is hardcoded in lib.rs spawn
const CLEANUP_INTERVAL: Duration = Duration::from_secs(30);

/// NET-1: Auto-reconnection constants.
const RECONNECT_INITIAL_DELAY: Duration = Duration::from_secs(1);
const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(60);
const RECONNECT_MAX_ATTEMPTS: u32 = 10;

/// QUANTA gossip topic — fixe pour tous les nœuds, dérivé d'un hash BLAKE3 stable.
pub fn quanta_topic_id() -> TopicId {
    TopicId::from_bytes(*blake3::hash(b"quanta-network-v1").as_bytes())
}

// ─── NET-1: Known Peer Registry ─────────────────────────────────────────────

/// State tracking for a known peer (for auto-reconnection).
#[derive(Debug, Clone)]
pub struct KnownPeer {
    /// Iroh EndpointId string (paste-able by users).
    pub endpoint_id: String,
    /// Last time we successfully connected or received a message.
    pub last_connected: Instant,
    /// Current reconnection attempt count (resets on success).
    pub reconnect_attempts: u32,
    /// Whether we're currently trying to reconnect.
    pub reconnecting: bool,
    /// Whether this peer is currently connected (NeighborUp received).
    pub connected: bool,
}

impl KnownPeer {
    pub fn new(endpoint_id: String) -> Self {
        Self {
            endpoint_id,
            last_connected: Instant::now(),
            reconnect_attempts: 0,
            reconnecting: false,
            connected: true,
        }
    }

    /// Compute the next backoff delay: 1s, 2s, 4s, 8s, 16s, 32s, 60s (capped).
    pub fn next_backoff(&self) -> Duration {
        let delay = RECONNECT_INITIAL_DELAY
            .checked_mul(2u32.saturating_pow(self.reconnect_attempts))
            .unwrap_or(RECONNECT_MAX_DELAY);
        delay.min(RECONNECT_MAX_DELAY)
    }

    /// Whether we've exhausted all reconnection attempts.
    pub fn exhausted(&self) -> bool {
        self.reconnect_attempts >= RECONNECT_MAX_ATTEMPTS
    }

    /// Mark peer as successfully connected (resets backoff).
    pub fn mark_connected(&mut self) {
        self.last_connected = Instant::now();
        self.reconnect_attempts = 0;
        self.reconnecting = false;
        self.connected = true;
    }

    /// Mark peer as disconnected (starts backoff cycle).
    pub fn mark_disconnected(&mut self) {
        self.connected = false;
    }
}

/// Real Iroh P2P Node — QUIC transport + content serving
pub struct WillowNode {
    pub reputation: Arc<RwLock<ReputationEngine>>,
    pub ledger: Arc<RwLock<Ledger>>,
    // Phase 2B/3 — Consensus Merkle-CRDT branché
    pub consensus: Arc<RwLock<ConsensusEngine>>,
    pub dag: Arc<RwLock<MerkleDAG>>,
    pub gossip: Arc<RwLock<GossipRouter>>,
    // Phase 2C — Oracle énergie + reports pays des peers (code_pays → nb_peers)
    pub energy_oracle: Arc<RwLock<EnergyOracle>>,
    pub peer_country_reports: Arc<RwLock<HashMap<String, u64>>>,
    /// B3: Rich peer info with liveness tracking (replaces bare peer_watts HashMap)
    pub peer_info: Arc<RwLock<HashMap<String, PeerInfo>>>,
    /// CRIT-1: Per-peer nonce tracker for gossip-level anti-replay
    pub nonce_tracker: Arc<RwLock<NonceTracker>>,
    /// Phase 3 — Marketplace de calcul distribué (tâches compute, BME burn 2%)
    pub marketplace: Arc<RwLock<Marketplace>>,
    /// P2P Web — pages publiées par les wallets
    pub page_store: Arc<RwLock<PageStore>>,
    /// V3 — Registre des noms de domaine `*.torus` (Harberger Tax)
    pub domains: Arc<RwLock<DomainRegistry>>,
    /// V3 — Index de recherche P2P (TF-IDF + signaux sociaux)
    pub search: Arc<RwLock<SearchIndex>>,
    /// V3 — État social (likes quadratiques, abonnements, tips, boost)
    pub social: Arc<RwLock<SocialState>>,
    /// V3 — Moteur de modération décentralisée (jury VRF, commit-reveal)
    pub moderation: Arc<RwLock<ModerationEngine>>,
    /// V3 — Forums (threads DAG + commentaires)
    pub forums: Arc<RwLock<ForumsEngine>>,
    /// V3 — Graphe de Follow (pour PageRank personnalisé Web of Trust)
    pub follow_graph: Arc<RwLock<FollowGraph>>,
    /// Phase 3 + NET-3 — channel sortant priorisé pour les enveloppes gossip.
    /// Quatre lanes (Critical/High/Medium/Low) drainées par ordre de priorité.
    /// Le drain est branché à Iroh dès qu'un endpoint est actif ; sinon il
    /// accumule en local pour rejouer.
    pub gossip_tx: PrioritySender,
    gossip_rx: Arc<RwLock<Option<PriorityReceiver>>>,
    /// Phase 4 — broadcaster sur le topic iroh-gossip ; rempli après init_endpoint().
    pub gossip_topic_sender: Arc<RwLock<Option<GossipSender>>>,
    /// Phase 4 — events entrants depuis le topic ; consommé par le dispatcher de lib.rs.
    gossip_topic_rx: Arc<RwLock<Option<GossipReceiver>>>,
    /// Phase 4 — Router Iroh : maintient l'acceptation des connexions sur GOSSIP_ALPN.
    /// Drop = arrêt du protocole, donc on le garde vivant ici.
    _router: Arc<RwLock<Option<Router>>>,
    node_id: String,
    node_addr: Arc<RwLock<Option<String>>>,
    puzzle_difficulty: u8,
    endpoint_active: Arc<RwLock<bool>>,
    /// CRIT-B: Count of remote blocks successfully validated & integrated.
    /// Feeds the Shapley "validation" factor (20% weight).
    pub blocks_validated: Arc<AtomicU64>,
    /// NET-1: Registry of peers we've connected to (for auto-reconnection).
    /// Key = Iroh EndpointId string.
    pub known_peers: Arc<RwLock<HashMap<String, KnownPeer>>>,
    /// NET-7: Per-peer DAG sync state — used by `handle_hello` to skip
    /// redundant WantNodes broadcasts when a peer's heads haven't changed
    /// since the last sync round. Key = sender public key hex.
    pub dag_sync: Arc<RwLock<HashMap<String, DagSyncState>>>,
    /// Graceful shutdown token — cancel() to stop all background tasks.
    pub shutdown: CancellationToken,
}

impl WillowNode {
    pub fn new() -> Self {
        let raw_id = blake3::hash(uuid::Uuid::new_v4().as_bytes());
        let node_id = hex::encode(raw_id.as_bytes());
        let (gossip_tx, gossip_rx) = priority_channel();
        Self {
            reputation: Arc::new(RwLock::new(ReputationEngine::new())),
            ledger: Arc::new(RwLock::new(Ledger::new())),
            consensus: Arc::new(RwLock::new(ConsensusEngine::new())),
            dag: Arc::new(RwLock::new(MerkleDAG::new())),
            gossip: Arc::new(RwLock::new(GossipRouter::new())),
            energy_oracle: Arc::new(RwLock::new(EnergyOracle::new())),
            peer_country_reports: Arc::new(RwLock::new(HashMap::new())),
            peer_info: Arc::new(RwLock::new(HashMap::new())),
            nonce_tracker: Arc::new(RwLock::new(NonceTracker::new())),
            marketplace: Arc::new(RwLock::new(Marketplace::new())),
            page_store: Arc::new(RwLock::new(PageStore::new())),
            domains: Arc::new(RwLock::new(DomainRegistry::new())),
            search: Arc::new(RwLock::new(SearchIndex::new())),
            social: Arc::new(RwLock::new(SocialState::new())),
            moderation: Arc::new(RwLock::new(ModerationEngine::new())),
            forums: Arc::new(RwLock::new(ForumsEngine::new())),
            follow_graph: Arc::new(RwLock::new(FollowGraph::new())),
            gossip_tx,
            gossip_rx: Arc::new(RwLock::new(Some(gossip_rx))),
            gossip_topic_sender: Arc::new(RwLock::new(None)),
            gossip_topic_rx: Arc::new(RwLock::new(None)),
            _router: Arc::new(RwLock::new(None)),
            node_id: node_id[..64].to_string(),
            node_addr: Arc::new(RwLock::new(None)),
            puzzle_difficulty: 3,
            endpoint_active: Arc::new(RwLock::new(false)),
            blocks_validated: Arc::new(AtomicU64::new(0)),
            known_peers: Arc::new(RwLock::new(HashMap::new())),
            dag_sync: Arc::new(RwLock::new(HashMap::new())),
            shutdown: CancellationToken::new(),
        }
    }

    /// B3: Remove peers that haven't sent a valid Hello within PEER_TTL.
    /// Returns the number of peers removed.
    pub async fn cleanup_dead_peers(&self) -> usize {
        let mut peers = self.peer_info.write().await;
        let before = peers.len();
        peers.retain(|_, info| info.elapsed() < PEER_TTL);
        let removed = before - peers.len();
        if removed > 0 {
            log::info!("♻ [B3] Removed {} dead peers ({} → {} alive)", removed, before, peers.len());
        }
        removed
    }

    /// B3: Compute total network watts from LIVE peers only.
    /// Dead peers (not sending Hello within TTL) are excluded.
    pub async fn total_network_watts(&self) -> f64 {
        let peers = self.peer_info.read().await;
        peers.values()
            .filter(|p| p.elapsed() < PEER_TTL)
            .map(|p| p.watts)
            .sum()
    }

    /// Phase 4 — prend (et consomme) le receiver d'évènements iroh-gossip.
    pub async fn take_gossip_topic_receiver(&self) -> Option<GossipReceiver> {
        self.gossip_topic_rx.write().await.take()
    }

    /// Phase 4 — connecte le nœud à un peer via son EndpointId (string).
    /// Le peer doit aussi être abonné au topic QUANTA pour que le sync démarre.
    /// After successful connection, triggers an immediate Hello broadcast
    /// so the peer detects us and chain sync can begin.
    /// NET-1: Registers the peer in known_peers for auto-reconnection.
    pub async fn connect_peer(&self, peer_id_str: &str) -> Result<(), String> {
        let sender_guard = self.gossip_topic_sender.read().await;
        let sender = sender_guard.as_ref().ok_or("Gossip not initialized")?;
        let peer_id = iroh::EndpointId::from_str(peer_id_str)
            .map_err(|e| format!("EndpointId invalide: {}", e))?;
        sender.join_peers(vec![peer_id]).await
            .map_err(|e| format!("join_peers failed: {}", e))?;

        // NET-1: Register in known_peers for auto-reconnect
        {
            let mut kp = self.known_peers.write().await;
            kp.entry(peer_id_str.to_string())
                .and_modify(|p| p.mark_connected())
                .or_insert_with(|| KnownPeer::new(peer_id_str.to_string()));
        }

        log::info!("◈ [P2P] Connected to peer {}", &peer_id_str[..peer_id_str.len().min(16)]);
        Ok(())
    }

    /// NET-1: Mark a peer as disconnected (NeighborDown). Starts backoff cycle.
    pub async fn mark_peer_down(&self, endpoint_id: &str) {
        let mut kp = self.known_peers.write().await;
        if let Some(peer) = kp.get_mut(endpoint_id) {
            peer.mark_disconnected();
            log::info!(
                "◈ [P2P] Peer {} marked down — will auto-reconnect (attempt {})",
                &endpoint_id[..endpoint_id.len().min(16)],
                peer.reconnect_attempts + 1
            );
        }
    }

    /// NET-1: Mark a peer as connected (NeighborUp). Resets backoff.
    pub async fn mark_peer_up(&self, endpoint_id: &str) {
        let mut kp = self.known_peers.write().await;
        if let Some(peer) = kp.get_mut(endpoint_id) {
            peer.mark_connected();
            log::info!(
                "◈ [P2P] Peer {} reconnected successfully",
                &endpoint_id[..endpoint_id.len().min(16)]
            );
        }
    }

    /// NET-1: Try to reconnect to a specific known peer.
    /// Returns Ok(true) if reconnection succeeded, Ok(false) if skipped, Err if failed.
    pub async fn try_reconnect(&self, endpoint_id: &str) -> Result<bool, String> {
        // Check if we should attempt
        {
            let mut kp = self.known_peers.write().await;
            let Some(peer) = kp.get_mut(endpoint_id) else {
                return Ok(false);
            };
            if peer.connected || peer.reconnecting || peer.exhausted() {
                return Ok(false);
            }
            peer.reconnecting = true;
            peer.reconnect_attempts += 1;
        }

        // Attempt reconnection
        let result = self.connect_peer(endpoint_id).await;

        // Update state based on result
        {
            let mut kp = self.known_peers.write().await;
            if let Some(peer) = kp.get_mut(endpoint_id) {
                peer.reconnecting = false;
                if result.is_ok() {
                    peer.mark_connected();
                }
            }
        }

        match &result {
            Ok(()) => Ok(true),
            Err(e) => {
                let kp = self.known_peers.read().await;
                let attempts = kp.get(endpoint_id)
                    .map(|p| p.reconnect_attempts).unwrap_or(0);
                log::warn!(
                    "◈ [P2P] Reconnect to {} failed (attempt {}/{}): {}",
                    &endpoint_id[..endpoint_id.len().min(16)],
                    attempts, RECONNECT_MAX_ATTEMPTS, e
                );
                Err(e.clone())
            }
        }
    }

    /// NET-1: Get list of disconnected peers that need reconnection, with their backoff delays.
    pub async fn peers_needing_reconnect(&self) -> Vec<(String, Duration)> {
        let kp = self.known_peers.read().await;
        kp.values()
            .filter(|p| !p.connected && !p.reconnecting && !p.exhausted())
            .map(|p| (p.endpoint_id.clone(), p.next_backoff()))
            .collect()
    }

    /// Phase 3 + NET-3 — prend (et consomme) le receiver gossip priorisé.
    /// À appeler une seule fois par la boucle de drain (Iroh ou stub local).
    pub async fn take_gossip_receiver(&self) -> Option<PriorityReceiver> {
        self.gossip_rx.write().await.take()
    }

    /// Initialize the real Iroh QUIC endpoint + iroh-gossip topic.
    pub async fn init_endpoint(&self) -> Result<(), String> {
        let active = self.endpoint_active.clone();
        let addr = self.node_addr.clone();

        // Try to create a real Iroh endpoint
        match iroh::Endpoint::builder(iroh::endpoint::presets::N0).bind().await {
            Ok(endpoint) => {
                let nid = endpoint.id();
                *addr.write().await = Some(nid.to_string());
                *active.write().await = true;
                log::info!("◈ [Iroh] QUIC endpoint bound — NodeId: {}", nid);

                // Phase 4 — Spawn iroh-gossip et l'enregistrer sur un Router
                // pour que les connexions GOSSIP_ALPN soient routées automatiquement.
                let gossip = Gossip::builder().spawn(endpoint.clone());
                let router = Router::builder(endpoint.clone())
                    .accept(GOSSIP_ALPN, gossip.clone())
                    .spawn();

                // Subscribe au topic QUANTA partagé. Pas de bootstrap — les peers se connectent
                // explicitement via `connect_peer` ou s'auto-découvrent via gossip discovery.
                let topic = quanta_topic_id();
                match gossip.subscribe(topic, vec![]).await {
                    Ok(gtopic) => {
                        let (sender, receiver) = gtopic.split();
                        *self.gossip_topic_sender.write().await = Some(sender);
                        *self.gossip_topic_rx.write().await = Some(receiver);
                        *self._router.write().await = Some(router);
                        log::info!("◈ [Gossip] Subscribed to QUANTA topic {}", topic.fmt_short());
                    }
                    Err(e) => {
                        log::warn!("◈ [Gossip] subscribe failed: {} — broadcast desactivé", e);
                        // Router quand même conservé pour les futures resouscriptions.
                        *self._router.write().await = Some(router);
                    }
                }

                Ok(())
            }
            Err(e) => {
                log::warn!("◈ [Iroh] Endpoint bind failed (offline mode): {}", e);
                // Fallback: local-only mode
                *active.write().await = false;
                Err(format!("Iroh endpoint unavailable: {}", e))
            }
        }
    }

    /// Get the shareable ticket for this node
    pub async fn get_ticket(&self) -> Option<String> {
        self.node_addr.read().await.clone()
    }

    /// Start syncing a site as a Willow subspace
    pub async fn get_status(&self) -> NodeStatus {
        let peer_count = self.peer_info.read().await.len() as u32;
        let is_online = *self.endpoint_active.read().await;
        let peer_id = self.node_addr.read().await.clone().unwrap_or_default();
        let kp = self.known_peers.read().await;
        let known_peer_count = kp.len() as u32;
        let connected_known = kp.values().filter(|p| p.connected).count() as u32;
        drop(kp);
        NodeStatus {
            node_id: self.node_id.clone(),
            peer_id,
            is_online,
            peer_count,
            known_peers: known_peer_count,
            connected_peers: connected_known,
            active_subspaces: 0,
            protocol: if is_online {
                "Torus P2P v2 — Connected".into()
            } else {
                "Torus P2P v2 — Local Mode".into()
            },
            puzzle_difficulty: self.puzzle_difficulty,
        }
    }

}

impl Default for WillowNode { fn default() -> Self { Self::new() } }
