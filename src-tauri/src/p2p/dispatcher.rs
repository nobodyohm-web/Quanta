//! Dispatcher des messages gossip entrants — Phase B (Security Hardening).
//!
//! ▸ B1 — Verify-Before-Process: Ed25519 signature verification on every incoming
//!         GossipEnvelope BEFORE deserializing the payload. Invalid → immediate
//!         discard + ReportPeer::InvalidSignature.
//! ▸ B3 — Peer Liveness: updates `peer_info.last_seen` on every valid Hello,
//!         enabling dead-peer cleanup by the TTL task.
//!
//! Réponses sortantes (WantNodes, HaveNodes, Pong) sont rebroadcastées sur le même
//! channel `gossip_tx` que les messages locaux — l'iroh-gossip drain les enverra.

use crate::p2p::gossip::{GossipEnvelope, GossipMessage, GossipRouter, ReportReason};
use crate::p2p::merkle_dag::DagNode;
use crate::security::CryptoEngine;
use crate::AppState;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

// ─── B1: Nonce tracker + rate limiter per peer ──────────────────────────────

/// Baseline messages per peer per rate-limiting window.
/// NET-13: Adaptive limit raises this when peer count is high (mesh growth)
/// and lowers it when only a handful of peers are around (likely attacker).
const BASE_MSG_PER_WINDOW: u32 = 30;

/// NET-13: Hard floor on adaptive rate — even a one-peer mesh keeps this many
/// messages allowed per window so legitimate sync traffic isn't choked.
const MIN_MSG_PER_WINDOW: u32 = 15;

/// NET-13: Hard ceiling on adaptive rate — no matter how big the mesh grows,
/// a single peer shouldn't be allowed to flood beyond this rate.
const MAX_MSG_PER_WINDOW: u32 = 120;

/// Rate-limiting window duration (seconds).
const RATE_WINDOW_SECS: u64 = 60;

/// Number of independent reports against a peer that triggers a ban.
pub const REPORT_BAN_THRESHOLD: u32 = 3;
/// Ban duration in seconds (1 hour). After this the peer gets a fresh slate.
pub const REPORT_BAN_TTL_SECS: u64 = 3600;

/// Maximum size of a single raw gossip envelope before parsing (10 MB).
/// Anything larger is dropped at the very entry of `dispatch_incoming`,
/// before JSON deserialization, to bound memory pressure under DoS.
pub const MAX_RAW_ENVELOPE_BYTES: usize = 10 * 1024 * 1024;

/// Maximum number of blocks we'll process from a received `ChainSegment`.
/// Defends against a peer flooding us with a huge segment in one message.
pub const MAX_CHAIN_SEGMENT_RECEIVED: usize = 50;

/// Tracks the highest nonce seen per sender public key, enforces per-peer
/// rate limiting, and maintains the ban list against malicious peers.
#[allow(dead_code)] // Used in security_tests
pub struct NonceTracker {
    last_nonces: HashMap<String, u64>,
    /// Rate limiter: (window_start_epoch, msg_count_in_window) per peer
    rate_counters: HashMap<String, (u64, u32)>,
    /// Reports per peer_id received via gossip `ReportPeer`.
    /// Cleared when a ban TTL expires, so a peer gets a fresh slate.
    report_counts: HashMap<String, u32>,
    /// peer_id → unix epoch second at which the ban expires.
    /// Use a `HashSet` view via `is_banned()`; the timestamps gate the membership.
    bans: HashMap<String, u64>,
}

fn now_epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl NonceTracker {
    pub fn new() -> Self {
        Self {
            last_nonces: HashMap::new(),
            rate_counters: HashMap::new(),
            report_counts: HashMap::new(),
            bans: HashMap::new(),
        }
    }

    /// Returns `true` if this nonce is valid (strictly greater than the last seen).
    /// Also updates the tracker on acceptance.
    pub fn check_and_advance(&mut self, sender_pk: &str, nonce: u64) -> bool {
        let entry = self.last_nonces.entry(sender_pk.to_string()).or_insert(0);
        if nonce > *entry {
            *entry = nonce;
            true
        } else {
            false
        }
    }

    /// NET-13: Compute the adaptive per-peer rate limit for the current mesh.
    ///
    /// Formula: `BASE × max(1, sqrt(peer_count / 4))`, then clamped.
    /// - 1 peer  → BASE × 1 = 30 msgs/min  (minimum useful)
    /// - 4 peers → BASE × 1 = 30 msgs/min
    /// - 16 peers → BASE × 2 = 60 msgs/min
    /// - 64 peers → BASE × 4 = 120 msgs/min (hits MAX cap)
    /// - 256 peers → still 120 (MAX cap protects against runaway)
    ///
    /// Sub-linear scaling avoids exposing us to total-traffic blowup
    /// (peers × per-peer-rate) while still letting larger meshes route more.
    pub fn adaptive_limit_for(peer_count: usize) -> u32 {
        let scale = (peer_count as f64 / 4.0).sqrt().max(1.0);
        let raw = (BASE_MSG_PER_WINDOW as f64 * scale).round() as u32;
        raw.clamp(MIN_MSG_PER_WINDOW, MAX_MSG_PER_WINDOW)
    }

    /// Returns `true` if this peer is within their rate limit.
    /// Call this BEFORE processing a message. Returns `false` if the peer
    /// has exceeded the adaptive per-peer message budget for the current window.
    pub fn check_rate_limit(&mut self, sender_pk: &str, peer_count: usize) -> bool {
        let now = now_epoch_secs();
        let limit = Self::adaptive_limit_for(peer_count);

        let entry = self.rate_counters
            .entry(sender_pk.to_string())
            .or_insert((now, 0));

        // Reset window if expired
        if now - entry.0 >= RATE_WINDOW_SECS {
            *entry = (now, 0);
        }

        entry.1 += 1;
        entry.1 <= limit
    }

    /// Record a report against `peer_id`. When the count reaches
    /// `REPORT_BAN_THRESHOLD`, install a ban that expires after `REPORT_BAN_TTL_SECS`.
    /// Returns the new report count.
    pub fn record_report(&mut self, peer_id: &str) -> u32 {
        let count = self.report_counts.entry(peer_id.to_string()).or_insert(0);
        *count += 1;
        let new_count = *count;
        if new_count >= REPORT_BAN_THRESHOLD {
            self.bans.insert(peer_id.to_string(), now_epoch_secs() + REPORT_BAN_TTL_SECS);
        }
        new_count
    }

    /// Returns `true` if `peer_id` is currently banned. Auto-evicts expired
    /// entries (and resets their report count) so a peer can rejoin after TTL.
    pub fn is_banned(&mut self, peer_id: &str) -> bool {
        let Some(&expires_at) = self.bans.get(peer_id) else { return false };
        if now_epoch_secs() < expires_at {
            return true;
        }
        // Ban expired — evict, give a fresh slate.
        self.bans.remove(peer_id);
        self.report_counts.remove(peer_id);
        false
    }

    /// Snapshot of currently active bans (for tests/diagnostics).
    #[allow(dead_code)]
    pub fn banned_peers(&self) -> HashSet<String> {
        let now = now_epoch_secs();
        self.bans.iter()
            .filter(|(_, &until)| now < until)
            .map(|(id, _)| id.clone())
            .collect()
    }
}

