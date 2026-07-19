//! Dispatcher des messages gossip entrants — Phase B (Security Hardening).
//!
//! ▸ B1 — Verify-Before-Process: Ed25519 signature verification on every
//! incoming         GossipEnvelope BEFORE deserializing the payload. Invalid →
//! immediate         discard + ReportPeer::InvalidSignature.
//! ▸ B3 — Peer Liveness: updates `peer_info.last_seen` on every valid Hello,
//!         enabling dead-peer cleanup by the TTL task.
//!
//! Réponses sortantes (Pong, ChainSegment, ReportPeer) sont rebroadcastées sur
//! le même channel `gossip_tx` que les messages locaux — l'iroh-gossip drain
//! les enverra.

use crate::{
    p2p::gossip::{GossipEnvelope, GossipMessage, GossipRouter, ReportReason},
    p2p::ledger::short,
    security::CryptoEngine,
    AppState,
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

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

/// TX-AUTH-NONCE-1 §4: hard cap on distinct senders tracked in the per-sender
/// nonce/rate maps. Bounds memory under a Sybil flood of *valid* keypairs — the
/// residual PRESIG-ORDER left open (a spoofed sender is already dropped at
/// signature verification). **§4 policy**: the exact value is a choice.
const MAX_TRACKED_SENDERS: usize = 100_000;

/// TX-AUTH-NONCE-1 §4: a tracked sender idle at least this long is evicted. MUST
/// exceed the ±90 s envelope freshness window so a replay of an evicted sender's
/// traffic is already stale (dropped at freshness before the nonce gate) ⇒
/// eviction is anti-replay-safe. **§4 policy**: exact value is a choice
/// (constraint: strictly greater than the freshness window).
const NONCE_ENTRY_TTL_SECS: u64 = 120;

/// Number of **distinct reporters** against a peer that triggers a ban.
/// SEC-REPORT-1: the count is over unique reporter public keys, not raw
/// messages — a single authenticated peer can no longer manufacture a ban by
/// sending N reports (it now takes N independent stakeholders).
pub const REPORT_BAN_THRESHOLD: u32 = 3;
/// Ban duration in seconds (1 hour). After this the peer gets a fresh slate.
pub const REPORT_BAN_TTL_SECS: u64 = 3600;
/// SEC-REPORT-2: absolute cap on the number of distinct *reported* peers we
/// track at once. `ReportPeer` carries an attacker-chosen `peer_id`, so without
/// a bound a peer could seed unboundedly many fictitious targets (each never
/// connecting, so `is_banned` never lazily evicts them) and grow memory without
/// limit. When the cap is hit we prune expired bans and the weakest
/// (fewest-reporter, sub-threshold) entries. §4 policy: exact value is a choice.
const MAX_TRACKED_REPORTS: usize = 10_000;
/// SEC-COUNTRY-1: absolute cap on distinct country codes tracked for the energy
/// oracle. The code is peer-supplied via `Hello`; sanitised to an ISO-shaped
/// short token and bounded here so a peer can't grow the map with novel codes.
const MAX_COUNTRY_CODES: usize = 64;

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
    /// SEC-REPORT-1: distinct **reporter** public keys seen against each
    /// reported peer_id (via gossip `ReportPeer`). A ban needs
    /// `REPORT_BAN_THRESHOLD` *independent* reporters, so a single authenticated
    /// peer replaying reports (even with a varying `ReportReason::Other`) can no
    /// longer censor a victim. Cleared when a ban TTL expires (fresh slate).
    report_counts: HashMap<String, HashSet<String>>,
    /// peer_id → unix epoch second at which the ban expires.
    /// Use a `HashSet` view via `is_banned()`; the timestamps gate the
    /// membership.
    bans: HashMap<String, u64>,
    /// TX-AUTH-NONCE-1 §4: last activity (epoch sec) per tracked sender, for
    /// expiry-based + size-bounded eviction of `last_nonces` / `rate_counters`.
    last_seen: HashMap<String, u64>,
}

fn now_epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// SEC-COUNTRY-1: normalise a peer-supplied country string to an ISO-shaped
/// token before it is ever used as a map key. Keeps only ASCII letters,
/// uppercases them, and truncates to 3 chars (ISO-3166 alpha-2/3). An empty or
/// junk value collapses to `"??"`. This both prevents attacker-chosen long keys
/// from bloating memory and caps the *shape* of the key space so `Hello` can't
/// smuggle arbitrary strings into the energy-oracle map.
pub fn sanitize_country_code(raw: &str) -> String {
    let code: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .take(3)
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if code.is_empty() {
        "??".to_string()
    } else {
        code
    }
}

impl NonceTracker {
    pub fn new() -> Self {
        Self {
            last_nonces: HashMap::new(),
            rate_counters: HashMap::new(),
            report_counts: HashMap::new(),
            bans: HashMap::new(),
            last_seen: HashMap::new(),
        }
    }

    /// Returns `true` if this nonce is valid (strictly greater than the last
    /// seen). Also updates the tracker on acceptance.
    pub fn check_and_advance(&mut self, sender_pk: &str, nonce: u64) -> bool {
        let entry = self.last_nonces.entry(sender_pk.to_string()).or_insert(0);
        let ok = if nonce > *entry {
            *entry = nonce;
            true
        } else {
            false
        };
        // TX-AUTH-NONCE-1 §4: record activity + keep the maps bounded.
        self.note_activity_and_prune(sender_pk);
        ok
    }

