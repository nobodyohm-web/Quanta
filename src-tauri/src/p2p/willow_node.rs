// p2p/willow_node.rs — Real Iroh P2P Node
// Creates a QUIC endpoint, serves content, enables peer connections via tickets.
// B3: PeerInfo with TTL-based dead peer cleanup.
// NET-1: Auto-reconnection with exponential backoff for lost peers.

use super::*;
use super::dispatcher::NonceTracker;
use super::reputation::ReputationEngine;
use super::ledger::Ledger;
use super::consensus::ConsensusEngine;
use super::gossip::GossipRouter;
use super::gossip_priority::{priority_channel, PrioritySender, PriorityReceiver};
use super::energy::EnergyOracle;
use super::username::UsernameRegistry;
use iroh::protocol::Router;
use iroh_gossip::{
    api::{GossipReceiver, GossipSender},
    net::{Gossip, GOSSIP_ALPN},
    proto::TopicId,
};
use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

/// B3: Maximum time without a Hello before a peer is considered dead.
/// Conservative 5-minute TTL. Solana uses 15s; we use 5m for our scale.
const PEER_TTL: Duration = Duration::from_secs(300);

/// NET-1: Auto-reconnection constants.
const RECONNECT_INITIAL_DELAY: Duration = Duration::from_secs(1);
const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(60);
const RECONNECT_MAX_ATTEMPTS: u32 = 10;

/// MEM-BOUNDS (HARDEN-HYGIENE-1): hard cap on the LOCAL auto-reconnect table.
/// `known_peers` is local connectivity state (never gossiped, not consensus),
/// so bounding it has no convergence effect — it only stops an attacker who
/// sprays valid EndpointIds (via `Hello.known_peer_ids`) from growing it without
/// limit.
const MAX_KNOWN_PEERS: usize = 1024;

/// QUANTA gossip topic — fixe pour tous les nœuds, dérivé d'un hash BLAKE3 stable.
/// Source unique : `rendezvous` dérive ses slots DHT de ces mêmes octets, donc
/// deux nœuds du même topic se retrouvent forcément au même rendez-vous.
pub fn quanta_topic_bytes() -> [u8; 32] {
    *blake3::hash(b"quanta-network-v1").as_bytes()
}

/// QUANTA gossip topic — fixe pour tous les nœuds, dérivé d'un hash BLAKE3 stable.
pub fn quanta_topic_id() -> TopicId {
    TopicId::from_bytes(quanta_topic_bytes())
}

/// RDV-0 — load (or mint once) this node's **persistent network identity**.
///
/// `Endpoint::builder(..).bind()` without a secret key mints a fresh Ed25519
/// keypair at every launch, so the NodeId — and therefore the ticket a user
/// shares — changed on every restart. Peers that had saved us could no longer
/// reach us, `known_peers` was worthless across a reboot, and any DHT record we
/// published died with the process. Rendezvous means nothing without a stable
/// identity, so this is the root fix that RDV-1 stands on.
///
/// The key is deliberately **not** derived from the wallet seed. It is transport
/// identity, not money: keeping them disjoint means a NodeId reveals nothing
/// about an account, two machines restoring the same wallet are not linkable by
/// their NodeId, and the endpoint can bind at boot without waiting for an unlock.
/// Losing the file costs only a new NodeId — peers rediscover us via the DHT.
pub fn load_or_create_node_key(data_dir: &Path) -> Result<iroh::SecretKey, String> {
    use zeroize::Zeroize;

    let path = data_dir.join("node_key");
    match std::fs::read(&path) {
        Ok(mut bytes) if bytes.len() == 32 => {
            let mut raw = [0u8; 32];
            raw.copy_from_slice(&bytes);
            bytes.zeroize();
            let key = iroh::SecretKey::from_bytes(&raw);
            raw.zeroize();
            return Ok(key);
        }
        Ok(mut bytes) => {
            // Truncated or corrupt: mint a new one rather than refusing to boot —
            // a broken network identity must never take the node offline.
            log::warn!("◈ [RDV-0] node_key illisible ({} o) — régénération", bytes.len());
            bytes.zeroize();
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => log::warn!("◈ [RDV-0] node_key non lu: {e} — régénération"),
    }

    let key = iroh::SecretKey::generate();
    let mut raw = key.to_bytes();
    let write = write_secret_file(&path, &raw);
    raw.zeroize();
    match write {
        Ok(()) => log::info!("◈ [RDV-0] Identité réseau persistée — NodeId stable: {}", key.public()),
        // Still usable this session; it just will not survive a restart.
        Err(e) => log::warn!("◈ [RDV-0] node_key non écrit: {e} — identité éphémère cette session"),
    }
    Ok(key)
}

/// Write 32 secret bytes owner-readable only (0600 on Unix).
fn write_secret_file(path: &Path, bytes: &[u8; 32]) -> Result<(), String> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(path).map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())
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

