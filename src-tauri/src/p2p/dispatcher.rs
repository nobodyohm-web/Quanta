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

/// **R8 (AUDIT-2026-08-13)** — seuil bas de l'éviction par cardinalité. Une passe
/// d'éviction descend jusqu'ici au lieu de retirer une seule entrée, ce qui amortit
/// le balayage O(n) sur les 5 000 messages suivants. La valeur exacte est une
/// politique ; ce qui compte est qu'elle soit strictement sous
/// [`MAX_TRACKED_SENDERS`].
const LOW_WATER_TRACKED_SENDERS: usize = MAX_TRACKED_SENDERS - 5_000;

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

/// **R10 (AUDIT-2026-08-13)** — durée de vie d'une cible signalée qui n'a pas
/// atteint le quorum. Alignée sur [`REPORT_BAN_TTL_SECS`] : une accusation ne doit
/// pas survivre plus longtemps que la sanction qu'elle vise.
const REPORT_ENTRY_TTL_SECS: u64 = REPORT_BAN_TTL_SECS;

/// H6 (AUDIT-2026-07-25) — hard cap on the *reporters* stored per target.
/// [`REPORT_BAN_THRESHOLD`] is 3; keeping a small margin above it preserves the
/// evidence a ban rests on while making the set impossible to inflate.
const MAX_REPORTERS_PER_TARGET: u32 = 8;

/// H6 — the key under which a peer is tracked in the `NonceTracker` maps.
///
/// Every per-peer map used to be keyed by the raw sender string. That was sized
/// for Ed25519 (64 hex chars) — but since PQ-ENVELOPE-1 the sender is an
/// **ML-DSA-65 public key**, i.e. 1952 bytes rendered as 3904 hex characters.
/// With `MAX_TRACKED_SENDERS = 100_000` that is roughly 390 MB of keys alone,
/// reachable by anyone willing to mint keypairs (microseconds each) and send one
/// signed envelope per key, which never trips the per-sender rate limit.
///
/// Hashing to a 32-byte BLAKE3 digest (64 hex chars) cuts that ~61× with no
/// security trade-off: the maps only ever test key equality, and a full 256-bit
/// digest leaves no collision margin worth attacking. Normalisation happens
/// *inside* each public method, so callers keep passing the real public key and
/// the maps cannot disagree about what a peer is called.
fn peer_key(public_key_hex: &str) -> String {
    hex::encode(blake3::hash(public_key_hex.as_bytes()).as_bytes())
}
/// SEC-COUNTRY-1: absolute cap on distinct country codes tracked for the energy
/// oracle. The code is peer-supplied via `Hello`; sanitised to an ISO-shaped
/// short token and bounded here so a peer can't grow the map with novel codes.
const MAX_COUNTRY_CODES: usize = 64;

/// Taille maximale d'une enveloppe gossip brute avant parsing.
///
/// **R3 (AUDIT-2026-08-13)** — c'était 10 Mo, soit 2 560 fois le défaut d'iroh
/// (4 Ko), et cette constante fixe AUSSI `Gossip::max_message_size` côté
/// transport. Or plumtree **relaie et met en cache 30 s AVANT authentification** :
/// un pair non authentifié pouvait donc faire stocker 10 Mo par message à tous
/// ses voisins, avec un cache non borné en nombre d'entrées — OOM distant.
///
/// La nouvelle valeur n'est pas choisie au doigt mouillé, elle est **dérivée** du
/// plus gros message légal du protocole :
/// - une transaction signée pèse ~11 Ko sur le fil (signature ML-DSA-65 de
///   3 309 o + clé publique de 1 952 o, en hexadécimal dans du JSON) ;
/// - un bloc en porte au plus [`crate::p2p::ledger::MAX_TXS_PER_BLOCK`] (256),
///   soit ~2,8 Mo ;
/// - l'enveloppe elle-même ajoute ~10,5 Ko d'authentification.
///
/// 4 Mio couvre donc le pire bloc légal avec de la marge, et rien au-delà.
///
/// Le `ChainSegment` (jusqu'à 50 blocs) ne tient évidemment pas dans cette borne :
/// c'est voulu. Le producteur s'arrête désormais à
/// [`CHAIN_SEGMENT_BYTE_BUDGET`] et la synchronisation prend un tour de plus,
/// au lieu que le transport porte une limite dimensionnée sur le pire cas.
///
/// **Ce qui reste ouvert** : le relais et le cache pré-authentification sont un
/// comportement d'`iroh-gossip`, pas du code d'ici. Réduire la borne divise le
/// coût par 2,5 ; elle ne le supprime pas.
pub const MAX_RAW_ENVELOPE_BYTES: usize = 4 * 1024 * 1024;

/// **R13** — nombre maximal d'identifiants de pairs considérés dans un `Hello`.
/// La découverte n'en retient de toute façon que 3 par message ; ce plafond
/// existe pour que le coût de traitement d'un `Hello` hostile soit borné AVANT
/// toute allocation, pas après.
pub const MAX_HELLO_PEER_IDS: usize = 32;

/// **R13** — longueur maximale d'un identifiant/ticket de pair. Un ticket iroh
/// sérialisé fait quelques centaines d'octets ; 4 Kio laisse une marge confortable
/// et coupe court aux chaînes mégaoctet.
pub const MAX_PEER_TICKET_LEN: usize = 4096;

/// **R13** — nombre maximal de têtes de chaîne annoncées dans un `Hello`.
/// Purement informatif côté réception ; borné pour la même raison.
pub const MAX_HELLO_HEADS: usize = 64;

/// R3 — budget d'octets d'un `ChainSegment` produit. Le producteur cesse
/// d'ajouter des blocs dès qu'il est atteint, même s'il n'a pas servi les
/// [`MAX_CHAIN_SEGMENT_RECEIVED`] blocs demandés. Choisi sous
/// [`MAX_RAW_ENVELOPE_BYTES`] avec la place pour l'enveloppe : un bloc légal
/// entre toujours, donc la synchronisation progresse quoi qu'il arrive.
pub const CHAIN_SEGMENT_BYTE_BUDGET: usize = 3 * 1024 * 1024;

/// Maximum number of blocks we'll process from a received `ChainSegment`.
/// Defends against a peer flooding us with a huge segment in one message.
pub const MAX_CHAIN_SEGMENT_RECEIVED: usize = 50;

/// **R6 (AUDIT-2026-08-13)** — longueur hexadécimale exacte d'un `sender` : c'est
/// une clé publique ML-DSA-65, jamais autre chose (PQ-ENVELOPE-1). La constante est
/// **dérivée** de `fips204`, pas recopiée, pour qu'un changement de paramétrage ne
/// puisse pas la laisser mentir.
const SENDER_HEX_LEN: usize = fips204::ml_dsa_65::PK_LEN * 2;

/// R6 — longueur hexadécimale exacte d'une signature ML-DSA-65.
const SIGNATURE_HEX_LEN: usize = fips204::ml_dsa_65::SIG_LEN * 2;

/// R6 — longueur d'un identifiant d'enveloppe : BLAKE3 en hexadécimal.
const ENVELOPE_ID_HEX_LEN: usize = 64;

/// R6 — longueur maximale d'un horodatage RFC3339. Un horodatage légal fait ~35
/// caractères ; 64 laisse la place aux fractions de seconde et aux décalages.
const MAX_TIMESTAMP_LEN: usize = 64;

/// **R6 / tâche 4 — rien ne doit être décodé sur la foi d'une longueur choisie par
/// l'attaquant.**
///
/// Avant authentification, les quatre champs de tête d'une enveloppe sont des
/// `String` libres, bornées par la seule taille de l'enveloppe (4 Mio). Les faire
/// passer aux couches suivantes coûtait, par message et sans qu'aucune clé ne soit
/// possédée : un `hex::decode` de la signature (jusqu'à 2 Mio alloués), une
/// comparaison d'identifiant sur 4 Mio, un parsing RFC3339 sur 4 Mio.
///
/// Or ces quatre longueurs sont **fixes** dans le protocole. Les vérifier est O(1),
/// n'alloue rien, et ne relâche aucune vérification : une enveloppe qui échoue ici
/// aurait de toute façon échoué à la signature (`verify_ml_dsa` refuse toute clé ou
/// signature de mauvaise taille), simplement après avoir payé le décodage.
fn envelope_shape_is_plausible(env: &GossipEnvelope) -> bool {
    env.sender.len() == SENDER_HEX_LEN
        && env.signature.len() == SIGNATURE_HEX_LEN
        && env.id.len() == ENVELOPE_ID_HEX_LEN
        && env.timestamp.len() <= MAX_TIMESTAMP_LEN
}

/// **R5 (AUDIT-2026-08-13)** — nombre de pairs élus pour répondre à une même
/// requête de chaîne. Deux, pas un : un seul répondant fait dépendre la
/// synchronisation d'un pair qui peut être mort entre-temps, et le demandeur
/// relance de lui-même à chaque `Hello` (donc avec un nonce neuf, donc une élection
/// neuve).
pub const CHAIN_ANSWER_RESPONDERS: usize = 2;

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
    /// **R10 (AUDIT-2026-08-13)** — date de création de chaque entrée de
    /// `report_counts`, pour les faire expirer par le temps plutôt que par la
    /// pression de cardinalité (voir [`Self::prune_reports_and_bans`]).
    report_first_seen: HashMap<String, u64>,
}

/// **R15 (AUDIT-2026-08-13)** — état du [`NonceTracker`] qui doit survivre à un
/// redémarrage : l'anti-rejeu et les bannissements en cours. Voir
/// [`NonceTracker::snapshot`] pour ce qui est délibérément laissé volatile.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct NonceTrackerSnapshot {
    /// Nonce le plus haut accepté, par clé publique d'expéditeur.
    #[serde(default)]
    pub last_nonces: HashMap<String, u64>,
    /// `peer_id → seconde epoch d'expiration`, bans encore actifs seulement.
    #[serde(default)]
    pub bans: HashMap<String, u64>,
}