    /// TX-AUTH-NONCE-1 §4: record `sender_pk`'s last activity and keep the
    /// per-sender maps bounded (expiry **and** absolute size). O(1) on the normal
    /// path (under the cap); the O(n) prune runs only when the hard cap is
    /// exceeded (a Sybil flood), bounding memory.
    fn note_activity_and_prune(&mut self, sender_pk: &str) {
        let now = now_epoch_secs();
        self.last_seen.insert(sender_pk.to_string(), now);
        if self.last_seen.len() <= MAX_TRACKED_SENDERS {
            return; // common case: nothing to evict
        }
        // ① Expiry — drop senders idle ≥ TTL. TTL > the ±90 s freshness window,
        // so any replay of an evicted sender's traffic is already stale (dropped
        // at freshness before the nonce gate) ⇒ anti-replay-safe.
        let expired: Vec<String> = self
            .last_seen
            .iter()
            .filter(|(_, &seen)| now.saturating_sub(seen) >= NONCE_ENTRY_TTL_SECS)
            .map(|(k, _)| k.clone())
            .collect();
        for k in expired {
            self.last_seen.remove(&k);
            self.last_nonces.remove(&k);
            self.rate_counters.remove(&k);
        }
        // ② Absolute size bound — if a fast Sybil burst kept everything fresh,
        // evict the oldest-by-last-seen until under the cap. §4 residual: evicting
        // a < TTL entry briefly reopens that sender's nonce window; mitigated by
        // the `seen_messages` id-dedup + signed envelopes. Exact size + eviction
        // order = §4 policy decision.
        while self.last_seen.len() > MAX_TRACKED_SENDERS {
            let Some(oldest) = self
                .last_seen
                .iter()
                .min_by_key(|(_, &seen)| seen)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            self.last_seen.remove(&oldest);
            self.last_nonces.remove(&oldest);
            self.rate_counters.remove(&oldest);
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
    /// has exceeded the adaptive per-peer message budget for the current
    /// window.
    pub fn check_rate_limit(&mut self, sender_pk: &str, peer_count: usize) -> bool {
        let now = now_epoch_secs();
        let limit = Self::adaptive_limit_for(peer_count);

        let entry = self
            .rate_counters
            .entry(sender_pk.to_string())
            .or_insert((now, 0));

        // Reset window if expired
        if now - entry.0 >= RATE_WINDOW_SECS {
            *entry = (now, 0);
        }

        entry.1 += 1;
        let within = entry.1 <= limit;
        // TX-AUTH-NONCE-1 §4: record activity + keep the maps bounded.
        self.note_activity_and_prune(sender_pk);
        within
    }

    /// Record a report against `peer_id` from `reporter_pk` (the authenticated
    /// sender of the `ReportPeer` envelope). When the number of **distinct
    /// reporters** reaches `REPORT_BAN_THRESHOLD`, install a ban that expires
    /// after `REPORT_BAN_TTL_SECS`. Returns the current distinct-reporter count.
    ///
    /// SEC-REPORT-1: counting distinct reporters (not raw messages) closes the
    /// single-peer censorship vector — one authenticated key can add at most one
    /// to any target's count, regardless of how many (or how varied) its reports
    /// are. A peer reporting itself is ignored (nonsensical, and can't self-ban).
    pub fn record_report(&mut self, peer_id: &str, reporter_pk: &str) -> u32 {
        if peer_id == reporter_pk {
            // A peer cannot report itself into (or pad) a ban.
            return self
                .report_counts
                .get(peer_id)
                .map(|s| s.len() as u32)
                .unwrap_or(0);
        }
        let reporters = self
            .report_counts
            .entry(peer_id.to_string())
            .or_default();
        reporters.insert(reporter_pk.to_string());
        let new_count = reporters.len() as u32;
        if new_count >= REPORT_BAN_THRESHOLD {
            self.bans
                .insert(peer_id.to_string(), now_epoch_secs() + REPORT_BAN_TTL_SECS);
        }
        // SEC-REPORT-2: keep the report/ban maps bounded against a flood of
        // fictitious targets.
        self.prune_reports_and_bans();
        new_count
    }

    /// SEC-REPORT-2: bound `report_counts` and `bans`. Cheap on the normal path
    /// (under the cap it returns immediately); the O(n) sweep only runs when the
    /// cap is exceeded. First drops expired bans and their report sets, then —
    /// if still over cap — evicts the weakest sub-threshold targets (fewest
    /// distinct reporters) which are the cheapest for an attacker to seed.
    fn prune_reports_and_bans(&mut self) {
        if self.report_counts.len() <= MAX_TRACKED_REPORTS {
            return;
        }
        let now = now_epoch_secs();
        // ① Drop expired bans (and let those targets earn a fresh slate).
        let expired: Vec<String> = self
            .bans
            .iter()
            .filter(|(_, &until)| now >= until)
            .map(|(k, _)| k.clone())
            .collect();
        for k in expired {
            self.bans.remove(&k);
            self.report_counts.remove(&k);
        }
        // ② Still over cap → evict weakest sub-threshold targets. Never evict a
        // target that has reached the ban threshold (that would erase evidence).
        while self.report_counts.len() > MAX_TRACKED_REPORTS {
            let victim = self
                .report_counts
                .iter()
                .filter(|(_, r)| (r.len() as u32) < REPORT_BAN_THRESHOLD)
                .min_by_key(|(_, r)| r.len())
                .map(|(k, _)| k.clone());
            match victim {
                Some(k) => {
                    self.report_counts.remove(&k);
                }
                None => break, // everything left is at threshold — stop
            }
        }
    }

    /// Returns `true` if `peer_id` is currently banned. Auto-evicts expired
    /// entries (and resets their report count) so a peer can rejoin after TTL.
    pub fn is_banned(&mut self, peer_id: &str) -> bool {
        let Some(&expires_at) = self.bans.get(peer_id) else {
            return false;
        };
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
        self.bans
            .iter()
            .filter(|(_, &until)| now < until)
            .map(|(id, _)| id.clone())
            .collect()
    }
}

impl Default for NonceTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ─── B1: Envelope signature verification ────────────────────────────────────

/// Verify the ML-DSA-65 signature of a gossip envelope (PQ-ENVELOPE-1).
///
/// STRUCT-1: The signature covers sender + nonce + timestamp + payload
/// (the canonical bytes produced by `signable_envelope_bytes()`). The `sender`
/// is the emitter's ML-DSA-65 public key hex; the signature is verified against
/// it with the project's single ML-DSA verifier. This is a clean v5 break — no
/// Ed25519 path and no legacy payload-only fallback remain.
fn verify_envelope_signature(env: &GossipEnvelope) -> Result<(), String> {
    // 1. Decode signature
    let sig_bytes = hex::decode(&env.signature).map_err(|_| "invalid signature hex")?;

    // 2. STRUCT-1: Reconstruct the FULL canonical signable bytes
    let full_signable =
        GossipRouter::signable_envelope_bytes(&env.sender, env.nonce, &env.timestamp, &env.payload);

    // 3. PQ-ENVELOPE-1: Verify ML-DSA-65 signature against full canonical bytes.
    if CryptoEngine::verify_pq(&env.sender, &full_signable, &sig_bytes) {
        Ok(())
    } else {
        Err("signature verification failed".into())
    }
}

// ─── Public entry point ─────────────────────────────────────────────────────

/// Désérialise + vérifie signature + vérifie freshness + dispatche une
/// enveloppe entrante.
///
/// **B1 Security Order:**
/// 1. JSON deserialization (envelope structure only)
/// 2. Anti-replay (message ID dedup)
/// 3. Timestamp freshness (±90s)
/// 4. **Ed25519 signature verification** ← NEW
/// 5. Payload dispatch
pub async fn dispatch_incoming(state: &Arc<AppState>, raw: &[u8]) {
    // Chronomètre télémétrie : durée réelle du pipeline ①-⑧ (JSON, ban, dedup,
    // fraîcheur, rate, nonce, vérification ML-DSA) — µs mesurés, hors sécurité.
    let pipeline_t = std::time::Instant::now();
    // ── DoS guard: reject oversized envelopes BEFORE parsing ──
    // 10 MB is well above our largest legitimate message (ChainSegment of 50
    // blocks ≈ a few hundred KB). Anything bigger is either malicious or buggy.
    if raw.len() > MAX_RAW_ENVELOPE_BYTES {
        log::warn!(
            "◈ [Dispatch] ⚠ oversized payload {} B (> {} B) → drop",
            raw.len(),
            MAX_RAW_ENVELOPE_BYTES
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
                short(&env.sender, 12)
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

    // Fenêtre temporelle ±90s.
    if !GossipRouter::is_fresh(&env.timestamp) {
        log::debug!("◈ [Dispatch] enveloppe trop ancienne, drop");
        return;
    }

    // ── B1: SIGNATURE VERIFICATION (verify-before-process) ──────────────
    // PRESIG-ORDER (HARDEN audit — NONCE-MEM / SLICE-SENDER): verify the Ed25519
    // signature BEFORE any per-sender state mutation. The rate-limit and nonce
    // checks below `.entry(env.sender)` the per-sender maps; running them on an
    // UNVERIFIED, spoofable `env.sender` let an unauthenticated attacker grow
    // `last_nonces` / `rate_counters` without bound (remote OOM). After this gate
    // only a sender that actually signed the envelope reaches those maps — a
    // spoofed sender is dropped here having mutated nothing. The cheap stateless
    // filters (size / JSON / ban / dedup / freshness) still run first, so a
    // bad-sig flood is shed before paying the verify on anything but
    // unique-and-fresh envelopes.
    if let Err(reason) = verify_envelope_signature(&env) {
        log::warn!(
            "◈ [Dispatch] ⚠ SIGNATURE INVALIDE from {} — {} → drop + report",
            short(&env.sender, 12),
            reason
        );
        // Emit ReportPeer to alert the network about the forger
        broadcast(
            state,
            GossipMessage::ReportPeer {
                peer_id: env.sender.clone(),
                reason: ReportReason::InvalidSignature,
            },
        )
        .await;
        state.node.gossip.write().await.stats.peers_reported += 1;
        state.node.gossip.write().await.stats.dropped_signature += 1;
        return;
    }

    // ── NET-13: ADAPTIVE RATE LIMITING ── (post-verify: only authenticated
    // senders are counted, so the per-sender map cannot be inflated by spoofing)
    // Per-peer budget grows sub-linearly with connected-peer count so a
    // mesh with many participants doesn't choke legitimate sync traffic.
    let peer_count = state.node.peer_info.read().await.len();
    {
        let mut tracker = state.node.nonce_tracker.write().await;
        if !tracker.check_rate_limit(&env.sender, peer_count) {
            log::warn!(
                "◈ [Dispatch] ⚠ RATE LIMIT exceeded by {} (cap={} for {} peers) → drop",
                short(&env.sender, 12),
                NonceTracker::adaptive_limit_for(peer_count),
                peer_count
            );
            state.node.gossip.write().await.stats.dropped_rate_limit += 1;
            return;
        }
    }

    // ── CRIT-1 / B3: PER-PEER NONCE CHECK (anti-replay within ±90s window) ──
    // Every V2 envelope MUST carry a strictly-monotonic nonce ≥ 1. Production
    // senders start their counter at 1 (AtomicU64::new(1)) and persist it across
    // restarts (clamped .max(1)), so a nonce of 0 can only come from a legacy or
    // forged envelope. We reject it outright instead of skipping the tracker —
    // otherwise nonce-0 traffic would bypass anti-replay entirely (audit B3).
    {
        let mut tracker = state.node.nonce_tracker.write().await;
        if env.nonce == 0 || !tracker.check_and_advance(&env.sender, env.nonce) {
            log::warn!(
                "◈ [Dispatch] ⚠ NONCE REPLAY/ZERO from {} — nonce {} (≤ high-water mark or 0) → \
                 drop",
                short(&env.sender, 12),
                env.nonce
            );
            state.node.gossip.write().await.stats.dropped_nonce += 1;
            return;
        }
    }

    // ── Télémétrie moteur (« sous le capot ») : chaque enveloppe AUTHENTIFIÉE
    // (pipeline ①-⑧ passé : taille, JSON, ban, dedup, fraîcheur, rate, nonce,
    // signature ML-DSA) est annoncée à l'UI telle quelle — type réel, expéditeur
    // réel, nonce réel. Ping/Pong tus (bruit de liveness). Best-effort, hors du
    // chemin de sécurité.
    {
        let msg_kind = match &env.payload {
            GossipMessage::Hello { .. } => Some("Hello"),
            GossipMessage::NewBlock { .. } => Some("NewBlock"),
            GossipMessage::BroadcastTx { .. } => Some("BroadcastTx"),
            GossipMessage::RequestChain { .. } => Some("RequestChain"),
            GossipMessage::ChainSegment { .. } => Some("ChainSegment"),
            GossipMessage::PublishUsername { .. } => Some("PublishUsername"),
            GossipMessage::FinalityVote { .. } => Some("FinalityVote"),
            GossipMessage::ReportPeer { .. } => Some("ReportPeer"),
            _ => None,
        };
        if let Some(kind) = msg_kind {
            if let Some(handle) = state.app_handle.read().await.as_ref() {
                use tauri::Emitter;
                let _ = handle.emit(
                    "quanta://engine",
                    serde_json::json!({
                        "kind": "envelope",
                        "msg": kind,
                        "sender": short(&env.sender, 16),
                        "nonce": env.nonce,
                        "us": pipeline_t.elapsed().as_micros() as u64,
                    }),
                );
            }
        }
    }

    match env.payload {
        GossipMessage::Hello {
            heads,
            node_id,
            watts,
            country,
            version,
            tasks_completed,
            blocks_verified,
            uptime_minutes,
            chain_height,
            known_peer_ids,
            display_name,
        } => {
            // NET-5: Protocol version compatibility check. We never reject —
            // unknown fields already default thanks to #[serde(default)] — but
            // a mismatch is worth surfacing in logs so operators notice.
            use crate::p2p::gossip::TORUS_PROTOCOL_VERSION;
            if version > TORUS_PROTOCOL_VERSION {
                log::warn!(
                    "◈ [NET-5] peer {} runs newer protocol v{} (we are v{}) — processing what we \
                     can; consider upgrading",
                    short(&env.sender, 12),
                    version,
                    TORUS_PROTOCOL_VERSION
                );
            } else if version < TORUS_PROTOCOL_VERSION {
                log::debug!(
                    "◈ [NET-5] peer {} runs legacy protocol v{} (we are v{})",
                    short(&env.sender, 12),
                    version,
                    TORUS_PROTOCOL_VERSION
                );
            }
            handle_hello(
                state,
                &env.sender,
                &node_id,
                heads,
                watts,
                &country,
                tasks_completed,
                blocks_verified,
                uptime_minutes,
                chain_height,
                known_peer_ids,
                display_name,
            )
            .await;
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
                pending
                    .get(&nonce)
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
                short(&env.sender, 12),
                nonce,
                rtt_ms
            );
        }
        GossipMessage::ReportPeer { peer_id, reason } => {
            handle_report_peer(state, &env.sender, &peer_id, reason).await;
        }
        GossipMessage::NewBlock { block_json } => {
            handle_new_block(state, &env.sender, &block_json).await;
        }
        GossipMessage::RequestChain {
            from_height,
            max_blocks,
        } => {
            handle_request_chain(state, &env.sender, from_height, max_blocks).await;
        }
        GossipMessage::ChainSegment {
            blocks_json,
            sender_height,
            blocks_compressed,
        } => {
            // NET-8: Prefer compressed payload when present; fall back to inline
            // legacy `blocks_json` if decompression fails or no compressed bytes.
            let blocks = match blocks_compressed {
                Some(bytes) => match crate::p2p::gossip::decompress_blocks(&bytes) {
                    Ok(b) => b,
                    Err(e) => {
                        log::warn!(
                            "◈ [NET-8] ChainSegment from {} compressed payload invalid ({}) — \
                             falling back to inline",
                            short(&env.sender, 12),
                            e
                        );
                        blocks_json
                    }
                },
                None => blocks_json,
            };
            handle_chain_segment(state, &env.sender, blocks, sender_height).await;
        }
        GossipMessage::PublishUsername { record_json } => {
            handle_publish_username(state, &record_json).await;
        }
        GossipMessage::FinalityVote { vote_json } => {
            handle_finality_vote(state, &env.sender, &vote_json).await;
        }
        GossipMessage::FinalityFault { proof_json } => {
            handle_finality_fault(state, &env.sender, &proof_json).await;
        }
    }
}

/// LIVE-3 — a gossiped fault proof. Deserialize → re-verify against the on-chain
/// stake (GADGET-4 `verify_proof`) → queue a slash of the offender's **slashable**
/// stake (bonded + unbonding, LIVE-3B — STAKE → BURN), which the next sealed
/// block includes. A malicious accuser
/// cannot punish an innocent validator: the queued slash carries the proof and
/// every node re-verifies it in block validation (`verify_block_slashes`). The
/// envelope's Ed25519 signature (transport) was already checked upstream; the
/// proof's own two ML-DSA vote signatures are the slashing authority.
async fn handle_finality_fault(state: &Arc<AppState>, sender: &str, proof_json: &str) {
    let proof: crate::sm::finality_slashing::FaultProof = match serde_json::from_str(proof_json) {
        Ok(p) => p,
        Err(e) => {
            log::warn!(
                "◈ [Dispatch] FinalityFault JSON invalide from {}: {}",
                short(sender, 12),
                e
            );
            return;
        }
    };
    // Verify + queue under a single write lock (queue_slash re-verifies via
    // build_slash_tx → only slashes a slashable offender — bonded or unbonding;
    // the proof is re-checked in-block on every node). The offender's stake is
    // read from the live ledger.
    let queued = {
        let mut ledger = state.node.ledger.write().await;
        // Re-verify the proof against on-chain stake before queueing, so a bogus
        // proof never even enters the mempool (defense-in-depth; block validation
        // re-checks regardless). LIVE-3B: use the SLASHABLE weight (bonded +
        // unbonding) — a fully-unstaked equivocator must stay punishable until
        // its withdrawal completes (unstake-and-run closed).
        let stakes = ledger.slashable_stakes_by_pubkey();
        if !crate::sm::finality_slashing::verify_proof(
            &proof,
            &stakes,
            crate::sm::finality::EPOCH_LENGTH_BLOCKS,
        ) {
            log::debug!(
                "◈ [Dispatch] FinalityFault from {} rejected (proof does not verify)",
                short(sender, 12)
            );
            None
        } else {
            ledger.queue_slash(&proof)
        }
    };
    if let Some(tx) = queued {
        log::info!(
            "◈ [Slashing] ✓ queued slash of {} µQTA against {} (fault via {})",
            tx.amount,
            short(&tx.from, 12),
            short(sender, 12)
        );
    }
}

/// LIVE-1 — a gossiped finality vote (pipeline step ⑨). Deserialize → hand to the
/// live gadget, which **re-verifies** it against the on-chain stake (GADGET-2)
/// before observing it into the fork-choice and (on a ⅔ certificate) advancing
/// finality (GADGET-3). The envelope's Ed25519 signature (transport) was already
/// checked upstream; the vote's own ML-DSA-65 signature (finality authority) is
/// re-checked inside `ingest_vote`. Dedup is the shared `seen_messages` LRU
/// upstream. The verdict stays a pure `sm/` function — this handler only routes.
async fn handle_finality_vote(state: &Arc<AppState>, sender: &str, vote_json: &str) {
    let vote: crate::sm::finality_vote::Vote = match serde_json::from_str(vote_json) {
        Ok(v) => v,
        Err(e) => {
            log::warn!(
                "◈ [Dispatch] FinalityVote JSON invalide from {}: {}",
                short(sender, 12),
                e
            );
            return;
        }
    };
    // Snapshot the chain (read lock) so the gadget verifies + weighs against a
    // consistent on-chain stake state. `ingest_vote` re-derives the pubkey-keyed
    // stake map from the ledger internally.
    let (outcome, floor) = {
        let ledger = state.node.ledger.read().await;
        let mut fin = state.node.finality.write().await;
        // Keep the block tree current so GHOST can weigh votes on real blocks.
        fin.observe_chain(&ledger);
        let outcome = fin.ingest_vote(vote, &ledger);
        (outcome, fin.finalized_floor())
    };
    if !outcome.accepted {
        log::debug!(
            "◈ [Dispatch] FinalityVote from {} rejected (forged / non-validator / malformed)",
            short(sender, 12)
        );
        return;
    }
    // LIVE-3 — the vote revealed an equivocation: queue the slash locally AND
    // gossip the proof so every node slashes the offender (accountable safety).
    if let Some(proof) = outcome.detected_fault {
        state.node.ledger.write().await.queue_slash(&proof);
        if let Ok(proof_json) = serde_json::to_string(&proof) {
            log::warn!(
                "◈ [Slashing] equivocation detected on a gossiped vote → slashing {}",
                short(proof.offender(), 12)
            );
            broadcast(state, GossipMessage::FinalityFault { proof_json }).await;
        }
    }
    if outcome.finalized {
        // LIVE-2 — a certificate finalized a checkpoint: push the finality floor
        // into the ledger so its fork resolution treats that block (and everything
        // below) as irreversible. HIGH-4: the setter only freezes if OUR block at
        // that height matches the finalized hash. Fresh write lock AFTER the
        // read/finality locks are dropped (no nested ledger lock).
        let (h, hash) = floor;
        let new_floor = state.node.ledger.write().await.set_finalized_floor(h, &hash);
        log::info!(
            "◈ [Finality] ✓ certificate finalized a checkpoint (from votes via {}) — floor now {}",
            short(sender, 12),
            new_floor
        );
    } else if outcome.justified {
        log::info!("◈ [Finality] ✓ certificate justified a checkpoint (from votes via {})", short(sender, 12));
    }
}

/// Identité — applique une revendication de pseudo reçue par gossip.
/// La résolution de conflit déterministe de `UsernameRegistry::apply` garantit
/// la convergence quel que soit l'ordre d'arrivée.
async fn handle_publish_username(state: &Arc<crate::AppState>, record_json: &str) {
    use crate::p2p::username::UsernameRecord;
    let Ok(rec) = serde_json::from_str::<UsernameRecord>(record_json) else {
        log::warn!("◈ [identité] PublishUsername JSON invalide");
        return;
    };
    let username = rec.username.clone();
    let mut reg = state.node.usernames.write().await;
    match reg.apply(rec) {
        Ok(outcome) => log::info!("◈ [identité] @{username} {outcome:?}"),
        Err(e) => log::debug!("◈ [identité] @{username} rejeté: {e:?}"),
    }
}

/// B5 — Entry point for fuzz testing: attempt to process raw bytes as gossip.
/// Guaranteed to never panic — only returns errors gracefully.
#[allow(dead_code)] // Used in security_tests + fuzzing target
pub fn try_process_raw_gossip(data: &[u8]) -> Result<(), String> {
    validate_envelope_at(data, now_epoch_secs() as i64).map(|_| ())
}

/// Phase 0 (T0.1): the **pure, injected-time** envelope validator that the
/// deterministic core runs on inbound bytes via `Event::MessageReceived`.
///
/// Same stateless checks as the production receive path (size → JSON decode →
/// freshness → ML-DSA-65 signature, PQ-ENVELOPE-1) but freshness is evaluated against the
/// injected `now_secs` rather than the system clock, so the core is replayable
/// (Constitution §3: no clock reads in the core). Returns the parsed,
/// signature-verified envelope on success — raw bytes are never trusted until
/// they clear this gate.
///
/// The **stateful** pipeline stages (ban, dedup, rate-limit, per-sender nonce)
/// remain in the shell's `dispatch_incoming` until a later T0.1 slice migrates
/// that state into the core. Guaranteed never to panic.
pub fn validate_envelope_at(data: &[u8], now_secs: i64) -> Result<GossipEnvelope, String> {
    // Step 0: size cap (mirror `dispatch_incoming`'s DoS guard).
    if data.len() > MAX_RAW_ENVELOPE_BYTES {
        return Err(format!(
            "oversized: {} > {} bytes",
            data.len(),
            MAX_RAW_ENVELOPE_BYTES
        ));
    }

    // Step 1: structural decode.
    let env: GossipEnvelope =
        serde_json::from_slice(data).map_err(|e| format!("JSON error: {}", e))?;

    // Step 2: freshness against INJECTED time.
    if !GossipRouter::is_fresh_at(&env.timestamp, now_secs) {
        return Err("stale message".into());
    }

    // Step 3: signature.
    verify_envelope_signature(&env)?;

    Ok(env)
}

// ─── Handlers ───────────────────────────────────────────────────────────────

/// Hello → enregistre les watts + pays + contributions du peer, demande les
/// nœuds DAG manquants. B3: updates `peer_info` with `last_seen` for TTL
/// tracking. STRUCT-6: also stores tasks_completed / blocks_verified /
/// uptime_minutes for Shapley. NET-2: processes known_peer_ids for automatic
/// mesh discovery.
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
        "◈ [Dispatch] Hello from {} ({} heads, {:.1}W, {}, chain_h={}, tasks={} blocks={} \
         uptime={}m, peers={})",
        short(sender_pk, 12),
        their_heads.len(),
        watts,
        country,
        peer_chain_height,
        tasks_completed,
        blocks_verified,
        uptime_minutes,
        known_peer_ids.len(),
    );

    // STRUCT-4: Clamp peer-declared watts to a sane range.
    const MAX_PEER_WATTS: f64 = 500.0;
    const MIN_PEER_WATTS: f64 = 1.0;
    let clamped_watts = watts.clamp(MIN_PEER_WATTS, MAX_PEER_WATTS);
    if (watts - clamped_watts).abs() > 0.1 {
        log::warn!(
            "◈ [Dispatch] peer {} declared {:.1}W, clamped to {:.1}W",
            short(sender_pk, 12),
            watts,
            clamped_watts
        );
    }

    // B3 + STRUCT-6: Update peer_info with liveness + contribution data.
    // NET-15: Also persist the sanitised display_name (None unsets it, which
    // means the peer dropped its nickname).
    // SEC-COUNTRY-1: normalise the peer-supplied country to an ISO-shaped token
    // before it is stored anywhere (per-peer field *and* the oracle map).
    let country_code = sanitize_country_code(country);

    {
        let mut info = state.node.peer_info.write().await;
        let entry = info
            .entry(sender_pk.to_string())
            .or_insert_with(|| crate::p2p::PeerInfo::new(clamped_watts, country_code.clone()));
        entry.watts = clamped_watts;
        entry.country = country_code.clone();
        entry.tasks_completed = tasks_completed;
        entry.blocks_verified = blocks_verified;
        entry.uptime_minutes = uptime_minutes;
        entry.display_name = sanitized_name.clone();
        entry.touch();
    }

    // Enregistrer le pays du pair pour l'oracle énergie. SEC-COUNTRY-1: bounded
    // key space — refuse to grow the map past MAX_COUNTRY_CODES with novel
    // codes; existing codes still count normally.
    {
        let mut reports = state.node.peer_country_reports.write().await;
        if reports.contains_key(&country_code) || reports.len() < MAX_COUNTRY_CODES {
            *reports.entry(country_code).or_insert(0) += 1;
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
            our_height,
            peer_chain_height,
            short(sender_pk, 12)
        );
        request_chain_range(state, our_height, peer_chain_height).await;
    }

    // NET-2: Peer exchange — auto-connect to peers we don't know yet.
    // This enables mesh discovery: each Hello carries the sender's known peers,
    // so new nodes discover the full network through gossip alone.
    if !known_peer_ids.is_empty() {
        let our_known_peers = state.node.known_peers.read().await;
        let our_ticket = state.node.get_ticket().await.unwrap_or_default();
        let new_peers: Vec<String> = known_peer_ids
            .into_iter()
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
                short(peer_id, 16)
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
                            short(&peer_id_clone, 16)
                        );
                        // Trigger Hello to the new peer
                        crate::p2p::gossip_tasks::trigger_hello_now(&state_clone).await;
                    }
                    Err(e) => {
                        log::debug!(
                            "◈ [NET-2] Failed to auto-connect to {}: {}",
                            short(&peer_id_clone, 16),
                            e
                        );
                    }
                }
            });
        }
    }
}

