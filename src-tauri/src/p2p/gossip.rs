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
/// **6→7 (AUDIT-2026-07-25) — remediation hard fork.** Four admission rules
/// changed, so a v6 node and a v7 node do not accept the same set of blocks and
/// envelopes:
/// - **C2** — a synthetic sender (`NETWORK`/`ESCROW`) is confined to the single
///   legitimate coinbase. v6 accepts an unsigned `Transfer` from `NETWORK` that
///   mints without limit; v7 rejects the block.
/// - **C3** — an `Unstake` is checked against the sender's bonded stake as of the
///   parent. v6 accepts one for any amount and matures it into spendable coins.
/// - **H2** — a vote attesting an epoch beyond the chain's own is refused before
///   pooling, so the pool's eviction order is no longer attacker-chosen.
/// - **H1/H3** — the gossip envelope id is the BLAKE3 of the canonical **signed**
///   pre-image (sender ‖ nonce ‖ timestamp ‖ payload), and a non-canonical id is
///   rejected. v6 computes `BLAKE3(payload)` alone, so every v6 envelope now fails
///   the id check on a v7 node.
///
/// Genesis is untouched by these rules, but the wire and validation surfaces are
/// incompatible, hence the bump.
/// **7→8 (MINT-EXACT-1) — la récompense de bloc devient une fonction pure de la
/// chaîne.** Une seule règle d'admission change, mais elle porte la politique
/// monétaire, donc un nœud v7 et un nœud v8 n'acceptent pas le même ensemble de
/// blocs :
/// - v7 bornait l'émission d'un bloc à `64 × emission_for_tick` — une marge de
///   `32 × N` au-dessus du montant honnête (`≈ 2 × emission_for_tick / N` avec N
///   nœuds vivants), parce que le montant était calculé **localement** par le
///   sceleur (part Shapley dérivée de watts auto-déclarés, invérifiables) et que
///   le réseau ne pouvait donc que le borner. À 100 nœuds, un validateur bondé
///   pouvait se frapper 3 200 fois sa récompense légitime à chaque bloc.
/// - v8 **recalcule** : `block_minted ≤ emission_for_block(offre minée avant ce
///   bloc)`, une fonction pure de la chaîne que producteur et vérificateur lisent
///   à l'identique. L'énergie auto-déclarée quitte le chemin monétaire (elle reste
///   un signal d'affichage) et l'émission réalisée cesse de s'effondrer en `1/N`
///   quand le réseau grandit.
///
/// - **OPEN-DOOR-1** — un bloc sur `OPEN_SLOT_EVERY_BLOCKS` (16) est un **slot
///   ouvert** que n'importe quelle adresse peut proposer, bondée ou non. v7
///   refusait tout proposeur non bondé dès qu'un seul compte avait staké, ce qui
///   refermait le réseau **définitivement** : sans faucet ni airdrop ni premine,
///   un nouvel arrivant n'avait aucun chemin vers sa première pièce. v8 laisse
///   passer ces blocs ; v7 les rejette. Capture Sybil bornée à 1/16 de l'émission.
///
/// **8→9 (REWARD-SHARE-1) — la récompense d'un bloc se partage.** v8 exigeait une
/// coinbase unique créditée au mineur ; v9 en accepte **une par bénéficiaire** et
/// impose la répartition : moitié au producteur, moitié à parts égales entre les
/// **participants récents** (les adresses distinctes ayant produit un bloc dans
/// les `SHARE_WINDOW_BLOCKS` précédents — une donnée que la chaîne prouve déjà via
/// `block.miner`, lié au hash par BLK-HASH-1). Chaque nœud **recalcule** le plan
/// (`validate_block_reward_plan`) : un producteur qui capte toute l'émission, ou
/// qui paie un tiers hors plan, est rejeté. Le plan est **invariant d'échelle** —
/// émettre moins reste permis, mais sans rogner la part des autres.
/// v8 rejette les blocs multi-coinbase de v9 ; v9 rejette les blocs v8 dès qu'un
/// participant récent existe. Aucun champ wire nouveau : le partage passe par des
/// tx `Mining` ordinaires.
///
/// La genèse est intacte ; c'est la surface de validation qui diverge, d'où le bump.
///
/// **9→10 (CANON-1 + NONCE-ONCHAIN-1, audit externe 2026-08-13) — rupture assumée.**
/// Deux changements, chacun suffisant à casser la compatibilité :
///
/// - **CANON-1 (CRIT-1/MOY-1)** — la préimage signée d'une transaction, la feuille
///   Merkle et l'en-tête de bloc étaient des `format!` joints par `:`/`|` sur des
///   champs libres de contenir ces séparateurs : **aucun des trois n'était
///   injectif**. Deux transactions sémantiquement différentes pouvaient partager
///   préimage, signature ET `tx.hash` — deux nœuds tenant la même chaîne, les
///   mêmes hashs de bloc, et des soldes différents. Les trois passent au modèle
///   déjà correct de `sm/finality_vote.rs` : séparateur de domaine + champs
///   préfixés en longueur. **Toute signature et tout hash changent, genèse
///   comprise.**
/// - **NONCE-ONCHAIN-1 (C-01)** — l'unicité d'une transaction n'avait aucune règle
///   on-chain : `seen_tx_hashes` ne gardait que l'admission mempool, et le chemin
///   bloc y insérait sans lire le retour. Une même transaction signée une fois
///   était incluse autant de fois que le solde de la victime le permettait. Le
///   nonce de compte est désormais **séquentiel et vérifié à l'inclusion** sur les
///   quatre chemins d'admission. Un nœud v9 accepte des blocs qu'un v10 rejette.
///
/// v9 et v10 ne peuvent pas converger : l'échange est refusé au `Hello`.
pub const TORUS_PROTOCOL_VERSION: u8 = 10;

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

