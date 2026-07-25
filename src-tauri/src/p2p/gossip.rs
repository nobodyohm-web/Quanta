//! Protocole de gossip P2P — échange d'état entre nœuds QUANTA
//!
//! Les messages gossip sont transportés via Iroh QUIC (existant).
//! Ce module définit les types de messages et la sérialisation.
//!
//! Flux de synchronisation de la chaîne :
//!   1. A → B : `Hello { chain_height }`
//!   2. B compare sa hauteur → `RequestChain { from_height }`
//!   3. A → B : `ChainSegment { blocks }` (max 50 blocs)

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
///
/// PQ-MIG-5 §4: bumped **2 → 3** — the post-quantum genesis (genesis reconstructed
/// on ML-DSA identities + addresses, canonical content-bound genesis hash) is a
/// protocol break, so a v3 node's genesis/chain is incompatible with a v2 node's.
///
/// LIVE-3B: bumped **3 → 4** — a consensus-rules change for `Slash` txs that
/// consume **unbonding** stake: they carry a `slash_unbonding` breakdown that is
/// bound into the tx hash AND the block's Merkle root, and their amount is the
/// ratified fraction of `staked + unbonding` (not bonded alone). A v3 node
/// neither produces nor validates that shape (it would recompute a bonded-only
/// expectation and a breakdown-less Merkle leaf → reject the block). Genesis and
/// all previously-valid history are UNCHANGED (purely-bonded slashes serialize
/// byte-identically), so v3↔v4 nodes only diverge once an unbonding-consuming
/// slash is sealed.
///
/// GENESIS-V4 / PROPOSER-1 / PQ-ENVELOPE-1: bumped **4 → 5** — the v4 hard-fork.
/// Three coupled protocol changes: (a) a **fresh launch genesis** (new frozen
/// hash, zero premine); (b) **PROPOSER-1** — a received non-genesis block whose
/// proposer is not a bonded validator (as of the parent) is rejected, once anyone
/// has staked (deterministic, clock-free); (c) **PQ-ENVELOPE-1** — gossip
/// envelopes are signed with ML-DSA-65, not Ed25519. A v4 node neither builds nor
/// validates any of these, so v4↔v5 nodes are incompatible from block 0.
///
/// MSIG-1: bumped **5 → 6** — native post-quantum **M-of-N multisig** authority. A
/// tx flagged `pq_public_key == "msig1"` is authorized by ≥ threshold valid ML-DSA
/// signatures from distinct registered keys over its pre-image (verified on-chain,
/// no threshold cryptography), and its `from` is the address that commits to the
/// policy `{keys, threshold}`. A v5 node has no multisig path — it would run the
/// single-key checks, fail the key↔address binding, and reject the tx (and any block
/// containing it). The change is otherwise **additive**: single-key txs are
/// byte-identical (no new `Transaction` wire field — the authority rides the existing
/// optional fields), so genesis and all prior history are UNCHANGED; v5↔v6 nodes only
/// diverge once a multisig tx is sealed.
pub const TORUS_PROTOCOL_VERSION: u8 = 6;

// ─── Messages gossip ────────────────────────────────────────────────────────

/// Identifiant unique d'un message (BLAKE3 hex du contenu).
pub type MsgId = String;

