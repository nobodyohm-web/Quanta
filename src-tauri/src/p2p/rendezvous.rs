//! # RDV-1 — Serverless rendezvous by topic (mainline BitTorrent DHT)
//!
//! ## The hole this closes
//!
//! `willow_node::init_endpoint` subscribes to the shared QUANTA topic with an
//! **empty** bootstrap list (`gossip.subscribe(topic, vec![])`). iroh-gossip is
//! not a global DHT: sharing a topic does not put you in its mesh, you need at
//! least one peer already in it. So a fresh node joined the topic **alone** and
//! the only way in was a human pasting a ticket (`commands::network::connect_peer`).
//! Two strangers on opposite sides of the world never met — each mined its own
//! divergent chain, and whichever one lost the eventual heal saw its blocks (and
//! its coins) rolled back.
//!
//! Everything *after* first contact already worked: NET-2 peer exchange
//! (`dispatcher`) carries known peers in every `Hello` and auto-dials up to 3 of
//! them, so the mesh spreads on its own. Only the **first handshake** was missing.
//!
//! ## Why the DHT, and why not iroh's own DHT support
//!
//! iroh's `DhtAddressLookup` (feature `address-lookup-pkarr-dht`, enabled
//! alongside this module) publishes *this* endpoint's address on the mainline
//! DHT under *its own* EndpointId, and resolves an EndpointId someone hands us.
//! That makes a ticket permanent — reachable across NAT and IP changes without
//! the n0 relays — but it answers "where is node X?", never "who else runs
//! Quanta?". There is no keyword lookup: you must already know the key.
//!
//! The mainline DHT (millions of nodes, no owner, no bill, running since 2005)
//! does offer a rendezvous primitive, but `announce_peer`/`get_peers` stores
//! `IP:port`, and iroh needs an **EndpointId** to open a session (the identity is
//! verified inside TLS). So we use BEP-44 *mutable items* instead, which store
//! arbitrary bytes under an ed25519 key of our choosing.
//!
//! ## The slot board
//!
//! We derive [`SLOTS`] ed25519 keypairs deterministically from the gossip topic.
//! Every node picks its slot from its own EndpointId and publishes a small
//! **board**: its own EndpointId first, then peers it currently knows. Reading a
//! handful of slots therefore yields a whole address book, not one node — which
//! is what makes a cold start converge in one round instead of many.
//!
//! Because the slot keys are derived from a public topic, **everyone holds the
//! private key of every slot**. That is deliberate, and it is why publishing
//! does a read-merge-write instead of a blind overwrite: two nodes sharing a slot
//! accumulate into the same board rather than erasing each other.
//!
//! ## Threat model — the board is HOSTILE input
//!
//! Anyone can write anything to any slot. Concretely that buys an attacker two
//! things, and only these:
//!
//! **Wasted dials** — entries pointing at endpoints that do not exist, or that
//! are theirs. A board entry is only ever a *candidate address to dial*:
//!
//! - every gossip envelope is still authenticated ML-DSA-65 (PQ-ENVELOPE-1) —
//!   a peer we met through the DHT has no more authority than any other;
//! - dials are capped per cycle ([`MAX_DIALS_PER_CYCLE`]) and boards are capped
//!   in size, so a stuffed slot cannot turn into a dial storm;
//! - `known_peers` is already bounded (`MAX_KNOWN_PEERS`), and the NET-12
//!   eclipse heuristic still watches for prefix-collision floods.
//!
//! **Suppression of the board itself** — the slot keys are public, so a vandal
//! can keep overwriting every slot with junk, or park a huge BEP-44 sequence
//! number to shout over honest writers. Publishing outbids any camped value
//! automatically ([`effective_seq`] takes `max(now, stored + 1)`), so a hard
//! freeze requires exactly `i64::MAX`, and sustained suppression means
//! rewriting all [`SLOTS`] slots faster than every honest node's cadence,
//! forever. Even then the blast radius is **bootstrap only**: nodes already
//! connected keep their mesh (NET-2 carries peers in every `Hello`), and a
//! manually pasted ticket still works. Denial of first contact is the ceiling
//! of what a DHT attacker can achieve.
//!
//! What the design itself costs, honestly: announcing on a public DHT makes the
//! Quanta network **enumerable** — anyone can list participating endpoints.
//! That is the same trade BitTorrent has always made, and it is the price of
//! having no server to ask.
//!
//! ## What this module does NOT do
//!
//! It does not touch consensus, balances, or any block. It is pure peer
//! discovery, entirely outside the security path — a bad board costs a failed
//! TCP dial, never a wrong balance.