/// NET-8: Decompress a gzipped `blocks_json` payload.
///
/// **R4 (AUDIT-2026-08-13)** — la borne était de 50 Mo, et elle ne portait que
/// sur les OCTETS. Un gzip de 47 Ko décompressait en un tableau JSON de centaines
/// de milliers d'éléments, chacun devenant une `String` allouée : ~366 Mo de tas
/// (×2 950) *avant* que la troncature à 50 blocs, qui arrive après le parse, ait
/// la moindre chance de servir. Deux corrections :
///
/// 1. le plafond d'octets descend à [`crate::p2p::dispatcher::MAX_RAW_ENVELOPE_BYTES`]
///    — décompresser au-delà de ce qu'une enveloppe peut légalement porter n'a
///    aucun sens ;
/// 2. le **nombre d'éléments** est borné à `MAX_CHAIN_SEGMENT_RECEIVED`, et ce
///    n'est pas une troncature après coup : un tableau plus long est REFUSÉ, donc
///    les `String` correspondantes ne sont jamais allouées.
pub fn decompress_blocks(compressed: &[u8]) -> Result<Vec<String>, String> {
    use flate2::read::GzDecoder;
    use std::io::Read;
    const MAX_DECOMPRESSED_BYTES: usize = crate::p2p::dispatcher::MAX_RAW_ENVELOPE_BYTES;
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
    let blocks = serde_json::from_slice::<Vec<String>>(&out)
        .map_err(|e| format!("decompressed JSON parse: {}", e))?;
    // R4 : la cardinalité, pas seulement les octets.
    let max = crate::p2p::dispatcher::MAX_CHAIN_SEGMENT_RECEIVED;
    if blocks.len() > max {
        return Err(format!(
            "segment décompressé : {} éléments pour un maximum de {} (R4)",
            blocks.len(),
            max
        ));
    }
    Ok(blocks)
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

// ─── R12 : bornes d'admission de la file de sortie ───────────────────────────

/// **R12 (AUDIT-2026-08-13)** — classe d'un message que le dispatcher produit
/// **en réponse** à du trafic entrant.
///
/// Les quatre variantes concernées (`Pong`, `ChainSegment`, `RequestChain`,
/// `FinalityFault`) n'ont, dans tout le nœud, qu'un seul producteur :
/// `dispatcher::broadcast`. Elles sont donc les seules dont un pair distant
/// cadence l'émission. Tout le reste part d'une horloge locale (`Hello`, `Ping`),
/// de la boucle de minage (`NewBlock`, `FinalityVote`) ou d'une action de
/// l'utilisateur (`BroadcastTx`, `PublishUsername`) — un attaquant ne peut pas en
/// accélérer la production.
///
/// La distinction Bulk/Light n'est pas cosmétique : un `ChainSegment` pèse jusqu'à
/// [`crate::p2p::dispatcher::CHAIN_SEGMENT_BYTE_BUDGET`] (3 Mio) alors qu'un `Pong`
/// pèse ~11 Ko. Une file bornée en **nombre** serait donc bornée en mémoire à
/// deux ordres de grandeur près si les deux partageaient le même compteur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EgressClass {
    /// Réponse volumineuse : un segment de chaîne.
    Bulk,
    /// Réponse courte : liveness, requête de chaîne, preuve de faute.
    Light,
}