/// MEM-BOUNDS (HARDEN-HYGIENE-1): register `peer_id` in the bounded local
/// reconnect table. A known peer is refreshed (never refused); a new peer is
/// admitted only if there is room, reclaiming one **exhausted** (terminal,
/// never-retried) entry first. Returns whether the peer is now tracked. Pure +
/// synchronous so it is unit-testable without the network. Eviction touches only
/// local state, so it has no consensus/convergence effect.
fn register_known_peer(kp: &mut HashMap<String, KnownPeer>, peer_id: &str, max: usize) -> bool {
    if let Some(p) = kp.get_mut(peer_id) {
        p.mark_connected();
        return true;
    }
    if kp.len() >= max {
        match kp.iter().find(|(_, p)| p.exhausted()).map(|(k, _)| k.clone()) {
            Some(victim) => {
                kp.remove(&victim);
            }
            None => return false, // full of live/pending peers — don't track overflow
        }
    }
    kp.insert(peer_id.to_string(), KnownPeer::new(peer_id.to_string()));
    true
}

/// Real Iroh P2P Node — QUIC transport + content serving
pub struct WillowNode {
    pub reputation: Arc<RwLock<ReputationEngine>>,
    pub ledger: Arc<RwLock<Ledger>>,
    // Phase 2B/3 — Consensus CRDT branché
    pub consensus: Arc<RwLock<ConsensusEngine>>,
    pub gossip: Arc<RwLock<GossipRouter>>,
    // Phase 2C — Oracle énergie + reports pays des peers (code_pays → nb_peers)
    pub energy_oracle: Arc<RwLock<EnergyOracle>>,
    pub peer_country_reports: Arc<RwLock<HashMap<String, u64>>>,
    /// B3: Rich peer info with liveness tracking (replaces bare peer_watts HashMap)
    pub peer_info: Arc<RwLock<HashMap<String, PeerInfo>>>,
    /// CRIT-1: Per-peer nonce tracker for gossip-level anti-replay
    pub nonce_tracker: Arc<RwLock<NonceTracker>>,
    /// Identité — registre de pseudos uniques `@handle` (adresse de wallet lisible)
    pub usernames: Arc<RwLock<UsernameRegistry>>,
    /// LIVE-1 — live finality gadget state: the fork-choice latest votes
    /// (GADGET-5A) plus the justify/finalize state (GADGET-3), fed from gossiped
    /// `FinalityVote`s. Anchored at the ledger's genesis hash. The verdict stays a
    /// pure `sm/` function; this store is the IO-layer holder.
    pub finality: Arc<RwLock<crate::p2p::finality_live::FinalityTracker>>,
    /// LIVE-4 — bounded orphan-branch buffer + live caller of `reorg_to_fork`
    /// (deep-fork / partition reconciliation). IO-layer state, never persisted.
    /// Lock order: acquire AFTER `ledger`, release before gossip broadcasts.
    pub fork_heal: Arc<RwLock<crate::p2p::fork_heal::ForkReconciler>>,
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
    /// Feeds the Shapley "validation" factor (25% weight).
    pub blocks_validated: Arc<AtomicU64>,
    /// NET-1: Registry of peers we've connected to (for auto-reconnection).
    /// Key = Iroh EndpointId string.
    pub known_peers: Arc<RwLock<HashMap<String, KnownPeer>>>,
    /// NET-9: Outstanding Ping nonces — keyed by nonce, value is when we
    /// broadcast that Ping. The first Pong with a given nonce attributes
    /// the round-trip time to its sender. Bounded by `MAX_PENDING_PINGS`
    /// so a non-responsive network can't grow this map without limit.
    pub pending_pings: Arc<RwLock<HashMap<u64, Instant>>>,
    /// Graceful shutdown token — cancel() to stop all background tasks.
    pub shutdown: CancellationToken,
}