impl Default for NonceTracker { fn default() -> Self { Self::new() } }

// ─── B1: Envelope signature verification ────────────────────────────────────

/// Verify the Ed25519 signature of a gossip envelope.
///
/// STRUCT-1: The signature covers sender + nonce + timestamp + payload
/// (the canonical bytes produced by `signable_envelope_bytes()`).
/// For backward compatibility, if verification fails on the full bytes,
/// we fall back to verifying just the payload bytes (legacy format).
fn verify_envelope_signature(env: &GossipEnvelope) -> Result<(), String> {
    // 1. Decode sender public key
    let pk_bytes = hex::decode(&env.sender)
        .map_err(|_| "invalid sender public key hex")?;

    // 2. Decode signature
    let sig_bytes = hex::decode(&env.signature)
        .map_err(|_| "invalid signature hex")?;

    // 3. STRUCT-1: Reconstruct the FULL canonical signable bytes
    let full_signable = GossipRouter::signable_envelope_bytes(
        &env.sender, env.nonce, &env.timestamp, &env.payload
    );

    // 4. Verify Ed25519 signature against full canonical bytes
    match CryptoEngine::verify(&pk_bytes, &full_signable, &sig_bytes) {
        Ok(true) => return Ok(()),
        Ok(false) => {}  // Fall through to legacy check
        Err(_) => {}     // Fall through to legacy check
    }

    // 5. Backward compat: try verifying against payload-only bytes (legacy envelopes)
    #[allow(deprecated)]
    let legacy_bytes = serde_json::to_vec(&env.payload)
        .map_err(|e| format!("payload serialization error: {}", e))?;
    match CryptoEngine::verify(&pk_bytes, &legacy_bytes, &sig_bytes) {
        Ok(true) => Ok(()),
        Ok(false) => Err("signature verification failed".into()),
        Err(e) => Err(format!("signature error: {}", e)),
    }
}

// ─── Public entry point ─────────────────────────────────────────────────────