use std::collections::BTreeSet;
use std::time::Duration;

use futures_util::future::join_all;
use mainline::{Dht, MutableItem, SigningKey, async_dht::AsyncDht};

/// Domain separator — keeps these derived keys disjoint from every other BLAKE3
/// use in the project (addresses, block hashes, the election beacon).
const RDV_DOMAIN: &[u8] = b"quanta-rendezvous-v1";

/// BEP-44 salt. Namespaces our records under the derived keys, so an unrelated
/// pkarr user of the same key never collides with us.
const RDV_SALT: &[u8] = b"qta-rdv";

/// Number of board slots. Sized so a small network spreads across distinct DHT
/// keys (few collisions) while a cold start can still read every slot in one
/// parallel sweep.
pub const SLOTS: u8 = 32;

/// Board wire magic — a version tag, so a future format change is detectable
/// rather than silently misparsed.
const BOARD_MAGIC: [u8; 4] = *b"QTA1";

/// Max endpoints per board. BEP-44 values are limited to 1000 bytes by the
/// protocol; `5 + 24*32 = 773` leaves comfortable headroom.
pub const MAX_IDS_PER_BOARD: usize = 24;

/// Hard ceiling on dials attempted per discovery cycle — a stuffed slot must not
/// become a connection storm.
pub const MAX_DIALS_PER_CYCLE: usize = 8;

/// Cruising rhythm once we have at least one live peer. BEP-44 records are
/// dropped by DHT nodes after roughly two hours, so this must stay well under
/// that.
pub const REPUBLISH_INTERVAL: Duration = Duration::from_secs(25 * 60);

/// Bootstrap rhythm: while this node has **zero live peers**, cycles retry fast,
/// doubling from this floor up to [`REPUBLISH_INTERVAL`] (see
/// [`next_cycle_delay`]). Without it, two machines started together would each
/// harvest before the other had published and then sleep 25 minutes — a blind
/// spot exactly where "it just connects" matters most. The fast cadence stops
/// the moment a peer is live, so steady-state DHT traffic is unchanged.
pub const BOOTSTRAP_RETRY_FLOOR: Duration = Duration::from_secs(30);

/// Retry pace when joining the DHT itself fails (UDP blocked at launch, machine
/// offline). Discovery must come back when the network does, not stay dead for
/// the whole session.
const DHT_INIT_RETRY: Duration = Duration::from_secs(5 * 60);

/// Bound on a single DHT sweep, so a slow or partitioned DHT can never wedge the
/// discovery task.
const SWEEP_TIMEOUT: Duration = Duration::from_secs(30);

/// A 32-byte iroh EndpointId, kept as raw bytes so the board format never
/// depends on a string encoding.
pub type RawEndpointId = [u8; 32];

/// Deterministically derive the signing key of `slot` from the gossip topic.
///
/// Public by construction: every node derives the same 32 keys from the same
/// public topic. See the threat-model note in the module docs — this is what
/// makes the board writable by anyone, and why publishing merges.
pub fn slot_signing_key(topic: &[u8; 32], slot: u8) -> SigningKey {
    let mut hasher = blake3::Hasher::new();
    hasher.update(RDV_DOMAIN);
    hasher.update(topic);
    hasher.update(&[slot]);
    SigningKey::from_bytes(hasher.finalize().as_bytes())
}

/// The slot a node publishes into — a pure function of its EndpointId, so a node
/// always lands in the same slot and its board entry stays refreshable.
pub fn slot_of(endpoint_id: &RawEndpointId) -> u8 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(RDV_DOMAIN);
    hasher.update(endpoint_id);
    hasher.finalize().as_bytes()[0] % SLOTS
}

/// Encode a board. Truncates to [`MAX_IDS_PER_BOARD`] — the caller decides the
/// priority order (self first, then freshest peers).
pub fn encode_board(ids: &[RawEndpointId]) -> Vec<u8> {
    let n = ids.len().min(MAX_IDS_PER_BOARD);
    let mut out = Vec::with_capacity(5 + n * 32);
    out.extend_from_slice(&BOARD_MAGIC);
    out.push(n as u8);
    for id in ids.iter().take(n) {
        out.extend_from_slice(id);
    }
    out
}