/// R12 — classe d'admission d'un message sortant, ou `None` s'il n'est pas
/// produit en réponse à du trafic distant (voir [`EgressClass`]).
pub fn dispatcher_egress_class(payload: &GossipMessage) -> Option<EgressClass> {
    match payload {
        GossipMessage::ChainSegment { .. } => Some(EgressClass::Bulk),
        GossipMessage::Pong { .. }
        | GossipMessage::RequestChain { .. }
        | GossipMessage::FinalityFault { .. } => Some(EgressClass::Light),
        _ => None,
    }
}

/// R12 — segments de chaîne simultanément en attente d'émission. Quatre suffisent
/// à saturer un lien pendant plusieurs secondes ; au-delà, la file ne sert plus
/// qu'à stocker du travail périmé (le demandeur aura redemandé).
pub const MAX_INFLIGHT_EGRESS_BULK: usize = 4;

/// R12 — réponses courtes simultanément en attente d'émission. Large devant le
/// trafic honnête (un `Pong` par pair toutes les 15 s, quatre `RequestChain` par
/// rattrapage), étroit devant ce qu'une inondation produirait.
pub const MAX_INFLIGHT_EGRESS_LIGHT: usize = 64;

/// **R5** — délai minimal entre deux `Pong` adressés au même demandeur. Sous la
/// cadence de `Ping` (15 s), donc invisible pour un pair honnête ; au-dessus, il
/// rend l'amplification indépendante du débit de l'attaquant.
pub const PONG_COOLDOWN_SECS: i64 = 12;

/// R5 — demandeurs dont on mémorise la date du dernier `Pong`. Petit : seuls des
/// pairs **connus** (donc passés par un `Hello` authentifié) y entrent.
const MAX_PONG_TRACKED: usize = 1024;

/// **R6 (AUDIT-2026-08-13)** — l'identifiant d'enveloppe à partir de la pré-image
/// signée déjà calculée.
///
/// Même définition que [`GossipRouter::envelope_id`] (dont c'est désormais le
/// corps), exposée séparément pour que le dispatcher ne re-sérialise pas le payload
/// une seconde fois : sur le chemin d'entrée, la pré-image est déjà en main parce
/// que la signature vient d'être vérifiée dessus. Une seule définition du digest,
/// donc aucun risque de divergence entre producteur et vérificateur.
pub fn envelope_id_of_signable(signable: &[u8]) -> String {
    hex::encode(blake3::hash(signable).as_bytes())
}

// ─── Routeur gossip ─────────────────────────────────────────────────────────