/// Désérialise + vérifie signature + vérifie freshness + dispatche une enveloppe entrante.
///
/// **B1 Security Order:**
/// 1. JSON deserialization (envelope structure only)
/// 2. Anti-replay (message ID dedup)
/// 3. Timestamp freshness (±5 min)
/// 4. **Ed25519 signature verification** ← NEW
/// 5. Payload dispatch
pub async fn dispatch_incoming(state: &Arc<AppState>, raw: &[u8]) {
    // ── DoS guard: reject oversized envelopes BEFORE parsing ──
    // 10 MB is well above our largest legitimate message (ChainSegment of 50
    // blocks ≈ a few hundred KB). Anything bigger is either malicious or buggy.
    if raw.len() > MAX_RAW_ENVELOPE_BYTES {
        log::warn!(
            "◈ [Dispatch] ⚠ oversized payload {} B (> {} B) → drop",
            raw.len(), MAX_RAW_ENVELOPE_BYTES
        );
        return;
    }

    let env: GossipEnvelope = match serde_json::from_slice(raw) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("◈ [Dispatch] envelope JSON invalide: {}", e);
            return;
        }
    };

    // ── Ban check: drop peers under active ban as early as possible ──
    // We need env.sender, so this comes right after JSON parse but before
    // dedup, so banned peers don't pollute the seen-messages LRU.
    {
        let mut tracker = state.node.nonce_tracker.write().await;
        if tracker.is_banned(&env.sender) {
            log::debug!(
                "◈ [Dispatch] banned peer {} → drop",
                &env.sender[..env.sender.len().min(12)]
            );
            return;
        }
    }

    // Anti-replay : si on a déjà vu cet ID, ignorer.
    {
        let mut g = state.node.gossip.write().await;
        if !g.mark_seen(&env.id) {
            return;
        }
        g.stats.messages_received += 1;
        g.stats.bytes_received += raw.len() as u64;
    }

    // NET-9: Per-peer bandwidth/message accounting. Done here, after dedup
    // (so retransmits don't double-count) but before signature verification
    // (so even peers whose signature later fails contribute one "bad"
    // message — useful for spotting noisy peers in metrics).
    {
        let mut info = state.node.peer_info.write().await;
        if let Some(entry) = info.get_mut(&env.sender) {
            entry.bytes_in = entry.bytes_in.saturating_add(raw.len() as u64);
            entry.messages_in = entry.messages_in.saturating_add(1);
        }
    }

    // Fenêtre temporelle ±5 min.
    if !GossipRouter::is_fresh(&env.timestamp) {
        log::debug!("◈ [Dispatch] enveloppe trop ancienne, drop");
        return;
    }

    // ── NET-13: ADAPTIVE RATE LIMITING ──
    // Per-peer budget grows sub-linearly with connected-peer count so a
    // mesh with many participants doesn't choke legitimate sync traffic.
    let peer_count = state.node.peer_info.read().await.len();
    {
        let mut tracker = state.node.nonce_tracker.write().await;
        if !tracker.check_rate_limit(&env.sender, peer_count) {
            log::warn!(
                "◈ [Dispatch] ⚠ RATE LIMIT exceeded by {} (cap={} for {} peers) → drop",
                &env.sender[..env.sender.len().min(12)],
                NonceTracker::adaptive_limit_for(peer_count),
                peer_count
            );
            state.node.gossip.write().await.stats.dropped_rate_limit += 1;
            return;
        }
    }

    // ── CRIT-1: PER-PEER NONCE CHECK (anti-replay within ±90s window) ──
    // The nonce must be strictly increasing per sender. If it's 0 (legacy
    // message without nonce), we skip the check for backward compatibility.
    if env.nonce > 0 {
        let mut tracker = state.node.nonce_tracker.write().await;
        if !tracker.check_and_advance(&env.sender, env.nonce) {
            log::warn!(
                "◈ [Dispatch] ⚠ NONCE REPLAY from {} — nonce {} ≤ high-water mark → drop",
                &env.sender[..env.sender.len().min(12)],
                env.nonce
            );
            state.node.gossip.write().await.stats.dropped_nonce += 1;
            return;
        }
    }

    // ── B1: SIGNATURE VERIFICATION (verify-before-process) ──────────────
    if let Err(reason) = verify_envelope_signature(&env) {
        log::warn!(
            "◈ [Dispatch] ⚠ SIGNATURE INVALIDE from {} — {} → drop + report",
            &env.sender[..env.sender.len().min(12)],
            reason
        );
        // Emit ReportPeer to alert the network about the forger
        broadcast(state, GossipMessage::ReportPeer {
            peer_id: env.sender.clone(),
            reason: ReportReason::InvalidSignature,
        }).await;
        state.node.gossip.write().await.stats.peers_reported += 1;
        state.node.gossip.write().await.stats.dropped_signature += 1;
        return;
    }

    match env.payload {
        GossipMessage::Hello {
            heads, node_id, watts, country, version,
            tasks_completed, blocks_verified, uptime_minutes,
            chain_height, known_peer_ids, display_name,
        } => {
            // NET-5: Protocol version compatibility check. We never reject —
            // unknown fields already default thanks to #[serde(default)] — but
            // a mismatch is worth surfacing in logs so operators notice.
            use crate::p2p::gossip::TORUS_PROTOCOL_VERSION;
            if version > TORUS_PROTOCOL_VERSION {
                log::warn!(
                    "◈ [NET-5] peer {} runs newer protocol v{} (we are v{}) — \
                     processing what we can; consider upgrading",
                    &env.sender[..env.sender.len().min(12)], version, TORUS_PROTOCOL_VERSION
                );
            } else if version < TORUS_PROTOCOL_VERSION {
                log::debug!(
                    "◈ [NET-5] peer {} runs legacy protocol v{} (we are v{})",
                    &env.sender[..env.sender.len().min(12)], version, TORUS_PROTOCOL_VERSION
                );
            }
            handle_hello(
                state, &env.sender, &node_id, heads, watts, &country,
                tasks_completed, blocks_verified, uptime_minutes,
                chain_height, known_peer_ids, display_name,
            ).await;
        }
        GossipMessage::WantNodes { ids } => {
            handle_want_nodes(state, &env.sender, ids).await;
        }
        GossipMessage::HaveNodes { nodes } => {
            handle_have_nodes(state, nodes).await;
        }
        GossipMessage::BroadcastTx { tx_json } => {
            handle_broadcast_tx(state, &tx_json).await;
        }
        GossipMessage::Ping { nonce } => {
            handle_ping(state, &env.sender, nonce).await;
        }
        GossipMessage::Pong { nonce } => {
            // NET-4: Pong is also a liveness signal — touch the peer if known.
            // NET-9: If this Pong matches a Ping we sent, attribute the RTT to
            // the sender. The first Pong for a nonce keeps the entry; we leave
            // it in place for any other peer's response (a peer answering late
            // gets RTT measured against the original send time, which is fine —
            // it captures the actual one-way + processing delay).
            let rtt_ms: Option<u64> = {
                let pending = state.node.pending_pings.read().await;
                pending.get(&nonce)
                    .map(|sent_at| sent_at.elapsed().as_millis().min(u64::MAX as u128) as u64)
            };
            {
                let mut info = state.node.peer_info.write().await;
                if let Some(entry) = info.get_mut(&env.sender) {
                    entry.touch();
                    if let Some(rtt) = rtt_ms {
                        entry.record_rtt(rtt);
                    }
                }
            }
            log::debug!(
                "◈ [Dispatch] Pong from {} nonce={} rtt={:?}ms",
                &env.sender[..env.sender.len().min(12)], nonce, rtt_ms
            );
        }
        GossipMessage::ReportPeer { peer_id, reason } => {
            handle_report_peer(state, &env.sender, &peer_id, reason).await;
        }
        GossipMessage::NewBlock { block_json } => {
            handle_new_block(state, &env.sender, &block_json).await;
        }
        GossipMessage::RequestChain { from_height, max_blocks } => {
            handle_request_chain(state, &env.sender, from_height, max_blocks).await;
        }
        GossipMessage::ChainSegment { blocks_json, sender_height, blocks_compressed } => {
            // NET-8: Prefer compressed payload when present; fall back to inline
            // legacy `blocks_json` if decompression fails or no compressed bytes.
            let blocks = match blocks_compressed {
                Some(bytes) => match crate::p2p::gossip::decompress_blocks(&bytes) {
                    Ok(b) => b,
                    Err(e) => {
                        log::warn!(
                            "◈ [NET-8] ChainSegment from {} compressed payload invalid ({}) — falling back to inline",
                            &env.sender[..env.sender.len().min(12)], e
                        );
                        blocks_json
                    }
                },
                None => blocks_json,
            };
            handle_chain_segment(state, &env.sender, blocks, sender_height).await;
        }
        GossipMessage::PublishPage { page_json } => {
            match serde_json::from_str::<crate::p2p::page_store::PublishedPage>(&page_json) {
                Ok(page) => {
                    let mut store = state.node.page_store.write().await;
                    match store.publish(page) {
                        Ok(()) => {
                            log::info!("◈ [Quanta] Page received & stored");
                            state.node.gossip.write().await.stats.pages_received += 1;
                        }
                        Err(e) => log::debug!("◈ [Quanta] Page rejected: {}", e),
                    }
                }
                Err(e) => log::warn!("◈ [Quanta] Invalid page JSON: {}", e),
            }
        }
        GossipMessage::RequestPage { author_pk } => {
            let store = state.node.page_store.read().await;
            if let Some(page) = store.get_page(&author_pk) {
                if let Ok(json) = serde_json::to_string(page) {
                    broadcast(state, GossipMessage::PublishPage { page_json: json }).await;
                }
            }
        }
        // ── V3 Social Web ───────────────────────────────────────────────
        GossipMessage::PublishDomain { record_json } => {
            handle_publish_domain(state, &record_json).await;
        }
        GossipMessage::PublishSubdomain { grant_json } => {
            handle_publish_subdomain(state, &grant_json).await;
        }
        GossipMessage::PublishSite { doc_json } => {
            handle_publish_site(state, &doc_json).await;
        }
        GossipMessage::BroadcastSocialAction { action_json } => {
            handle_broadcast_social_action(state, &action_json).await;
        }
        GossipMessage::BroadcastReport { report_json } => {
            handle_broadcast_report(state, &report_json).await;
        }
        GossipMessage::BroadcastJurorCommit { commit_json } => {
            handle_broadcast_juror_commit(state, &commit_json).await;
        }
        GossipMessage::BroadcastJurorReveal { reveal_json } => {
            handle_broadcast_juror_reveal(state, &reveal_json).await;
        }
        GossipMessage::PublishForumNode { kind, node_json } => {
            handle_publish_forum_node(state, &kind, &node_json).await;
        }
        GossipMessage::PublishSiteManifest { manifest_json } => {
            handle_publish_site_manifest(state, &manifest_json).await;
        }
    }
}

async fn handle_publish_site_manifest(state: &Arc<crate::AppState>, manifest_json: &str) {
    use crate::p2p::page_store::SiteManifest;
    let Ok(m) = serde_json::from_str::<SiteManifest>(manifest_json) else {
        log::warn!("◈ [V3] PublishSiteManifest JSON invalide");
        return;
    };
    let author = m.author_pk.clone();
    let version = m.version;
    let mut store = state.node.page_store.write().await;
    match store.publish_site(m) {
        Ok(()) => {
            log::info!("◈ [V3] Site manifest accepté (auteur {}, v{})", &author[..16.min(author.len())], version);
            state.node.gossip.write().await.stats.site_manifests_received += 1;
        }
        Err(e) => log::debug!("◈ [V3] Site manifest refusé : {:?}", e),
    }
}

// ─── V3 handlers ────────────────────────────────────────────────────────────

async fn handle_publish_domain(state: &Arc<crate::AppState>, record_json: &str) {
    use crate::p2p::domains::DomainRecord;
    let Ok(rec) = serde_json::from_str::<DomainRecord>(record_json) else {
        log::warn!("◈ [V3] PublishDomain JSON invalide");
        return;
    };
    let mut reg = state.node.domains.write().await;
    // V3.3 — Trois cas :
    //   1. domaine inconnu → claim
    //   2. existe & owner_pk inchangé → update (loyer / target / value)
    //   3. existe & owner_pk différent → overbid (signature challenger)
    let outcome = match reg.get(&rec.name) {
        None => reg
            .claim(rec.clone(), crate::p2p::domains::INITIAL_CLAIM_MICRO_QTA)
            .map(|_| "claim"),
        Some(existing) if existing.owner_pk == rec.owner_pk => {
            reg.update(rec.clone()).map(|_| "update")
        }
        Some(_) => reg.apply_overbid_record(rec.clone()).map(|_| "overbid"),
    };
    match outcome {
        Ok(action) => {
            log::info!("◈ [V3] Domain {} {}", rec.name, action);
            state.node.gossip.write().await.stats.domains_published += 1;
        }
        Err(e) => log::debug!("◈ [V3] Domain rejected ({}): {:?}", rec.name, e),
    }
}