/// Enveloppe signée d'un message gossip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipEnvelope {
    pub id: MsgId,
    /// Clé publique ML-DSA-65 de l'émetteur (PQ-ENVELOPE-1)
    pub sender: String,
    pub payload: GossipMessage,
    /// Signature ML-DSA-65 des bytes canoniques de l'enveloppe (PQ-ENVELOPE-1)
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
        /// NET-15: Optional display name for the frontend.
        /// Trimmed to MAX_DISPLAY_NAME_LEN by the dispatcher; carrying a longer
        /// string is treated as adversarial input and clamped without rejection.
        /// The Hello envelope is already ML-DSA-65-signed (PQ-ENVELOPE-1), so an
        /// attacker can't inject a name without the wallet's private key — backward compat is
        /// guaranteed by `#[serde(default)]`.
        #[serde(default)]
        display_name: Option<String>,
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
    ///
    /// NET-8: Two payload modes coexist:
    /// 1. Legacy: `blocks_json` carries the per-block JSON strings inline.
    /// 2. Compressed (preferred): `blocks_compressed` carries gzipped bytes
    ///    of the same `blocks_json` array, encoded as JSON before compression.
    ///
    /// A compatible peer ALWAYS reads `blocks_compressed` first; if absent or
    /// invalid, it falls back to `blocks_json`. Senders may emit one, the
    /// other, or both — `#[serde(default)]` keeps everyone interoperable.
    ChainSegment {
        #[serde(default)]
        blocks_json: Vec<String>,
        /// Total chain height of the sender (so requester knows if more is needed).
        sender_height: u64,
        /// NET-8: Optional gzipped JSON of the `blocks_json` array. When present,
        /// receivers should prefer this over the inline field. Wire-format
        /// compatible with peers that only know about the legacy field.
        #[serde(default)]
        blocks_compressed: Option<Vec<u8>>,
    },
    /// Identité — revendication signée d'un pseudo unique `@handle`.
    /// `record_json` = `UsernameRecord` sérialisé. Le receveur applique via
    /// `username::UsernameRegistry::apply()` (vérifie la signature + résolution
    /// de conflit déterministe).
    PublishUsername {
        record_json: String,
    },
    /// LIVE-1 — a finality **vote** (attestation) broadcast by a staked validator.
    /// `vote_json` = a [`crate::sm::finality_vote::Vote`] serialized. The receiver
    /// re-verifies it (`Vote::verify` against the on-chain stake, GADGET-2) before
    /// feeding it to the live fork-choice ([`crate::sm::fork_choice::LatestVotes`])
    /// and, once a ⅔ certificate forms, the finality rule
    /// ([`crate::sm::finality_rule::FinalityState`], GADGET-3). The gadget's
    /// verdict stays a **pure function of the votes + on-chain stake** — this
    /// variant only carries the vote across the wire; the envelope's ML-DSA-65
    /// signature (PQ-ENVELOPE-1) is transport authentication, the vote's own ML-DSA-65
    /// signature is the finality authority.
    FinalityVote {
        vote_json: String,
    },
    /// LIVE-3 — a **fault proof** (GADGET-4): two contradictory ML-DSA votes from
    /// the same validator (double-vote or surround). `proof_json` = a
    /// [`crate::sm::finality_slashing::FaultProof`] serialized. Each receiver
    /// re-verifies it against the on-chain stake and, if valid, queues a slash
    /// (`Ledger::queue_slash`) that destroys the offender's bonded stake
    /// (STAKE → BURN) in the next sealed block — accountable safety with teeth.
    FinalityFault {
        proof_json: String,
    },
}

/// NET-5: Default the embedded `Hello.version` field for legacy envelopes
/// that predate the protocol-version constant.
fn default_protocol_version() -> u8 {
    1
}

/// NET-15: Maximum length of a peer-supplied display_name, in bytes.
/// Anything longer is silently truncated by the dispatcher to defeat
/// abuse (huge names spam the UI; control characters break terminals).
pub const MAX_DISPLAY_NAME_LEN: usize = 32;

/// NET-15: Sanitize a peer-supplied display name.
/// Returns `None` if empty after sanitisation. Strips control chars and
/// truncates to `MAX_DISPLAY_NAME_LEN` UTF-8 bytes (truncating at a char
/// boundary to avoid producing invalid UTF-8).
pub fn sanitize_display_name(raw: &str) -> Option<String> {
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .to_string();
    if cleaned.is_empty() {
        return None;
    }
    // Truncate at a char boundary up to MAX_DISPLAY_NAME_LEN bytes.
    let mut end = MAX_DISPLAY_NAME_LEN.min(cleaned.len());
    while !cleaned.is_char_boundary(end) {
        end -= 1;
    }
    Some(cleaned[..end].to_string())
}

// ─── NET-8: ChainSegment compression helpers ─────────────────────────────────

/// NET-8: Compress a `blocks_json` array using gzip. Returns `None` if the
/// input fits in fewer than 256 bytes (compression overhead exceeds savings)
/// or if encoding fails. Pure-Rust backend, no native deps.
pub fn compress_blocks(blocks: &[String]) -> Option<Vec<u8>> {
    use flate2::{Compression, write::GzEncoder};
    use std::io::Write;
    let json = serde_json::to_vec(blocks).ok()?;
    if json.len() < 256 {
        return None; // not worth it
    }
    let mut enc = GzEncoder::new(Vec::with_capacity(json.len() / 4), Compression::fast());
    enc.write_all(&json).ok()?;
    enc.finish().ok()
}