/// Decode a board from **untrusted** DHT bytes.
///
/// Returns an empty vec on anything malformed — a corrupt or hostile record must
/// degrade to "no peers discovered", never to an error that stops discovery.
pub fn decode_board(bytes: &[u8]) -> Vec<RawEndpointId> {
    if bytes.len() < 5 || bytes[..4] != BOARD_MAGIC {
        return Vec::new();
    }
    let claimed = bytes[4] as usize;
    // Never trust the count: derive the real one from the actual length, and cap
    // it. A record claiming 255 entries in 40 bytes yields what it truly holds.
    let available = (bytes.len() - 5) / 32;
    let n = claimed.min(available).min(MAX_IDS_PER_BOARD);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let start = 5 + i * 32;
        let mut id = [0u8; 32];
        id.copy_from_slice(&bytes[start..start + 32]);
        // An all-zero key is not a valid ed25519 point and never a real node.
        if id != [0u8; 32] {
            out.push(id);
        }
    }
    out
}

/// Merge our view into an existing board: `self_id` first (it is the entry we
/// can actually vouch for), then `peers`, then whatever was already published.
///
/// Deduplicates while preserving that priority, and truncates. Read-merge-write
/// is what lets two nodes share a slot without erasing each other.
pub fn merge_board(
    self_id: &RawEndpointId,
    peers: &[RawEndpointId],
    existing: &[RawEndpointId],
) -> Vec<RawEndpointId> {
    let mut seen: BTreeSet<RawEndpointId> = BTreeSet::new();
    let mut out = Vec::with_capacity(MAX_IDS_PER_BOARD);
    for id in std::iter::once(self_id).chain(peers.iter()).chain(existing.iter()) {
        if out.len() >= MAX_IDS_PER_BOARD {
            break;
        }
        if *id != [0u8; 32] && seen.insert(*id) {
            out.push(*id);
        }
    }
    out
}

/// The sequence number a publication must carry to be accepted.
///
/// BEP-44 nodes only store an update whose `seq` is **strictly higher** than the
/// one they hold, and every writer shares the slot key. Wall-clock seconds alone
/// would therefore lose twice: two colocated nodes publishing within the same
/// second silently reject each other, and a vandal who parked a huge `seq` on
/// the slot would silence every honest writer until the clock caught up.
/// `max(now, stored + 1)` outbids both; only `i64::MAX` itself is unbeatable
/// (saturating, and documented as the ceiling in the module threat model).
fn effective_seq(now: i64, stored: Option<i64>) -> i64 {
    match stored {
        Some(s) => now.max(s.saturating_add(1)),
        None => now,
    }
}

/// Live handle on the mainline DHT, scoped to one gossip topic.
pub struct Rendezvous {
    dht: AsyncDht,
    topic: [u8; 32],
}

impl Rendezvous {
    /// Join the mainline DHT. Blocks until the routing table has bootstrapped so
    /// the first publish is not shouted into the void.
    pub async fn new(topic: [u8; 32]) -> Result<Self, String> {
        let dht = Dht::builder()
            .build()
            .map_err(|e| format!("DHT init failed: {e}"))?
            .as_async();
        // Best-effort: a `false` here means the routing table is still thin, not
        // that the DHT is unusable — publishing and reading both still work and
        // improve as it fills.
        if !dht.bootstrapped().await {
            log::warn!("◈ [RDV-1] DHT bootstrap incomplete — discovery will be degraded this cycle");
        }
        Ok(Self { dht, topic })
    }

    /// Publish our board into our own slot (read-merge-write).
    ///
    /// `now` is wall-clock seconds; the sequence number actually sent is
    /// [`effective_seq`] of it against whatever the slot already holds.
    pub async fn publish(
        &self,
        self_id: &RawEndpointId,
        peers: &[RawEndpointId],
        now: i64,
    ) -> Result<usize, String> {
        let slot = slot_of(self_id);
        let key = slot_signing_key(&self.topic, slot);
        let pubkey = key.verifying_key().to_bytes();

        let stored = self.dht.get_mutable_most_recent(&pubkey, Some(RDV_SALT)).await;
        let existing = stored.as_ref().map(|item| decode_board(item.value())).unwrap_or_default();

        let board = merge_board(self_id, peers, &existing);
        let value = encode_board(&board);
        let seq = effective_seq(now, stored.as_ref().map(|item| item.seq()));
        let item = MutableItem::new(key, &value, seq, Some(RDV_SALT));

        self.dht
            .put_mutable(item, None)
            .await
            .map_err(|e| format!("DHT publish to slot {slot} failed: {e}"))?;

        log::info!("◈ [RDV-1] Published {} endpoints to slot {slot}", board.len());
        Ok(board.len())
    }