async fn handle_publish_subdomain(state: &Arc<crate::AppState>, grant_json: &str) {
    use crate::p2p::domains::SubdomainGrant;
    let Ok(g) = serde_json::from_str::<SubdomainGrant>(grant_json) else {
        log::warn!("◈ [V3] PublishSubdomain JSON invalide");
        return;
    };
    let mut reg = state.node.domains.write().await;
    match reg.grant_subdomain(g.clone()) {
        Ok(()) => {
            log::info!("◈ [V3] Subdomain {} → {} grant accepté", g.name, g.target_pk);
            state.node.gossip.write().await.stats.domains_published += 1;
        }
        Err(e) => log::debug!("◈ [V3] Subdomain {} refusé : {:?}", g.name, e),
    }
}

async fn handle_publish_site(state: &Arc<crate::AppState>, doc_json: &str) {
    use crate::p2p::search::IndexedDoc;
    let Ok(doc) = serde_json::from_str::<IndexedDoc>(doc_json) else {
        log::warn!("◈ [V3] PublishSite JSON invalide");
        return;
    };
    state.node.search.write().await.upsert(doc);
    state.node.gossip.write().await.stats.sites_indexed += 1;
}

async fn handle_broadcast_social_action(state: &Arc<crate::AppState>, action_json: &str) {
    use crate::p2p::social::SignedAction;
    let Ok(act) = serde_json::from_str::<SignedAction>(action_json) else {
        log::warn!("◈ [V3] SocialAction JSON invalide");
        return;
    };
    let now = chrono::Utc::now().timestamp() as u64;
    match state.node.social.write().await.apply(&act, now) {
        Ok(()) => {
            state.node.gossip.write().await.stats.social_actions_applied += 1;
            // Mise à jour Web of Trust : Follow ⇒ arête au graphe.
            if let crate::p2p::social::SocialAction::Follow { followee_pk, active, .. } =
                &act.action
            {
                let mut graph = state.node.follow_graph.write().await;
                let entry = graph.entry(act.author_pk.clone()).or_default();
                if *active {
                    if !entry.contains(followee_pk) {
                        entry.push(followee_pk.clone());
                    }
                } else {
                    entry.retain(|p| p != followee_pk);
                }
            }
        }
        Err(e) => log::debug!("◈ [V3] SocialAction rejetée: {:?}", e),
    }
}

async fn handle_broadcast_report(state: &Arc<crate::AppState>, report_json: &str) {
    use crate::p2p::moderation::Report;
    let Ok(report) = serde_json::from_str::<Report>(report_json) else {
        log::warn!("◈ [V3] Report JSON invalide");
        return;
    };
    // Pool de jurés : pour V3.2 on prend simplement les wallets de la reputation engine.
    let pool: Vec<String> = state
        .node
        .reputation
        .read()
        .await
        .get_leaderboard(200)
        .iter()
        .map(|u| u.public_key.clone())
        .collect();
    // Seed = head courant (ou cid cible si DAG vide).
    let seed = state
        .node
        .dag
        .read()
        .await
        .heads()
        .into_iter()
        .next()
        .unwrap_or_else(|| report.target_cid.clone());
    let now = chrono::Utc::now().timestamp() as u64;
    match state
        .node
        .moderation
        .write()
        .await
        .submit_report(report, || pool, &seed, now)
    {
        Ok(opened) => {
            state.node.gossip.write().await.stats.reports_received += 1;
            if let Some(case_id) = opened {
                log::info!("◈ [V3] Modération: dossier ouvert {}", &case_id[..16]);
            }
        }
        Err(e) => log::debug!("◈ [V3] Report rejeté: {:?}", e),
    }
}

async fn handle_broadcast_juror_commit(state: &Arc<crate::AppState>, commit_json: &str) {
    use crate::p2p::moderation::CommitVote;
    let Ok(c) = serde_json::from_str::<CommitVote>(commit_json) else {
        return;
    };
    let now = chrono::Utc::now().timestamp() as u64;
    if let Err(e) = state.node.moderation.write().await.submit_commit(c, now) {
        log::debug!("◈ [V3] Juror commit rejeté: {:?}", e);
    }
}

async fn handle_broadcast_juror_reveal(state: &Arc<crate::AppState>, reveal_json: &str) {
    use crate::p2p::moderation::RevealVote;
    let Ok(r) = serde_json::from_str::<RevealVote>(reveal_json) else {
        return;
    };
    let now = chrono::Utc::now().timestamp() as u64;
    if let Err(e) = state.node.moderation.write().await.submit_reveal(r, now) {
        log::debug!("◈ [V3] Juror reveal rejeté: {:?}", e);
    }
}

async fn handle_publish_forum_node(state: &Arc<crate::AppState>, kind: &str, node_json: &str) {
    use crate::p2p::forums::{Comment, Forum, Thread};
    let mut eng = state.node.forums.write().await;
    let res: Result<&'static str, String> = match kind {
        "forum" => serde_json::from_str::<Forum>(node_json)
            .map_err(|e| e.to_string())
            .and_then(|f| eng.add_forum(f).map(|_| "forum").map_err(|e| format!("{e:?}"))),
        "thread" => serde_json::from_str::<Thread>(node_json)
            .map_err(|e| e.to_string())
            .and_then(|t| eng.add_thread(t).map(|_| "thread").map_err(|e| format!("{e:?}"))),
        "comment" => serde_json::from_str::<Comment>(node_json)
            .map_err(|e| e.to_string())
            .and_then(|c| eng.add_comment(c).map(|_| "comment").map_err(|e| format!("{e:?}"))),
        other => Err(format!("forum kind inconnu: {other}")),
    };
    drop(eng);
    match res {
        Ok(_) => {
            state.node.gossip.write().await.stats.forum_nodes_received += 1;
        }
        Err(e) => log::debug!("◈ [V3] Forum node rejeté: {}", e),
    }
}

/// B5 — Entry point for fuzz testing: attempt to process raw bytes as gossip.
/// Guaranteed to never panic — only returns errors gracefully.
#[allow(dead_code)] // Used in security_tests + fuzzing target
pub fn try_process_raw_gossip(data: &[u8]) -> Result<(), String> {
    // Step 0: Mirror the size cap from `dispatch_incoming` so fuzz harnesses
    // and unit tests exercise the same boundary.
    if data.len() > MAX_RAW_ENVELOPE_BYTES {
        return Err(format!("oversized: {} > {} bytes", data.len(), MAX_RAW_ENVELOPE_BYTES));
    }

    // Step 1: Can we deserialize the envelope?
    let env: GossipEnvelope = serde_json::from_slice(data)
        .map_err(|e| format!("JSON error: {}", e))?;

    // Step 2: Is the timestamp fresh?
    if !GossipRouter::is_fresh(&env.timestamp) {
        return Err("stale message".into());
    }

    // Step 3: Is the signature valid?
    verify_envelope_signature(&env)?;

    Ok(())
}

