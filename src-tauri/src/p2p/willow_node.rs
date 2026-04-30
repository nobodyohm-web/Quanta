// p2p/willow_node.rs — Real Iroh P2P Node
// Creates a QUIC endpoint, serves content, enables peer connections via tickets.

use super::*;
use super::reputation::ReputationEngine;
use super::ledger::Ledger;
use super::consensus::ConsensusEngine;
use super::merkle_dag::MerkleDAG;
use super::gossip::{GossipEnvelope, GossipRouter};
use super::energy::EnergyOracle;
use super::marketplace::Marketplace;
use iroh::protocol::Router;
use iroh_gossip::{
    api::{GossipReceiver, GossipSender},
    net::{Gossip, GOSSIP_ALPN},
    proto::TopicId,
};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// SOVA gossip topic — fixe pour tous les nœuds, dérivé d'un hash BLAKE3 stable.
pub fn sova_topic_id() -> TopicId {
    TopicId::from_bytes(*blake3::hash(b"sova-network-v1").as_bytes())
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
    /// V2: watts mesurés par chaque pair (peer_id → watts)
    pub peer_watts: Arc<RwLock<HashMap<String, f64>>>,
    /// Phase 3 — Marketplace de calcul distribué (tâches compute, BME burn 2%)
    pub marketplace: Arc<RwLock<Marketplace>>,
    /// Phase 3 — channel sortant pour les enveloppes gossip. Le drain est branché à
    /// Iroh dès qu'un endpoint est actif ; sinon il accumule en local pour rejouer.
    pub gossip_tx: mpsc::UnboundedSender<GossipEnvelope>,
    gossip_rx: Arc<RwLock<Option<mpsc::UnboundedReceiver<GossipEnvelope>>>>,
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
}

impl WillowNode {
    pub fn new() -> Self {
        let raw_id = blake3::hash(uuid::Uuid::new_v4().as_bytes());
        let node_id = hex::encode(raw_id.as_bytes());
        let (gossip_tx, gossip_rx) = mpsc::unbounded_channel();
        Self {
            reputation: Arc::new(RwLock::new(ReputationEngine::new())),
            ledger: Arc::new(RwLock::new(Ledger::new())),
            consensus: Arc::new(RwLock::new(ConsensusEngine::new())),
            dag: Arc::new(RwLock::new(MerkleDAG::new())),
            gossip: Arc::new(RwLock::new(GossipRouter::new())),
            energy_oracle: Arc::new(RwLock::new(EnergyOracle::new())),
            peer_country_reports: Arc::new(RwLock::new(HashMap::new())),
            peer_watts: Arc::new(RwLock::new(HashMap::new())),
            marketplace: Arc::new(RwLock::new(Marketplace::new())),
            gossip_tx,
            gossip_rx: Arc::new(RwLock::new(Some(gossip_rx))),
            gossip_topic_sender: Arc::new(RwLock::new(None)),
            gossip_topic_rx: Arc::new(RwLock::new(None)),
            _router: Arc::new(RwLock::new(None)),
            node_id: node_id[..64].to_string(),
            node_addr: Arc::new(RwLock::new(None)),
            puzzle_difficulty: 3,
            endpoint_active: Arc::new(RwLock::new(false)),
        }
    }

    /// Phase 4 — prend (et consomme) le receiver d'évènements iroh-gossip.
    pub async fn take_gossip_topic_receiver(&self) -> Option<GossipReceiver> {
        self.gossip_topic_rx.write().await.take()
    }

    /// Phase 4 — connecte le nœud à un peer via son EndpointId (string).
    /// Le peer doit aussi être abonné au topic SOVA pour que le sync démarre.
    pub async fn connect_peer(&self, peer_id_str: &str) -> Result<(), String> {
        let sender_guard = self.gossip_topic_sender.read().await;
        let sender = sender_guard.as_ref().ok_or("Gossip not initialized")?;
        let peer_id = iroh::EndpointId::from_str(peer_id_str)
            .map_err(|e| format!("EndpointId invalide: {}", e))?;
        sender.join_peers(vec![peer_id]).await
            .map_err(|e| format!("join_peers failed: {}", e))?;
        Ok(())
    }

    /// Phase 3 — prend (et consomme) le receiver gossip. À appeler une seule fois
    /// par la boucle de drain (Iroh ou stub local).
    pub async fn take_gossip_receiver(&self) -> Option<mpsc::UnboundedReceiver<GossipEnvelope>> {
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

                // Subscribe au topic SOVA partagé. Pas de bootstrap — les peers se connectent
                // explicitement via `connect_peer` ou s'auto-découvrent via gossip discovery.
                let topic = sova_topic_id();
                match gossip.subscribe(topic, vec![]).await {
                    Ok(gtopic) => {
                        let (sender, receiver) = gtopic.split();
                        *self.gossip_topic_sender.write().await = Some(sender);
                        *self.gossip_topic_rx.write().await = Some(receiver);
                        *self._router.write().await = Some(router);
                        log::info!("◈ [Gossip] Subscribed to SOVA topic {}", topic.fmt_short());
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
        let peer_count = self.peer_watts.read().await.len() as u32;
        let is_online = *self.endpoint_active.read().await;
        NodeStatus {
            node_id: self.node_id.clone(),
            is_online,
            peer_count,
            active_subspaces: 0,
            protocol: if is_online {
                "Iroh QUIC — Connected".into()
            } else {
                "Willow/QUIC — Local Mode".into()
            },
            puzzle_difficulty: self.puzzle_difficulty,
        }
    }

}

impl Default for WillowNode { fn default() -> Self { Self::new() } }
