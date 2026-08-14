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
//! Anyone can write anything to any slot. Concretely that buys an attacker
//! wasted dials and suppression of the board — plus, until **R9** below, the
//! outright substitution of every peer a cold node dialed.
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
//! forever. Even then a node already connected keeps its mesh (NET-2 carries
//! peers in every `Hello`) and a manually pasted ticket still works.
//!
//! ## **R9 (AUDIT-2026-08-13) — l'ordre de composition était minable**
//!
//! Ce paragraphe concluait que « le refus du premier contact est le plafond de
//! ce qu'un attaquant DHT peut obtenir ». C'était faux : ce n'était pas un
//! refus, c'était une **substitution**. [`Rendezvous::harvest`] agrégeait les
//! identifiants dans un `BTreeSet` puis renvoyait `into_iter().collect()` —
//! donc **triés par octets bruts** — et [`cycle`] composait les
//! [`MAX_DIALS_PER_CYCLE`] premiers inconnus, sans limite par source ni
//! mélange. Or un `EndpointId` **est** une clé publique ed25519 : en miner une
//! dont les trois ou quatre premiers octets sont nuls coûte 2²⁴ à 2³² essais,
//! quelques secondes à quelques minutes de CPU. Huit identités minées une fois,
//! écrites dans les 32 slots dont l'attaquant détient la clé comme tout le
//! monde, et tout nœud froid se connectait **exclusivement à lui** : il ne
//! voyait plus que la chaîne, les blocs, les votes de finalité et les `@pseudo`
//! qu'on voulait bien lui montrer — au moment précis où l'utilisateur reçoit
//! ses premières pièces. Le tri n'était pas une commodité de présentation,
//! c'était le classement d'un concours de preuve de travail que l'attaquant
//! était seul à disputer.
//!
//! Deux corrections : l'ordre de composition est tiré de l'entropie **locale**
//! du nœud à chaque cycle (`OsRng` dans [`select_candidates`] — une graine
//! publique ou dérivée du topic serait reproductible par l'attaquant, donc
//! minable de nouveau), et un slot ne peut plus livrer plus de
//! [`MAX_ADOPTED_PER_SLOT`] candidats.
//!
//! ### Ce que ce correctif n'achète pas
//!
//! Tout le monde détient la clé privée de chaque slot, et cela ne change pas :
//! l'attaquant peut toujours réécrire les [`SLOTS`] slots en boucle et y saturer
//! les [`MAX_IDS_PER_BOARD`] entrées. On passe d'une éclipse **certaine et
//! gratuite** — miner huit clés une fois, publier une fois, s'en aller — à une
//! éclipse **probabiliste et entretenue**, qui suppose de dominer durablement
//! les slots contre la republication des nœuds honnêtes.
//!
//! Le calcul, sans arrondi flatteur. Après plafonnement le vivier compte au plus
//! `SLOTS × MAX_ADOPTED_PER_SLOT = 64` candidats, dans lequel le cycle en tire
//! [`MAX_DIALS_PER_CYCLE`] = 8 **sans remise**. Si l'attaquant occupe une
//! fraction `f` de ce vivier, soit `K = f·N` entrées, la probabilité que les 8
//! connexions soient toutes à lui vaut
//!
//! ```text
//! P = Π (K − i) / (N − i)  pour i de 0 à 7,   soit P < f⁸
//! ```
//!
//! strictement **inférieur** à `f⁸` parce que chaque tirage épuise le stock de
//! l'attaquant : à `f = 1/2` c'est 0,238 % (et non 0,391 %), à `f = 3/4` c'est
//! 8,53 % (et non 10,01 %). À `f = 1`, P vaut 1 — un attaquant qui domine
//! **tous** les slots éclipse toujours, et aucun mélange n'y peut rien.
//!
//! Le plafond par slot agit sur le second terme, et c'est lui qui compte : `f`
//! n'est plus la fraction des **entrées écrites**, gratuites, mais celle des
//! **slots dominés**. Bourrer un board de 24 identités pèse désormais autant que
//! d'en écrire 2 ; les 768 entrées publiables que mesurait l'audit en valent 64.
//!
//! Ce qui reste ouvert, et n'est pas traité ici : aucune diversité de `/24` IP
//! ni d'ASN n'est exigée, parce que l'information n'existe pas à ce stade. Un
//! board ne porte que des `EndpointId` de 32 octets et l'adresse n'est résolue
//! qu'au moment du dial, à l'intérieur d'iroh ([`Rendezvous`] n'en voit jamais
//! une seule). L'exiger imposerait une résolution DHT par candidat **avant** de
//! composer, soit un aller-retour réseau supplémentaire par entrée : c'est un
//! autre étage, pas une ligne à ajouter.
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
use rand::rngs::OsRng;
use rand::seq::SliceRandom;

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