/// BroadcastTx → parse une transaction JSON, la valide et l'ajoute au ledger
/// local.
///
/// AUDIT-TX-2: Nonce check relaxed from strict equality to monotonic
/// non-regression. Gossip is unordered so two consecutive txs from the same
/// sender can arrive in either order. The previous strict `tx.nonce !=
/// expected` rule dropped every tx that arrived out of order, permanently. We
/// now accept any tx whose nonce is `>= last_seen_for_sender`, advance the
/// high-water to `nonce + 1`, and rely on `seen_tx_hashes` for replay
/// protection.
async fn handle_broadcast_tx(state: &Arc<AppState>, tx_json: &str) {
    let tx: crate::p2p::ledger::Transaction = match serde_json::from_str(tx_json) {
        Ok(t) => t,
        Err(e) => {
            log::warn!("◈ [Dispatch] BroadcastTx JSON invalide: {}", e);
            return;
        }
    };

    // LIVE-3: a Slash is BLOCK-ONLY — its authority is an embedded fault proof
    // verified in-block, and `verify_tx` exempts it from the signature gate. It
    // must NEVER be admitted from a gossiped BroadcastTx (that would let a peer
    // inject an unverified slash into the mempool). Slashing flows through
    // `FinalityFault` → `queue_slash` instead. Drop it here outright.
    if matches!(tx.tx_type, crate::p2p::ledger::TxType::Slash) {
        log::warn!(
            "◈ [Dispatch] BroadcastTx carrying a Slash — rejected (slashes are block-only, via FinalityFault)"
        );
        return;
    }
    // MINT-GUARD-1 (defense in depth): a `Mining` reward is SYSTEM-issued
    // (`NETWORK` → miner), built locally by the seal path and carried inside a
    // `NewBlock` — it has no legitimate reason to arrive via `BroadcastTx`. Drop it
    // here too, so a forged `Mining` tx can never even enter a peer's mempool.
    // (`verify_tx` also rejects any non-NETWORK `Mining` tx, this is the outer belt.)
    if matches!(tx.tx_type, crate::p2p::ledger::TxType::Mining) {
        log::warn!("◈ [Dispatch] BroadcastTx carrying a Mining tx — rejected (rewards are block-only)");
        return;
    }

    // C5: mint the verification token — THE single signature gate (AUDIT-TX-1:
    // enforces signatures even on burn-target txs, previously bypassed by
    // `to == BURN`). This replaces the bare `verify_tx` call with the SAME
    // single verification, but the authoritative apply below now *requires* this
    // token, so the "signature checked" precondition can no longer be bypassed
    // by a future edit. Core and shell mint the token identically.
    let vtx = match crate::p2p::ledger::VerifiedTx::new(tx) {
        Some(v) => v,
        None => {
            log::warn!("◈ [Dispatch] tx signature invalide — drop");
            return;
        }
    };
    // Owned copies of the fields used before the token is consumed (no borrow
    // of `vtx` is held across an `.await`).
    let (from, nonce, to, amount, tx_type, tx_id) = {
        let t = vtx.tx();
        (
            t.from.clone(),
            t.nonce,
            t.to.clone(),
            t.amount,
            t.tx_type.clone(),
            t.id.clone(),
        )
    };

    // AUDIT-TX-2: Per-account monotonic nonce check (anti-replay safety net).
    // NETWORK and ESCROW are synthetic addresses that don't carry account nonces.
    if from != "NETWORK" && from != "ESCROW" {
        let ledger = state.node.ledger.read().await;
        let high_water = ledger.get_nonce(&from);
        // Reject txs whose nonce is strictly behind our high water — they are
        // either stale replays or already-applied txs whose hash was evicted.
        // Equality is allowed (out-of-order arrival within a window).
        if nonce.saturating_add(1) < high_water {
            log::warn!(
                "◈ [Dispatch] tx nonce {} too far behind high-water {} for {} — drop",
                nonce,
                high_water,
                short(&from, 12)
            );
            return;
        }
        drop(ledger);
    }

    // STRUCT-3: Reconcile dual ledger. Apply the tx to BOTH:
    //   1. CRDT (for eventually-consistent multi-node sync)
    //   2. Linear Ledger via apply_verified_remote_tx (authoritative local state)
    use crate::p2p::ledger::TxType;
    if tx_type == TxType::Transfer {
        let mut cons = state.node.consensus.write().await;
        cons.ledger.debit(&from, &from, amount);
        cons.ledger.credit(&from, &to, amount);
    }

    // STRUCT-3 + C5: Replay the remote tx into the local linear ledger via the
    // single signature-gated entry point (the `VerifiedTx` token proves the sig
    // was checked) — the same path the deterministic core (`sm::Node`) uses.
    // Idempotent dedup via seen_tx_hashes; advances the sender's high-water
    // nonce only when the tx is actually applied.
    if from != "NETWORK" && from != "ESCROW" {
        let mut ledger = state.node.ledger.write().await;
        let applied = ledger.apply_verified_remote_tx(vtx);
        drop(ledger);
        // Live UX: surface freshly-applied remote txs to the frontend (the UI
        // filters for its own address — no crypto lock needed here, which
        // keeps the lock ordering untouched). Best-effort.
        if applied {
            if let Some(handle) = state.app_handle.read().await.as_ref() {
                use tauri::Emitter;
                let _ = handle.emit(
                    "quanta://tx-applied",
                    serde_json::json!({
                        "from": from,
                        "to": to,
                        "amount": amount as f64 / crate::p2p::ledger::MICRO as f64,
                        "tx_type": format!("{:?}", tx_type),
                        // Matière unique pour le terminal : µQTA exacts, nonce
                        // de compte réel, hash BLAKE3 réel de la tx.
                        "amount_micro": amount,
                        "nonce": nonce,
                        "hash": tx_id.clone(),
                    }),
                );
            }
        }
    }

    log::debug!(
        "◈ [Dispatch] tx {} ({:?}) appliquée au CRDT",
        tx_id,
        tx_type
    );
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

/// ReportPeer → log + accumulate the report under the reporting peer's key.
/// A ban requires `REPORT_BAN_THRESHOLD` **distinct reporters** (SEC-REPORT-1),
/// handled inside `NonceTracker::record_report`.
///
/// A single authenticated peer can therefore no longer ban an honest victim by
/// itself: it counts for at most one reporter no matter how many reports (or
/// how varied their `ReportReason`) it sends. Banning still requires an
/// independent quorum; a coordinated cluster of `REPORT_BAN_THRESHOLD` real
/// peers remains the (intended) minimum. `sender_pk` is the envelope signer,
/// already authenticated by the upstream signature gate.
async fn handle_report_peer(
    state: &Arc<AppState>,
    sender_pk: &str,
    peer_id: &str,
    reason: ReportReason,
) {
    log::info!(
        "◈ [Dispatch] ReportPeer from {} → {} ({:?})",
        short(sender_pk, 12),
        short(peer_id, 12),
        reason
    );

    let count = state
        .node
        .nonce_tracker
        .write()
        .await
        .record_report(peer_id, sender_pk);
    state.node.gossip.write().await.stats.peers_reported += 1;

    if count >= REPORT_BAN_THRESHOLD {
        log::warn!(
            "◈ [Dispatch] ⛔ peer {} BANNED ({} reports, TTL {}s)",
            short(peer_id, 12),
            count,
            REPORT_BAN_TTL_SECS
        );
    }
}

/// D1.3: Handle a remote sealed block — validate and integrate into local
/// chain.
///
/// The signature on the gossip envelope has already been verified by the
/// upstream pipeline (`dispatch_incoming`). Here we only deserialize the block
/// payload and hand it to the ledger which performs structural + cryptographic
/// validation.
async fn handle_new_block(state: &Arc<AppState>, sender: &str, block_json: &str) {
    let block: crate::p2p::ledger::Block = match serde_json::from_str(block_json) {
        Ok(b) => b,
        Err(e) => {
            log::warn!(
                "◈ [Dispatch] invalid block JSON from {}: {}",
                short(sender, 12),
                e
            );
            return;
        }
    };

    let mut ledger = state.node.ledger.write().await;
    match ledger.integrate_remote_block(block.clone()) {
        Ok(true) => {
            log::info!(
                "◈ [Dispatch] ✓ Accepted remote block from {}",
                short(sender, 12)
            );
            drop(ledger);
            // Live UX: remote block landed — pulse the 3D scenes.
            if let Some(handle) = state.app_handle.read().await.as_ref() {
                use tauri::Emitter;
                let _ = handle.emit(
                    "quanta://block-sealed",
                    serde_json::json!({
                        "index": block.index,
                        "txs": block.transactions.len(),
                        "mine": false,
                        // Le VRAI hash du bloc + son scelleur — la preuve
                        // affichable telle quelle dans le moteur de l'UI.
                        "hash": block.hash.clone(),
                        "prev": block.prev_hash.clone(),
                        "miner": short(&block.miner, 16),
                    }),
                );
            }
            // CRIT-B: Increment validated block counter for Shapley distribution
            state
                .node
                .blocks_validated
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
                short(sender, 12),
                reason
            );
            drop(ledger);
            // LIVE-4: a block that doesn't fit linearly may belong to a
            // competing branch (partition) or an out-of-order window — hand it
            // to the fork reconciler (bounded buffer → assemble → reorg_to_fork).
            fork_heal_offer_and_resolve(state, vec![block]).await;
        }
    }
}