    /// Read every slot in parallel and return the discovered endpoints, minus
    /// ourselves. Slots that are empty, malformed or slow simply contribute
    /// nothing.
    pub async fn harvest(&self, self_id: &RawEndpointId) -> Vec<RawEndpointId> {
        let reads = (0..SLOTS).map(|slot| {
            let key = slot_signing_key(&self.topic, slot);
            let pubkey = key.verifying_key().to_bytes();
            async move {
                match tokio::time::timeout(
                    SWEEP_TIMEOUT,
                    self.dht.get_mutable_most_recent(&pubkey, Some(RDV_SALT)),
                )
                .await
                {
                    Ok(Some(item)) => decode_board(item.value()),
                    // Timed out, or nobody has published here yet.
                    Ok(None) | Err(_) => Vec::new(),
                }
            }
        });

        let mut found: BTreeSet<RawEndpointId> = BTreeSet::new();
        for board in join_all(reads).await {
            for id in board {
                if id != *self_id {
                    found.insert(id);
                }
            }
        }
        let out: Vec<RawEndpointId> = found.into_iter().collect();
        log::info!("◈ [RDV-1] Harvested {} distinct endpoints from {SLOTS} slots", out.len());
        out
    }
}

/// Delay before the first DHT cycle: the endpoint must be bound and its home
/// relay established, otherwise we would publish an address nobody can dial.
const STARTUP_DELAY: Duration = Duration::from_secs(20);

/// Parse an EndpointId string (as stored in `known_peers` / tickets) to raw bytes.
fn parse_endpoint(s: &str) -> Option<RawEndpointId> {
    use std::str::FromStr;
    iroh::EndpointId::from_str(s).ok().map(|id| *id.as_bytes())
}

/// Render raw bytes back to the string form `connect_peer` accepts.
fn render_endpoint(id: &RawEndpointId) -> Option<String> {
    iroh::EndpointId::from_bytes(id).ok().map(|id| id.to_string())
}

/// Run one discovery cycle: harvest, dial what is new, then republish.
///
/// Harvest happens **before** publish so a cold-starting node is useful to
/// itself immediately, and publishes a board that already includes the peers it
/// just found.
async fn cycle(state: &std::sync::Arc<crate::AppState>, rdv: &Rendezvous) {
    let Some(self_id) = state.node.get_ticket().await.as_deref().and_then(parse_endpoint) else {
        log::debug!("◈ [RDV-1] pas d'EndpointId local — cycle ignoré");
        return;
    };

    // ── 1. Harvest ────────────────────────────────────────────────────────
    let discovered = rdv.harvest(&self_id).await;

    // ── 2. Dial what we do not already know, hard-capped ──────────────────
    let known: BTreeSet<String> = state.node.known_peers.read().await.keys().cloned().collect();
    let mut dialed = 0usize;
    for (idx, id) in discovered.iter().enumerate() {
        if dialed >= MAX_DIALS_PER_CYCLE {
            log::info!(
                "◈ [RDV-1] plafond de {MAX_DIALS_PER_CYCLE} connexions atteint — {} candidats restants pour le prochain cycle",
                discovered.len() - idx
            );
            break;
        }
        let Some(peer) = render_endpoint(id) else { continue };
        if known.contains(&peer) {
            continue;
        }
        match state.node.connect_peer(&peer).await {
            Ok(()) => {
                dialed += 1;
                log::info!("◈ [RDV-1] Connecté à un pair découvert par la DHT: {}", &peer[..peer.len().min(16)]);
            }
            // A stale or hostile board entry costs exactly one failed dial.
            Err(e) => log::debug!("◈ [RDV-1] pair injoignable {}: {e}", &peer[..peer.len().min(16)]),
        }
    }
    if dialed > 0 {
        crate::p2p::gossip_tasks::trigger_hello_now(state).await;
    }

    // ── 3. Republish our board (self + current peers) ─────────────────────
    let peers: Vec<RawEndpointId> = state
        .node
        .known_peers
        .read()
        .await
        .keys()
        .filter_map(|s| parse_endpoint(s))
        .take(MAX_IDS_PER_BOARD - 1)
        .collect();
    let now = chrono::Utc::now().timestamp();
    if let Err(e) = rdv.publish(&self_id, &peers, now).await {
        log::warn!("◈ [RDV-1] publication DHT échouée: {e}");
    }
}