/// Statistiques de gossip pour le monitoring.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct GossipStats {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub peers_reported: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    /// Messages dropped due to invalid signature
    pub dropped_signature: u64,
    /// Messages dropped due to rate limiting
    pub dropped_rate_limit: u64,
    /// Messages dropped due to nonce replay
    pub dropped_nonce: u64,
    /// V2 (audit de vie) — messages dropped because their timestamp fell outside
    /// the ±90 s freshness window.
    ///
    /// This one is a **node health signal, not an attack signal**: there is no
    /// NTP anywhere in Quanta, so a machine whose clock drifts past 90 s rejects
    /// *every* inbound envelope while still reporting itself online. Before this
    /// counter the rejection was a `log::debug!` with no tally — invisible in a
    /// release build — which made the failure undiagnosable from outside. A
    /// climbing `dropped_stale` next to a flat `messages_received` means "check
    /// this machine's clock", and nothing else produces that shape.
    #[serde(default)]
    pub dropped_stale: u64,
    /// V4 (audit de vie) — envelopes dropped because the sender announced an
    /// incompatible `TORUS_PROTOCOL_VERSION`.
    #[serde(default)]
    pub dropped_incompatible: u64,
    /// **R6 (AUDIT-2026-08-13)** — enveloppes rejetées sur la forme avant tout
    /// décodage : `sender` ou `signature` n'ont pas la longueur d'une clé/signature
    /// ML-DSA-65. C'est le seul compteur alimenté par du trafic non authentifié, et
    /// c'est voulu : la mesure du bruit doit rester possible sans rien allouer.
    #[serde(default)]
    pub dropped_malformed: u64,
    /// **R12** — réponses abandonnées faute de place dans la file de sortie.
    /// Un compteur qui monte ici veut dire « le drain réseau ne suit pas » : le
    /// nœud a préféré perdre des réponses plutôt que la mémoire.
    #[serde(default)]
    pub egress_dropped: u64,
    /// **R5** — `Pong` non émis parce que le demandeur est inconnu ou qu'il en a
    /// déjà reçu un dans la fenêtre de refroidissement.
    #[serde(default)]
    pub pongs_suppressed: u64,
    /// **R5** — `ChainSegment` non émis parce qu'un autre pair est élu pour
    /// répondre à cette requête.
    #[serde(default)]
    pub chain_answers_suppressed: u64,
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
    /// V4 (audit de vie) — senders that announced an incompatible
    /// `TORUS_PROTOCOL_VERSION`, keyed by BLAKE3 of the sender key.
    ///
    /// Hashed, not raw: an ML-DSA public key is 3904 hex chars, and H6 of the
    /// July audit was exactly this class of bug (peer maps keyed by 3.9 KB
    /// strings). Bounded by [`MAX_INCOMPATIBLE_SENDERS`] and cleared wholesale
    /// when full — this is a cheap "do not bother" hint, not security state, so
    /// losing it merely costs one more Hello before we notice again.
    incompatible_senders: std::collections::HashSet<MsgId>,
    /// V2 — unix second of the last stale-envelope warning. Runtime-only, never
    /// persisted: it only paces the log, `stats.dropped_stale` stays exact.
    last_stale_warn_unix: i64,
    /// R1 — unix second of the last bad-signature warning. Same reasoning as
    /// `last_stale_warn_unix`: this path is reached by unauthenticated traffic,
    /// so the log must be paced while `stats.dropped_signature` stays exact.
    last_bad_sig_warn_unix: i64,
    /// R12 — segments de chaîne admis dans la file de sortie et pas encore
    /// dépilés par le drain. Jamais persisté : la file est vide au démarrage.
    egress_inflight_bulk: usize,
    /// R12 — réponses courtes admises et pas encore dépilées.
    egress_inflight_light: usize,
    /// R12 — unix second du dernier avertissement de file pleine (même pacing que
    /// les deux au-dessus : le compteur est exact, le journal est rythmé).
    last_egress_warn_unix: i64,
    /// R5 — date du dernier `Pong` par demandeur, clé = BLAKE3 de sa clé publique
    /// (une clé ML-DSA fait 3 904 caractères ; H6 est exactement ce piège).
    last_pong_unix: std::collections::HashMap<MsgId, i64>,
}

/// V4 — cap on remembered incompatible senders. Small on purpose: after a fork
/// the set of stale peers is tiny, and this must never become a memory vector.
const MAX_INCOMPATIBLE_SENDERS: usize = 512;

/// V2 — at most one stale-envelope warning per this many seconds. Freshness is
/// checked *before* the signature in the dispatch pipeline, so unauthenticated
/// traffic reaches this path: an attacker replaying dated envelopes at line
/// rate must inflate a counter, never our own log.
const STALE_WARN_EVERY_SECS: i64 = 30;

impl GossipRouter {
    pub fn new() -> Self {
        Self {
            seen_messages: std::collections::HashSet::new(),
            seen_order: std::collections::VecDeque::new(),
            stats: GossipStats::default(),
            outgoing_nonce: AtomicU64::new(1),
            incompatible_senders: std::collections::HashSet::new(),
            last_stale_warn_unix: 0,
            last_bad_sig_warn_unix: 0,
            egress_inflight_bulk: 0,
            egress_inflight_light: 0,
            last_egress_warn_unix: 0,
            last_pong_unix: std::collections::HashMap::new(),
        }
    }

    /// R1 — say whether this bad-signature drop has earned a log line (at most
    /// one per [`STALE_WARN_EVERY_SECS`]). Does **not** touch the counter: the
    /// caller owns `stats.dropped_signature` so the count stays exact under a
    /// flood. Verification failure is now a silent, cheap drop — it emits no
    /// `ReportPeer` and no signature of ours, so the log line is the only
    /// remaining per-message cost and it is paced.
    pub fn note_bad_signature_drop(&mut self) -> bool {
        let now = chrono::Utc::now().timestamp();
        if now - self.last_bad_sig_warn_unix >= STALE_WARN_EVERY_SECS {
            self.last_bad_sig_warn_unix = now;
            true
        } else {
            false
        }
    }