/// **R9 (AUDIT-2026-08-13) — nombre maximal de candidats adoptés par slot.**
///
/// Il n'y en avait aucun. Un board porte jusqu'à [`MAX_IDS_PER_BOARD`] entrées et
/// n'importe qui peut l'écrire : un seul slot bourré fournissait donc 24
/// candidats, trois fois le budget de connexions d'un cycle, et le poids d'un
/// attaquant se mesurait en **entrées écrites** — gratuites — au lieu de slots
/// dominés.
///
/// 2 et pas 1 : deux nœuds qui partagent un slot (une collision de [`slot_of`]
/// devient probable dès ~7 nœuds sur [`SLOTS`] slots) doivent rester découvrables
/// dans le même cycle, sinon le perdant du tirage reste invisible plusieurs
/// cycles alors qu'il a bien publié. 2 et pas 4 : le vivier plafonne ainsi à
/// `SLOTS × 2 = 64`, huit fois [`MAX_DIALS_PER_CYCLE`] — assez de marge pour que
/// les slots honnêtes pèsent dans le tirage, trop peu pour qu'un board bourré le
/// sature à lui seul.
pub const MAX_ADOPTED_PER_SLOT: usize = 2;

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

/// **R9 (AUDIT-2026-08-13) — composer dans un ordre que personne ne peut miner.**
///
/// Compose la liste des candidats d'un cycle à partir des boards bruts : au plus
/// [`MAX_ADOPTED_PER_SLOT`] par slot, dédupliqués, nous-mêmes exclus, et dans un
/// ordre entièrement tiré de `rng`. Trois mélanges, chacun pour une raison
/// distincte :
///
/// - **l'ordre des slots**, parce qu'une identité présente dans plusieurs boards
///   est comptée dans le premier slot où on la rencontre : figé, cet ordre dit à
///   l'attaquant quels boards honnêtes sont visités avant les siens, donc où
///   planter ses identités pour y consommer un quota honnête ;
/// - **l'ordre à l'intérieur d'un board**, sinon l'attaquant choisit lesquelles
///   de ses entrées franchissent le plafond, et surtout il les place devant les
///   entrées honnêtes du même board ;
/// - **l'ordre final**, qui est celui dans lequel [`cycle`] compose et s'arrête
///   à [`MAX_DIALS_PER_CYCLE`].
///
/// `rng` est un paramètre pour qu'un test puisse rejouer un tirage ; en
/// production c'est `OsRng`, donc propre à ce nœud et renouvelé à chaque cycle.
/// Un ordre dérivé du topic, du slot ou de l'EndpointId local serait
/// reproductible par l'attaquant — c'est-à-dire minable, exactement le défaut
/// corrigé ici.
fn select_candidates<R: rand::Rng + ?Sized>(
    mut boards: Vec<Vec<RawEndpointId>>,
    self_id: &RawEndpointId,
    rng: &mut R,
) -> Vec<RawEndpointId> {
    boards.shuffle(rng);

    let mut seen: BTreeSet<RawEndpointId> = BTreeSet::new();
    let mut out: Vec<RawEndpointId> = Vec::with_capacity(boards.len() * MAX_ADOPTED_PER_SLOT);
    for board in boards.iter_mut() {
        board.shuffle(rng);
        let mut adopted = 0usize;
        for id in board.iter() {
            if adopted >= MAX_ADOPTED_PER_SLOT {
                break;
            }
            // Un doublon ne consomme pas le quota : sinon il suffirait de
            // republier les mêmes identités partout pour assécher les slots
            // honnêtes sans rien y ajouter.
            if *id != *self_id && seen.insert(*id) {
                out.push(*id);
                adopted += 1;
            }
        }
    }
    out.shuffle(rng);
    out
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
    ///
    /// **R9 (AUDIT-2026-08-13)** — la liste sortait triée par octets bruts, ce
    /// qui la rendait minable ; elle est désormais plafonnée par slot et tirée
    /// au sort, voir [`select_candidates`].
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

        let boards = join_all(reads).await;
        let read: usize = boards.iter().map(|board| board.len()).sum();
        // `OsRng` et pas une graine dérivée du topic : l'ordre doit être propre à
        // ce nœud et à ce cycle, sinon il redevient prédictible donc minable.
        let out = select_candidates(boards, self_id, &mut OsRng);
        log::info!(
            "◈ [RDV-1] {read} entrées lues sur {SLOTS} slots → {} candidats retenus (≤{MAX_ADOPTED_PER_SLOT}/slot, ordre tiré au sort)",
            out.len()
        );
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
    // **R9** — `discovered` arrive déjà plafonné par slot et mélangé : parcourir
    // cette liste dans l'ordre n'avantage plus aucune identité. Le tri d'avant
    // faisait de ce simple `for` le tirage d'un concours minable.
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

    /// Générateur reproductible. La production tire sur `OsRng` ; un test qui
    /// dépendrait de l'entropie réelle ne prouverait rien de stable.
    fn seeded(seed: u64) -> rand::rngs::StdRng {
        use rand::SeedableRng;
        rand::rngs::StdRng::seed_from_u64(seed)
    }

    /// Identité « minée » : préfixe nul, ce que l'attaquant obtient pour 2²⁴ à
    /// 2³² essais de clés ed25519 et qui la plaçait en tête de l'ordre trié.
    fn mined_id(n: u8) -> RawEndpointId {
        let mut out = [0u8; 32];
        out[31] = n;
        out
    }

    /// Identité honnête : premier octet haut, donc toujours classée **après**
    /// n'importe quelle identité minée dans un tri par octets bruts.
    fn honest_id<R: rand::Rng>(rng: &mut R) -> RawEndpointId {
        let mut out = [0u8; 32];
        rng.fill(&mut out[..]);
        out[0] |= 0x80;
        out
    }

    /// Board hostile réaliste : l'attaquant écrit **en tête** (il possède la clé
    /// du slot), les entrées honnêtes suivent, le tout tronqué au format de fil.
    fn hostile_board(attackers: &[RawEndpointId], honest: &[RawEndpointId]) -> Vec<RawEndpointId> {
        let mut board: Vec<RawEndpointId> = attackers.to_vec();
        board.extend_from_slice(honest);
        board.truncate(MAX_IDS_PER_BOARD);
        board
    }

    #[test]
    fn r9_les_identites_minees_ne_raflent_plus_les_premieres_connexions() {
        // Le scénario mesuré par l'audit (audit_net::n10) : 100 pairs honnêtes
        // et 8 identités à préfixe nul. L'attaquant détenant la clé de chaque
        // slot, il écrit ses 8 identités dans les 32 boards, pas dans un seul.
        let mut rng = seeded(0x5244_5631);
        let me = honest_id(&mut rng);
        let attackers: Vec<RawEndpointId> = (1..=8u8).map(mined_id).collect();
        let honest: Vec<RawEndpointId> = (0..100).map(|_| honest_id(&mut rng)).collect();

        let mut par_slot: Vec<Vec<RawEndpointId>> = vec![Vec::new(); SLOTS as usize];
        for peer in &honest {
            par_slot[slot_of(peer) as usize].push(*peer);
        }
        let boards: Vec<Vec<RawEndpointId>> =
            par_slot.iter().map(|honest| hostile_board(&attackers, honest)).collect();

        // Témoin : ce que faisait l'ancienne composition. Le `BTreeSet` de
        // `harvest` triait par octets bruts, `cycle` prenait les premiers.
        let trie: Vec<RawEndpointId> =
            boards.iter().flatten().copied().collect::<BTreeSet<_>>().into_iter().collect();
        assert!(
            trie.iter().take(MAX_DIALS_PER_CYCLE).all(|peer| attackers.contains(peer)),
            "témoin : l'ordre trié donnait bien les {MAX_DIALS_PER_CYCLE} connexions à l'attaquant"
        );

        // Le correctif : sur 200 cycles, aucun ne doit être entièrement composé
        // d'identités minées.
        let mut rafles = 0usize;
        let mut minees_en_tete = 0usize;
        for _ in 0..200 {
            let tete: Vec<RawEndpointId> = select_candidates(boards.clone(), &me, &mut rng)
                .into_iter()
                .take(MAX_DIALS_PER_CYCLE)
                .collect();
            assert_eq!(tete.len(), MAX_DIALS_PER_CYCLE, "le vivier doit rester au-dessus du budget");
            assert!(!tete.contains(&me), "un nœud ne se compose jamais lui-même");
            let minees = tete.iter().filter(|peer| attackers.contains(peer)).count();
            minees_en_tete += minees;
            if minees == MAX_DIALS_PER_CYCLE {
                rafles += 1;
            }
        }
        assert_eq!(rafles, 0, "le préfixe nul ne doit plus acheter aucune éclipse complète");
        // 8 identités minées dans un vivier de 64 : environ une par cycle. Un
        // ordre resté minable en donnerait 8 sur 8.
        assert!(
            minees_en_tete < 200 * 3,
            "part des identités minées anormalement haute : {minees_en_tete} sur {}",
            200 * MAX_DIALS_PER_CYCLE
        );
    }

    #[test]
    fn r9_un_slot_bourre_ne_livre_que_le_plafond() {
        // 24 entrées dans un seul board valaient 24 candidats, soit trois fois
        // le budget de connexions d'un cycle.
        let mut boards: Vec<Vec<RawEndpointId>> = vec![Vec::new(); SLOTS as usize];
        boards[3] = (1..=MAX_IDS_PER_BOARD as u8).map(id).collect();
        let out = select_candidates(boards, &id(200), &mut seeded(11));
        assert_eq!(
            out.len(),
            MAX_ADOPTED_PER_SLOT,
            "un board bourré de {MAX_IDS_PER_BOARD} entrées ne doit en livrer que {MAX_ADOPTED_PER_SLOT}"
        );

        // Les 32 slots bourrés d'identités toutes distinctes : le vivier
        // plafonne à 64, pas aux 768 entrées publiables mesurées par l'audit.
        let sature: Vec<Vec<RawEndpointId>> = (0..SLOTS as u16)
            .map(|slot| {
                (0..MAX_IDS_PER_BOARD as u16)
                    .map(|k| {
                        let mut out = [1u8; 32];
                        out[..2].copy_from_slice(&(slot * 1000 + k).to_be_bytes());
                        out
                    })
                    .collect()
            })
            .collect();
        let out = select_candidates(sature, &id(200), &mut seeded(12));
        assert_eq!(
            out.len(),
            SLOTS as usize * MAX_ADOPTED_PER_SLOT,
            "le vivier d'un cycle est borné par les slots, plus par les entrées écrites"
        );
    }

    #[test]
    fn r9_l_ordre_de_composition_n_est_ni_trie_ni_stable() {
        let mut rng = seeded(13);
        let me = honest_id(&mut rng);
        let boards: Vec<Vec<RawEndpointId>> = (0..SLOTS)
            .map(|_| (0..4).map(|_| honest_id(&mut rng)).collect())
            .collect();

        let premier = select_candidates(boards.clone(), &me, &mut rng);
        let second = select_candidates(boards.clone(), &me, &mut rng);

        let mut trie = premier.clone();
        trie.sort_unstable();
        assert_ne!(premier, trie, "l'ordre trié est précisément ce qui était minable");
        // Par nœud et par cycle : un ordre dérivé d'une graine publique serait
        // rejouable par l'attaquant, donc de nouveau minable.
        assert_ne!(premier, second, "deux cycles ne doivent pas composer dans le même ordre");
    }

    #[test]
    fn r9_une_identite_publiee_partout_ne_compte_quune_fois() {
        let me = id(250);
        let squatteur = mined_id(1);
        let out = select_candidates(vec![vec![squatteur]; 8], &me, &mut seeded(14));
        assert_eq!(out, vec![squatteur], "la même identité lue dans 8 slots reste un candidat");
    }

    #[test]
    fn r9_un_doublon_ne_consomme_pas_le_quota_dun_slot_honnete() {
        // Le squatteur est dans les 8 boards ; chaque board porte en plus deux
        // identités honnêtes distinctes. S'il consommait un quota à chaque
        // passage, le vivier tomberait sous les 8 × MAX_ADOPTED_PER_SLOT places.
        let me = id(250);
        let squatteur = mined_id(1);
        let boards: Vec<Vec<RawEndpointId>> =
            (1..=8u8).map(|k| vec![squatteur, id(k), id(k + 100)]).collect();
        let out = select_candidates(boards, &me, &mut seeded(15));
        assert_eq!(
            out.len(),
            8 * MAX_ADOPTED_PER_SLOT,
            "un doublon déjà vu doit être ignoré sans dépenser de place"
        );
    }

    #[test]
    fn r9_le_taux_declipse_suit_le_tirage_sans_remise() {
        // Vérifie le chiffre annoncé dans l'en-tête du module : à f = 1/2 (16
        // slots entièrement à l'attaquant, 16 entièrement honnêtes), la
        // probabilité que les 8 connexions soient toutes à lui vaut
        // Π (32−i)/(64−i) ≈ 0,238 %, et non 100 % comme avec l'ordre trié.
        let mut rng = seeded(16);
        let me = honest_id(&mut rng);
        let mut boards: Vec<Vec<RawEndpointId>> = Vec::with_capacity(SLOTS as usize);
        let mut mines: BTreeSet<RawEndpointId> = BTreeSet::new();
        // Les slots de l'attaquant sont groupés en tête de la liste : un ordre
        // qui suivrait l'index de slot, comme un ordre trié, donnerait alors les
        // 8 connexions à l'attaquant — le test doit voir les deux défauts.
        for slot in 0..SLOTS {
            if slot < SLOTS / 2 {
                let board: Vec<RawEndpointId> = (0..4)
                    .map(|k| {
                        let mut out = [0u8; 32];
                        out[30..].copy_from_slice(&(u16::from(slot) * 10 + k + 1).to_be_bytes());
                        out
                    })
                    .collect();
                mines.extend(board.iter().copied());
                boards.push(board);
            } else {
                boards.push((0..4).map(|_| honest_id(&mut rng)).collect());
            }
        }

        let essais = 2000usize;
        let mut rafles = 0usize;
        for _ in 0..essais {
            let tete: Vec<RawEndpointId> = select_candidates(boards.clone(), &me, &mut rng)
                .into_iter()
                .take(MAX_DIALS_PER_CYCLE)
                .collect();
            if tete.iter().all(|peer| mines.contains(peer)) {
                rafles += 1;
            }
        }
        // Seuil à 1 % : quarante fois la valeur théorique, quatre-vingt-dix-neuf
        // fois moins que l'éclipse certaine d'avant le correctif.
        assert!(rafles * 100 < essais, "taux d'éclipse {rafles}/{essais}, attendu ~0,24 %");
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