/// NET-8: Decompress a gzipped `blocks_json` payload. Hard cap: refuses to
/// inflate beyond `MAX_DECOMPRESSED_BYTES` (50 MB) to defeat zip-bomb DoS.
pub fn decompress_blocks(compressed: &[u8]) -> Result<Vec<String>, String> {
    use flate2::read::GzDecoder;
    use std::io::Read;
    const MAX_DECOMPRESSED_BYTES: usize = 50 * 1024 * 1024;
    let mut dec = GzDecoder::new(compressed);
    let mut out = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = dec.read(&mut buf)
            .map_err(|e| format!("decompress read: {}", e))?;
        if n == 0 {
            break;
        }
        if out.len() + n > MAX_DECOMPRESSED_BYTES {
            return Err(format!("decompressed payload would exceed {} bytes", MAX_DECOMPRESSED_BYTES));
        }
        out.extend_from_slice(&buf[..n]);
    }
    serde_json::from_slice::<Vec<String>>(&out)
        .map_err(|e| format!("decompressed JSON parse: {}", e))
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
    /// H1 (AUDIT-2026-07-25) — read-only dedup probe. Lets the dispatcher shed
    /// retransmits early WITHOUT inserting anything, so an unauthenticated peer can
    /// no longer seed the LRU before its signature has been checked.
    pub fn has_seen(&self, msg_id: &str) -> bool {
        self.seen_messages.contains(msg_id)
    }

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
        Self::is_fresh_at(timestamp, chrono::Utc::now().timestamp())
    }

    /// Freshness check against an **injected** wall-clock value (`now_secs`,
    /// Unix seconds): the timestamp must be within the ±90 s anti-replay window.
    /// Pure — no clock read — so the deterministic core validates inbound
    /// messages reproducibly (Phase 0, Constitution §3). [`Self::is_fresh`] is
    /// the production wrapper that reads the real clock at the boundary.
    pub fn is_fresh_at(timestamp: &str, now_secs: i64) -> bool {
        let Ok(ts) = chrono::DateTime::parse_from_rfc3339(timestamp) else {
            return false;
        };
        let drift = (now_secs - ts.timestamp()).unsigned_abs();
        drift <= 90
    }

    /// Construit un message `Hello` pour initier un sync (V2: watts + pays + STRUCT-6 contribs).
    /// NET-15: Optional `display_name` for human-readable peer labels in the UI.
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
        display_name: Option<String>,
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
            display_name,
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
    /// **H3 (AUDIT-2026-07-25) — the dedup identifier.**
    ///
    /// BLAKE3 over the exact bytes the signature covers (sender, nonce, timestamp,
    /// payload). It used to be `BLAKE3(payload_json)` alone, excluding all three of
    /// the fields the signature binds — so any two envelopes carrying a
    /// byte-identical payload shared ONE dedup slot, network-wide and permanently
    /// (the LRU is persisted in the gossip snapshot).
    ///
    /// `RequestChain{from_height, max_blocks}` and `Ping{nonce}`/`Pong{nonce}` are
    /// exactly such payloads — no per-sender, per-attempt entropy. So the second
    /// node to request the same chain range had its request silently dropped by
    /// every peer that had already answered the first, and never synced.
    ///
    /// Binding the id to the full pre-image also closes H1: the id becomes a pure
    /// function of signed material, so `validate_envelope_id` can reject a forged
    /// one before it ever touches the dedup LRU. Anti-replay is unaffected — it is
    /// carried by the per-sender monotonic nonce and the freshness window.
    pub fn envelope_id(
        sender: &str,
        nonce: u64,
        timestamp: &str,
        payload: &GossipMessage,
    ) -> String {
        let full = Self::signable_envelope_bytes(sender, nonce, timestamp, payload);
        hex::encode(blake3::hash(&full).as_bytes())
    }

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
    /// The `sig_bytes` MUST be the ML-DSA-65 signature (PQ-ENVELOPE-1) of `signable_envelope_bytes(sender, nonce, &timestamp, &payload)`.
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
        // H3 (AUDIT-2026-07-25): the id is derived from the CANONICAL SIGNED
        // PRE-IMAGE, not from the payload alone — see `envelope_id`.
        let id = Self::envelope_id(&sender, nonce, &timestamp, &payload);
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
    fn h3_same_payload_from_two_senders_gets_distinct_ids() {
        // H3 (AUDIT-2026-07-25): the id was BLAKE3(payload) alone, excluding
        // sender, nonce and timestamp — all three of which the signature binds. So
        // two nodes requesting the same chain range shared ONE dedup slot,
        // network-wide and permanently (the LRU is persisted). The second node's
        // RequestChain was silently dropped by every peer that had answered the
        // first, and it never synced.
        let payload = GossipMessage::RequestChain { from_height: 0, max_blocks: 50 };
        let a = GossipRouter::envelope_id("sender-a", 1, "2026-07-25T00:00:00Z", &payload);
        let b = GossipRouter::envelope_id("sender-b", 1, "2026-07-25T00:00:00Z", &payload);
        assert_ne!(a, b, "the dedup key must bind the sender");

        // Same sender retrying with a fresh nonce must also get a fresh slot.
        let retry = GossipRouter::envelope_id("sender-a", 2, "2026-07-25T00:00:00Z", &payload);
        assert_ne!(a, retry, "a retry must not collide with the original attempt");
    }

    #[test]
    fn h3_envelope_id_is_the_canonical_signed_preimage_digest() {
        // The id must be a pure function of the signed material, so a receiver can
        // recompute it and refuse an attacker-chosen one (that is what closes H1).
        let payload = GossipMessage::Ping { nonce: 7 };
        let expected = hex::encode(
            blake3::hash(&GossipRouter::signable_envelope_bytes(
                "s", 3, "2026-07-25T00:00:00Z", &payload,
            ))
            .as_bytes(),
        );
        assert_eq!(
            GossipRouter::envelope_id("s", 3, "2026-07-25T00:00:00Z", &payload),
            expected
        );
    }

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
    fn is_fresh_at_is_injected_time_driven() {
        // Phase 0 (T0.1): freshness is a pure function of the INJECTED clock.
        let ts = "2026-03-01T12:00:00+00:00";
        let t0 = chrono::DateTime::parse_from_rfc3339(ts).unwrap().timestamp();
        // Inside the ±90 s window (either side) → fresh.
        assert!(GossipRouter::is_fresh_at(ts, t0));
        assert!(GossipRouter::is_fresh_at(ts, t0 + 90));
        assert!(GossipRouter::is_fresh_at(ts, t0 - 90));
        // Outside the window → stale.
        assert!(!GossipRouter::is_fresh_at(ts, t0 + 91));
        assert!(!GossipRouter::is_fresh_at(ts, t0 - 91));
        // Unparseable timestamp → never fresh.
        assert!(!GossipRouter::is_fresh_at("not-a-date", t0));
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
            vec![], "n".into(), 10.0, "FR".into(), 0, 0, 0, 0, vec![], None
        );
        match hello {
            GossipMessage::Hello { version, .. } => {
                assert_eq!(version, TORUS_PROTOCOL_VERSION);
            }
            _ => panic!("expected Hello variant"),
        }
    }

    #[test]
    fn sanitize_display_name_strips_control_and_truncates() {
        // NET-15: control chars dropped, whitespace trimmed, capped at MAX.
        assert_eq!(sanitize_display_name(""), None);
        assert_eq!(sanitize_display_name("   "), None);
        assert_eq!(sanitize_display_name("\t\nhi"), Some("hi".into()));
        let long = "a".repeat(100);
        let sanitized = sanitize_display_name(&long).unwrap();
        assert!(sanitized.len() <= MAX_DISPLAY_NAME_LEN);
    }

    #[test]
    fn sanitize_display_name_preserves_unicode_at_boundary() {
        // Multi-byte char near the cap should not be split mid-codepoint.
        let mut s = String::from("alex");
        // Repeat a 4-byte emoji until close to cap.
        while s.len() + 4 <= MAX_DISPLAY_NAME_LEN + 4 {
            s.push('🚀');
        }
        let sanitised = sanitize_display_name(&s).unwrap();
        // Must still be valid UTF-8 (rust String enforces this) and within cap.
        assert!(sanitised.len() <= MAX_DISPLAY_NAME_LEN);
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
    fn compress_decompress_roundtrip() {
        // NET-8: A non-trivial blocks_json must round-trip through gzip cleanly.
        let blocks: Vec<String> = (0..30)
            .map(|i| format!(r#"{{"height":{},"prev":"abc{:0>60}","tx":[]}}"#, i, i))
            .collect();
        let inline_size: usize = blocks.iter().map(|s| s.len()).sum();
        let compressed = compress_blocks(&blocks).expect("must compress");
        // Must be smaller than the inline form for repetitive JSON.
        assert!(
            compressed.len() < inline_size,
            "gzip should reduce repetitive JSON: {} bytes inline vs {} bytes compressed",
            inline_size, compressed.len()
        );
        let decoded = decompress_blocks(&compressed).expect("must decode");
        assert_eq!(decoded, blocks, "round-trip identity");
    }

    #[test]
    fn compress_skips_tiny_payloads() {
        // For inputs under the 256-byte threshold compression overhead exceeds
        // savings. Return None so the caller stays on the inline path.
        let tiny = vec!["a".to_string(); 5];
        assert!(compress_blocks(&tiny).is_none());
    }

    #[test]
    fn decompress_rejects_garbage() {
        // Malformed gzip must error, never panic.
        assert!(decompress_blocks(&[0xFF, 0xFE, 0x00, 0x01]).is_err());
        assert!(decompress_blocks(&[]).is_err());
    }

    #[test]
    fn chain_segment_with_compressed_field_deserializes_legacy_default() {
        // NET-8: A peer sending only `blocks_json` (legacy wire format) must
        // deserialize cleanly with `blocks_compressed = None`.
        let legacy = serde_json::json!({
            "type": "ChainSegment",
            "data": {
                "blocks_json": ["{}", "{}"],
                "sender_height": 7
            }
        });
        let parsed: GossipMessage = serde_json::from_value(legacy).unwrap();
        match parsed {
            GossipMessage::ChainSegment { blocks_compressed, sender_height, .. } => {
                assert!(blocks_compressed.is_none());
                assert_eq!(sender_height, 7);
            }
            _ => panic!("expected ChainSegment"),
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