    /// V2 — record one stale-envelope drop and say whether this occurrence has
    /// earned a log line (at most one per [`STALE_WARN_EVERY_SECS`]). The
    /// counter is the diagnostic; the log line is only its narrator, and it must
    /// stay readable under a flood the counter is precisely there to measure.
    pub fn note_stale_drop(&mut self) -> bool {
        self.stats.dropped_stale += 1;
        let now = chrono::Utc::now().timestamp();
        if now - self.last_stale_warn_unix >= STALE_WARN_EVERY_SECS {
            self.last_stale_warn_unix = now;
            true
        } else {
            false
        }
    }

    /// **R12 (AUDIT-2026-08-13) — la file de sortie était non bornée.**
    ///
    /// Les quatre lanes de `gossip_priority` sont des `mpsc::unbounded_channel`, et
    /// le commentaire du module justifiait ce choix par « la protection anti-DoS
    /// vit au niveau du dispatcher ». Elle n'y vivait pas : `Pong`, `ChainSegment`,
    /// `RequestChain` et `FinalityFault` sont produits **en réponse** à du trafic
    /// distant, donc à la cadence de l'attaquant, alors que le drain
    /// (`gossip_tasks::spawn_outgoing_drain`) émet message par message en
    /// `await`ant le réseau. Dès que le drain descend sous le producteur, la file
    /// grandit sans limite — ~11 Ko l'entrée pour un `Pong`, jusqu'à 3 Mio pour un
    /// `ChainSegment`.
    ///
    /// La borne est posée ici, à l'**admission**, et non en rendant le canal
    /// bloquant : bloquer le producteur bloquerait la boucle de dispatch, donc la
    /// **lecture réseau** elle-même — on aurait échangé une bombe mémoire contre un
    /// blocage complet du nœud sous inondation. La politique est explicite :
    /// au-delà de la borne on **jette la réponse**, on compte
    /// ([`GossipStats::egress_dropped`]) et on le dit (journal rythmé). Perdre un
    /// `Pong` coûte une mesure de latence ; perdre un `ChainSegment` coûte un tour
    /// de synchronisation, que le demandeur relance de lui-même.
    ///
    /// Retourne `true` si le message peut être mis en file.
    pub fn try_admit_egress(&mut self, payload: &GossipMessage) -> bool {
        let Some(class) = dispatcher_egress_class(payload) else {
            // Cadence locale (Hello/Ping/NewBlock/tx utilisateur) : aucun pair
            // distant ne peut l'accélérer, donc rien à borner ici.
            return true;
        };
        let (inflight, max) = match class {
            EgressClass::Bulk => (&mut self.egress_inflight_bulk, MAX_INFLIGHT_EGRESS_BULK),
            EgressClass::Light => (&mut self.egress_inflight_light, MAX_INFLIGHT_EGRESS_LIGHT),
        };
        if *inflight >= max {
            self.stats.egress_dropped += 1;
            return false;
        }
        *inflight += 1;
        true
    }

    /// R12 — un message vient d'être dépilé par le drain : il ne consomme plus de
    /// place. Appelé **au moment du dépilage**, pas après l'émission réseau : la
    /// borne compte ce qui est en file, pas ce qui est en vol.
    pub fn note_egress_drained(&mut self, payload: &GossipMessage) {
        self.release_egress_slot(dispatcher_egress_class(payload));
    }

    /// R12 — rendre une place réservée dont le message n'atteindra jamais la file
    /// (échec de signature, canal fermé). Sans ce chemin la borne se refermerait
    /// définitivement, message après message.
    pub fn release_egress_slot(&mut self, class: Option<EgressClass>) {
        match class {
            Some(EgressClass::Bulk) => {
                self.egress_inflight_bulk = self.egress_inflight_bulk.saturating_sub(1)
            }
            Some(EgressClass::Light) => {
                self.egress_inflight_light = self.egress_inflight_light.saturating_sub(1)
            }
            None => {}
        }
    }

    /// R12 — ce refus a-t-il mérité une ligne de journal ? Même raison que pour les
    /// deux compteurs de rejet au-dessus : la saturation est justement l'état où le
    /// journal ne doit pas devenir la deuxième bombe.
    pub fn note_egress_drop_warn(&mut self) -> bool {
        let now = chrono::Utc::now().timestamp();
        if now - self.last_egress_warn_unix >= STALE_WARN_EVERY_SECS {
            self.last_egress_warn_unix = now;
            true
        } else {
            false
        }
    }