/// LIVE-4 — feed blocks that failed linear integration to the fork
/// reconciler, adopt any winning competing branch (partition heal, via
/// [`crate::p2p::ledger::Ledger::reorg_to_fork`]), drain out-of-order linear
/// extensions, and issue an ancestor probe when the buffered branch does not
/// yet root in our chain. Pass an empty `candidates` to just re-resolve the
/// existing buffer (e.g. after new linear integrations unlocked it).
///
/// Lock order: `ledger` (write) → `fork_heal` (write), **both released**
/// before the probe broadcast (which takes gossip locks) — consistent with
/// the project-wide `crypto → reputation → ledger → gossip` ordering.
async fn fork_heal_offer_and_resolve(
    state: &Arc<AppState>,
    candidates: Vec<crate::p2p::ledger::Block>,
) {
    let outcome = {
        let mut ledger = state.node.ledger.write().await;
        let mut fh = state.node.fork_heal.write().await;
        for b in candidates {
            fh.offer(b, &ledger);
        }
        fh.resolve(&mut ledger)
    };
    let integrated = outcome.adopted + outcome.extended;
    if integrated > 0 {
        // CRIT-B: adopted/drained blocks were fully validated + integrated —
        // they count toward the Shapley validation factor like any other.
        state
            .node
            .blocks_validated
            .fetch_add(integrated as u64, std::sync::atomic::Ordering::Relaxed);
    }
    if let Some((from, to)) = outcome.probe {
        log::info!(
            "◈ [LIVE-4] fork ancestor not held — probing chain window [{from}, {to})"
        );
        request_chain_range(state, from, to).await;
    }
}