/// **R16 (AUDIT-2026-08-13)** — verdict de bannissement observé sans mutation, pour
/// que le chemin non authentifié n'ait jamais besoin d'un verrou d'écriture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BanState {
    /// Aucun ban enregistré pour ce pair.
    None,
    /// Ban en cours : le message doit être jeté.
    Active,
    /// Ban enregistré mais périmé : le pair repart avec une ardoise nette, et
    /// l'entrée sera évincée par [`NonceTracker::clear_expired_ban`].
    Expired,
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
            report_first_seen: HashMap::new(),
        }
    }

    /// **R15 (AUDIT-2026-08-13) — un anti-rejeu qu'un redémarrage oublie ne
    /// protège de rien.**
    ///
    /// Le `NonceTracker` porte deux choses qui n'ont aucune raison d'être
    /// volatiles : le **nonce le plus haut vu par expéditeur** (l'anti-rejeu) et
    /// les **bannissements en cours**. À chaque redémarrage, les deux repartaient
    /// de zéro. Conséquences symétriques et toutes deux mauvaises : une enveloppe
    /// authentique capturée avant l'arrêt redevenait acceptable après (le rejeu se
    /// rouvre exactement le temps qu'il faut à un nœud pour redémarrer, ce qu'un
    /// attaquant peut provoquer), et un pair banni pour abus repartait avec une
    /// ardoise nette sans avoir rien changé à son comportement.
    ///
    /// Ce qui est persisté, et ce qui ne l'est **pas** : les nonces et les bans,
    /// oui. Les compteurs de débit (`rate_counters`) et les signalements
    /// (`report_counts`) non — ils sont fenêtrés sur le temps courant, donc les
    /// restaurer d'une session précédente fausserait la fenêtre plutôt que de la
    /// prolonger. Un ban déjà expiré n'est pas réécrit : le restaurer serait une
    /// double peine.
    pub fn snapshot(&self) -> NonceTrackerSnapshot {
        let now = now_epoch_secs();
        NonceTrackerSnapshot {
            last_nonces: self.last_nonces.clone(),
            bans: self
                .bans
                .iter()
                .filter(|(_, &until)| until > now)
                .map(|(k, &v)| (k.clone(), v))
                .collect(),
        }
    }

    /// R15 — restaure l'anti-rejeu et les bannissements encore en cours.
    ///
    /// La restauration est **monotone** sur les nonces (`max`) : si l'état vivant
    /// a déjà vu plus haut, on ne redescend pas — redescendre rouvrirait
    /// précisément le rejeu que le champ existe pour fermer.
    pub fn restore(&mut self, snap: NonceTrackerSnapshot) -> (usize, usize) {
        let now = now_epoch_secs();
        for (pk, nonce) in snap.last_nonces {
            let e = self.last_nonces.entry(pk).or_insert(0);
            *e = (*e).max(nonce);
        }
        let mut bans = 0usize;
        for (pk, until) in snap.bans {
            if until > now {
                self.bans.insert(pk, until);
                bans += 1;
            }
        }
        (self.last_nonces.len(), bans)
    }

    /// Returns `true` if this nonce is valid (strictly greater than the last
    /// seen). Also updates the tracker on acceptance.
    pub fn check_and_advance(&mut self, sender_pk: &str, nonce: u64) -> bool {
        let sender_pk = &peer_key(sender_pk);
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
        //
        // **R8 (AUDIT-2026-08-13) — l'éviction était O(n) PAR MESSAGE.**
        //
        // La boucle cherchait le minimum sur toute la carte, puis retirait **une**
        // entrée : une fois le plafond atteint (~16 s d'inondation à 6 000 clés/s,
        // chiffre de l'audit), chaque message excédentaire relançait un balayage de
        // 100 000 entrées. Le plafond mémoire tenait ; il achetait une bombe CPU en
        // échange, sur la tâche qui porte aussi le dispatch.
        //
        // L'éviction descend maintenant à un **seuil bas** en une seule passe : un
        // balayage O(n) amorti sur `MAX_TRACKED_SENDERS - LOW_WATER_TRACKED_SENDERS`
        // messages, soit O(1) par message sur le chemin chaud. La politique
        // d'éviction est inchangée (les plus anciennement vus d'abord).
        if self.last_seen.len() > MAX_TRACKED_SENDERS {
            let to_remove = self.last_seen.len() - LOW_WATER_TRACKED_SENDERS;
            let mut aged: Vec<(u64, String)> = self
                .last_seen
                .iter()
                .map(|(k, &seen)| (seen, k.clone()))
                .collect();
            // `select_nth_unstable` partitionne autour du n-ième plus ancien en O(n)
            // sans trier le reste. On partitionne — on ne compare pas à une valeur
            // seuil : sous une rafale Sybil toutes les entrées portent la même
            // seconde, et un seuil aurait alors vidé la carte entière.
            let (older, pivot, _) = aged.select_nth_unstable(to_remove - 1);
            let doomed: Vec<String> = older
                .iter()
                .map(|(_, k)| k.clone())
                .chain(std::iter::once(pivot.1.clone()))
                .collect();
            for k in &doomed {
                self.last_seen.remove(k);
                self.last_nonces.remove(k);
                self.rate_counters.remove(k);
            }
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
        let sender_pk = &peer_key(sender_pk);
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
        let peer_id = &peer_key(peer_id);
        let reporter_pk = &peer_key(reporter_pk);
        if peer_id == reporter_pk {
            // A peer cannot report itself into (or pad) a ban.
            return self
                .report_counts
                .get(peer_id)
                .map(|s| s.len() as u32)
                .unwrap_or(0);
        }
        // R10 — date de première apparition de la cible, pour l'expiration par
        // TTL et pour évincer les plus ANCIENNES plutôt que les plus faibles.
        self.report_first_seen
            .entry(peer_id.to_string())
            .or_insert_with(now_epoch_secs);
        let reporters = self
            .report_counts
            .entry(peer_id.to_string())
            .or_default();
        // H6: a ban needs REPORT_BAN_THRESHOLD distinct reporters, so storing more
        // than a small margin buys nothing — and `prune_reports_and_bans` only ever
        // bounded the number of TARGETS, never the reporters of one target. Without
        // this cap, an attacker minting fresh ML-DSA keypairs (microseconds each,
        // one message per key so the per-sender rate limit never fires) grows a
        // single set without bound.
        if (reporters.len() as u32) < MAX_REPORTERS_PER_TARGET {
            reporters.insert(reporter_pk.to_string());
        }
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
    /// cap is exceeded.
    ///
    /// **R10 (AUDIT-2026-08-13) — la borne tenait, sa conséquence était un
    /// fail-open.**
    ///
    /// L'éviction retirait la cible la plus **faible** (le moins de rapporteurs).
    /// Or une cible fraîche a exactement un rapporteur : c'était donc toujours elle
    /// la victime. Une fois les 10 000 places occupées, plus **aucune** nouvelle
    /// cible ne pouvait atteindre le seuil — elle était évincée avant son 3ᵉ
    /// rapporteur. Le bannissement, seule réponse du protocole à un pair
    /// malveillant, devenait inopérant **pour tout le réseau**, et l'attaquant
    /// pouvait s'en servir juste après avoir fait bannir ses cibles.
    ///
    /// Deux changements :
    /// - les entrées sous-seuil expirent par le **temps**
    ///   ([`REPORT_ENTRY_TTL_SECS`]) : une saturation ne se maintient qu'en la
    ///   réalimentant en permanence ;
    /// - l'éviction porte sur les plus **anciennes** sous-seuil, pas sur les plus
    ///   faibles. Une accusation légitime (trois rapports en quelques secondes)
    ///   déloge donc le squat au lieu d'être délogée par lui.
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
            self.report_first_seen.remove(&k);
        }
        // ② R10 — expiration par TTL des cibles sous-seuil : une accusation qui
        // n'a pas réuni son quorum en une heure n'est plus une accusation.
        let stale: Vec<String> = self
            .report_counts
            .iter()
            .filter(|(k, r)| {
                (r.len() as u32) < REPORT_BAN_THRESHOLD
                    && now.saturating_sub(*self.report_first_seen.get(*k).unwrap_or(&now))
                        >= REPORT_ENTRY_TTL_SECS
            })
            .map(|(k, _)| k.clone())
            .collect();
        for k in stale {
            self.report_counts.remove(&k);
            self.report_first_seen.remove(&k);
        }
        // ③ Still over cap → evict the OLDEST sub-threshold targets. Never evict a
        // target that has reached the ban threshold (that would erase evidence).
        while self.report_counts.len() > MAX_TRACKED_REPORTS {
            let victim = self
                .report_counts
                .iter()
                .filter(|(_, r)| (r.len() as u32) < REPORT_BAN_THRESHOLD)
                .min_by_key(|(k, _)| self.report_first_seen.get(*k).copied().unwrap_or(0))
                .map(|(k, _)| k.clone());
            match victim {
                Some(k) => {
                    self.report_counts.remove(&k);
                    self.report_first_seen.remove(&k);
                }
                None => break, // everything left is at threshold — stop
            }
        }
    }

    /// **R16 (AUDIT-2026-08-13)** — l'état de bannissement, observé **sans muter**.
    ///
    /// [`Self::is_banned`] prend un `&mut self` parce qu'il évince paresseusement les
    /// bans expirés. Le dispatcher l'appelait pour **chaque** message entrant, y
    /// compris non authentifié : cela imposait un verrou en **écriture** sur le
    /// tracker global — partagé avec le limiteur de débit et le contrôle de nonce —
    /// sur un chemin qu'un attaquant sans identité sollicite à volonté. C'était un
    /// point de sérialisation global offert gratuitement.
    ///
    /// La sonde est désormais séparée de l'éviction : le chemin chaud prend un
    /// verrou de **lecture**, et seul le cas rare (« ce ban a expiré ») paie une
    /// écriture, une fois, après authentification.
    pub fn ban_state(&self, peer_id: &str) -> BanState {
        let peer_id = &peer_key(peer_id);
        let Some(&expires_at) = self.bans.get(peer_id) else {
            return BanState::None;
        };
        if now_epoch_secs() < expires_at {
            BanState::Active
        } else {
            BanState::Expired
        }
    }

    /// R16 — éviction paresseuse d'un ban expiré (ardoise nette pour le pair).
    /// Appelé uniquement quand [`Self::ban_state`] a répondu [`BanState::Expired`].
    pub fn clear_expired_ban(&mut self, peer_id: &str) {
        let peer_id = &peer_key(peer_id);
        if let Some(&expires_at) = self.bans.get(peer_id) {
            if now_epoch_secs() >= expires_at {
                self.bans.remove(peer_id);
                self.report_counts.remove(peer_id);
                self.report_first_seen.remove(peer_id);
            }
        }
    }

    /// Returns `true` if `peer_id` is currently banned. Auto-evicts expired
    /// entries (and resets their report count) so a peer can rejoin after TTL.
    pub fn is_banned(&mut self, peer_id: &str) -> bool {
        match self.ban_state(peer_id) {
            BanState::Active => true,
            BanState::Expired => {
                self.clear_expired_ban(peer_id);
                false
            }
            BanState::None => false,
        }
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
    // STRUCT-1: Reconstruct the FULL canonical signable bytes
    let full_signable =
        GossipRouter::signable_envelope_bytes(&env.sender, env.nonce, &env.timestamp, &env.payload);
    verify_envelope_signature_with(env, &full_signable)
}

/// **R6 (AUDIT-2026-08-13)** — même vérification, sur une pré-image **déjà
/// calculée**.
///
/// La pré-image canonique est une re-sérialisation JSON complète du payload : sur
/// le chemin d'entrée, elle était produite deux fois par enveloppe (une fois pour
/// l'identifiant canonique, une fois pour la signature), donc deux fois le coût
/// dominant du pipeline. Elle est désormais produite une seule fois et sert aux
/// deux — la signature ici, le digest BLAKE3 pour l'identifiant.
fn verify_envelope_signature_with(env: &GossipEnvelope, signable: &[u8]) -> Result<(), String> {
    let sig_bytes = hex::decode(&env.signature).map_err(|_| "invalid signature hex")?;
    // PQ-ENVELOPE-1: Verify ML-DSA-65 signature against full canonical bytes.
    if CryptoEngine::verify_pq(&env.sender, signable, &sig_bytes) {
        Ok(())
    } else {
        Err("signature verification failed".into())
    }
}

// ─── Public entry point ─────────────────────────────────────────────────────