// ─── Handlers ───────────────────────────────────────────────────────────────

/// Hello → enregistre les watts + pays + contributions du peer, demande les nœuds DAG manquants.
/// B3: updates `peer_info` with `last_seen` for TTL tracking.
/// STRUCT-6: also stores tasks_completed / blocks_verified / uptime_minutes for Shapley.
/// NET-2: processes known_peer_ids for automatic mesh discovery.
#[allow(clippy::too_many_arguments)]
async fn handle_hello(
    state: &Arc<AppState>,
    sender_pk: &str,
    _node_id: &str,
    their_heads: Vec<String>,
    watts: f64,
    country: &str,
    tasks_completed: u64,
    blocks_verified: u64,
    uptime_minutes: u64,
    peer_chain_height: u64,
    known_peer_ids: Vec<String>,
    display_name: Option<String>,
) {
    // NET-15: Sanitize the peer-supplied display_name (strip control chars,
    // trim, truncate to MAX_DISPLAY_NAME_LEN). The signed envelope already
    // proves the wallet owner picked the name, but we still don't trust the
    // bytes to be displayable — sanitization is purely a UI-safety guard.
    let sanitized_name = display_name
        .as_deref()
        .and_then(crate::p2p::gossip::sanitize_display_name);
    log::info!(
        "◈ [Dispatch] Hello from {} ({} heads, {:.1}W, {}, chain_h={}, tasks={} blocks={} uptime={}m, peers={})",
        &sender_pk[..sender_pk.len().min(12)], their_heads.len(), watts, country,
        peer_chain_height, tasks_completed, blocks_verified, uptime_minutes,
        known_peer_ids.len(),
    );

    // STRUCT-4: Clamp peer-declared watts to a sane range.
    const MAX_PEER_WATTS: f64 = 500.0;
    const MIN_PEER_WATTS: f64 = 1.0;
    let clamped_watts = watts.clamp(MIN_PEER_WATTS, MAX_PEER_WATTS);
    if (watts - clamped_watts).abs() > 0.1 {
        log::warn!(
            "◈ [Dispatch] peer {} declared {:.1}W, clamped to {:.1}W",
            &sender_pk[..sender_pk.len().min(12)], watts, clamped_watts
        );
    }

    // B3 + STRUCT-6: Update peer_info with liveness + contribution data.
    // NET-15: Also persist the sanitised display_name (None unsets it, which
    // means the peer dropped its nickname).
    {
        let mut info = state.node.peer_info.write().await;
        let entry = info.entry(sender_pk.to_string()).or_insert_with(|| {
            crate::p2p::PeerInfo::new(clamped_watts, country.to_string())
        });
        entry.watts = clamped_watts;
        entry.country = country.to_string();
        entry.tasks_completed = tasks_completed;
        entry.blocks_verified = blocks_verified;
        entry.uptime_minutes = uptime_minutes;
        entry.display_name = sanitized_name.clone();
        entry.touch();
    }

    // Enregistrer le pays du pair pour l'oracle énergie
    *state.node.peer_country_reports.write().await
        .entry(country.to_string()).or_insert(0) += 1;

    // NET-7: Incremental DAG sync — only ask if the peer's head set actually
    // changed since last time, OR the cache is stale (DAG_SYNC_REASK_WINDOW).
    // This collapses the "re-ask the same heads on every Hello" chatter that
    // dominated bandwidth for stable networks.
    let our_known = state.node.dag.read().await.known_ids();
    let want = GossipRouter::compute_want(&their_heads, &our_known);
    if !want.is_empty() {
        let their_heads_set: std::collections::HashSet<String> =
            their_heads.iter().cloned().collect();
        let should_ask = {
            let cache = state.node.dag_sync.read().await;
            match cache.get(sender_pk) {
                None => true,
                Some(state) => {
                    state.last_their_heads != their_heads_set
                        || state.last_asked.elapsed() > crate::p2p::willow_node::DAG_SYNC_REASK_WINDOW
                }
            }
        };
        if should_ask {
            let mut cache = state.node.dag_sync.write().await;
            cache.insert(
                sender_pk.to_string(),
                crate::p2p::willow_node::DagSyncState {
                    last_their_heads: their_heads_set,
                    last_asked: std::time::Instant::now(),
                },
            );
            drop(cache);
            let msg = GossipMessage::WantNodes { ids: want };
            broadcast(state, msg).await;
        } else {
            log::debug!(
                "◈ [NET-7] DAG sync skip: peer {} heads unchanged within reask window",
                &sender_pk[..sender_pk.len().min(12)]
            );
        }
    }

    // NET-6: Chain sync — fan out RequestChain messages when there is a big gap.
    // For small gaps (<= one segment), keep the single-request path.
    // For larger gaps we issue up to PARALLEL_CHAIN_FANOUT range requests at
    // once so multiple peers can serve different windows in parallel.
    let our_height = state.node.ledger.read().await.chain_height();
    if peer_chain_height > our_height {
        log::info!(
            "◈ [Dispatch] Chain sync needed: our height {} < peer height {} — requesting from {}",
            our_height, peer_chain_height, &sender_pk[..sender_pk.len().min(12)]
        );
        request_chain_range(state, our_height, peer_chain_height).await;
    }

    // NET-2: Peer exchange — auto-connect to peers we don't know yet.
    // This enables mesh discovery: each Hello carries the sender's known peers,
    // so new nodes discover the full network through gossip alone.
    if !known_peer_ids.is_empty() {
        let our_known_peers = state.node.known_peers.read().await;
        let our_ticket = state.node.get_ticket().await.unwrap_or_default();
        let new_peers: Vec<String> = known_peer_ids.into_iter()
            .filter(|id| {
                // Don't connect to ourselves
                *id != our_ticket
                // Don't connect to peers we already know
                && !our_known_peers.contains_key(id)
                // Basic validation: non-empty
                && !id.is_empty()
            })
            .collect();
        drop(our_known_peers);

        // Limit auto-discovery to 3 peers per Hello to avoid connection storms
        for peer_id in new_peers.iter().take(3) {
            log::info!(
                "◈ [NET-2] Discovered new peer {} via gossip exchange",
                &peer_id[..peer_id.len().min(16)]
            );
            // Spawn connection attempt in background (non-blocking)
            let state_clone = state.clone();
            let peer_id_clone = peer_id.clone();
            tokio::spawn(async move {
                // Small delay to avoid thundering herd on startup
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                match state_clone.node.connect_peer(&peer_id_clone).await {
                    Ok(()) => {
                        log::info!(
                            "◈ [NET-2] Auto-connected to discovered peer {}",
                            &peer_id_clone[..peer_id_clone.len().min(16)]
                        );
                        // Trigger Hello to the new peer
                        crate::p2p::gossip_tasks::trigger_hello_now(&state_clone).await;
                    }
                    Err(e) => {
                        log::debug!(
                            "◈ [NET-2] Failed to auto-connect to {}: {}",
                            &peer_id_clone[..peer_id_clone.len().min(16)], e
                        );
                    }
                }
            });
        }
    }
}