/// NET-9: Hard cap on outstanding Ping nonces. At PING_INTERVAL=15s we'd
/// add ~240 entries/hour without responses; capping at 256 keeps memory
/// bounded while still letting peers respond minutes later.
pub const MAX_PENDING_PINGS: usize = 256;

/// NET-12: Minimum peer count before eclipse detection kicks in.
/// Below this we don't have enough samples to call anything suspicious.
pub const ECLIPSE_MIN_PEERS: usize = 5;

/// NET-12: Number of leading hex chars used to bucket public keys for
/// eclipse detection. 8 chars = 32 bits — picking that many leading bits
/// at random is a 1-in-4-billion event, so a high-density bucket is a
/// strong signal of correlated keygen.
pub const ECLIPSE_PREFIX_LEN: usize = 8;

/// NET-12: Share of peers in the largest prefix bucket above which we
/// raise the eclipse warning. 0.8 = 80%.
pub const ECLIPSE_THRESHOLD: f64 = 0.8;

impl WillowNode {
    pub fn new() -> Self {
        let raw_id = blake3::hash(uuid::Uuid::new_v4().as_bytes());
        let node_id = hex::encode(raw_id.as_bytes());
        let (gossip_tx, gossip_rx) = priority_channel();
        let ledger = Ledger::new();
        // LIVE-1: anchor the finality tracker at the chain's genesis checkpoint.
        let finality =
            crate::p2p::finality_live::FinalityTracker::new(ledger.genesis_hash());
        Self {
            reputation: Arc::new(RwLock::new(ReputationEngine::new())),
            ledger: Arc::new(RwLock::new(ledger)),
            consensus: Arc::new(RwLock::new(ConsensusEngine::new())),
            gossip: Arc::new(RwLock::new(GossipRouter::new())),
            energy_oracle: Arc::new(RwLock::new(EnergyOracle::new())),
            peer_country_reports: Arc::new(RwLock::new(HashMap::new())),
            peer_info: Arc::new(RwLock::new(HashMap::new())),
            nonce_tracker: Arc::new(RwLock::new(NonceTracker::new())),
            usernames: Arc::new(RwLock::new(UsernameRegistry::new())),
            finality: Arc::new(RwLock::new(finality)),
            fork_heal: Arc::new(RwLock::new(
                crate::p2p::fork_heal::ForkReconciler::new(),
            )),
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
            pending_pings: Arc::new(RwLock::new(HashMap::new())),
            shutdown: CancellationToken::new(),
        }
    }

    /// B3: Remove peers that haven't sent a valid Hello within PEER_TTL.
    /// Returns the number of peers removed.
    pub async fn cleanup_dead_peers(&self) -> usize {
        // Scope the peer_info lock so it is released before taking known_peers
        // (no two willow stores held at once → no lock-order risk).
        let removed = {
            let mut peers = self.peer_info.write().await;
            let before = peers.len();
            peers.retain(|_, info| info.elapsed() < PEER_TTL);
            before - peers.len()
        };
        // MEM-BOUNDS (HARDEN-HYGIENE-1): GC the LOCAL reconnect table too — an
        // exhausted peer (all reconnect attempts spent) is terminal and never
        // retried (try_reconnect skips it), so it only wastes memory. Local
        // state, so this eviction has no consensus/convergence effect.
        let known_gc = {
            let mut kp = self.known_peers.write().await;
            let kb = kp.len();
            kp.retain(|_, p| !p.exhausted());
            kb - kp.len()
        };
        if removed > 0 || known_gc > 0 {
            log::info!(
                "♻ [B3] Removed {} dead peers, {} exhausted known-peers",
                removed,
                known_gc
            );
        }
        removed
    }