    /// R12 — profondeur courante de la file de sortie (bulk, light), pour les tests
    /// et le diagnostic.
    pub fn egress_inflight(&self) -> (usize, usize) {
        (self.egress_inflight_bulk, self.egress_inflight_light)
    }

    /// **R5 (AUDIT-2026-08-13)** — un `Pong` par demandeur et par
    /// [`PONG_COOLDOWN_SECS`].
    ///
    /// Toute réponse part en diffusion sur le topic entier (il n'existe aucun
    /// chemin unicast ici), donc un `Ping` reçu par N nœuds fait émettre N `Pong`
    /// lus N fois : l'amplification est en O(N²) et son facteur est le **débit de
    /// pings de l'attaquant**. Le refroidissement casse ce dernier lien : sous la
    /// cadence honnête (un `Ping` toutes les 15 s), aucun `Pong` légitime n'est
    /// perdu ; au-dessus, l'attaquant n'obtient plus rien de plus en frappant plus
    /// vite.
    ///
    /// La table est bornée et **échoue fermé** quand elle est pleine : perdre un
    /// `Pong` ne coûte qu'une mesure de latence, alors qu'une table de
    /// refroidissement contournable coûterait l'amplification entière.
    pub fn may_answer_ping(&mut self, sender_pk: &str, now_unix: i64) -> bool {
        let key = hex::encode(blake3::hash(sender_pk.as_bytes()).as_bytes());
        if let Some(&last) = self.last_pong_unix.get(&key) {
            if now_unix - last < PONG_COOLDOWN_SECS {
                return false;
            }
            self.last_pong_unix.insert(key, now_unix);
            return true;
        }
        if self.last_pong_unix.len() >= MAX_PONG_TRACKED {
            // Purge des entrées dont le refroidissement est de toute façon écoulé.
            self.last_pong_unix
                .retain(|_, last| now_unix - *last < PONG_COOLDOWN_SECS);
            if self.last_pong_unix.len() >= MAX_PONG_TRACKED {
                return false;
            }
        }
        self.last_pong_unix.insert(key, now_unix);
        true
    }