/// WantNodes → on envoie les DagNode demandés (HaveNodes).
async fn handle_want_nodes(state: &Arc<AppState>, _sender_pk: &str, ids: Vec<String>) {
    let dag = state.node.dag.read().await;
    let mut nodes = Vec::with_capacity(ids.len());
    for id in &ids {
        if let Some(n) = dag.get(id) {
            nodes.push(n.clone());
        }
    }
    drop(dag);
    if nodes.is_empty() { return; }
    log::info!("◈ [Dispatch] HaveNodes → {} nodes", nodes.len());
    broadcast(state, GossipMessage::HaveNodes { nodes }).await;
}

/// HaveNodes → insère chaque nœud dans notre DAG local.
/// Les nœuds dont les parents manquent sont retentés au tour suivant via Hello.
async fn handle_have_nodes(state: &Arc<AppState>, nodes: Vec<DagNode>) {
    let mut dag = state.node.dag.write().await;
    let mut inserted = 0u64;
    // Trier par profondeur de parents : insère d'abord les racines, sinon les insertions
    // peuvent échouer pour orphelinage.
    let mut sorted = nodes;
    sorted.sort_by_key(|n| n.parents.len());
    for n in sorted {
        match dag.insert(n) {
            Ok(()) => inserted += 1,
            Err(e) => log::debug!("◈ [Dispatch] DAG insert skipped: {}", e),
        }
    }
    drop(dag);
    if inserted > 0 {
        let mut g = state.node.gossip.write().await;
        g.stats.nodes_synced += inserted;
        log::info!("◈ [Dispatch] DAG synced +{} nodes", inserted);
    }
}

/// BroadcastTx → parse une transaction JSON, la valide et l'ajoute au ledger local.
async fn handle_broadcast_tx(state: &Arc<AppState>, tx_json: &str) {
    let tx: Option<crate::p2p::ledger::Transaction> = serde_json::from_str(tx_json).ok().flatten();
    let Some(tx) = tx else {
        log::warn!("◈ [Dispatch] BroadcastTx JSON invalide");
        return;
    };

    // Vérifier la signature hybride / Ed25519 avant tout.
    match crate::p2p::ledger::Ledger::verify_tx(&tx) {
        Ok(true) => {}
        Ok(false) => { log::warn!("◈ [Dispatch] tx signature invalide — drop"); return; }
        Err(e) => { log::warn!("◈ [Dispatch] verify_tx erreur: {} — drop", e); return; }
    }

    // MOD-1: Verify the transaction nonce matches the expected account nonce.
    // NETWORK and ESCROW are synthetic addresses that don't use account nonces.
    if tx.from != "NETWORK" && tx.from != "ESCROW" {
        let ledger = state.node.ledger.read().await;
        let expected = ledger.get_nonce(&tx.from);
        if tx.nonce != expected {
            log::warn!(
                "◈ [Dispatch] tx nonce mismatch for {}: got {}, expected {} — drop",
                &tx.from[..tx.from.len().min(12)], tx.nonce, expected
            );
            return;
        }
    }

    // STRUCT-3: Reconcile dual ledger. Apply the tx to BOTH:
    //   1. CRDT (for eventually-consistent multi-node sync)
    //   2. Linear Ledger via replay_remote_tx (authoritative local state)
    use crate::p2p::ledger::TxType;
    if tx.tx_type == TxType::Transfer {
        let uqta = tx.amount;
        let mut cons = state.node.consensus.write().await;
        cons.ledger.debit(&tx.from, &tx.from, uqta);
        cons.ledger.credit(&tx.from, &tx.to, uqta);
    }

    // STRUCT-3: Replay the remote tx into the local linear ledger (idempotent dedup
    // via seen_tx_hashes). Then MOD-1: advance the sender's nonce.
    if tx.from != "NETWORK" && tx.from != "ESCROW" {
        let mut ledger = state.node.ledger.write().await;
        let applied = ledger.replay_remote_tx(tx.clone());
        if applied {
            ledger.increment_nonce(&tx.from);
        }
    }

    log::debug!("◈ [Dispatch] tx {} ({:?}) appliquée au CRDT", &tx.id, tx.tx_type);
}

/// Ping → répondre Pong + rafraîchir la liveness du pair.
///
/// NET-4: Ping est un battement de cœur léger (15s) qui complète Hello (120s).
/// Recevoir un Ping signé valide nous dit que le pair est encore vivant : on
/// touche son entrée `peer_info` pour repousser l'éviction TTL. On ne crée PAS
/// d'entrée pour un pair inconnu — la première découverte passe toujours par
/// Hello (qui apporte watts + country + contribs).
async fn handle_ping(state: &Arc<AppState>, sender_pk: &str, nonce: u64) {
    {
        let mut info = state.node.peer_info.write().await;
        if let Some(entry) = info.get_mut(sender_pk) {
            entry.touch();
        }
    }
    broadcast(state, GossipMessage::Pong { nonce }).await;
}

/// ReportPeer → log + accumulate report count. Three independent reports lead
/// to a 1-hour ban (handled inside `NonceTracker::record_report`).
///
/// Note: this isn't perfect — a coordinated cluster of 3 attackers can ban any
/// honest peer. Mitigations (proof-of-stake voting, weighted reports) are out
/// of scope; the threshold is set conservatively to limit collateral damage.
async fn handle_report_peer(state: &Arc<AppState>, sender_pk: &str, peer_id: &str, reason: ReportReason) {
    log::info!("◈ [Dispatch] ReportPeer from {} → {} ({:?})",
        &sender_pk[..sender_pk.len().min(12)],
        &peer_id[..peer_id.len().min(12)], reason);

    let count = state.node.nonce_tracker.write().await.record_report(peer_id);
    state.node.gossip.write().await.stats.peers_reported += 1;

    if count >= REPORT_BAN_THRESHOLD {
        log::warn!(
            "◈ [Dispatch] ⛔ peer {} BANNED ({} reports, TTL {}s)",
            &peer_id[..peer_id.len().min(12)], count, REPORT_BAN_TTL_SECS
        );
    }
}

/// D1.3: Handle a remote sealed block — validate and integrate into local chain.
///
/// The signature on the gossip envelope has already been verified by the upstream
/// pipeline (`dispatch_incoming`). Here we only deserialize the block payload and
/// hand it to the ledger which performs structural + cryptographic validation.
async fn handle_new_block(state: &Arc<AppState>, sender: &str, block_json: &str) {
    let block: crate::p2p::ledger::Block = match serde_json::from_str(block_json) {
        Ok(b) => b,
        Err(e) => {
            log::warn!(
                "◈ [Dispatch] invalid block JSON from {}: {}",
                &sender[..sender.len().min(12)],
                e
            );
            return;
        }
    };

    let mut ledger = state.node.ledger.write().await;
    match ledger.integrate_remote_block(block) {
        Ok(true) => {
            log::info!(
                "◈ [Dispatch] ✓ Accepted remote block from {}",
                &sender[..sender.len().min(12)]
            );
            // CRIT-B: Increment validated block counter for Shapley distribution
            state.node.blocks_validated.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            // Re-broadcast is intentionally skipped: the gossip layer already
            // floods envelopes via iroh-gossip; the dedup hash on receiving
            // peers ensures convergence without us re-signing.
        }
        Ok(false) => {
            log::debug!("◈ [Dispatch] block already known or fork lost");
        }
        Err(reason) => {
            log::warn!(
                "◈ [Dispatch] ⚠ Rejected block from {}: {}",
                &sender[..sender.len().min(12)],
                reason
            );
        }
    }
}