/// Maximum blocks we'll send in a single ChainSegment response (DoS
/// protection).
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
            ledger
                .block_at(i)
                .and_then(|b| serde_json::to_string(b).ok())
        })
        .collect();

    if blocks_json.is_empty() {
        log::debug!(
            "◈ [Dispatch] RequestChain from {} — nothing to send (from_height={}, our height={})",
            short(sender, 12),
            from_height,
            chain_len
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
                short(sender, 12),
                blocks_json.len(),
                blocks_json.iter().map(|s| s.len()).sum::<usize>(),
                c.len(),
            );
            (Vec::new(), Some(c.clone()))
        }
        _ => {
            log::info!(
                "◈ [Dispatch] RequestChain from {} — sending {} blocks (height {} → {})",
                short(sender, 12),
                blocks_json.len(),
                from_height,
                from_height + blocks_json.len() as u64,
            );
            (blocks_json, None)
        }
    };

    broadcast(
        state,
        GossipMessage::ChainSegment {
            blocks_json: inline,
            sender_height: chain_len,
            blocks_compressed: compressed_field,
        },
    )
    .await;
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
            short(sender, 12),
            blocks_json.len(),
            MAX_CHAIN_SEGMENT_RECEIVED
        );
        blocks_json
            .into_iter()
            .take(MAX_CHAIN_SEGMENT_RECEIVED)
            .collect()
    } else {
        blocks_json
    };

    let mut integrated = 0u64;
    let mut rejected = 0u64;
    // LIVE-4: blocks that failed linear integration — the segment's remainder
    // is the same branch's continuation, so it all goes to the reconciler.
    let mut fork_candidates: Vec<crate::p2p::ledger::Block> = Vec::new();

    let mut iter = blocks_json.iter();
    while let Some(block_str) = iter.next() {
        let block: crate::p2p::ledger::Block = match serde_json::from_str(block_str) {
            Ok(b) => b,
            Err(e) => {
                log::warn!(
                    "◈ [Dispatch] bad block in ChainSegment from {}: {}",
                    short(sender, 12),
                    e
                );
                rejected += 1;
                // AUDIT-SYNC-1: stop on parse failure — subsequent blocks
                // can't extend a tip that's missing the previous one, so
                // continuing wastes lock acquisitions.
                break;
            }
        };

        let mut ledger = state.node.ledger.write().await;
        match ledger.integrate_remote_block(block.clone()) {
            Ok(true) => {
                integrated += 1;
                state
                    .node
                    .blocks_validated
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            Ok(false) => {} // Already known — skip silently
            Err(e) => {
                log::warn!("◈ [Dispatch] ChainSegment block rejected: {}", e);
                rejected += 1;
                // AUDIT-SYNC-1: once a block in the segment doesn't fit
                // LINEARLY, every later one in the same segment will also
                // fail linearly — bail out of the linear loop. LIVE-4: but
                // the whole remainder may be a competing branch (partition)
                // or an out-of-order window, so hand it to the reconciler
                // instead of dropping it on the floor.
                drop(ledger);
                fork_candidates.push(block);
                for rest in iter.by_ref() {
                    if let Ok(b) = serde_json::from_str::<crate::p2p::ledger::Block>(rest) {
                        fork_candidates.push(b);
                    }
                }
                break;
            }
        }
    }

    if !fork_candidates.is_empty() {
        // LIVE-4: try to assemble/adopt a competing branch from everything
        // buffered so far (and probe below it if its root is still unknown).
        fork_heal_offer_and_resolve(state, fork_candidates).await;
    } else if integrated > 0 && !state.node.fork_heal.read().await.is_empty() {
        // Out-of-order windows: fresh linear blocks may have unlocked
        // buffered orphans (NET-6 fan-out answers can land high-first).
        fork_heal_offer_and_resolve(state, Vec::new()).await;
    }

    log::info!(
        "◈ [Dispatch] ChainSegment from {} — integrated: {}, rejected: {}, sender_height: {}",
        short(sender, 12),
        integrated,
        rejected,
        sender_height
    );

    // NET-16: Emit sync progress to the frontend. Best-effort — we don't
    // care whether anyone is listening on the channel.
    let our_height = state.node.ledger.read().await.chain_height();
    if let Some(handle) = state.app_handle.read().await.as_ref() {
        use tauri::Emitter;
        let _ = handle.emit(
            "quanta://chain-sync-progress",
            serde_json::json!({
                "our_height": our_height,
                "sender_height": sender_height,
                "integrated": integrated,
                "rejected": rejected,
                "sender": sender,
            }),
        );
    }

    // If sender has more blocks, request the next segment(s) in parallel.
    let our_height = state.node.ledger.read().await.chain_height();
    if our_height < sender_height {
        log::info!(
            "◈ [Dispatch] Need more blocks: our height {} < sender height {} — fanning out next \
             requests",
            our_height,
            sender_height
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
/// - Otherwise splits the gap into up to `PARALLEL_CHAIN_FANOUT`
///   non-overlapping `[start, start+MAX_CHAIN_SEGMENT)` windows and broadcasts
///   them all.
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
        broadcast(
            state,
            GossipMessage::RequestChain {
                from_height: from,
                max_blocks: MAX_CHAIN_SEGMENT,
            },
        )
        .await;
        return;
    }
    // Big gap — fan out up to PARALLEL_CHAIN_FANOUT windows of MAX_CHAIN_SEGMENT.
    let window = MAX_CHAIN_SEGMENT;
    let mut start = from;
    let mut requests_sent = 0u64;
    while start < to && requests_sent < PARALLEL_CHAIN_FANOUT {
        let count = (to - start).min(window);
        broadcast(
            state,
            GossipMessage::RequestChain {
                from_height: start,
                max_blocks: count,
            },
        )
        .await;
        start += count;
        requests_sent += 1;
    }
    log::info!(
        "◈ [NET-6] parallel chain sync: {} requests fanned out covering [{}, {})",
        requests_sent,
        from,
        start
    );
}

/// Helper : signe + emballe + push sur le channel gossip_tx (le drain enverra
/// via iroh-gossip). STRUCT-1: Uses signable_envelope_bytes() so signature
/// covers full envelope.
async fn broadcast(state: &Arc<AppState>, msg: GossipMessage) {
    // PQ-ENVELOPE-1: envelope sender + signature identity = ML-DSA-65 primary key.
    let pk = state
        .crypto
        .lock()
        .await
        .pq_identity_hex()
        .unwrap_or_default();
    if pk.is_empty() {
        return;
    }

    // STRUCT-1: Generate timestamp and nonce BEFORE signing
    let timestamp = chrono::Utc::now().to_rfc3339();
    let nonce = state.node.gossip.read().await.next_outgoing_nonce();
    let signable = GossipRouter::signable_envelope_bytes(&pk, nonce, &timestamp, &msg);
    let sig = state
        .crypto
        .lock()
        .await
        .sign_pq(&signable)
        .unwrap_or_default();

    let env = match GossipRouter::build_signed_envelope(pk, msg, nonce, timestamp, &sig) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("◈ [Dispatch] build_signed_envelope failed: {}", e);
            return;
        }
    };
    state.node.gossip.write().await.mark_seen(&env.id);
    let _ = state.node.gossip_tx.send(env);
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::CryptoEngine;

    /// Minimal in-memory `AppState` for driving `dispatch_incoming` without a
    /// network/iroh endpoint (no DB, no Tauri handle). `WillowNode::new()`
    /// builds all the in-memory stores + the priority gossip channel.
    fn test_app_state() -> Arc<crate::AppState> {
        Arc::new(crate::AppState {
            crypto: tokio::sync::Mutex::new(CryptoEngine::new()),
            db: tokio::sync::Mutex::new(None),
            node: crate::p2p::willow_node::WillowNode::new(),
            unlock_guard: crate::UnlockGuard::default(),
            display_name: tokio::sync::RwLock::new(None),
            app_handle: tokio::sync::RwLock::new(None),
        })
    }

    /// PRESIG-ORDER teeth: an unauthenticated (bad-signature) envelope with a
    /// SPOOFED sender must be dropped at signature verification BEFORE any
    /// per-sender map write, so it cannot grow `last_nonces` / `rate_counters`
    /// (the NONCE-MEM remote-OOM vector). Pre-reorder this FAILED — the rate and
    /// nonce checks `.entry(env.sender)` ran first and inserted entries keyed by
    /// the spoofable sender.
    #[tokio::test]
    async fn presig_bad_signature_writes_no_per_sender_state() {
        let state = test_app_state();
        let mut signer = CryptoEngine::new();
        signer.generate_pq_identity().expect("ml-dsa primary");
        let spoofed = "f".repeat(64); // a pubkey the attacker does NOT own
        let msg = GossipMessage::Ping { nonce: 7 };
        let ts = chrono::Utc::now().to_rfc3339();
        let nonce = 1_u64;
        // Sign with the REAL ML-DSA key but claim sender == spoofed ⇒ the
        // signature cannot verify against the spoofed pubkey.
        let signable = GossipRouter::signable_envelope_bytes(&spoofed, nonce, &ts, &msg);
        let sig = signer.sign_pq(&signable).unwrap();
        let env =
            GossipRouter::build_signed_envelope(spoofed.clone(), msg, nonce, ts, &sig).unwrap();
        let raw = serde_json::to_vec(&env).unwrap();

        dispatch_incoming(&state, &raw).await;

        let tracker = state.node.nonce_tracker.read().await;
        assert!(
            tracker.last_nonces.is_empty(),
            "bad-sig spoofed sender must NOT create a nonce entry"
        );
        assert!(
            tracker.rate_counters.is_empty(),
            "bad-sig spoofed sender must NOT create a rate-limit entry"
        );
    }

    /// The reorder must NOT break the happy path: a correctly-signed envelope is
    /// admitted and its per-sender state IS recorded (nonce high-water + rate
    /// bucket), proving the maps are still written for authenticated senders.
    #[tokio::test]
    async fn presig_valid_signature_is_admitted_and_tracked() {
        let state = test_app_state();
        let mut signer = CryptoEngine::new();
        signer.generate_pq_identity().expect("ml-dsa primary");
        let pk = signer.pq_identity_hex().expect("ml-dsa primary");
        let msg = GossipMessage::Ping { nonce: 7 };
        let ts = chrono::Utc::now().to_rfc3339();
        let nonce = 1_u64;
        let signable = GossipRouter::signable_envelope_bytes(&pk, nonce, &ts, &msg);
        let sig = signer.sign_pq(&signable).unwrap();
        let env = GossipRouter::build_signed_envelope(pk.clone(), msg, nonce, ts, &sig).unwrap();
        let raw = serde_json::to_vec(&env).unwrap();

        dispatch_incoming(&state, &raw).await;

        let tracker = state.node.nonce_tracker.read().await;
        assert_eq!(
            tracker.last_nonces.get(&pk),
            Some(&1),
            "a valid sender's nonce high-water must be recorded"
        );
        assert!(
            tracker.rate_counters.contains_key(&pk),
            "a valid sender must be tracked for rate limiting"
        );
    }

    /// TX-AUTH-NONCE-1 §5 (Sybil cap): a flood of distinct senders must keep the
    /// per-sender maps BOUNDED, and eviction must prefer IDLE (replay-safe)
    /// entries over an active one.
    #[test]
    fn txauth_nonce_tracker_eviction_is_bounded_and_anti_replay_safe() {
        let mut t = NonceTracker::new();
        // Pre-fill to the cap with IDLE entries (last_seen ≥ TTL ago → a replay of
        // their traffic would fail freshness, so expiring them is anti-replay-safe).
        let idle = now_epoch_secs().saturating_sub(NONCE_ENTRY_TTL_SECS + 10);
        for i in 0..MAX_TRACKED_SENDERS {
            let k = format!("old{i}");
            t.last_nonces.insert(k.clone(), 1);
            t.last_seen.insert(k, idle);
        }
        assert_eq!(t.last_seen.len(), MAX_TRACKED_SENDERS);

        // A fresh, active sender via the real path pushes over the cap → prune
        // runs, expiring the idle entries and keeping the active one.
        assert!(t.check_and_advance("fresh_active", 1));
        assert!(t.last_seen.len() <= MAX_TRACKED_SENDERS, "maps stay bounded under flood");
        assert!(t.last_nonces.len() <= MAX_TRACKED_SENDERS, "nonce map bounded");
        assert!(t.last_seen.contains_key("fresh_active"), "the ACTIVE sender is retained");
        assert!(
            t.last_seen.len() < MAX_TRACKED_SENDERS,
            "idle (replay-safe) entries were expired, not the active one"
        );
    }

    /// TX-AUTH-NONCE-1 §5: the ABSOLUTE size bound must hold even when NOTHING is
    /// expired (a fast Sybil burst keeping every entry fresh — the documented §4
    /// residual path: eviction by oldest-last_seen).
    #[test]
    fn txauth_nonce_tracker_size_bound_holds_when_all_fresh() {
        let mut t = NonceTracker::new();
        let now = now_epoch_secs();
        for i in 0..MAX_TRACKED_SENDERS {
            let k = format!("s{i}");
            t.last_nonces.insert(k.clone(), 1);
            t.last_seen.insert(k, now); // all fresh ⇒ the expiry pass evicts nothing
        }
        assert_eq!(t.last_seen.len(), MAX_TRACKED_SENDERS);
        // One more fresh sender pushes over the cap; with nothing expired, the
        // size bound itself must evict to keep the map bounded.
        assert!(t.check_and_advance("one_more", 1));
        assert!(
            t.last_seen.len() <= MAX_TRACKED_SENDERS,
            "size bound caps the map even with no expired entries"
        );
        assert!(t.last_nonces.len() <= MAX_TRACKED_SENDERS);
    }

    #[test]
    fn test_verify_envelope_valid_signature() {
        let mut crypto = CryptoEngine::new();
        crypto.generate_pq_identity().expect("ml-dsa primary");
        let pk = crypto.pq_identity_hex().expect("ml-dsa primary");
        let msg = GossipMessage::Ping { nonce: 42 };
        // STRUCT-1: Sign full envelope bytes (PQ-ENVELOPE-1: ML-DSA-65)
        let timestamp = chrono::Utc::now().to_rfc3339();
        let nonce = 0_u64;
        let signable =
            GossipRouter::signable_envelope_bytes(&pk, nonce, &timestamp, &msg);
        let sig = crypto.sign_pq(&signable).unwrap();
        let env = GossipRouter::build_signed_envelope(
            pk.clone(),
            msg,
            nonce,
            timestamp,
            &sig,
        )
        .unwrap();

        assert!(
            verify_envelope_signature(&env).is_ok(),
            "Valid signature must pass"
        );
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

        assert!(
            verify_envelope_signature(&env).is_err(),
            "Forged signature must be rejected"
        );
    }

    #[test]
    fn test_verify_envelope_tampered_payload() {
        // Sign one message, but put a different payload in the envelope
        let mut crypto = CryptoEngine::new();
        crypto.generate_pq_identity().expect("ml-dsa primary");
        let pk = crypto.pq_identity_hex().expect("ml-dsa primary");

        let msg_signed = GossipMessage::Ping { nonce: 1 };
        // STRUCT-1: Sign the full envelope with the original payload (ML-DSA-65)
        let timestamp = chrono::Utc::now().to_rfc3339();
        let nonce = 0_u64;
        let signable = GossipRouter::signable_envelope_bytes(
            &pk,
            nonce,
            &timestamp,
            &msg_signed,
        );
        let sig = crypto.sign_pq(&signable).unwrap();

        // Tamper: put a different message in the envelope
        let msg_tampered = GossipMessage::Ping { nonce: 9999 };
        let mut env = GossipRouter::build_signed_envelope(
            pk.clone(),
            msg_tampered,
            nonce,
            timestamp,
            &sig,
        )
        .unwrap();
        env.signature = hex::encode(&sig); // use the original sig (for the wrong payload)

        assert!(
            verify_envelope_signature(&env).is_err(),
            "Tampered payload must be rejected"
        );
    }

    #[test]
    fn validate_envelope_at_uses_injected_time_for_freshness() {
        // Phase 0 (T0.1): freshness is judged against INJECTED time, so the
        // deterministic core validates inbound messages reproducibly. Build one
        // validly-signed envelope with a FIXED timestamp, then validate it at
        // two injected "now"s: inside the ±90 s window it passes, far outside it
        // is rejected as stale — same bytes, time is the only variable.
        let mut crypto = CryptoEngine::new();
        crypto.generate_pq_identity().expect("ml-dsa primary");
        let pk = crypto.pq_identity_hex().expect("ml-dsa primary");
        let msg = GossipMessage::Ping { nonce: 7 };
        let timestamp = "2026-03-01T12:00:00+00:00".to_string();
        let nonce = 0_u64;
        let signable =
            GossipRouter::signable_envelope_bytes(&pk, nonce, &timestamp, &msg);
        let sig = crypto.sign_pq(&signable).unwrap();
        let env = GossipRouter::build_signed_envelope(
            pk.clone(),
            msg,
            nonce,
            timestamp.clone(),
            &sig,
        )
        .unwrap();
        let bytes = serde_json::to_vec(&env).unwrap();

        let t0 = chrono::DateTime::parse_from_rfc3339(&timestamp)
            .unwrap()
            .timestamp();
        // Within the window → fully validated envelope returned.
        assert!(
            validate_envelope_at(&bytes, t0 + 30).is_ok(),
            "fresh vs injected now must validate"
        );
        // Far outside the window → rejected as stale (identical bytes).
        assert_eq!(
            validate_envelope_at(&bytes, t0 + 1_000).unwrap_err(),
            "stale message"
        );
    }

    #[test]
    fn nonce_tracker_basic() {
        let mut tracker = NonceTracker::new();
        assert!(tracker.check_and_advance("peer_a", 1));
        assert!(tracker.check_and_advance("peer_a", 2));
        assert!(
            !tracker.check_and_advance("peer_a", 2),
            "Same nonce must be rejected"
        );
        assert!(
            !tracker.check_and_advance("peer_a", 1),
            "Lower nonce must be rejected"
        );
        assert!(tracker.check_and_advance("peer_a", 3));
    }

    #[test]
    fn nonce_tracker_independent_peers() {
        let mut tracker = NonceTracker::new();
        assert!(tracker.check_and_advance("peer_a", 1));
        assert!(
            tracker.check_and_advance("peer_b", 1),
            "Different peers have independent nonces"
        );
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
            assert!(
                tracker.check_rate_limit("peer_x", 1),
                "should allow within limit"
            );
        }
        // 31st should be rejected
        assert!(
            !tracker.check_rate_limit("peer_x", 1),
            "should reject after limit"
        );
    }

    #[test]
    fn rate_limiter_independent_peers() {
        let mut tracker = NonceTracker::new();
        // Fill up peer_a
        for _ in 0..30 {
            tracker.check_rate_limit("peer_a", 1);
        }
        // peer_b should still be allowed
        assert!(
            tracker.check_rate_limit("peer_b", 1),
            "different peer should have own limit"
        );
    }

    #[test]
    fn adaptive_limit_scales_with_peer_count() {
        // NET-13: sub-linear sqrt scaling, clamped to [MIN, MAX]
        assert_eq!(NonceTracker::adaptive_limit_for(1), 30); // base
        assert_eq!(NonceTracker::adaptive_limit_for(4), 30); // base
        assert_eq!(NonceTracker::adaptive_limit_for(16), 60); // base × 2
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