    /// NET-12: Eclipse-attack heuristic.
    ///
    /// Returns Some(prefix) when more than `ECLIPSE_THRESHOLD` (default 80%)
    /// of currently-known peers share a common public-key prefix of length
    /// `ECLIPSE_PREFIX_LEN` (8 hex chars = 32 bits). A real Sybil cluster can
    /// still pick keys that don't share a prefix, so this catches naive
    /// attackers (e.g. a single attacker spinning up 50 nodes from one
    /// keygen seed) without producing false positives on diverse meshes.
    ///
    /// Returns None when:
    /// - We have fewer than `ECLIPSE_MIN_PEERS` peers (statistically irrelevant)
    /// - The largest prefix bucket holds <= ECLIPSE_THRESHOLD share
    ///
    /// Heuristic — *no* automated action: caller logs a warning and the
    /// operator decides whether to manually disconnect or reseed peers.
    pub async fn check_eclipse_risk(&self) -> Option<String> {
        let info = self.peer_info.read().await;
        if info.len() < ECLIPSE_MIN_PEERS {
            return None;
        }
        let mut buckets: HashMap<String, usize> = HashMap::new();
        for pk in info.keys() {
            if pk.len() < ECLIPSE_PREFIX_LEN {
                continue;
            }
            let prefix = pk[..ECLIPSE_PREFIX_LEN].to_string();
            *buckets.entry(prefix).or_insert(0) += 1;
        }
        let total = info.len() as f64;
        buckets.into_iter()
            .max_by_key(|(_, n)| *n)
            .filter(|(_, n)| (*n as f64) / total > ECLIPSE_THRESHOLD)
            .map(|(prefix, _)| prefix)
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

        // NET-1: Register in known_peers for auto-reconnect (MEM-BOUNDS: bounded
        // to MAX_KNOWN_PEERS so a flood of valid EndpointIds cannot grow it).
        {
            let mut kp = self.known_peers.write().await;
            register_known_peer(&mut kp, peer_id_str, MAX_KNOWN_PEERS);
        }

        log::info!("◈ [P2P] Connected to peer {}", &peer_id_str[..peer_id_str.len().min(16)]);
        Ok(())
    }

    /// RDV-2 — re-seed the reconnect table from a persisted snapshot (startup).
    ///
    /// Entries come back **disconnected** with a fresh backoff, so the NET-1
    /// auto-reconnect task re-dials them immediately — the fastest path back
    /// into the mesh after a restart, with no DHT round trip and no human.
    /// Returns how many entries the bounded table actually accepted.
    pub async fn restore_known_peers(&self, ids: Vec<String>) -> usize {
        let mut kp = self.known_peers.write().await;
        let mut accepted = 0usize;
        for id in ids {
            if register_known_peer(&mut kp, &id, MAX_KNOWN_PEERS) {
                if let Some(p) = kp.get_mut(&id) {
                    p.mark_disconnected();
                }
                accepted += 1;
            }
        }
        accepted
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
    ///
    /// `secret` is the **persistent** network identity (RDV-0). Passing `None`
    /// falls back to a fresh random keypair — acceptable only for tests and for
    /// a node whose data dir is unwritable, since an ephemeral NodeId makes both
    /// saved peers and DHT records worthless on the next boot.
    pub async fn init_endpoint(&self, secret: Option<iroh::SecretKey>) -> Result<(), String> {
        let active = self.endpoint_active.clone();
        let addr = self.node_addr.clone();

        // RDV-1 — publish this endpoint's address on the mainline BitTorrent DHT
        // (pkarr record under our own EndpointId). Combined with the persistent
        // key above, a ticket becomes **permanent**: it keeps resolving across
        // NAT changes, IP changes and restarts, with no server of ours involved.
        // The default address filter publishes the home relay only — enough to be
        // dialable (iroh hole-punches to a direct path afterwards) while keeping
        // our local addresses off a public DHT.
        let address_lookup = iroh::address_lookup::DhtAddressLookup::builder();

        let mut builder = iroh::Endpoint::builder(iroh::endpoint::presets::N0)
            .address_lookup(address_lookup);
        if let Some(sk) = secret {
            builder = builder.secret_key(sk);
        }

        // Try to create a real Iroh endpoint
        match builder.bind().await {
            Ok(endpoint) => {
                let nid = endpoint.id();
                *addr.write().await = Some(nid.to_string());
                log::info!("◈ [Iroh] QUIC endpoint bound — NodeId: {}", nid);
                // V3 (audit de vie) — `endpoint_active` (which drives the app's
                // "en ligne" badge) is deliberately NOT set here. A bound QUIC
                // endpoint is necessary but not sufficient: without a successful
                // topic subscription the node can neither send nor receive a
                // single gossip message. It used to be set right after `bind()`,
                // so a failed `subscribe` left the user looking at an online node
                // that was in fact mute. It is now set only on the Ok branch below.

                // Phase 4 — Spawn iroh-gossip et l'enregistrer sur un Router
                // pour que les connexions GOSSIP_ALPN soient routées automatiquement.
                // GOSSIP-MTU-1 — **le nœud était muet sur le vrai réseau.**
                //
                // `iroh-gossip` plafonne un message à `DEFAULT_MAX_MESSAGE_SIZE`
                // = 4096 octets, et rien ne le relevait ici. Or depuis
                // PQ-ENVELOPE-1 (hard-fork v4) une enveloppe porte une signature
                // ML-DSA-65 (3 309 o) ET la clé publique de l'émetteur (1 952 o),
                // toutes deux **en hexadécimal** dans du JSON — soit ~10,5 Ko de
                // seule authentification. Un simple `Hello` pèse ~15 Ko, près de
                // QUATRE FOIS le plafond.
                //
                // Conséquence, constatée en vivant entre deux daemons sur la vraie
                // DHT : ils se découvrent (RDV-1), établissent QUIC, se déclarent
                // voisins (`NeighborUp`)… et **plus rien ne passe**. Ni Hello, ni
                // bloc, ni transaction, ni vote de finalité. L'émission est même
                // comptée comme réussie côté `stats.messages_sent`, donc rien ne
                // le signale. Le « P2P vérifié entre 2 machines » du journal date
                // du 06/05/2026 — soit AVANT PQ-ENVELOPE-1 (18/07) : la régression
                // n'avait jamais été éprouvée en vrai.
                //
                // On aligne donc le plafond du transport sur celui du protocole
                // (`MAX_RAW_ENVELOPE_BYTES`, la garde anti-DoS déjà appliquée à
                // l'étape ① du dispatcher) : une seule limite, gravée au même
                // endroit, que le réseau ne peut pas contredire en silence.
                let gossip = Gossip::builder()
                    .max_message_size(crate::p2p::dispatcher::MAX_RAW_ENVELOPE_BYTES)
                    .spawn(endpoint.clone());
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
                        // V3 — only now is the node genuinely reachable.
                        *active.write().await = true;
                        log::info!("◈ [Gossip] Subscribed to QUANTA topic {}", topic.fmt_short());
                    }
                    Err(e) => {
                        log::error!(
                            "◈ [Gossip] abonnement au topic ÉCHOUÉ: {e} — le nœud est MUET \
                             (ni émission ni réception) et se déclare hors ligne"
                        );
                        // Router quand même conservé pour les futures resouscriptions.
                        *self._router.write().await = Some(router);
                        *active.write().await = false;
                        return Err(format!("gossip subscribe failed: {e}"));
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
                "v2 · Connecté".into()
            } else {
                "v2 · Mode local".into()
            },
            puzzle_difficulty: self.puzzle_difficulty,
        }
    }

}

impl Default for WillowNode { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod mem_bounds_tests {
    //! MEM-BOUNDS (HARDEN-HYGIENE-1): the LOCAL reconnect table stays bounded
    //! under a flood of valid EndpointIds, never evicts a live/pending peer, and
    //! reclaims terminal (exhausted) slots. Pure helper → no network needed.
    use super::{register_known_peer, KnownPeer, RECONNECT_MAX_ATTEMPTS};
    use std::collections::HashMap;