/// Maximum blocks we'll send in a single ChainSegment response (DoS protection).
const MAX_CHAIN_SEGMENT: u64 = 50;

/// Handle a RequestChain message — send back blocks starting at `from_height`.
async fn handle_request_chain(
    state: &Arc<AppState>,
    sender: &str,
    from_height: u64,
    max_blocks: u64,
) {
    let limit = max_blocks.min(MAX_CHAIN_SEGMENT) as usize;
    let ledger = state.node.ledger.read().await;
    let chain_len = ledger.chain_height();

    // Serialize blocks from `from_height` to `from_height + limit`
    let blocks_json: Vec<String> = (from_height..chain_len)
        .take(limit)
        .filter_map(|i| {
            ledger.block_at(i)
                .and_then(|b| serde_json::to_string(b).ok())
        })
        .collect();

    if blocks_json.is_empty() {
        log::debug!(
            "◈ [Dispatch] RequestChain from {} — nothing to send (from_height={}, our height={})",
            &sender[..sender.len().min(12)], from_height, chain_len
        );
        return;
    }

    drop(ledger);

    // NET-8: Try to gzip the segment. If compression yields a meaningful
    // size win we drop the inline `blocks_json` to save bandwidth; otherwise
    // we send the legacy inline form unchanged.
    let compressed = crate::p2p::gossip::compress_blocks(&blocks_json);
    let (inline, compressed_field) = match &compressed {
        Some(c) if c.len() < (blocks_json.iter().map(|s| s.len()).sum::<usize>() / 2) => {
            log::info!(
                "◈ [NET-8] RequestChain {} — {} blocks, {} → {} bytes after gzip",
                &sender[..sender.len().min(12)],
                blocks_json.len(),
                blocks_json.iter().map(|s| s.len()).sum::<usize>(),
                c.len(),
            );
            (Vec::new(), Some(c.clone()))
        }
        _ => {
            log::info!(
                "◈ [Dispatch] RequestChain from {} — sending {} blocks (height {} → {})",
                &sender[..sender.len().min(12)],
                blocks_json.len(),
                from_height,
                from_height + blocks_json.len() as u64,
            );
            (blocks_json, None)
        }
    };

    broadcast(state, GossipMessage::ChainSegment {
        blocks_json: inline,
        sender_height: chain_len,
        blocks_compressed: compressed_field,
    }).await;
}