/// Pure cadence rule for the discovery loop.
///
/// Connected (≥1 live peer): cruise at [`REPUBLISH_INTERVAL`] — discovery is
/// maintenance. Alone: retry fast with exponential backoff from
/// [`BOOTSTRAP_RETRY_FLOOR`], capped at the cruising rhythm. This is what turns
/// "two machines started together meet after 25 min" into "they meet in about a
/// minute": each publishes on its first cycle, and the other's next harvest is
/// 30 s away, not 25 min.
fn next_cycle_delay(live_peers: usize, zero_peer_cycles: u32) -> Duration {
    if live_peers > 0 {
        return REPUBLISH_INTERVAL;
    }
    // 30 s, 1 m, 2 m, 4 m, 8 m, 16 m, then the cap. Shift is clamped so the
    // counter can never overflow the multiplication.
    let factor = 1u32 << zero_peer_cycles.min(6);
    REPUBLISH_INTERVAL.min(BOOTSTRAP_RETRY_FLOOR * factor)
}

/// Spawn the rendezvous loop — the task that makes two strangers on opposite
/// sides of the world find each other with no server and no pasted code.
pub fn spawn(state: std::sync::Arc<crate::AppState>) {
    tokio::spawn(async move {
        tokio::time::sleep(STARTUP_DELAY).await;

        // Joining the DHT can fail transiently (UDP blocked at launch, no
        // network yet). Keep trying: this task is the only automatic way into
        // the network, so it must never die for a whole session. Manual tickets
        // and NET-2 peer exchange keep working in the meantime.
        let rdv = loop {
            match Rendezvous::new(crate::p2p::willow_node::quanta_topic_bytes()).await {
                Ok(r) => break r,
                Err(e) => {
                    log::warn!(
                        "◈ [RDV-1] DHT indisponible: {e} — nouvel essai dans {} min",
                        DHT_INIT_RETRY.as_secs() / 60
                    );
                    tokio::time::sleep(DHT_INIT_RETRY).await;
                }
            }
        };
        log::info!("◈ [RDV-1] Rendez-vous DHT actif — découverte automatique en ligne");

        let mut zero_peer_cycles: u32 = 0;
        loop {
            cycle(&state, &rdv).await;
            let live_peers = state.node.peer_info.read().await.len();
            let delay = next_cycle_delay(live_peers, zero_peer_cycles);
            zero_peer_cycles =
                if live_peers == 0 { zero_peer_cycles.saturating_add(1) } else { 0 };
            tokio::time::sleep(delay).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(byte: u8) -> RawEndpointId {
        [byte; 32]
    }

    #[test]
    fn board_roundtrip_preserves_order_and_content() {
        let ids = vec![id(1), id(2), id(3)];
        let decoded = decode_board(&encode_board(&ids));
        assert_eq!(decoded, ids, "un board encodé puis décodé doit être identique");
    }

    #[test]
    fn board_encoding_is_capped() {
        // Starts at 1: `id(0)` is the all-zero key, which `decode_board` drops by
        // design (see `decode_rejects_garbage_without_panicking`).
        let ids: Vec<RawEndpointId> = (1..=100u8).map(id).collect();
        let bytes = encode_board(&ids);
        assert!(bytes.len() <= 800, "le board doit rester sous la limite BEP-44 (1000 o), got {}", bytes.len());
        assert_eq!(decode_board(&bytes).len(), MAX_IDS_PER_BOARD);
    }

    #[test]
    fn decode_rejects_garbage_without_panicking() {
        // Hostile input is the norm here — every one of these must yield "no peers".
        assert!(decode_board(&[]).is_empty(), "vide");
        assert!(decode_board(b"nope").is_empty(), "trop court");
        assert!(decode_board(b"XXXX\x01").is_empty(), "mauvais magic");
        // Right magic, count lies (claims 255 entries, carries none).
        assert!(decode_board(b"QTA1\xff").is_empty(), "compte mensonger");
        // Right magic, count lies upward over a partial entry.
        let mut truncated = Vec::from(&b"QTA1\x05"[..]);
        truncated.extend_from_slice(&[7u8; 40]); // 1 full entry + 8 stray bytes
        assert_eq!(decode_board(&truncated).len(), 1, "n'accepte que les entrées complètes");
    }

    #[test]
    fn decode_drops_zero_keys() {
        let ids = vec![id(0), id(9)];
        assert_eq!(decode_board(&encode_board(&ids)), vec![id(9)], "la clé nulle est ignorée");
    }

    #[test]
    fn merge_puts_self_first_and_dedups() {
        let me = id(1);
        let merged = merge_board(&me, &[id(2), id(1)], &[id(3), id(2)]);
        assert_eq!(merged, vec![id(1), id(2), id(3)], "self en tête, sans doublon");
    }

    #[test]
    fn merge_is_capped_and_keeps_priority() {
        let me = id(200);
        let peers: Vec<RawEndpointId> = (0..50u8).map(id).collect();
        let merged = merge_board(&me, &peers, &[]);
        assert_eq!(merged.len(), MAX_IDS_PER_BOARD);
        assert_eq!(merged[0], me, "self survit toujours à la troncature");
    }

    #[test]
    fn merge_of_a_colocated_node_keeps_both() {
        // Two nodes sharing a slot: the second publisher reads the first's board
        // and merges instead of overwriting — this is what makes slot collisions
        // survivable rather than lossy.
        let a = id(1);
        let b = id(2);
        let board_a = merge_board(&a, &[], &[]);
        let board_b = merge_board(&b, &[], &board_a);
        assert!(board_b.contains(&a) && board_b.contains(&b), "les deux colocataires restent listés");
    }

    #[test]
    fn slot_is_deterministic_and_in_range() {
        let x = id(42);
        assert_eq!(slot_of(&x), slot_of(&x), "même id → même slot");
        for b in 0..=255u8 {
            assert!(slot_of(&id(b)) < SLOTS, "le slot reste dans [0, SLOTS)");
        }
    }

    #[test]
    fn slot_keys_are_deterministic_and_topic_bound() {
        let t1 = [7u8; 32];
        let t2 = [8u8; 32];
        assert_eq!(
            slot_signing_key(&t1, 3).to_bytes(),
            slot_signing_key(&t1, 3).to_bytes(),
            "dérivation reproductible — deux nœuds lisent le même slot"
        );
        assert_ne!(
            slot_signing_key(&t1, 3).to_bytes(),
            slot_signing_key(&t1, 4).to_bytes(),
            "slots distincts"
        );
        assert_ne!(
            slot_signing_key(&t1, 3).to_bytes(),
            slot_signing_key(&t2, 3).to_bytes(),
            "un autre topic ne partage aucun slot"
        );
    }

    #[test]
    fn slots_spread_across_the_board() {
        // A pathological derivation (everyone in slot 0) would silently collapse
        // discovery into one record. Assert real spread.
        let used: BTreeSet<u8> = (0..=255u8).map(|b| slot_of(&id(b))).collect();
        assert!(used.len() > (SLOTS as usize) / 2, "répartition dégénérée : {} slots utilisés", used.len());
    }

    #[test]
    fn effective_seq_outbids_and_never_ties() {
        assert_eq!(effective_seq(1000, None), 1000, "slot vierge → l'horloge suffit");
        assert_eq!(effective_seq(1000, Some(500)), 1000, "l'horloge domine un slot ancien");
        // Two colocated nodes publishing within the same second must not reject
        // each other — BEP-44 demands a strictly higher seq.
        assert_eq!(effective_seq(1000, Some(1000)), 1001, "même seconde → surenchère");
        // A vandal parking an inflated seq is outbid instead of silencing us.
        assert_eq!(effective_seq(1000, Some(999_999)), 1_000_000, "seq gonflé → surenchéri");
        // Only i64::MAX freezes a slot; the math must saturate, not overflow.
        assert_eq!(effective_seq(1000, Some(i64::MAX)), i64::MAX, "saturation sans overflow");
    }

    #[test]
    fn bootstrap_cadence_is_fast_when_alone_and_slow_once_connected() {
        // Connected → cruising rhythm, regardless of history.
        assert_eq!(next_cycle_delay(1, 0), REPUBLISH_INTERVAL);
        assert_eq!(next_cycle_delay(3, 42), REPUBLISH_INTERVAL);
        // Alone → fast, doubling: this is what closes the 25-minute blind spot
        // when two machines are started together.
        assert_eq!(next_cycle_delay(0, 0), BOOTSTRAP_RETRY_FLOOR);
        assert_eq!(next_cycle_delay(0, 2), BOOTSTRAP_RETRY_FLOOR * 4);
        // The backoff caps at the cruising rhythm and never overflows.
        assert_eq!(next_cycle_delay(0, 6), REPUBLISH_INTERVAL);
        assert_eq!(next_cycle_delay(0, u32::MAX), REPUBLISH_INTERVAL);
    }
}