/// Désérialise + vérifie signature + vérifie freshness + dispatche une
/// enveloppe entrante.
///
/// **Ordre du pipeline** — il est la propriété de sécurité, pas un détail
/// d'implémentation. Cette liste décrivait encore l'ordre d'une version antérieure ;
/// l'audit du 13/08/2026 a relevé qu'« un commentaire de sécurité faux est pire
/// qu'absent », et R6 était précisément ce décalage. Elle suit désormais le code :
///
/// 1. taille brute ([`MAX_RAW_ENVELOPE_BYTES`])
/// 2. décodage JSON structurel
/// 3. forme des champs de tête ([`envelope_shape_is_plausible`], R6)
/// 4. bannissement — sonde en **lecture** ([`NonceTracker::ban_state`], R16)
/// 5. **signature ML-DSA-65** sur la pré-image canonique ← porte d'authentification
/// 6. éviction d'un ban expiré (cas rare, seul verrou d'écriture)
/// 7. identifiant canonique (BLAKE3 de la pré-image déjà calculée, H1)
/// 8. fraîcheur ±90 s
/// 9. dédup en lecture puis statistiques globales
/// 10. comptabilité par pair (R11)
/// 11. insertion dans la LRU de dédup
/// 12. limitation de débit adaptative, puis contrôle de nonce
/// 13. dispatch du payload
///
/// Rien entre 1 et 5 n'alloue ni ne mute quoi que ce soit à une taille choisie par
/// l'émetteur : c'est la règle que R6, R11 et R16 imposent ensemble.
pub async fn dispatch_incoming(state: &Arc<AppState>, raw: &[u8]) {
    // Chronomètre télémétrie : durée réelle du pipeline ①-⑬ — µs mesurés, hors
    // chemin de sécurité.
    let pipeline_t = std::time::Instant::now();

    // **R6 (AUDIT-2026-08-13) — le nœud travaillait avant de vérifier.**
    //
    // L'ordre était : taille, JSON, ban (verrou d'ÉCRITURE), identifiant canonique
    // (re-sérialisation complète du payload + BLAKE3), dedup, statistiques
    // globales, comptabilité par pair, fraîcheur, **puis** signature. Mesure de
    // l'audit : 8 Mo non authentifiés coûtaient 16 ms de CPU à la victime avant
    // qu'une seule vérification de signature n'ait lieu, et hors limiteur de débit
    // (qui, lui, est après la porte de signature). La boucle de dispatch étant
    // unique et séquentielle, ce travail-là est exactement ce qui la monopolise.
    //
    // Le seul ordre défendable est : taille → forme → signature → tout le reste.
    // Trois choses en découlent, chacune fermant un constat distinct :
    //   • R6  — la re-sérialisation canonique (le poste coûteux) passe APRÈS
    //           l'authentification, et n'est plus payée qu'une fois : la pré-image
    //           signée sert à la fois de matière à vérifier et de source de l'id.
    //   • R11 — la comptabilité par pair (`bytes_in`, `messages_in`) n'est plus
    //           écrite sur la foi d'un `sender` usurpable : elle était falsifiable
    //           à distance, donc les métriques sur lesquelles un opérateur
    //           s'appuierait PENDANT une attaque mentaient.
    //   • R16 — la sonde de bannissement passe en verrou de LECTURE.
    //
    // Ce que ça coûte : une enveloppe périmée mais correctement signée paie
    // désormais une vérification ML-DSA (~160 µs). Ce n'est pas un chemin
    // d'attaque — un attaquant met toujours un horodatage frais — et le trafic
    // périmé honnête est marginal.

    // ── ① DoS guard: reject oversized envelopes BEFORE parsing ──
    // La borne est dérivée du plus gros message légal (voir MAX_RAW_ENVELOPE_BYTES).
    if raw.len() > MAX_RAW_ENVELOPE_BYTES {
        log::warn!(
            "◈ [Dispatch] ⚠ oversized payload {} B (> {} B) → drop",
            raw.len(),
            MAX_RAW_ENVELOPE_BYTES
        );
        return;
    }

    // ── ② Décodage structurel ──
    let env: GossipEnvelope = match serde_json::from_slice(raw) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("◈ [Dispatch] envelope JSON invalide: {}", e);
            return;
        }
    };

    // ── ③ R6 / tâche 4 : la forme, avant tout décodage piloté par l'attaquant ──
    // O(1), aucune allocation. Voir `envelope_shape_is_plausible`.
    if !envelope_shape_is_plausible(&env) {
        let (should_warn, total) = {
            let mut gossip = state.node.gossip.write().await;
            gossip.stats.dropped_malformed += 1;
            let warn = gossip.note_bad_signature_drop();
            (warn, gossip.stats.dropped_malformed)
        };
        if should_warn {
            log::warn!(
                "◈ [Dispatch] ⚠ enveloppe malformée (longueurs sender/signature/id/horodatage                  hors protocole) → drop ({} au total)",
                total
            );
        }
        return;
    }

    // ── ④ R16 : bannissement — sonde en LECTURE seule ──
    // Elle reste avant la signature parce qu'elle est le filtre qui coûte le moins
    // cher (une lecture de table) et qu'elle sert précisément à ne PAS payer la
    // vérification pour un pair déjà jugé. Elle ne mute plus rien : un `sender`
    // usurpé ne peut donc plus provoquer d'écriture sur le tracker global.
    let ban_state = state.node.nonce_tracker.read().await.ban_state(&env.sender);
    if ban_state == BanState::Active {
        log::debug!(
            "◈ [Dispatch] banned peer {} → drop",
            short(&env.sender, 12)
        );
        return;
    }

    // ── ⑤ R6 : pré-image canonique calculée UNE FOIS, puis signature ──
    //
    // R1 (AUDIT-2026-08-13) — REPORT-NOAUTH-1: on a signature failure we drop and
    // count, and we denounce NOBODY.
    //
    // This path used to broadcast `ReportPeer{peer_id: env.sender}`. `env.sender`
    // is an attacker-chosen field of an envelope whose signature was JUST ruled
    // invalid — i.e. the one piece of data we have formally declared untrustworthy.
    // Sending the same forgery to `REPORT_BAN_THRESHOLD` honest nodes made each of
    // them denounce the victim independently, banning any node on the network for
    // `REPORT_BAN_TTL_SECS` with zero keys and zero stake (proven end-to-end).
    // It also turned one unauthenticated 10 kB datagram into a full ML-DSA-65
    // signature plus a fan-out to every peer, *before* the rate limiter.
    //
    // The rule is absolute: a report may only ever name a peer whose identity was
    // established by a signature we verified ourselves. A failed signature yields
    // no identity, therefore no report.
    let signable =
        GossipRouter::signable_envelope_bytes(&env.sender, env.nonce, &env.timestamp, &env.payload);
    if let Err(reason) = verify_envelope_signature_with(&env, &signable) {
        let (should_warn, total) = {
            let mut gossip = state.node.gossip.write().await;
            gossip.stats.dropped_signature += 1;
            let warn = gossip.note_bad_signature_drop();
            (warn, gossip.stats.dropped_signature)
        };
        if should_warn {
            log::warn!(
                "◈ [Dispatch] ⚠ SIGNATURE INVALIDE (prétendu expéditeur {}) — {} → drop \
                 silencieux ({} au total). Aucun signalement émis : l'expéditeur d'une \
                 enveloppe non authentifiée n'est pas une identité.",
                short(&env.sender, 12),
                reason,
                total
            );
        }
        return;
    }

    // ── ⑥ R16 : le pair est authentifié et son ban a expiré → ardoise nette.
    // Seul ce cas rare paie un verrou d'écriture.
    if ban_state == BanState::Expired {
        state
            .node
            .nonce_tracker
            .write()
            .await
            .clear_expired_ban(&env.sender);
    }

    // ── ⑦ Identifiant canonique ──
    // H1 (AUDIT-2026-07-25): the id must be the canonical digest of the SIGNED
    // pre-image. It used to be a free-form String taken straight off the wire and
    // nothing recomputed it, so an unauthenticated peer could choose arbitrary
    // dedup keys — e.g. precompute the ids of the RequestChain messages every node
    // emits and poison them in advance, censoring chain sync for free. Rejected
    // here, before the LRU is touched. R6 : le digest porte sur la pré-image déjà
    // en main, donc ce contrôle ne coûte plus qu'un BLAKE3.
    if env.id != crate::p2p::gossip::envelope_id_of_signable(&signable) {
        log::warn!(
            "◈ [Dispatch] ⚠ identifiant d'enveloppe non canonique from {} → drop",
            short(&env.sender, 12)
        );
        return;
    }

    // ── ⑧ Fenêtre temporelle ±90s ──
    //
    // V2 (audit de vie) — this used to be a bare `log::debug!` with no counter,
    // so a node whose clock had drifted rejected 100 % of inbound gossip while
    // reporting itself online, and nothing anywhere said so. There is no NTP in
    // Quanta: the clock is an unchecked assumption, which makes this the single
    // most likely cause of "my two machines will not talk". Count it and say it
    // out loud, with the measured drift so the diagnosis is immediate.
    //
    // R11 : le contrôle est passé APRÈS la signature. Le compteur reste le même
    // signal de dérive d'horloge (une horloge décalée rejette le trafic de TOUS
    // les pairs, qui sont authentifiés), mais il n'est plus gonflable par un
    // inconnu — ce qui empoisonnait le diagnostic qu'il existe pour porter.
    if !GossipRouter::is_fresh(&env.timestamp) {
        let drift_secs = chrono::DateTime::parse_from_rfc3339(&env.timestamp)
            .map(|ts| chrono::Utc::now().timestamp() - ts.timestamp())
            .unwrap_or(0);
        let (should_warn, total) = {
            let mut gossip = state.node.gossip.write().await;
            let warn = gossip.note_stale_drop();
            (warn, gossip.stats.dropped_stale)
        };
        if should_warn {
            log::warn!(
                "◈ [Dispatch] ⚠ enveloppe hors fenêtre de fraîcheur (±90 s) de {} — décalage \
                 {} s ({} droppée(s) au total). Si ce compteur monte alors que rien n'est reçu, \
                 l'horloge d'une des deux machines est désynchronisée.",
                short(&env.sender, 12),
                drift_secs,
                total
            );
        }
        return;
    }

    // ── ⑨ Anti-replay : si on a déjà vu cet ID, ignorer ──
    //
    // H1: this is a READ-ONLY probe. The insertion moved below, because inserting
    // here let an unauthenticated peer fill the 100 K LRU with garbage and evict
    // real message ids.
    {
        let g = state.node.gossip.read().await;
        if g.has_seen(&env.id) {
            return;
        }
    }
    {
        let mut g = state.node.gossip.write().await;
        g.stats.messages_received += 1;
        g.stats.bytes_received += raw.len() as u64;
    }

    // ── ⑩ NET-9: Per-peer bandwidth/message accounting ──
    //
    // **R11 (AUDIT-2026-08-13)** — c'était écrit AVANT la signature, « pour que
    // même les pairs dont la signature échoue comptent un mauvais message ». Le
    // raisonnement était juste et la donnée fausse : `env.sender` est choisi par
    // l'attaquant, donc ce n'était pas le pair bruyant qui était comptabilisé mais
    // **celui que l'attaquant désignait**. 410 Ko imputés à un pair honnête par une
    // seule enveloppe forgée, dans l'audit. Le comptage n'a de sens que sur une
    // identité prouvée ; il est donc ici, après la porte de signature.
    {
        let mut info = state.node.peer_info.write().await;
        if let Some(entry) = info.get_mut(&env.sender) {
            entry.bytes_in = entry.bytes_in.saturating_add(raw.len() as u64);
            entry.messages_in = entry.messages_in.saturating_add(1);
        }
    }

    // ── ⑪ H1 (AUDIT-2026-07-25): NOW the id may enter the dedup LRU — the envelope is
    // authenticated, so only a real signer can consume a slot. A `false` here means
    // a concurrent task admitted the same envelope first; treat it as a duplicate.
    if !state.node.gossip.write().await.mark_seen(&env.id) {
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
    // (pipeline ①-⑬ passé : taille, forme, ban, signature ML-DSA, identifiant
    // canonique, fraîcheur, dedup, rate, nonce) est annoncée à l'UI telle quelle —
    // type réel, expéditeur
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

    // V4 (audit de vie) — a sender that already announced an incompatible
    // protocol version is dropped here, before any handler runs. Its Hello was
    // refused above, so it is not in `peer_info`; without this gate its blocks
    // and transactions would still reach the ledger and be rejected one by one,
    // which is exactly the loop we are closing. Cheap check, after
    // authentication, so it can never be triggered by a spoofed sender.
    {
        // Read-lock on the hot path: virtually every message comes from a
        // compatible sender, and this gate runs for all of them. The write lock
        // is only taken on the rare drop. (A sender marked between the two locks
        // just has this one message counted on its next arrival — harmless.)
        let flagged = state.node.gossip.read().await.is_incompatible(&env.sender);
        if flagged {
            state.node.gossip.write().await.stats.dropped_incompatible += 1;
            return;
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
            // NET-5 / V4 (audit de vie) — protocol version gate.
            //
            // This used to log and carry on ("we never reject"). That was wrong:
            // `TORUS_PROTOCOL_VERSION` is bumped precisely when *validation rules*
            // change — v7 alone moved synthetic-sender confinement (C2), unstake
            // bounding (C3), vote-epoch bounding (H2) and envelope identity
            // (H1/H3). Two nodes on different versions therefore disagree about
            // which blocks and transactions are valid: they connect, exchange,
            // reject each other's work in a loop, pollute each other's mempool and
            // never converge — while each shows the other as a healthy peer.
            //
            // Ruling: an incompatible peer is **not** a peer. We drop its Hello
            // (so it never enters `peer_info`, never triggers a chain sync) and
            // remember the sender so its later traffic is dropped cheaply. This is
            // not a ban: it is not misbehaviour, it is a different network. The
            // count is surfaced in `GossipStats` so the UI can say "3 pairs sur
            // une autre version" instead of silently doing nothing.
            use crate::p2p::gossip::TORUS_PROTOCOL_VERSION;
            if version != TORUS_PROTOCOL_VERSION {
                let mut gossip = state.node.gossip.write().await;
                if gossip.mark_incompatible(&env.sender) {
                    log::warn!(
                        "◈ [NET-5] pair {} en protocole v{} (nous sommes en v{}) — IGNORÉ : \
                         les règles de validation diffèrent, converger est impossible. \
                         {}",
                        short(&env.sender, 12),
                        version,
                        TORUS_PROTOCOL_VERSION,
                        if version > TORUS_PROTOCOL_VERSION {
                            "Mets Quanta à jour."
                        } else {
                            "Ce pair doit mettre Quanta à jour."
                        }
                    );
                }
                gossip.stats.dropped_incompatible += 1;
                return;
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
            // R5 : le nonce de l'enveloppe entre dans l'élection des répondants —
            // il change à chaque relance, donc l'élection se rejoue.
            handle_request_chain(state, &env.sender, env.nonce, from_height, max_blocks).await;
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

/// **SC-06 (AUDIT-2026-08-13) — le fuzzing commençait APRÈS un mur qu'il ne
/// pouvait pas franchir.**
///
/// La cible existante entre par [`try_process_raw_gossip`], qui vérifie une
/// signature ML-DSA-65. Aucune entrée produite par un fuzzer ne porte de
/// signature valide : **100 % des cas meurent au mur d'authentification**, et la
/// couverture réelle du parseur — le `serde_json` sur des octets hostiles, le
/// décodage hex, la décompression gzip, la désérialisation des variantes de
/// `GossipMessage` — est **nulle**. La porte existait, elle ne testait rien.
///
/// Cette entrée expose les parseurs *tels qu'ils sont atteints une fois
/// l'authentification franchie*, c'est-à-dire ce qu'un pair authentifié — donc
/// n'importe qui, une identité ML-DSA coûtant 165 µs — peut faire avaler au
/// nœud. Elle ne relâche aucune vérification en production : c'est une seconde
/// porte d'entrée pour le fuzzer, pas un contournement du dispatcher.
///
/// Comme [`try_process_raw_gossip`], elle ne doit **jamais** paniquer, déborder
/// ou boucler ; c'est exactement la propriété que le fuzzer cherche à casser.
#[allow(dead_code)] // Used by the fuzzing target
pub fn fuzz_parse_payload(data: &[u8]) -> Result<(), String> {
    // ① Le corps d'enveloppe lui-même, sans le mur de signature.
    if let Ok(env) = serde_json::from_slice::<GossipEnvelope>(data) {
        // Fraîcheur : parsing RFC3339 sur une chaîne hostile.
        let _ = GossipRouter::is_fresh(&env.timestamp);
        // Identifiant canonique : re-sérialisation + BLAKE3 du payload décodé.
        let _ = GossipRouter::envelope_id(&env.sender, env.nonce, &env.timestamp, &env.payload);
        // Décodage hex de la signature (longueur arbitraire).
        let _ = hex::decode(&env.signature);
        // Les parseurs propres à chaque variante, atteints après authentification.
        match &env.payload {
            GossipMessage::ChainSegment { blocks_json, blocks_compressed, .. } => {
                if let Some(bytes) = blocks_compressed {
                    // Chemin gzip : bombe de décompression, cardinalité, JSON.
                    let _ = crate::p2p::gossip::decompress_blocks(bytes);
                }
                for b in blocks_json.iter().take(MAX_CHAIN_SEGMENT_RECEIVED) {
                    let _ = serde_json::from_str::<crate::p2p::ledger::Block>(b);
                }
            }
            GossipMessage::NewBlock { block_json } => {
                let _ = serde_json::from_str::<crate::p2p::ledger::Block>(block_json);
            }
            GossipMessage::BroadcastTx { tx_json } => {
                let _ = serde_json::from_str::<crate::p2p::ledger::Transaction>(tx_json);
            }
            GossipMessage::PublishUsername { record_json } => {
                let _ = serde_json::from_str::<crate::p2p::username::UsernameRecord>(record_json);
            }
            GossipMessage::Hello { country, display_name, .. } => {
                let _ = crate::p2p::gossip::sanitize_display_name(country);
                if let Some(n) = display_name {
                    let _ = crate::p2p::gossip::sanitize_display_name(n);
                }
            }
            _ => {}
        }
    }
    // ② Les mêmes octets vus comme un payload gzip nu : le chemin de
    // décompression doit tenir sur n'importe quoi, y compris ce qui n'est pas du
    // gzip du tout.
    let _ = crate::p2p::gossip::decompress_blocks(data);
    Ok(())
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

    // Step 1b: R6 — la forme, avant tout décodage dimensionné par l'émetteur.
    // Même règle que sur le chemin de production (`dispatch_incoming`) : les
    // longueurs de `sender`, `signature`, `id` et de l'horodatage sont fixées par le
    // protocole, les vérifier ne relâche rien et évite un `hex::decode` de plusieurs
    // mégaoctets sur une entrée hostile (c'est aussi ce que voit la cible de fuzz).
    if !envelope_shape_is_plausible(&env) {
        return Err("malformed envelope fields".into());
    }

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
    // R13 : `their_heads` n'est utilisé qu'à l'affichage, mais un `Vec<String>`
    // désérialisé garde tout ce que le pair a envoyé. On le tronque tôt pour que
    // rien en aval ne puisse le parcourir en entier.
    let their_heads: Vec<String> = their_heads.into_iter().take(MAX_HELLO_HEADS).collect();
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
    // M-11 (AUDIT-2026-08-13) : les watts étaient clampés, les trois autres
    // compteurs auto-déclarés ne l'étaient pas — et ils sont sommés en u64 dans
    // le tick de minage. On les borne au même endroit, pour la même raison, avec
    // des plafonds très au-dessus de tout nœud honnête (un an de fonctionnement
    // continu fait 525 600 minutes).
    const MAX_PEER_COUNTER: u64 = 1_000_000_000;
    let tasks_completed = tasks_completed.min(MAX_PEER_COUNTER);
    let blocks_verified = blocks_verified.min(MAX_PEER_COUNTER);
    let uptime_minutes = uptime_minutes.min(MAX_PEER_COUNTER);
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
        // **R13 (AUDIT-2026-08-13)** — `known_peer_ids` n'avait ni plafond de
        // cardinalité ni plafond de longueur, et le `collect()` COMPLET précédait
        // le `.take(3)` : un `Hello` de 10 Mo se matérialisait donc en ~1 million
        // de `String` avant qu'on n'en garde trois. Le `.take` monte en amont du
        // filtre, et les identifiants absurdement longs sont écartés au passage —
        // un ticket iroh valide fait quelques centaines d'octets.
        let new_peers: Vec<String> = known_peer_ids
            .into_iter()
            .filter(|id| id.len() <= MAX_PEER_TICKET_LEN)
            .take(MAX_HELLO_PEER_IDS)
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
///
/// **R5 (AUDIT-2026-08-13) — une réponse point-à-point était diffusée à tout le
/// maillage.**
///
/// Il n'existe aucun chemin unicast dans ce module : `broadcast` pousse sur le
/// topic entier. Un `Ping` reçu par N nœuds faisait donc émettre N `Pong`, chacun
/// reçu N fois — amplification O(N²) dont le facteur était le **débit de pings de
/// l'attaquant**, qui n'a besoin d'aucun enjeu pour l'exercer. Le nœud répondait de
/// surcroît à un expéditeur totalement inconnu (la mise à jour de liveness était
/// conditionnelle, la réponse ne l'était pas).
///
/// Deux règles, sans changer le format du fil :
/// - on ne répond qu'à un pair **connu** (présent dans `peer_info`, donc entré par
///   un `Hello` authentifié) : un inconnu ne fait plus rien émettre du tout ;
/// - au plus un `Pong` par demandeur et par [`crate::p2p::gossip::PONG_COOLDOWN_SECS`]
///   (12 s, sous la cadence honnête de 15 s) : l'amplification cesse d'être
///   proportionnelle au débit de l'attaquant.
///
/// Ce que ça ne fait pas : supprimer le O(N²) du trafic **honnête**. Cela demande
/// un envoi unicast, donc une capacité de la couche transport qui n'existe pas ici.
async fn handle_ping(state: &Arc<AppState>, sender_pk: &str, nonce: u64) {
    let known = {
        let mut info = state.node.peer_info.write().await;
        match info.get_mut(sender_pk) {
            Some(entry) => {
                entry.touch();
                true
            }
            None => false,
        }
    };
    let may_answer = known
        && state
            .node
            .gossip
            .write()
            .await
            .may_answer_ping(sender_pk, chrono::Utc::now().timestamp());
    if !may_answer {
        state.node.gossip.write().await.stats.pongs_suppressed += 1;
        return;
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
    // **R10 (AUDIT-2026-08-13) — saturer la table de signalement désactivait le
    // bannissement pour tout le monde.**
    //
    // `peer_id` est un champ libre du message : rien n'exigeait qu'il désigne un
    // pair existant. Trois clés ML-DSA (494 µs) et 30 000 `ReportPeer` sur des
    // identifiants inventés remplissaient les 10 000 places, après quoi aucune
    // nouvelle cible ne pouvait plus atteindre le quorum — le bannissement, seule
    // réponse du protocole à un pair malveillant, devenait inopérant pour le réseau
    // entier. Combiné à R1, l'attaquant bannissait ses cibles puis fermait la porte
    // derrière lui.
    //
    // Règle : « je ne connais pas cette cible, je ne la comptabilise pas ». Une
    // cible connue est un pair entré dans `peer_info` par un `Hello` authentifié —
    // le signalement ne peut donc plus porter sur du vide, et la table ne peut plus
    // être remplie que par des identités réellement présentes sur le maillage.
    if !state.node.peer_info.read().await.contains_key(peer_id) {
        log::debug!(
            "◈ [Dispatch] ReportPeer de {} contre une cible inconnue {} — ignoré (R10)",
            short(sender_pk, 12),
            short(peer_id, 12)
        );
        return;
    }

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
                // « Qui a trouvé le bloc » : le @pseudo du scelleur quand son
                // adresse est enregistrée (lecture courte, hors lock ledger).
                let miner_name = state.node.usernames.read().await.username_of(&block.miner);
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
                        "miner_name": miner_name,
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
// AUDIT-2026-07-25: was an independent literal that could drift from
// MAX_CHAIN_SEGMENT_RECEIVED — one DoS cap, one source of truth.
const MAX_CHAIN_SEGMENT: u64 = MAX_CHAIN_SEGMENT_RECEIVED as u64;

/// **R5 (AUDIT-2026-08-13)** — score déterministe d'un candidat pour répondre à une
/// requête de chaîne donnée. Plus petit = plus prioritaire.
///
/// Le score lie la requête (demandeur, nonce d'enveloppe, hauteur de départ) au
/// candidat : tous les nœuds calculent le même classement à partir de la même
/// requête, sans échanger un octet de plus. Le **nonce** est ce qui rend le tirage
/// renouvelable : une relance du demandeur porte un nonce neuf, donc élit d'autres
/// répondants — sans quoi une élection malheureuse serait définitive.
fn chain_answer_score(requester: &str, nonce: u64, from_height: u64, candidate: &str) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"quanta-chain-answer-v1");
    h.update(&nonce.to_le_bytes());
    h.update(&from_height.to_le_bytes());
    h.update(requester.as_bytes());
    h.update(candidate.as_bytes());
    *h.finalize().as_bytes()
}

/// R5 — sommes-nous parmi les [`CHAIN_ANSWER_RESPONDERS`] pairs élus pour répondre
/// à cette requête ?
///
/// Fonction pure : mêmes entrées ⇒ même verdict sur tous les nœuds partageant la
/// même vue. Un nœud dont la vue est **plus petite** que la réalité se croit mieux
/// classé et répond, donc l'erreur de vue penche du côté « une réponse de trop »,
/// jamais « aucune réponse » — sauf si la vue contient des pairs morts, cas que le
/// nettoyage TTL des pairs et la relance du demandeur résorbent.
fn is_elected_chain_responder<'a>(
    our_pk: &str,
    requester_pk: &str,
    nonce: u64,
    from_height: u64,
    peers: impl Iterator<Item = &'a str>,
) -> bool {
    let ours = chain_answer_score(requester_pk, nonce, from_height, our_pk);
    let mut better = 0usize;
    for candidate in peers {
        // Le demandeur ne se répond pas à lui-même, et nous ne comptons pas deux fois.
        if candidate == our_pk || candidate == requester_pk {
            continue;
        }
        if chain_answer_score(requester_pk, nonce, from_height, candidate) < ours {
            better += 1;
            if better >= CHAIN_ANSWER_RESPONDERS {
                return false;
            }
        }
    }
    true
}

/// R5 — taille de maillage à partir de laquelle l'élection s'applique. En dessous,
/// tout le monde répond comme avant : le facteur d'amplification y est trop petit
/// pour justifier le moindre risque sur la synchronisation, et c'est le régime des
/// réseaux de test.
pub const CHAIN_ANSWER_MIN_PEERS: usize = 8;

/// Handle a RequestChain message — send back blocks starting at `from_height`.
///
/// **R5 (AUDIT-2026-08-13) — chaque nœud répondait à chaque requête, en diffusion.**
///
/// `RequestChain` atteint les N nœuds du topic ; chacun répondait par un
/// `ChainSegment` complet **diffusé au topic entier**, sans vérifier que quiconque
/// l'avait demandé ni que d'autres répondaient déjà. Mesure de l'audit : pour
/// N = 20 pairs et un segment de 200 Ko, une requête de 10,8 Ko déclenchait 80 Mo
/// de trafic maillé (≈ ×7 400), et chaque répondant payait en plus la lecture de 50
/// blocs, leur sérialisation, un gzip et une signature ML-DSA.
///
/// Faute de chemin unicast dans cette couche, la réduction passe par le nombre de
/// répondants : une élection **déterministe et sans échange** désigne
/// [`CHAIN_ANSWER_RESPONDERS`] pairs par requête. L'amplification devient
/// indépendante de la taille du maillage.
async fn handle_request_chain(
    state: &Arc<AppState>,
    sender: &str,
    request_nonce: u64,
    from_height: u64,
    max_blocks: u64,
) {
    // R5 — élection avant tout travail : ne pas être élu doit coûter un hachage,
    // pas une lecture de 50 blocs.
    //
    // Les trois verrous sont pris l'un APRÈS l'autre, jamais imbriqués : tenir
    // `peer_info` en attendant `crypto` créerait un ordre de verrouillage qui
    // n'existe nulle part ailleurs dans le projet (`crypto → reputation → ledger →
    // gossip`), donc un interblocage latent contre n'importe quelle tâche qui fait
    // l'inverse.
    let peer_count = state.node.peer_info.read().await.len();
    if peer_count >= CHAIN_ANSWER_MIN_PEERS {
        let our_pk = state
            .crypto
            .lock()
            .await
            .pq_identity_hex()
            .unwrap_or_default();
        let elected = {
            let peers = state.node.peer_info.read().await;
            is_elected_chain_responder(
                &our_pk,
                sender,
                request_nonce,
                from_height,
                peers.keys().map(|k| k.as_str()),
            )
        };
        if !elected {
            state
                .node
                .gossip
                .write()
                .await
                .stats
                .chain_answers_suppressed += 1;
            log::debug!(
                "◈ [R5] RequestChain de {} (h={}) — un autre pair est élu pour répondre",
                short(sender, 12),
                from_height
            );
            return;
        }
    }

    let limit = max_blocks.min(MAX_CHAIN_SEGMENT) as usize;
    let ledger = state.node.ledger.read().await;
    let chain_len = ledger.chain_height();

    // Serialize blocks from `from_height` to `from_height + limit`.
    //
    // R3 : deux plafonds, pas un. Le nombre de blocs bornait le segment, mais pas
    // sa TAILLE — or un bloc légal peut peser ~2,8 Mo, donc 50 blocs pouvaient
    // dépasser de deux ordres de grandeur toute borne de transport raisonnable.
    // On s'arrête au premier des deux atteints. Le premier bloc est toujours
    // inclus, quelle que soit sa taille : sans ça, un bloc plus gros que le budget
    // arrêterait définitivement la synchronisation du réseau.
    let mut blocks_json: Vec<String> = Vec::new();
    let mut budget_used = 0usize;
    for i in (from_height..chain_len).take(limit) {
        let Some(json) = ledger.block_at(i).and_then(|b| serde_json::to_string(b).ok()) else {
            continue;
        };
        if !blocks_json.is_empty() && budget_used + json.len() > CHAIN_SEGMENT_BYTE_BUDGET {
            log::debug!(
                "◈ [Dispatch] ChainSegment tronqué à {} blocs ({} o) — budget {} o atteint, \
                 le pair redemandera la suite",
                blocks_json.len(),
                budget_used,
                CHAIN_SEGMENT_BYTE_BUDGET
            );
            break;
        }
        budget_used += json.len();
        blocks_json.push(json);
    }

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
///
/// **R12 (AUDIT-2026-08-13)** — l'admission dans la file de sortie est bornée ici,
/// avant la signature. Deux raisons de la placer avant plutôt qu'après : jeter un
/// message coûte alors zéro opération cryptographique (une signature ML-DSA-65
/// pèse ~456 µs, sur la tâche qui porte aussi le dispatch), et le refus se voit
/// dans un compteur plutôt que dans la mémoire résidente. Voir
/// [`crate::p2p::gossip::GossipRouter::try_admit_egress`] pour la politique.
async fn broadcast(state: &Arc<AppState>, msg: GossipMessage) {
    // R12 — borne d'admission : la file de sortie n'est pas extensible. La classe
    // est retenue ici parce que `msg` est consommé plus bas : sans elle, un échec
    // tardif rendrait la place introuvable et la borne fuirait message par message.
    let egress_class = crate::p2p::gossip::dispatcher_egress_class(&msg);
    {
        let mut gossip = state.node.gossip.write().await;
        if !gossip.try_admit_egress(&msg) {
            let (bulk, light) = gossip.egress_inflight();
            let total = gossip.stats.egress_dropped;
            let warn = gossip.note_egress_drop_warn();
            drop(gossip);
            if warn {
                log::warn!(
                    "◈ [R12] file de sortie saturée (segments={} réponses={}) — réponse \
                     abandonnée ({} au total). Le drain réseau ne suit pas le rythme des \
                     requêtes entrantes.",
                    bulk,
                    light,
                    total
                );
            }
            return;
        }
    }
    // PQ-ENVELOPE-1: envelope sender + signature identity = ML-DSA-65 primary key.
    let pk = state
        .crypto
        .lock()
        .await
        .pq_identity_hex()
        .unwrap_or_default();
    if pk.is_empty() {
        // Place réservée puis non consommée : la rendre, sinon la borne fuit.
        state
            .node
            .gossip
            .write()
            .await
            .release_egress_slot(egress_class);
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
            state
                .node
                .gossip
                .write()
                .await
                .release_egress_slot(egress_class);
            return;
        }
    };
    state.node.gossip.write().await.mark_seen(&env.id);
    if state.node.gossip_tx.send(env).is_err() {
        // Canal fermé (arrêt du nœud) : la place ne sera jamais dépilée.
        state
            .node
            .gossip
            .write()
            .await
            .release_egress_slot(egress_class);
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::CryptoEngine;

    // ─── Helpers de test ────────────────────────────────────────────────────

    /// Enveloppe correctement signée, prête à passer sur le fil.
    fn signed_envelope(
        crypto: &CryptoEngine,
        pk: &str,
        msg: GossipMessage,
        nonce: u64,
    ) -> Vec<u8> {
        let ts = chrono::Utc::now().to_rfc3339();
        let signable = GossipRouter::signable_envelope_bytes(pk, nonce, &ts, &msg);
        let sig = crypto.sign_pq(&signable).expect("signature");
        let env = GossipRouter::build_signed_envelope(pk.to_string(), msg, nonce, ts, &sig)
            .expect("enveloppe");
        serde_json::to_vec(&env).expect("json")
    }

    /// Enveloppe de la **bonne forme** mais dont la signature est du remplissage :
    /// exactement ce qu'un attaquant sans clé produit en une microseconde.
    fn forged_envelope(victim_pk: &str, msg: GossipMessage, nonce: u64) -> GossipEnvelope {
        let ts = chrono::Utc::now().to_rfc3339();
        GossipEnvelope {
            id: GossipRouter::envelope_id(victim_pk, nonce, &ts, &msg),
            sender: victim_pk.to_string(),
            payload: msg,
            signature: hex::encode(vec![0u8; fips204::ml_dsa_65::SIG_LEN]),
            timestamp: ts,
            nonce,
        }
    }

    /// **R11 (AUDIT-2026-08-13) — la comptabilité par pair était écrite avant la
    /// signature, donc falsifiable à distance.**
    ///
    /// `bytes_in` / `messages_in` étaient incrémentés sur `env.sender`, un champ
    /// libre de l'enveloppe, AVANT la vérification. Ce n'était donc pas le pair
    /// bruyant qui était comptabilisé mais **celui que l'attaquant désignait** :
    /// l'audit impute 410 734 octets à un pair honnête avec une seule enveloppe
    /// forgée. Les métriques réseau — celles sur lesquelles un opérateur
    /// s'appuierait précisément pendant une attaque — mentaient.
    ///
    /// Le test forge une enveloppe au nom d'une victime **connue** du nœud et exige
    /// que rien ne bouge : ni son compteur d'octets, ni son compteur de messages,
    /// ni les statistiques globales.
    #[tokio::test]
    async fn r11_a_forged_envelope_cannot_bill_an_honest_peer() {
        let state = test_app_state();
        state
            .crypto
            .lock()
            .await
            .generate_pq_identity()
            .expect("identité du nœud");

        let mut victim = CryptoEngine::new();
        victim.generate_pq_identity().expect("identité victime");
        let victim_pk = victim.pq_identity_hex().expect("pk victime");

        // La victime est un pair connu : c'est le cas où la falsification portait.
        state
            .node
            .peer_info
            .write()
            .await
            .insert(victim_pk.clone(), crate::p2p::PeerInfo::new(10.0, "FR".into()));

        let forged = forged_envelope(&victim_pk, GossipMessage::Ping { nonce: 1 }, 1);
        let raw = serde_json::to_vec(&forged).expect("json");
        dispatch_incoming(&state, &raw).await;

        let info = state.node.peer_info.read().await;
        let entry = info.get(&victim_pk).expect("la victime est connue");
        assert_eq!(
            entry.bytes_in, 0,
            "R11 : une enveloppe non authentifiée ne doit rien imputer à un pair honnête"
        );
        assert_eq!(entry.messages_in, 0, "R11 : ni un message");
        drop(info);
        let g = state.node.gossip.read().await;
        assert_eq!(
            g.stats.messages_received, 0,
            "R11 : les statistiques globales ne comptent que du trafic authentifié"
        );
        assert_eq!(g.stats.bytes_received, 0);
        assert_eq!(g.stats.dropped_signature, 1, "la forgerie est bien détectée");
    }

    /// **R6 (AUDIT-2026-08-13) — la signature est vérifiée AVANT l'identifiant
    /// canonique.**
    ///
    /// L'ancien ordre recalculait l'identifiant canonique — une re-sérialisation
    /// JSON complète du payload plus un BLAKE3, donc le poste le plus cher du
    /// pipeline — sur des octets non authentifiés, et rejetait l'enveloppe là.
    /// Mesure de l'audit : 8 Mo non authentifiés coûtaient 16 ms de CPU avant la
    /// moindre vérification de signature, hors limiteur de débit.
    ///
    /// L'observable qui distingue les deux ordres : une enveloppe **à la fois** mal
    /// signée et d'identifiant non canonique. Dans l'ancien ordre elle mourait au
    /// contrôle d'identifiant, donc `dropped_signature` restait à zéro ; dans le
    /// nouveau, elle est rejetée à la porte de signature et comptée comme telle.
    #[tokio::test]
    async fn r6_signature_is_checked_before_the_canonical_id_is_recomputed() {
        let state = test_app_state();
        let mut victim = CryptoEngine::new();
        victim.generate_pq_identity().expect("identité");
        let victim_pk = victim.pq_identity_hex().expect("pk");

        let mut forged = forged_envelope(&victim_pk, GossipMessage::Ping { nonce: 1 }, 1);
        // Identifiant de la bonne FORME (64 hex) mais faux : seul l'ordre du
        // pipeline décide de la porte qui le rejette.
        forged.id = "d".repeat(ENVELOPE_ID_HEX_LEN);
        let raw = serde_json::to_vec(&forged).expect("json");

        dispatch_incoming(&state, &raw).await;

        let g = state.node.gossip.read().await;
        assert_eq!(
            g.stats.dropped_signature, 1,
            "R6 : l'authentification doit précéder tout travail proportionnel au message"
        );
    }

    /// **R6 / tâche 4 — rien n'est décodé sur la foi d'une longueur choisie par
    /// l'attaquant.**
    ///
    /// Une signature de 2 Mio d'hexadécimal était décodée (donc allouée) avant
    /// d'être jugée, une seule fois par message, sans qu'aucune clé ne soit
    /// possédée. Les longueurs sont fixes dans le protocole : les vérifier est
    /// O(1) et ne relâche rien — la même enveloppe échouait de toute façon à la
    /// vérification, simplement après avoir payé le décodage.
    #[tokio::test]
    async fn r6_an_absurdly_long_signature_is_refused_before_being_decoded() {
        let state = test_app_state();
        let mut victim = CryptoEngine::new();
        victim.generate_pq_identity().expect("identité");
        let victim_pk = victim.pq_identity_hex().expect("pk");

        let mut forged = forged_envelope(&victim_pk, GossipMessage::Ping { nonce: 1 }, 1);
        forged.signature = "ab".repeat(512 * 1024); // 1 Mio décodé si on la décodait
        let raw = serde_json::to_vec(&forged).expect("json");

        dispatch_incoming(&state, &raw).await;

        let g = state.node.gossip.read().await;
        assert_eq!(
            g.stats.dropped_malformed, 1,
            "tâche 4 : la forme se juge avant toute allocation"
        );
        assert_eq!(
            g.stats.dropped_signature, 0,
            "l'enveloppe ne doit même pas atteindre le décodage hexadécimal"
        );
    }

    /// **R16 (AUDIT-2026-08-13) — le chemin non authentifié prenait un verrou en
    /// ÉCRITURE sur le tracker global.**
    ///
    /// `is_banned` évince paresseusement les bans expirés, donc mute, donc exige
    /// `nonce_tracker.write()`. Ce verrou était pris pour **chaque** message
    /// entrant, avant tout le reste : un point de sérialisation global que
    /// n'importe qui pouvait solliciter sans identité, partagé avec le limiteur de
    /// débit et le contrôle de nonce.
    ///
    /// Le test tient un verrou de **lecture** pendant le dispatch : si le pipeline
    /// avait encore besoin d'écrire, il attendrait indéfiniment. Avec la sonde en
    /// lecture seule, il termine.
    #[tokio::test]
    async fn r16_an_unauthenticated_envelope_never_needs_the_tracker_write_lock() {
        let state = test_app_state();
        let mut victim = CryptoEngine::new();
        victim.generate_pq_identity().expect("identité");
        let victim_pk = victim.pq_identity_hex().expect("pk");
        let forged = forged_envelope(&victim_pk, GossipMessage::Ping { nonce: 1 }, 1);
        let raw = serde_json::to_vec(&forged).expect("json");

        let held = state.node.nonce_tracker.read().await;
        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            dispatch_incoming(&state, &raw),
        )
        .await;
        drop(held);

        assert!(
            outcome.is_ok(),
            "R16 : un message non authentifié ne doit pas exiger le verrou d'écriture global"
        );
    }

    /// **R5 (AUDIT-2026-08-13) — un `Ping` d'un inconnu faisait émettre un `Pong` à
    /// tout le maillage.**
    ///
    /// Il n'existe aucun chemin unicast ici : la réponse part sur le topic entier.
    /// Un `Ping` reçu par N nœuds faisait donc émettre N `Pong` lus N fois. Le nœud
    /// répondait même à un expéditeur absent de `peer_info` — un inconnu obtenait
    /// l'amplification sans jamais s'être présenté.
    #[tokio::test]
    async fn r5_a_ping_from_an_unknown_peer_emits_nothing() {
        let state = test_app_state();
        state
            .crypto
            .lock()
            .await
            .generate_pq_identity()
            .expect("identité du nœud");

        let mut stranger = CryptoEngine::new();
        stranger.generate_pq_identity().expect("identité");
        let stranger_pk = stranger.pq_identity_hex().expect("pk");
        let raw = signed_envelope(&stranger, &stranger_pk, GossipMessage::Ping { nonce: 1 }, 1);

        dispatch_incoming(&state, &raw).await;

        let g = state.node.gossip.read().await;
        assert_eq!(
            g.egress_inflight().1,
            0,
            "R5 : un inconnu ne doit rien faire émettre"
        );
        assert_eq!(g.stats.pongs_suppressed, 1);
    }

    /// R5 — un pair connu obtient un `Pong`, mais **un seul** par fenêtre de
    /// refroidissement. La cadence honnête est de 15 s ; l'attaquant qui frappe
    /// plus vite n'obtient plus rien de plus.
    #[tokio::test]
    async fn r5_a_known_peer_gets_one_pong_per_cooldown() {
        let state = test_app_state();
        state
            .crypto
            .lock()
            .await
            .generate_pq_identity()
            .expect("identité du nœud");

        let mut peer = CryptoEngine::new();
        peer.generate_pq_identity().expect("identité");
        let peer_pk = peer.pq_identity_hex().expect("pk");
        state
            .node
            .peer_info
            .write()
            .await
            .insert(peer_pk.clone(), crate::p2p::PeerInfo::new(10.0, "FR".into()));

        for nonce in 1..=5u64 {
            let raw = signed_envelope(&peer, &peer_pk, GossipMessage::Ping { nonce }, nonce);
            dispatch_incoming(&state, &raw).await;
        }

        let g = state.node.gossip.read().await;
        assert_eq!(
            g.egress_inflight().1,
            1,
            "R5 : cinq pings rapprochés ne doivent produire qu'un seul Pong"
        );
        assert_eq!(g.stats.pongs_suppressed, 4);
    }

    /// **R5 — l'élection des répondants borne l'amplification `ChainSegment`.**
    ///
    /// Chaque nœud répondait à chaque `RequestChain`, en diffusion : pour N = 20
    /// pairs et un segment de 200 Ko, une requête de 10,8 Ko déclenchait 80 Mo de
    /// trafic maillé (≈ ×7 400). L'élection est déterministe et sans échange : tous
    /// les nœuds partageant la même vue désignent les mêmes
    /// [`CHAIN_ANSWER_RESPONDERS`] répondants.
    #[test]
    fn r5_exactly_k_peers_answer_a_given_chain_request() {
        let nodes: Vec<String> = (0..64).map(|i| format!("noeud-{i:03}")).collect();
        let requester = "demandeur";
        let elected = nodes
            .iter()
            .filter(|me| {
                is_elected_chain_responder(
                    me,
                    requester,
                    7,
                    100,
                    nodes.iter().map(|s| s.as_str()),
                )
            })
            .count();
        assert_eq!(
            elected, CHAIN_ANSWER_RESPONDERS,
            "sur 64 nœuds de vue identique, exactement {CHAIN_ANSWER_RESPONDERS} répondent \
             (ils répondaient tous)"
        );
    }

    /// R5 — l'élection doit être **renouvelable**, sinon un tirage malheureux
    /// (répondants sans les blocs demandés, ou morts) figerait la synchronisation.
    /// Le nonce d'enveloppe change à chaque relance, donc le tirage aussi.
    #[test]
    fn r5_a_retry_elects_other_responders() {
        let nodes: Vec<String> = (0..64).map(|i| format!("noeud-{i:03}")).collect();
        let mut seen: HashSet<String> = HashSet::new();
        for nonce in 0..16u64 {
            for me in &nodes {
                if is_elected_chain_responder(
                    me,
                    "demandeur",
                    nonce,
                    100,
                    nodes.iter().map(|s| s.as_str()),
                ) {
                    seen.insert(me.clone());
                }
            }
        }
        assert!(
            seen.len() > CHAIN_ANSWER_RESPONDERS,
            "seize relances doivent solliciter plus de {CHAIN_ANSWER_RESPONDERS} pairs, sinon \
             l'élection est un point de panne (obtenu : {})",
            seen.len()
        );
    }

    /// R5 — vu du dispatcher : un nœud non élu ne produit aucun `ChainSegment`,
    /// et ne lit même pas la chaîne pour s'en apercevoir.
    #[tokio::test]
    async fn r5_a_non_elected_node_answers_no_chain_request() {
        let state = test_app_state();
        let our_pk = {
            let mut c = state.crypto.lock().await;
            c.generate_pq_identity().expect("identité du nœud");
            c.pq_identity_hex().expect("pk")
        };
        // Une vue assez grande pour que l'élection s'applique (CHAIN_ANSWER_MIN_PEERS).
        let peers: Vec<String> = (0..32).map(|i| format!("pair-{i:03}")).collect();
        {
            let mut info = state.node.peer_info.write().await;
            for p in &peers {
                info.insert(p.clone(), crate::p2p::PeerInfo::new(10.0, "FR".into()));
            }
        }
        let requester = "demandeur";
        // Un nonce pour lequel nous ne sommes PAS élus : c'est le cas majoritaire.
        let candidates: Vec<&str> = peers
            .iter()
            .map(|s| s.as_str())
            .chain(std::iter::once(our_pk.as_str()))
            .collect();
        let nonce = (1u64..1000)
            .find(|n| {
                !is_elected_chain_responder(&our_pk, requester, *n, 0, candidates.iter().copied())
            })
            .expect("un nonce non élu");

        handle_request_chain(&state, requester, nonce, 0, 50).await;

        let g = state.node.gossip.read().await;
        assert_eq!(
            g.stats.chain_answers_suppressed, 1,
            "R5 : un nœud non élu ne répond pas"
        );
        assert_eq!(g.egress_inflight().0, 0, "aucun segment en file");
    }

    /// **R12 (AUDIT-2026-08-13) — la file de sortie était non bornée.**
    ///
    /// Les quatre lanes de `gossip_priority` sont des `mpsc::unbounded_channel` ;
    /// `Pong` et `ChainSegment` y sont produits à la cadence du trafic distant,
    /// tandis que le drain émet message par message en attendant le réseau. Dès que
    /// le drain décroche, la file grandit sans limite — ~11 Ko l'entrée pour un
    /// `Pong`, jusqu'à 3 Mio pour un `ChainSegment`.
    ///
    /// Le test met le drain à l'arrêt (personne ne dépile) et pousse deux fois la
    /// borne : l'admission doit s'arrêter net, compter les refus, et **ne pas
    /// bloquer** l'appelant — bloquer le producteur bloquerait la boucle de
    /// dispatch, donc la lecture réseau elle-même.
    #[tokio::test]
    async fn r12_the_outgoing_queue_is_bounded_and_drops_explicitly() {
        use crate::p2p::gossip::MAX_INFLIGHT_EGRESS_LIGHT;
        let state = test_app_state();
        state
            .crypto
            .lock()
            .await
            .generate_pq_identity()
            .expect("identité du nœud");

        let attempts = MAX_INFLIGHT_EGRESS_LIGHT * 2;
        for nonce in 0..attempts {
            broadcast(&state, GossipMessage::Pong { nonce: nonce as u64 }).await;
        }

        {
            let g = state.node.gossip.read().await;
            assert_eq!(
                g.egress_inflight().1,
                MAX_INFLIGHT_EGRESS_LIGHT,
                "R12 : la file ne doit pas dépasser sa borne"
            );
            assert_eq!(
                g.stats.egress_dropped,
                (attempts - MAX_INFLIGHT_EGRESS_LIGHT) as u64,
                "R12 : chaque refus doit être compté, pas silencieux"
            );
        }

        // La borne est une borne de FILE, pas un quota de vie : dépiler libère.
        let mut rx = state
            .node
            .take_gossip_receiver()
            .await
            .expect("le récepteur du drain");
        let env = rx.recv().await.expect("un message en file");
        state
            .node
            .gossip
            .write()
            .await
            .note_egress_drained(&env.payload);
        broadcast(&state, GossipMessage::Pong { nonce: 9_999 }).await;
        let g = state.node.gossip.read().await;
        assert_eq!(
            g.egress_inflight().1,
            MAX_INFLIGHT_EGRESS_LIGHT,
            "une place rendue est une place réutilisable"
        );
    }

    /// **R10 (AUDIT-2026-08-13) — saturer la table de signalement désactivait le
    /// bannissement pour tout le monde (fail-open).**
    ///
    /// `peer_id` est un champ libre : trois clés et 30 000 `ReportPeer` sur des
    /// identifiants inventés remplissaient les 10 000 places, après quoi toute
    /// nouvelle cible était évincée avant son 3ᵉ rapporteur. Une cible inconnue
    /// n'entre plus dans la table du tout.
    #[tokio::test]
    async fn r10_reports_against_unknown_targets_are_not_tracked() {
        let state = test_app_state();
        let mut reporter = CryptoEngine::new();
        reporter.generate_pq_identity().expect("identité");
        let reporter_pk = reporter.pq_identity_hex().expect("pk");

        for i in 0..2_000 {
            handle_report_peer(
                &state,
                &reporter_pk,
                &format!("cible-fictive-{i}"),
                ReportReason::MalformedMessage,
            )
            .await;
        }
        assert!(
            state.node.nonce_tracker.read().await.report_counts.is_empty(),
            "R10 : une cible que nous ne connaissons pas ne consomme aucune place"
        );

        // Et un vrai pair reste bannissable par trois rapporteurs distincts.
        let victim = "pair-reel";
        state
            .node
            .peer_info
            .write()
            .await
            .insert(victim.to_string(), crate::p2p::PeerInfo::new(10.0, "FR".into()));
        for i in 0..REPORT_BAN_THRESHOLD {
            handle_report_peer(
                &state,
                &format!("rapporteur-{i}"),
                victim,
                ReportReason::MalformedMessage,
            )
            .await;
        }
        assert!(
            state.node.nonce_tracker.write().await.is_banned(victim),
            "R10 : le mécanisme de bannissement doit rester opérant"
        );
    }

    /// R10 — même table saturée, une accusation légitime doit aboutir.
    ///
    /// L'éviction retirait la cible la plus **faible**, or une cible fraîche a
    /// exactement un rapporteur : c'était donc toujours elle qui mourait, et aucune
    /// nouvelle cible ne pouvait plus jamais atteindre le quorum. L'éviction porte
    /// désormais sur les plus **anciennes** sous-seuil.
    #[test]
    fn r10_a_saturated_table_still_lets_a_real_offender_be_banned() {
        let mut t = NonceTracker::new();
        // Saturation par des cibles fictives à DEUX rapporteurs : sous le seuil,
        // mais strictement « plus fortes » qu'une accusation naissante.
        for i in 0..(MAX_TRACKED_REPORTS + 50) {
            t.record_report(&format!("squat-{i}"), "rapporteur-a");
            t.record_report(&format!("squat-{i}"), "rapporteur-b");
        }
        assert!(t.report_counts.len() <= MAX_TRACKED_REPORTS, "la borne tient");

        let victim = "vrai-malveillant";
        for i in 0..REPORT_BAN_THRESHOLD {
            t.record_report(victim, &format!("temoin-{i}"));
        }
        assert!(
            t.is_banned(victim),
            "R10 : la saturation ne doit pas rendre le bannissement inopérant"
        );
    }

    /// **R8 (AUDIT-2026-08-13) — l'éviction par cardinalité était O(n) PAR
    /// MESSAGE.**
    ///
    /// La boucle cherchait le minimum sur 100 000 entrées puis en retirait **une**.
    /// Une fois le plafond atteint — ~16 s d'inondation de clés fraîches selon
    /// l'audit — chaque message excédentaire relançait le balayage complet, sur la
    /// tâche qui porte aussi le dispatch. Le plafond mémoire tenait, il achetait une
    /// bombe CPU en échange.
    ///
    /// L'observable : une passe d'éviction descend au seuil bas, donc les milliers
    /// de messages suivants n'en déclenchent aucune.
    #[test]
    fn r8_over_cap_eviction_drops_to_the_low_water_mark_in_one_pass() {
        let mut t = NonceTracker::new();
        let now = now_epoch_secs();
        for i in 0..MAX_TRACKED_SENDERS {
            let k = format!("s{i}");
            t.last_nonces.insert(k.clone(), 1);
            // Des dates distinctes : c'est l'ordre d'ancienneté qui pilote l'éviction.
            t.last_seen.insert(k, now - (i % 60) as u64);
        }
        assert_eq!(t.last_seen.len(), MAX_TRACKED_SENDERS);

        assert!(t.check_and_advance("une_de_plus", 1));

        assert!(
            t.last_seen.len() <= LOW_WATER_TRACKED_SENDERS + 1,
            "R8 : une passe doit descendre au seuil bas ({} entrées restantes)",
            t.last_seen.len()
        );
        assert!(
            t.last_seen.contains_key(&peer_key("une_de_plus")),
            "l'expéditeur actif est conservé"
        );
        assert_eq!(
            t.last_nonces.len(),
            t.last_seen.len(),
            "les cartes par expéditeur restent alignées"
        );
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
            tracker.last_nonces.get(&peer_key(&pk)),
            Some(&1),
            "a valid sender's nonce high-water must be recorded"
        );
        assert!(
            tracker.rate_counters.contains_key(&peer_key(&pk)),
            "a valid sender must be tracked for rate limiting"
        );
    }

    /// H1 (AUDIT-2026-07-25): the dedup LRU was written at stage ④ while the
    /// signature was only checked at stage ⑧, and `env.id` was a free-form String
    /// nothing recomputed. So an unauthenticated peer could precompute the ids of
    /// the RequestChain messages every node emits, poison them for free, and have
    /// every genuine sync request silently dropped — censorship, no stake, no rate
    /// limit (the limiter runs after the signature gate the attacker never reaches).
    #[tokio::test]
    async fn h1_forged_envelope_id_cannot_poison_the_dedup_lru() {
        let state = test_app_state();
        let mut signer = CryptoEngine::new();
        signer.generate_pq_identity().expect("ml-dsa primary");
        let pk = signer.pq_identity_hex().expect("ml-dsa primary");
        let msg = GossipMessage::Ping { nonce: 7 };
        let ts = chrono::Utc::now().to_rfc3339();
        let nonce = 1_u64;
        let signable = GossipRouter::signable_envelope_bytes(&pk, nonce, &ts, &msg);
        let sig = signer.sign_pq(&signable).unwrap();
        let mut env =
            GossipRouter::build_signed_envelope(pk.clone(), msg, nonce, ts, &sig).unwrap();

        // The id an honest node WILL later use for its own chain request.
        let victim_payload = GossipMessage::RequestChain { from_height: 0, max_blocks: 50 };
        let victim_id =
            GossipRouter::envelope_id("some-honest-peer", 9, "2026-07-25T00:00:00Z", &victim_payload);
        env.id = victim_id.clone();
        let raw = serde_json::to_vec(&env).unwrap();

        dispatch_incoming(&state, &raw).await;

        assert!(
            !state.node.gossip.read().await.has_seen(&victim_id),
            "a forged id must never reach the dedup LRU"
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
        // H6: the maps are keyed by peer_key(), so look the active sender up the
        // same way the tracker stores it.
        let active = peer_key("fresh_active");
        assert!(t.last_seen.len() <= MAX_TRACKED_SENDERS, "maps stay bounded under flood");
        assert!(t.last_nonces.len() <= MAX_TRACKED_SENDERS, "nonce map bounded");
        assert!(t.last_seen.contains_key(&active), "the ACTIVE sender is retained");
        assert!(
            t.last_seen.len() < MAX_TRACKED_SENDERS,
            "idle (replay-safe) entries were expired, not the active one"
        );
    }

    /// H6 (AUDIT-2026-07-25): `prune_reports_and_bans` bounds the number of
    /// TARGETS, never the reporters of one target — so a single victim's reporter
    /// set grew without limit. Each entry used to be a 3904-char ML-DSA key, and
    /// minting keypairs is microseconds work with one message per key, which never
    /// trips the per-sender rate limit. Assert the set is capped, and that a ban
    /// still forms (the cap must not break the mechanism it bounds).
    #[test]
    fn h6_reporter_set_is_capped_per_target() {
        let mut t = NonceTracker::new();
        for i in 0..10_000 {
            t.record_report("victim", &format!("reporter-{i}"));
        }
        let stored = t
            .report_counts
            .get(&peer_key("victim"))
            .map(|s| s.len())
            .unwrap_or(0);
        assert!(
            stored <= MAX_REPORTERS_PER_TARGET as usize,
            "reporter set must stay capped, got {stored}"
        );
        assert!(t.is_banned("victim"), "the ban still forms from distinct reporters");
    }

    /// H6: keys are stored hashed, so a map entry is 64 hex chars regardless of
    /// how long the sender's public key is — the property that turns ~390 MB of
    /// worst-case key storage into ~6 MB.
    #[test]
    fn h6_peer_keys_are_stored_hashed_not_raw() {
        let mldsa_sized = "a".repeat(3904);
        let mut t = NonceTracker::new();
        assert!(t.check_and_advance(&mldsa_sized, 1));
        assert!(
            !t.last_nonces.contains_key(&mldsa_sized),
            "the raw 3904-char key must never be a map key"
        );
        let k = t.last_nonces.keys().next().expect("one entry");
        assert_eq!(k.len(), 64, "stored key is a BLAKE3 digest in hex");
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


    /// **R1 (AUDIT-2026-08-13) — REPORT-NOAUTH-1 : le nœud ne dénonce plus personne
    /// sur la foi d'une donnée non authentifiée.**
    ///
    /// C'est le PoC de l'audit, retourné en test de non-régression. Un attaquant
    /// **sans aucune clé** forge une enveloppe qui prétend venir de la victime,
    /// avec une signature bidon. Avant le correctif, le nœud honnête diffusait un
    /// `ReportPeer{peer_id: env.sender}` — c'est-à-dire qu'il dénonçait
    /// l'expéditeur d'une enveloppe dont la signature venait précisément d'être
    /// jugée fausse. Trois nœuds honnêtes atteints par la même forgerie
    /// dénonçaient chacun de leur côté, et `REPORT_BAN_THRESHOLD = 3` bannissait
    /// la victime une heure, partout.
    ///
    /// Le test exige les deux moitiés de la correction : la forgerie est bien
    /// **détectée** (le compteur monte), et elle ne produit **aucun signalement**
    /// (le nœud ne signe rien, n'émet rien — l'amplification disparaît avec).
    #[tokio::test]
    async fn r1_a_forged_envelope_denounces_nobody() {
        let state = test_app_state();
        // Le nœud honnête a une identité : s'il VOULAIT diffuser, il le pourrait.
        // Sans cela le test passerait pour la mauvaise raison.
        state
            .crypto
            .lock()
            .await
            .generate_pq_identity()
            .expect("identité du nœud honnête");

        // La victime : n'importe quelle clé publique connue du réseau. Elle est
        // publique par construction — c'est le champ `sender` de tous ses messages.
        let mut victim = CryptoEngine::new();
        victim.generate_pq_identity().expect("identité victime");
        let victim_pk = victim.pq_identity_hex().expect("pk victime");

        // L'attaquant ne possède RIEN : il choisit le sender, le nonce, l'heure,
        // le payload, calcule l'identifiant canonique (fonction publique) et met
        // une signature arbitraire.
        let payload = GossipMessage::Ping { nonce: 1 };
        let ts = chrono::Utc::now().to_rfc3339();
        let nonce = 1u64;
        let id = GossipRouter::envelope_id(&victim_pk, nonce, &ts, &payload);
        let forged = GossipEnvelope {
            id,
            sender: victim_pk.clone(),
            payload,
            signature: hex::encode([0u8; 3309]), // signature ML-DSA-65 bidon
            timestamp: ts,
            nonce,
        };
        let raw = serde_json::to_vec(&forged).expect("json");

        let before = state.node.gossip.read().await.stats.peers_reported;
        dispatch_incoming(&state, &raw).await;
        let g = state.node.gossip.read().await;

        assert_eq!(g.stats.dropped_signature, 1, "la forgerie doit être détectée");
        assert_eq!(
            g.stats.peers_reported, before,
            "R1 : une enveloppe non authentifiée ne doit produire AUCUN signalement — \
             son « expéditeur » n'est pas une identité"
        );
        assert_eq!(
            g.stats.messages_sent, 0,
            "R1 : et donc aucune émission — l'amplification (une signature ML-DSA-65 \
             complète + un envoi à tous les pairs par forgerie) disparaît avec elle"
        );
    }

    /// **R1 — le seuil de bannissement lui-même est inchangé, et c'est voulu.**
    ///
    /// Trois rapporteurs *authentifiés et distincts* bannissent toujours. Ce qui
    /// a changé, c'est qu'un attaquant sans clé ne peut plus **fabriquer** ces
    /// rapporteurs à partir de nœuds honnêtes. Le test épingle la distinction :
    /// le mécanisme de quorum reste, sa manipulation à distance disparaît.
    ///
    /// À dire clairement : `REPORT_BAN_THRESHOLD` compte des **identités**, et une
    /// identité ML-DSA coûte ~165 µs. Trois marionnettes bannissent donc encore
    /// n'importe qui. Adosser le signalement à un coût non falsifiable (l'enjeu
    /// bondé) reste ouvert — c'est une décision de conception, pas un correctif.
    #[test]
    fn r1_the_report_quorum_still_needs_distinct_authenticated_reporters() {
        let mut tracker = NonceTracker::new();
        let victim = "VICTIME_PK";
        // Un seul rapporteur, même acharné, ne bannit pas.
        for _ in 0..10 {
            tracker.record_report(victim, "UN_SEUL_RAPPORTEUR");
        }
        assert!(!tracker.is_banned(victim), "un rapporteur unique ne bannit pas");
        // Trois distincts, oui.
        for i in 0..REPORT_BAN_THRESHOLD {
            tracker.record_report(victim, &format!("RAPPORTEUR_{i}"));
        }
        assert!(tracker.is_banned(victim), "trois rapporteurs distincts bannissent");
    }

    /// **R15 (AUDIT-2026-08-13) — un anti-rejeu qu'un redémarrage oublie ne
    /// protège de rien.**
    ///
    /// Le `NonceTracker` portait le nonce le plus haut vu par expéditeur et les
    /// bannissements en cours, et les deux repartaient de zéro à chaque
    /// redémarrage : une enveloppe authentique capturée avant l'arrêt redevenait
    /// acceptable après — et un attaquant peut provoquer le redémarrage — tandis
    /// qu'un pair banni pour abus repartait avec une ardoise nette.
    #[test]
    fn r15_replay_protection_and_bans_survive_a_restart() {
        let pk = "aa".repeat(32);
        let mut t = NonceTracker::new();
        assert!(t.check_and_advance(&pk, 5), "premier nonce accepté");
        assert!(!t.check_and_advance(&pk, 5), "rejeu refusé à chaud");

        // Redémarrage : un tracker neuf, restauré depuis l'instantané.
        let snap = t.snapshot();
        let json = serde_json::to_string(&snap).expect("sérialisable");
        let restored: NonceTrackerSnapshot = serde_json::from_str(&json).expect("relisible");
        let mut fresh = NonceTracker::new();
        assert!(
            fresh.check_and_advance(&pk, 5),
            "sans restauration, le rejeu passe — c'est le bug"
        );
        let mut fresh2 = NonceTracker::new();
        fresh2.restore(restored);
        assert!(
            !fresh2.check_and_advance(&pk, 5),
            "après restauration, l'enveloppe rejouée est refusée (R15)"
        );
        assert!(fresh2.check_and_advance(&pk, 6), "un nonce plus haut reste accepté");
    }

    /// La restauration est **monotone** : un instantané plus ancien que l'état
    /// vivant ne doit jamais faire redescendre le compteur — redescendre
    /// rouvrirait exactement le rejeu que le champ existe pour fermer.
    #[test]
    fn r15_restore_never_lowers_a_live_nonce() {
        let pk = "bb".repeat(32);
        let mut t = NonceTracker::new();
        assert!(t.check_and_advance(&pk, 100));
        let mut stale = NonceTrackerSnapshot::default();
        stale.last_nonces.insert(crate::p2p::dispatcher::peer_key(&pk), 3);
        t.restore(stale);
        assert!(!t.check_and_advance(&pk, 50), "un instantané périmé ne rouvre rien");
        assert!(t.check_and_advance(&pk, 101), "et l'avance normale continue");
    }

    /// Un ban expiré n'est pas réécrit au redémarrage : le restaurer serait une
    /// double peine, et la sanction a une durée pour une raison.
    #[test]
    fn r15_an_expired_ban_is_not_restored() {
        let mut t = NonceTracker::new();
        let mut snap = NonceTrackerSnapshot::default();
        snap.bans.insert("victime".into(), 1); // epoch 1 = expiré depuis 1970
        let (_, bans) = t.restore(snap);
        assert_eq!(bans, 0, "un ban périmé ne revient pas d'entre les morts");
    }
}