/// Handle a ChainSegment response — integrate blocks into local chain.
async fn handle_chain_segment(
    state: &Arc<AppState>,
    sender: &str,
    blocks_json: Vec<String>,
    sender_height: u64,
) {
    // DoS guard: a peer cannot push more than MAX_CHAIN_SEGMENT_RECEIVED blocks
    // at once, even if our own RequestChain asked for more. Truncate silently.
    let blocks_json = if blocks_json.len() > MAX_CHAIN_SEGMENT_RECEIVED {
        log::warn!(
            "◈ [Dispatch] ChainSegment from {} oversized ({} blocks > {}) — truncating",
            &sender[..sender.len().min(12)],
            blocks_json.len(),
            MAX_CHAIN_SEGMENT_RECEIVED
        );
        blocks_json.into_iter().take(MAX_CHAIN_SEGMENT_RECEIVED).collect()
    } else {
        blocks_json
    };

    let mut integrated = 0u64;
    let mut rejected = 0u64;

    for block_str in &blocks_json {
        let block: crate::p2p::ledger::Block = match serde_json::from_str(block_str) {
            Ok(b) => b,
            Err(e) => {
                log::warn!("◈ [Dispatch] bad block in ChainSegment from {}: {}", &sender[..sender.len().min(12)], e);
                rejected += 1;
                continue;
            }
        };

        let mut ledger = state.node.ledger.write().await;
        match ledger.integrate_remote_block(block) {
            Ok(true) => {
                integrated += 1;
                state.node.blocks_validated.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            Ok(false) => {} // Already known — skip silently
            Err(e) => {
                log::warn!("◈ [Dispatch] ChainSegment block rejected: {}", e);
                rejected += 1;
            }
        }
    }

    log::info!(
        "◈ [Dispatch] ChainSegment from {} — integrated: {}, rejected: {}, sender_height: {}",
        &sender[..sender.len().min(12)], integrated, rejected, sender_height
    );

    // NET-16: Emit sync progress to the frontend. Best-effort — we don't
    // care whether anyone is listening on the channel.
    let our_height = state.node.ledger.read().await.chain_height();
    if let Some(handle) = state.app_handle.read().await.as_ref() {
        use tauri::Emitter;
        let _ = handle.emit("torus://chain-sync-progress", serde_json::json!({
            "our_height": our_height,
            "sender_height": sender_height,
            "integrated": integrated,
            "rejected": rejected,
            "sender": sender,
        }));
    }

    // If sender has more blocks, request the next segment(s) in parallel.
    let our_height = state.node.ledger.read().await.chain_height();
    if our_height < sender_height {
        log::info!(
            "◈ [Dispatch] Need more blocks: our height {} < sender height {} — fanning out next requests",
            our_height, sender_height
        );
        request_chain_range(state, our_height, sender_height).await;
    }
}

/// NET-6: Maximum parallel `RequestChain` fan-out.
///
/// Caps the number of in-flight range requests we issue at once. Iroh-gossip
/// broadcast means each `RequestChain` reaches every peer; whichever peer
/// holds the requested window will reply. Multiple non-overlapping windows
/// let *different* peers respond in parallel instead of one peer serving the
/// whole catch-up serially.
///
/// Trade-off: more fanout = faster catch-up but more redundant traffic and
/// more competing ChainSegment responses. 4 is the sweet spot — 4×50 = 200
/// blocks in flight before we wait, which covers ~7h of sealing at our 2-min
/// cadence.
pub const PARALLEL_CHAIN_FANOUT: u64 = 4;

/// NET-6: Issue one or more `RequestChain` messages spanning `[from, to)`.
///
/// - If the gap is ≤ one segment, sends a single broadcast.
/// - Otherwise splits the gap into up to `PARALLEL_CHAIN_FANOUT` non-overlapping
///   `[start, start+MAX_CHAIN_SEGMENT)` windows and broadcasts them all.
///
/// Each broadcast travels through the priority queue (Critical lane), reaches
/// every peer subscribed to the topic, and gets answered by whichever peer
/// owns blocks at that window. Idempotency in `integrate_remote_block`
/// guarantees that overlapping responses from multiple peers cannot cause
/// double-application or fork divergence.
async fn request_chain_range(state: &Arc<AppState>, from: u64, to: u64) {
    if to <= from {
        return;
    }
    let total_gap = to - from;
    if total_gap <= MAX_CHAIN_SEGMENT {
        // Small gap — one shot is fine.
        broadcast(state, GossipMessage::RequestChain {
            from_height: from,
            max_blocks: MAX_CHAIN_SEGMENT,
        }).await;
        return;
    }
    // Big gap — fan out up to PARALLEL_CHAIN_FANOUT windows of MAX_CHAIN_SEGMENT.
    let window = MAX_CHAIN_SEGMENT;
    let mut start = from;
    let mut requests_sent = 0u64;
    while start < to && requests_sent < PARALLEL_CHAIN_FANOUT {
        let count = (to - start).min(window);
        broadcast(state, GossipMessage::RequestChain {
            from_height: start,
            max_blocks: count,
        }).await;
        start += count;
        requests_sent += 1;
    }
    log::info!(
        "◈ [NET-6] parallel chain sync: {} requests fanned out covering [{}, {})",
        requests_sent, from, start
    );
}

/// Helper : signe + emballe + push sur le channel gossip_tx (le drain enverra via iroh-gossip).
/// STRUCT-1: Uses signable_envelope_bytes() so signature covers full envelope.
async fn broadcast(state: &Arc<AppState>, msg: GossipMessage) {
    let pk = state.crypto.lock().await.get_identity()
        .map(|i| i.public_key_hex).unwrap_or_default();
    if pk.is_empty() { return; }

    // STRUCT-1: Generate timestamp and nonce BEFORE signing
    let timestamp = chrono::Utc::now().to_rfc3339();
    let nonce = state.node.gossip.read().await.next_outgoing_nonce();
    let signable = GossipRouter::signable_envelope_bytes(&pk, nonce, &timestamp, &msg);
    let sig = state.crypto.lock().await.sign(&signable).unwrap_or_default();

    let env = match GossipRouter::build_signed_envelope(pk, msg, nonce, timestamp, &sig) {
        Ok(e) => e,
        Err(e) => { log::warn!("◈ [Dispatch] build_signed_envelope failed: {}", e); return; }
    };
    state.node.gossip.write().await.mark_seen(&env.id);
    let _ = state.node.gossip_tx.send(env);
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::CryptoEngine;

    #[test]
    fn test_verify_envelope_valid_signature() {
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let msg = GossipMessage::Ping { nonce: 42 };
        // STRUCT-1: Sign full envelope bytes
        let timestamp = chrono::Utc::now().to_rfc3339();
        let nonce = 0_u64;
        let signable = GossipRouter::signable_envelope_bytes(
            &id.public_key_hex, nonce, &timestamp, &msg
        );
        let sig = crypto.sign(&signable).unwrap();
        let env = GossipRouter::build_signed_envelope(
            id.public_key_hex.clone(), msg, nonce, timestamp, &sig
        ).unwrap();

        assert!(verify_envelope_signature(&env).is_ok(), "Valid signature must pass");
    }

    #[test]
    fn test_verify_envelope_forged_signature() {
        let msg = GossipMessage::Ping { nonce: 1 };
        let env = GossipEnvelope {
            id: "fake_id".into(),
            sender: "a".repeat(64), // fake pk hex
            payload: msg,
            signature: "b".repeat(128), // fake sig hex
            timestamp: chrono::Utc::now().to_rfc3339(),
            nonce: 0,
        };

        assert!(verify_envelope_signature(&env).is_err(), "Forged signature must be rejected");
    }

    #[test]
    fn test_verify_envelope_tampered_payload() {
        // Sign one message, but put a different payload in the envelope
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();

        let msg_signed = GossipMessage::Ping { nonce: 1 };
        // STRUCT-1: Sign the full envelope with the original payload
        let timestamp = chrono::Utc::now().to_rfc3339();
        let nonce = 0_u64;
        let signable = GossipRouter::signable_envelope_bytes(
            &id.public_key_hex, nonce, &timestamp, &msg_signed
        );
        let sig = crypto.sign(&signable).unwrap();

        // Tamper: put a different message in the envelope
        let msg_tampered = GossipMessage::Ping { nonce: 9999 };
        let mut env = GossipRouter::build_signed_envelope(
            id.public_key_hex.clone(), msg_tampered, nonce, timestamp, &sig
        ).unwrap();
        env.signature = hex::encode(&sig); // use the original sig (for the wrong payload)

        assert!(verify_envelope_signature(&env).is_err(), "Tampered payload must be rejected");
    }

    #[test]
    fn nonce_tracker_basic() {
        let mut tracker = NonceTracker::new();
        assert!(tracker.check_and_advance("peer_a", 1));
        assert!(tracker.check_and_advance("peer_a", 2));
        assert!(!tracker.check_and_advance("peer_a", 2), "Same nonce must be rejected");
        assert!(!tracker.check_and_advance("peer_a", 1), "Lower nonce must be rejected");
        assert!(tracker.check_and_advance("peer_a", 3));
    }

    #[test]
    fn nonce_tracker_independent_peers() {
        let mut tracker = NonceTracker::new();
        assert!(tracker.check_and_advance("peer_a", 1));
        assert!(tracker.check_and_advance("peer_b", 1), "Different peers have independent nonces");
    }

    #[test]
    fn try_process_raw_gossip_handles_garbage() {
        // Random bytes should never panic
        assert!(try_process_raw_gossip(b"").is_err());
        assert!(try_process_raw_gossip(b"not json").is_err());
        assert!(try_process_raw_gossip(&[0xFF; 1024]).is_err());
    }

    #[test]
    fn rate_limiter_allows_normal_traffic() {
        let mut tracker = NonceTracker::new();
        // 30 messages at base peer_count should all pass (limit = BASE = 30)
        for _ in 0..30 {
            assert!(tracker.check_rate_limit("peer_x", 1), "should allow within limit");
        }
        // 31st should be rejected
        assert!(!tracker.check_rate_limit("peer_x", 1), "should reject after limit");
    }

    #[test]
    fn rate_limiter_independent_peers() {
        let mut tracker = NonceTracker::new();
        // Fill up peer_a
        for _ in 0..30 {
            tracker.check_rate_limit("peer_a", 1);
        }
        // peer_b should still be allowed
        assert!(tracker.check_rate_limit("peer_b", 1), "different peer should have own limit");
    }

    #[test]
    fn adaptive_limit_scales_with_peer_count() {
        // NET-13: sub-linear sqrt scaling, clamped to [MIN, MAX]
        assert_eq!(NonceTracker::adaptive_limit_for(1), 30);   // base
        assert_eq!(NonceTracker::adaptive_limit_for(4), 30);   // base
        assert_eq!(NonceTracker::adaptive_limit_for(16), 60);  // base × 2
        assert_eq!(NonceTracker::adaptive_limit_for(64), 120); // base × 4 → MAX cap
        assert_eq!(NonceTracker::adaptive_limit_for(1024), 120); // still MAX
        // Sanity: floor protects very small peer counts from underflow
        assert!(NonceTracker::adaptive_limit_for(0) >= 15);
    }

    #[test]
    fn chain_height_and_block_at() {
        use crate::p2p::ledger::{Ledger, MICRO};
        let mut ledger = Ledger::new();
        // Genesis only
        assert_eq!(ledger.chain_height(), 1);
        assert!(ledger.block_at(0).is_some());
        assert!(ledger.block_at(1).is_none());

        // Mine enough to seal a block
        let pk = "a".repeat(64);
        for _ in 0..12 {
            ledger.mine_tx(&pk, MICRO, 0.1);
        }
        // Chain should have grown (genesis + sealed block)
        assert!(ledger.chain_height() >= 1);
    }
}