    /// CRIT-A: Get and increment the outgoing nonce (atomic, no &mut needed).
    ///
    /// **V1 (audit de vie, 2026-08-01) — the counter is seeded from wall-clock
    /// microseconds, never from 1.**
    ///
    /// A pure in-memory counter persisted only by the 30 s snapshot could
    /// *regress*: peers reject any nonce ≤ the high-water mark they remember
    /// (`dispatcher::check_and_advance`), and that memory is only pruned once
    /// 100 000 senders are tracked — i.e. never, at real network size. A brutal
    /// crash cost a few rejected messages; restoring a wallet on another machine
    /// (RECOVER-1 — same ML-DSA sender key, fresh database) restarted the counter
    /// at 1 against a peer holding a mark in the thousands, and **every message
    /// we sent was silently dropped** until that peer itself restarted.
    ///
    /// Seeding from the clock removes the failure by construction: every nonce
    /// we emit is at least the clock at emission time and overshoots it by at
    /// most the burst width (see the exact guarantee below), so a restart —
    /// with or without a database — resumes strictly above within microseconds.
    /// The stored value is still honoured (`restore`), and the atomic
    /// max-then-increment keeps nonces strictly increasing when several
    /// messages land in the same microsecond or when the clock steps backwards.
    ///
    /// Wire-compatible: peers only ever compare `nonce > high_water`, so a jump
    /// from a small counter to an epoch-micros value is accepted by existing
    /// nodes. No protocol bump.
    ///
    /// **Exact guarantee, and its bound.** Each emitted nonce is ≥ the clock at
    /// emission time. A burst still overshoots the clock by the number of nonces
    /// handed out inside one microsecond, so a restart is only guaranteed to
    /// resume above once that many microseconds have elapsed. Quanta emits a
    /// handful of envelopes per minute and a restart takes seconds, so the
    /// overshoot is nil in practice — and microsecond resolution would require
    /// a million messages per second to make it otherwise. A restored snapshot
    /// still raises the floor further (`restore`), so the only case relying on
    /// the clock alone is a lost database.
    pub fn next_outgoing_nonce(&self) -> u64 {
        let now_us = chrono::Utc::now().timestamp_micros().max(1) as u64;
        // Raise the floor to the clock, then hand out the next value. `fetch_add`
        // still gives every concurrent caller a distinct, strictly increasing
        // nonce; `fetch_max` only ever moves the counter up, so a backwards clock
        // step cannot make it regress.
        self.outgoing_nonce.fetch_max(now_us, Ordering::Relaxed);
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
        envelope_id_of_signable(&full)
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

    // **R14 (AUDIT-2026-08-13) — `payload_bytes`, `wrap_outgoing` et
    // `wrap_outgoing_with_nonce` ont été SUPPRIMÉS. Ne les recréez pas.**
    //
    // Les trois étaient du code mort : aucun appelant, ni en production ni en
    // test. Ce n'était pas un contournement — les enveloppes que `wrap_outgoing`
    // produisait étaient rejetées deux fois par le dispatcher (identifiant calculé
    // sur le payload seul au lieu de la préimage signée, donc `validate_envelope_id`
    // échouait ; et `nonce: 0` figé, donc l'anti-rejeu échouait aussi). C'était un
    // **piège** : trois fonctions publiques, documentées, portant les noms les plus
    // évidents du module, qu'un développeur pressé aurait appelées en croyant faire
    // ce qu'il fallait. Elles auraient produit des enveloppes silencieusement
    // ignorées par tout le réseau.
    //
    // L'API correcte est [`Self::build_signed_envelope`], seule construction
    // d'enveloppe du binaire : elle dérive l'identifiant de la préimage signée
    // (H3) et exige le nonce anti-rejeu à l'appel. La signature doit couvrir
    // [`Self::signable_envelope_bytes`].

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
            // Not persisted on purpose: after an upgrade our own version has
            // changed, so yesterday's "incompatible" verdicts are stale. Starting
            // empty re-evaluates every peer on its next Hello.
            incompatible_senders: std::collections::HashSet::new(),
            last_stale_warn_unix: 0,
            last_bad_sig_warn_unix: 0,
            // R12 — la file de sortie est vide au démarrage : un compteur restauré
            // serait un plafond consommé par une exécution précédente, donc une
            // panne d'émission permanente.
            egress_inflight_bulk: 0,
            egress_inflight_light: 0,
            last_egress_warn_unix: 0,
            last_pong_unix: std::collections::HashMap::new(),
        }
    }

    /// V4 — remember a sender as protocol-incompatible.
    ///
    /// Returns `true` the first time a given sender is marked, so the caller can
    /// log once per peer instead of once per message.
    pub fn mark_incompatible(&mut self, sender_pk: &str) -> bool {
        if self.incompatible_senders.len() >= MAX_INCOMPATIBLE_SENDERS {
            self.incompatible_senders.clear();
        }
        let key = hex::encode(blake3::hash(sender_pk.as_bytes()).as_bytes());
        self.incompatible_senders.insert(key)
    }

    /// V4 — has this sender already announced an incompatible protocol version?
    pub fn is_incompatible(&self, sender_pk: &str) -> bool {
        let key = hex::encode(blake3::hash(sender_pk.as_bytes()).as_bytes());
        self.incompatible_senders.contains(&key)
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// **R14 (AUDIT-2026-08-13) — le piège a été retiré, qu'il ne repousse pas.**
    ///
    /// `wrap_outgoing` / `wrap_outgoing_with_nonce` / `payload_bytes` étaient trois
    /// fonctions publiques, documentées, portant les noms les plus évidents du
    /// module — et sans un seul appelant. Les enveloppes qu'elles produisaient
    /// étaient rejetées deux fois par le dispatcher (identifiant dérivé du payload
    /// seul, `nonce: 0` figé). Un développeur pressé les aurait appelées en croyant
    /// bien faire, et aurait émis des messages que tout le réseau ignore en
    /// silence — la pire classe de panne : celle qui ne lève aucune erreur.
    ///
    /// Ce test verrouille l'unique constructeur restant sur la propriété qui
    /// manquait aux fonctions supprimées : l'identifiant est celui de la préimage
    /// **signée**, et le nonce est celui qu'on lui passe.
    #[test]
    fn r14_the_only_envelope_builder_binds_the_id_to_the_signed_preimage() {
        let sender = "aa".repeat(32);
        let payload = GossipMessage::Ping { nonce: 7 };
        let ts = "2027-01-01T00:00:00+00:00".to_string();

        let env = GossipRouter::build_signed_envelope(
            sender.clone(),
            payload.clone(),
            42,
            ts.clone(),
            &[0u8; 8],
        )
        .expect("l'enveloppe se construit");

        assert_eq!(env.nonce, 42, "le nonce anti-rejeu est celui demandé, jamais 0");
        assert_eq!(
            env.id,
            GossipRouter::envelope_id(&sender, 42, &ts, &payload),
            "l'identifiant dérive de la préimage SIGNÉE (sender+nonce+ts+payload), pas du payload seul"
        );
        // La propriété qui manquait : deux enveloppes de même payload mais de
        // nonce différent ne partagent PAS un créneau de déduplication.
        let other = GossipRouter::build_signed_envelope(sender, payload, 43, ts, &[0u8; 8])
            .expect("seconde enveloppe");
        assert_ne!(env.id, other.id, "le nonce doit séparer les identifiants");
    }


    #[test]
    fn v1_nonce_is_clock_seeded_so_a_stateless_restart_resumes_above() {
        // The failure this closes: restoring a wallet on a new machine (same
        // ML-DSA sender key, empty database) restarted the counter at 1 while
        // peers still remembered a high-water mark in the thousands, so every
        // message we sent was dropped by `check_and_advance` — silently, and
        // until that peer itself restarted.
        //
        // The property that actually delivers this is: **every nonce is at least
        // the clock at emission time**. A restart therefore resumes above any
        // nonce emitted more than a burst-width earlier — and a restart takes
        // seconds while a burst is microseconds. Asserting the property directly
        // (rather than restarting inside the same microsecond, which is not a
        // real scenario) is what makes this test meaningful instead of flaky.
        let router = GossipRouter::new();
        for _ in 0..64 {
            let floor = chrono::Utc::now().timestamp_micros() as u64;
            let nonce = router.next_outgoing_nonce();
            assert!(
                nonce >= floor,
                "nonce {nonce} sous l'horloge {floor} : un redémarrage sans état régresserait"
            );
        }

        // And a stateless restart lands in the same epoch-micros range rather
        // than back at 1 — the concrete regression that broke wallet restore.
        let after_restart = GossipRouter::new().next_outgoing_nonce();
        assert!(
            after_restart > 1_700_000_000_000_000,
            "un nœud reparti sans état doit repartir de l'horloge, pas de 1 (got {after_restart})"
        );
    }

    #[test]
    fn v1_nonce_is_strictly_increasing_within_one_millisecond() {
        // Wall-clock seeding must not cost strict monotonicity: several messages
        // in the same millisecond still need distinct, increasing nonces, or a
        // peer would reject all but the first.
        let router = GossipRouter::new();
        let nonces: Vec<u64> = (0..1000).map(|_| router.next_outgoing_nonce()).collect();
        for pair in nonces.windows(2) {
            assert!(pair[1] > pair[0], "nonce non strictement croissant: {} → {}", pair[0], pair[1]);
        }
    }

    #[test]
    fn v1_nonce_survives_a_restore_from_an_old_snapshot() {
        // Legacy snapshots carry a small counter (they predate wall-clock
        // seeding). Restoring one must not drag us back below the clock.
        let restored = GossipRouter::restore(GossipRouterSnapshot {
            seen_messages: Default::default(),
            seen_order: Default::default(),
            stats: GossipStats::default(),
            outgoing_nonce: 7,
        });
        assert!(
            restored.next_outgoing_nonce() > 7,
            "un vieux compteur ne doit pas rouvrir une fenêtre de nonces déjà consommés"
        );
    }

    #[test]
    fn v4_incompatible_sender_is_remembered_once_and_bounded() {
        let mut router = GossipRouter::new();
        assert!(router.mark_incompatible("peer-a"), "premier marquage → true (log une fois)");
        assert!(!router.mark_incompatible("peer-a"), "second marquage → false (pas de spam)");
        assert!(router.is_incompatible("peer-a"));
        assert!(!router.is_incompatible("peer-b"), "un pair sain n'est jamais marqué");

        // The set must never grow without bound, even under a flood of distinct
        // (authenticated) senders announcing junk versions.
        for i in 0..MAX_INCOMPATIBLE_SENDERS * 2 {
            router.mark_incompatible(&format!("flood-{i}"));
        }
        assert!(
            router.incompatible_senders.len() <= MAX_INCOMPATIBLE_SENDERS,
            "ensemble non borné: {}",
            router.incompatible_senders.len()
        );
    }

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