    #[test]
    fn known_peers_table_stays_bounded() {
        let mut kp: HashMap<String, KnownPeer> = HashMap::new();
        let max = 4;
        for i in 0..max {
            assert!(
                register_known_peer(&mut kp, &format!("peer{i}"), max),
                "a fresh peer under the cap is tracked"
            );
        }
        assert_eq!(kp.len(), max);

        // A NEW peer when the table is full of LIVE peers is refused — the table
        // stays bounded and no live peer is evicted.
        assert!(
            !register_known_peer(&mut kp, "overflow", max),
            "overflow refused when full of live peers"
        );
        assert_eq!(kp.len(), max, "table stays at the cap");
        assert!(!kp.contains_key("overflow"));

        // An ALREADY-known peer is always refreshed (never refused), even at cap.
        assert!(register_known_peer(&mut kp, "peer0", max));
        assert_eq!(kp.len(), max);

        // Drive one entry to EXHAUSTED → a new peer reclaims that terminal slot.
        kp.get_mut("peer1").unwrap().reconnect_attempts = RECONNECT_MAX_ATTEMPTS;
        assert!(
            register_known_peer(&mut kp, "fresh", max),
            "a new peer reclaims an exhausted (terminal) slot"
        );
        assert_eq!(kp.len(), max, "still bounded after the reclaim");
        assert!(kp.contains_key("fresh"));
        assert!(!kp.contains_key("peer1"), "the exhausted peer was evicted, not a live one");
    }
}
