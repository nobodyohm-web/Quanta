//! `Node` — the deterministic, synchronous state-machine core (harness T0.1).
//!
//! [`Node::handle`] is the single entry point and a **pure function** of
//! `(state, Event, &mut Rng)`: no I/O, no system-clock read, no `OsRng`. Time
//! enters via [`Event::Tick`]; the core's only notion of "now" is
//! `self.now_ms`.
//!
//! Build status (T0.1 landed in verifiable slices — extraction, not rewrite):
//! - `Tick` advances virtual time and prunes the mempool deterministically via
//!   [`Ledger::prune_mempool_at`] (injected time, never the wall clock).
//! - `MessageReceived` runs the stateless security gate
//!   ([`crate::p2p::dispatcher::validate_envelope_at`], injected-time
//!   freshness) and admits a verified `BroadcastTx` to the ledger through the
//!   shared [`Ledger::apply_remote_tx_checked`] path.
//! - `Command` and the remaining gossip payloads (Hello, chain sync, NewBlock,
//!   Ping/Pong, …) migrate in later slices; until then the `p2p` modules remain
//!   the source of truth.

use super::{
    effect::Effect,
    event::{Event, PeerId, TimerId},
    finality::{FinalizedSet, EPOCH_LENGTH_BLOCKS},
    finality_rule::{FinalityState, StepOutcome},
    finality_vote::FinalityCertificate,
    fork_choice::{anchors, ghost_head, BlockTree, LatestVotes},
    rng::Rng,
};
use crate::{
    p2p::{
        gossip::{GossipMessage, GossipRouter},
        ledger::{Block, Ledger, Transaction},
        pos_consensus::{self, Validator},
    },
    security::CryptoEngine,
};
use std::collections::{HashMap, HashSet};

/// Inspectable outcomes of consensus-relevant inbound messages (C3).
///
/// Each `MessageReceived` that touches consensus updates exactly one counter,
/// so the harness can assert *what the core decided* — e.g. "this byzantine
/// block was **rejected**" — at the core boundary, instead of inferring it from
/// `chain.len()`. Purely additive: it never changes what the core sends, and
/// holds only neutral statuses (counts), never sensitive state. Deterministic,
/// so it stays reproducible alongside the rest of the core.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConsensusTelemetry {
    /// Remote blocks that were new and extended/reorged our chain (`Ok(true)`).
    pub blocks_integrated: u64,
    /// Remote blocks already known — no-op duplicates (`Ok(false)`).
    pub blocks_duplicate: u64,
    /// Remote blocks refused by validation/fork rules (`Err(_)`).
    pub blocks_rejected: u64,
    /// Remote txs newly admitted to the ledger.
    pub txs_admitted: u64,
    /// Remote txs NOT applied — bad signature, stale nonce, OR duplicate.
    /// (The admission path returns a single bool, so these are not split.)
    pub txs_dropped: u64,
}

/// The deterministic state-machine core.
pub struct Node {
    ledger: Ledger,
    /// Latest virtual time (ms since the Unix epoch) delivered via
    /// [`Event::Tick`]. The core never reads the system clock (Constitution §3)
    /// — this is its only clock, and it advances monotonically.
    now_ms: u64,
    /// Local signing identity, or `None` for an observer that cannot emit
    /// signed messages. Ed25519 signing is deterministic, so a seed-derived
    /// identity makes the core's outgoing traffic reproducible (harness
    /// §3.3).
    identity: Option<CryptoEngine>,
    /// Monotonic per-node outgoing gossip nonce (anti-replay on what we send).
    out_nonce: u64,
    /// C3: inspectable outcomes of consensus decisions (additive
    /// observability).
    telemetry: ConsensusTelemetry,
    /// T0.4 tr.2: the validator set this node elects against, held in-core so
    /// proposal can be driven by the consensus timer (not an orchestrated
    /// kick). **Transitional**: per ADR-002 (stake on-chain seul) this
    /// becomes an epoch-derived on-chain-stake snapshot; until staking
    /// lands, the shell / simulator sets it via [`Node::set_validators`].
    validators: Vec<Validator>,
    /// Whether the periodic seal timer has been armed (armed once on the first
    /// `Tick` of an identity-bearing node; re-armed on each fire).
    seal_timer_armed: bool,
    /// The virtual time the seal timer is scheduled for. Re-arming advances
    /// this by `SEAL_INTERVAL_MS` **monotonically** (NOT `now_ms +
    /// interval`): between ticks `now_ms` is frozen, so anchoring the next
    /// fire to `now_ms` would reschedule the same instant forever — a tight
    /// loop in the simulator.
    next_seal_at_ms: u64,
    /// **GADGET-1 → GADGET-3**: this node's finality state — the justified set
    /// (GADGET-3 §1) alongside the finalized set (GADGET-1), both init
    /// `{genesis}` (genesis is justified **and** finalized by definition,
    /// `DESIGN-FINALITY-GADGET` §4). GADGET-3's justify/finalize rule now grows
    /// it via [`Node::apply_finality_certificate`], so the harness
    /// finality-safety invariant (which reads [`Node::finalized`]) guards **real**
    /// finalized history, not just genesis. The rule is a pure function of
    /// (certificate, stake snapshot) — no clock, no entropy — so the `sm/`
    /// sans-IO / C1 determinism guarantee is untouched.
    finality: FinalityState,
}

/// Consensus seal cadence (ms). Mirrors production
/// `SEAL_EVERY_N_TICKS (2) × MINE_INTERVAL_SECS (60)` = 2 min.
const SEAL_INTERVAL_MS: u64 = 120_000;
/// Reserved timer id for the periodic block-seal timer. Distinct from any id a
/// test or other subsystem uses, so unrelated `TimerFired`s stay inert.
const SEAL_TIMER_ID: TimerId = TimerId(0xFFFF_FFFF_FFFF_FFF0);

impl Node {
    /// New observer node: fresh genesis ledger, virtual time 0, no signing
    /// identity (cannot emit signed messages).
    pub fn new() -> Self {
        let ledger = Ledger::new();
        let finality = Self::genesis_finality(&ledger);
        Self {
            ledger,
            now_ms: 0,
            identity: None,
            out_nonce: 0,
            telemetry: ConsensusTelemetry::default(),
            validators: Vec::new(),
            seal_timer_armed: false,
            next_seal_at_ms: 0,
            finality,
        }
    }

    /// GADGET-1/3: the genesis-only finality state for a given ledger — its
    /// block 0 hash, justified **and** finalized by definition (design §4).
    /// Panic-free: a fresh/restored ledger always carries genesis at position 0,
    /// but we read it via `first()` so a malformed empty chain degrades to an
    /// empty hash, never a panic (Constitution Rust rule 2).
    fn genesis_finality(ledger: &Ledger) -> FinalityState {
        FinalityState::genesis_only(
            ledger
                .chain
                .first()
                .map(|b| b.hash.clone())
                .unwrap_or_default(),
        )
    }

    /// Wrap an existing ledger (e.g. one restored from a snapshot by the
    /// shell). Observer — no signing identity.
    pub fn from_ledger(ledger: Ledger) -> Self {
        let finality = Self::genesis_finality(&ledger);
        Self {
            ledger,
            now_ms: 0,
            identity: None,
            out_nonce: 0,
            telemetry: ConsensusTelemetry::default(),
            validators: Vec::new(),
            seal_timer_armed: false,
            next_seal_at_ms: 0,
            finality,
        }
    }

    /// New node that can SIGN outgoing messages with `identity` as its local
    /// wallet/validator key. The production shell passes the real engine; the
    /// simulation shell passes a seed-derived one so the whole run is
    /// replayable.
    pub fn with_identity(identity: CryptoEngine) -> Self {
        let ledger = Ledger::new();
        let finality = Self::genesis_finality(&ledger);
        Self {
            ledger,
            now_ms: 0,
            identity: Some(identity),
            out_nonce: 0,
            telemetry: ConsensusTelemetry::default(),
            validators: Vec::new(),
            seal_timer_armed: false,
            next_seal_at_ms: 0,
            finality,
        }
    }

    /// Current virtual time (ms since epoch) as last advanced by a `Tick`.
    pub fn now_ms(&self) -> u64 {
        self.now_ms
    }

    /// C3: read-only view of the core's consensus-decision outcomes, so the
    /// harness can assert whether a given message was integrated / rejected /
    /// admitted / dropped at the core boundary.
    pub fn telemetry(&self) -> &ConsensusTelemetry {
        &self.telemetry
    }

    /// Read-only view of the owned ledger (balances, chain, totals).
    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    /// Mutable access to the owned ledger.
    ///
    /// Transitional: later T0.1 slices route all mutations through `handle`
    /// (via `Event::Command` / `Event::MessageReceived`). Until those land,
    /// the shell and tests seed ledger state through this accessor.
    pub fn ledger_mut(&mut self) -> &mut Ledger {
        &mut self.ledger
    }

    /// **GADGET-1**: read-only view of this node's finalized checkpoint set, so
    /// the harness finality-safety invariant can assert that no two nodes hold a
    /// conflicting finalized checkpoint at the same epoch. Init genesis-only;
    /// **GADGET-3**'s justify/finalize rule grows it past genesis via
    /// [`Node::apply_finality_certificate`], so the invariant now guards real
    /// finalized history.
    pub fn finalized(&self) -> &FinalizedSet {
        self.finality.finalized()
    }

    /// **GADGET-3**: read-only view of this node's justified checkpoint set (the
    /// state the justify step grows; a checkpoint is justified before it can be
    /// finalized).
    pub fn justified(&self) -> &super::finality_rule::JustifiedSet {
        self.finality.justified()
    }

    /// **GADGET-3 — the justify/finalize rule.** Apply one super-majority
    /// certificate (a ⅔ link `source → target`, GADGET-2) through the two-step
    /// rule, advancing this node's justified / finalized sets. `stakes` is the
    /// on-chain stake snapshot the certificate is weighed against; its provenance
    /// (re-keying [`Ledger::validator_stakes`] to the finality-vote identity) is
    /// the open ADR-002 / identity reconciliation flagged in GADGET-2 §4, so it
    /// is **supplied by the caller** rather than read here, and no live gossip
    /// path is wired yet (mirroring GADGET-2). Pure — no clock, no entropy.
    /// Returns what the step advanced.
    pub fn apply_finality_certificate<C: FinalityCertificate>(
        &mut self,
        cert: &C,
        stakes: &HashMap<String, u64>,
    ) -> StepOutcome {
        self.finality
            .apply_certificate(cert, stakes, EPOCH_LENGTH_BLOCKS)
    }

    /// **GADGET-1 — test injection only.** Record a checkpoint as finalized
    /// **without any justification check**, bypassing the real
    /// [`Node::apply_finality_certificate`] rule. Exists purely so the harness
    /// teeth test can **plant** a cross-node conflict and prove the
    /// finality-safety invariant bites. `#[cfg(test)]` keeps it out of the
    /// production surface.
    #[cfg(test)]
    pub(crate) fn record_finalized_for_test(&mut self, cp: super::finality::Checkpoint) {
        self.finality.record_finalized_for_test(cp);
    }

    /// T0.4 tr.2: set the validator set this node elects against (transitional
    /// — see the field doc / ADR-002). The simulator/shell calls this;
    /// later it is derived from an on-chain stake snapshot per epoch.
    pub fn set_validators(&mut self, validators: Vec<Validator>) {
        self.validators = validators;
    }

    /// THE deterministic entry point. Pure function of `(state, Event, rng)`:
    /// no I/O, no clock read, no `OsRng`. Returns the [`Effect`]s the shell
    /// must carry out.
    pub fn handle(&mut self, event: Event, _rng: &mut dyn Rng) -> Vec<Effect> {
        match event {
            Event::Tick { now_ms } => self.on_tick(now_ms),
            Event::MessageReceived { from, bytes } => self.on_message(from, bytes),
            // The periodic seal timer drives event-driven block production; any
            // other timer id is inert. `Command` migrates in a later slice.
            Event::TimerFired { id } if id == SEAL_TIMER_ID => self.on_seal_timer(),
            Event::Command(_) | Event::TimerFired { .. } => Vec::new(),
        }
    }

    /// Validate an inbound raw message through the **stateless security gate**
    /// (size → JSON decode → freshness vs *injected* time → Ed25519 signature)
    /// and act on it.
    ///
    /// Raw bytes are never trusted: anything that fails the gate is dropped
    /// (Constitution §3), and the gate never panics on hostile input. A
    /// verified `BroadcastTx` is admitted to the authoritative ledger via
    /// the shared [`Ledger::apply_remote_tx_checked`] path; the other
    /// gossip payloads are migrated in later T0.1 slices.
    fn on_message(&mut self, from: PeerId, bytes: Vec<u8>) -> Vec<Effect> {
        let now_secs = (self.now_ms / 1_000) as i64;
        // Stateless security gate first — raw bytes are never trusted.
        let Ok(env) = crate::p2p::dispatcher::validate_envelope_at(&bytes, now_secs) else {
            return Vec::new(); // drop untrusted / stale / malformed input
        };
        match env.payload {
            // A verified transaction is admitted to the authoritative linear
            // ledger via the shared `apply_remote_tx_checked` path. C3: record
            // whether it was newly admitted or dropped (bad sig/nonce/dup).
            GossipMessage::BroadcastTx { tx_json } => {
                let admitted = serde_json::from_str::<Transaction>(&tx_json)
                    .map(|tx| self.ledger.apply_remote_tx_checked(tx))
                    .unwrap_or(false);
                if admitted {
                    self.telemetry.txs_admitted += 1;
                } else {
                    self.telemetry.txs_dropped += 1;
                }
                Vec::new()
            }
            // Liveness: answer a Ping with a signed Pong (first outgoing Effect).
            GossipMessage::Ping { nonce } => self.on_ping(nonce),
            // A sealed block is validated + integrated via the consensus-critical
            // `integrate_remote_block` — the single path shared with the
            // production dispatcher (all checks + fork resolution inside). C3:
            // the integrate/duplicate/reject outcome is recorded so the harness
            // can assert it at the core boundary (network behaviour unchanged).
            GossipMessage::NewBlock { block_json } => {
                match serde_json::from_str::<Block>(&block_json)
                    .map_err(|e| e.to_string())
                    .and_then(|block| self.ledger.integrate_remote_block(block))
                {
                    Ok(true) => self.telemetry.blocks_integrated += 1,
                    Ok(false) => self.telemetry.blocks_duplicate += 1,
                    Err(_) => self.telemetry.blocks_rejected += 1,
                }
                Vec::new()
            }
            // Chain sync — a lagging/late peer asks for blocks from a height; we
            // answer with a **targeted** signed `ChainSegment` (the core's first
            // `Effect::Send`). Mirrors `handle_request_chain` (inline, paginated,
            // max 50). A node with no identity can't sign → no reply.
            GossipMessage::RequestChain {
                from_height,
                max_blocks,
            } => self.on_request_chain(from, from_height, max_blocks),
            // Integrate a received segment (paginated catch-up). Inline path only
            // (the core emits uncompressed); NET-8 gzip decode stays in the shell.
            GossipMessage::ChainSegment { blocks_json, .. } => {
                self.on_chain_segment(blocks_json);
                Vec::new()
            }
            // Remaining payloads (Hello, PublishUsername, …) migrate in later
            // slices.
            _ => Vec::new(),
        }
    }

    /// Chain-sync responder: serialize blocks `[from_height, …)` (capped at 50)
    /// and return them as a single **targeted** signed `ChainSegment` to
    /// `requester`. Pure extraction of `handle_request_chain` (inline form). No
    /// blocks to send, or no signing identity → no effect.
    fn on_request_chain(
        &mut self,
        requester: PeerId,
        from_height: u64,
        max_blocks: u64,
    ) -> Vec<Effect> {
        const MAX_CHAIN_SEGMENT: u64 = 50;
        let limit = max_blocks.min(MAX_CHAIN_SEGMENT) as usize;
        let chain_len = self.ledger.chain_height();
        let blocks_json: Vec<String> = (from_height..chain_len)
            .take(limit)
            .filter_map(|i| {
                self.ledger
                    .block_at(i)
                    .and_then(|b| serde_json::to_string(b).ok())
            })
            .collect();
        if blocks_json.is_empty() {
            return Vec::new();
        }
        let msg = GossipMessage::ChainSegment {
            blocks_json,
            sender_height: chain_len,
            blocks_compressed: None,
        };
        self.sign_send(requester, msg).into_iter().collect()
    }

    /// Chain-sync requester side: integrate a received segment block-by-block
    /// via the shared `integrate_remote_block`. DoS-capped at 50. A block that
    /// won't extend our chain linearly (a **competing fork** off some common
    /// ancestor, the partition-heal case) is **not** rejected outright — it is
    /// held and, once the whole segment is read, handed to the GADGET-5B GHOST
    /// reconciliation ([`Node::reconcile_fork`]). A genuine *gap* (blocks rooted
    /// at a block we don't hold) never forms a valid fork, so it stays a no-op —
    /// AUDIT-SYNC-1's "don't waste effort on an unconnectable block" is preserved
    /// by reconciliation declining it, not by a hard `break`. Outcomes feed C3
    /// telemetry.
    fn on_chain_segment(&mut self, blocks_json: Vec<String>) {
        const MAX_CHAIN_SEGMENT_RECEIVED: usize = 50;
        // Keep EVERY well-formed block of the segment (not only the ones that fail
        // linear integration): a competing fork's tip can be *swallowed* by the
        // single-block fork path (it returns `Ok(false)` "keep ours" when its hash
        // is the lower one), so reconstructing the fork from `Err`s alone would miss
        // it. The full set feeds the GHOST union tree below.
        let mut received: Vec<Block> = Vec::new();
        let mut forked = false;
        for block_str in blocks_json.into_iter().take(MAX_CHAIN_SEGMENT_RECEIVED) {
            match serde_json::from_str::<Block>(&block_str) {
                Ok(block) => {
                    received.push(block.clone());
                    match self.ledger.integrate_remote_block(block) {
                        Ok(true) => self.telemetry.blocks_integrated += 1,
                        Ok(false) => self.telemetry.blocks_duplicate += 1,
                        // Couldn't extend linearly → a competing fork (or a gap):
                        // hand the whole segment to GHOST reconciliation rather than
                        // rejecting outright (GADGET-5B).
                        Err(_) => forked = true,
                    }
                }
                // A malformed entry is dropped; the rest of the (bounded) segment is
                // still read, so one bad block can't hide a reconcilable fork.
                Err(_) => self.telemetry.blocks_rejected += 1,
            }
        }
        if forked {
            self.reconcile_fork(received);
        }
    }

    /// **GADGET-5B — partition reconciliation via the GADGET-5A GHOST engine.**
    /// At partition heal, build the **union block tree** (local chain ∪ the
    /// `competing` fork received over sync), run the finality-anchored GHOST head
    /// — anchored at the last **justified** checkpoint, floored at the last
    /// **finalized** (absolute) — and, when it lands on the competing fork,
    /// reorganize the ledger to it with full conservation
    /// ([`Ledger::reorg_to_fork`]: the abandoned branch's emission/state reverted,
    /// its user txs re-queued, its rewards never re-minted).
    ///
    /// **Determinism (C1).** Pure: no clock, no entropy, and **no map iteration in
    /// the verdict** — the tree is built from the ordered chain/segment, the head
    /// from the engine's `BTreeMap`/`BTreeSet` (smallest-hash tie-break), the
    /// winning chain reconstructed by a **bounded** parent walk. So two healed
    /// partitions, seeing the same blocks, converge on the **same** head.
    ///
    /// Vote gossip is not wired yet (the deferred vote-key ↔ stake-key
    /// reconciliation), so the latest-vote store is empty here — the block tree,
    /// the finality floor and on-chain stake already make the choice deterministic
    /// and convergent; once votes flow, the engine's weight (§2) drives it
    /// unchanged.
    fn reconcile_fork(&mut self, competing: Vec<Block>) {
        let Some(local_tip) = self.ledger.chain.last().map(|b| b.hash.clone()) else {
            return;
        };
        // Union block tree + a hash → Block backing store (to rebuild the winner).
        // The local chain's block 0 is the genesis ROOT; everything else links to
        // its parent. A competing block links to its parent (in either set).
        let mut tree = BlockTree::new();
        let mut by_hash: HashMap<String, Block> = HashMap::new();
        for (i, b) in self.ledger.chain.iter().enumerate() {
            if i == 0 {
                tree.add_root(&b.hash);
            } else {
                tree.add_block(&b.hash, &b.prev_hash);
            }
            by_hash.insert(b.hash.clone(), b.clone());
        }
        for b in &competing {
            tree.add_block(&b.hash, &b.prev_hash);
            by_hash.entry(b.hash.clone()).or_insert_with(|| b.clone());
        }
        // GHOST head: anchored at last justified, floored at last finalized.
        let (anchor, floor) = anchors(&self.finality);
        let stakes = self.ledger.validator_stakes();
        let votes = LatestVotes::new();
        let head = ghost_head(&tree, &votes, &stakes, &anchor, &floor);
        if head == local_tip {
            return; // GHOST keeps our chain — nothing to reorganize
        }
        // Reconstruct the winning chain: walk parents from `head` down to the first
        // block already on our chain (the common ancestor), collecting the blocks
        // ABOVE it. Bounded by the known-block count (no hang on cyclic input).
        let local: HashSet<String> = self.ledger.chain.iter().map(|b| b.hash.clone()).collect();
        let mut winners: Vec<Block> = Vec::new();
        let mut cur = head;
        for _ in 0..=by_hash.len() {
            if local.contains(&cur) {
                break; // common ancestor reached
            }
            let Some(b) = by_hash.get(&cur) else {
                return; // a winning block we don't actually hold — abort, keep ours
            };
            winners.push(b.clone());
            cur = b.prev_hash.clone();
        }
        if winners.is_empty() {
            return;
        }
        winners.reverse(); // ascending index for the ledger reorg
        // The last finalized block's index — the reorg floor (never popped).
        let floor_index = self
            .ledger
            .chain
            .iter()
            .find(|b| b.hash == floor)
            .map(|b| b.index)
            .unwrap_or(0);
        if let Ok(true) = self.ledger.reorg_to_fork(&winners, floor_index) {
            self.telemetry.blocks_integrated = self
                .telemetry
                .blocks_integrated
                .saturating_add(winners.len() as u64);
        }
    }

    /// C7: attempt to seal a block as the elected PoS leader, at **injected**
    /// time, emitting the signed `NewBlock` to broadcast (or `None` if we're
    /// not eligible, have nothing pending, or have no identity).
    ///
    /// Faithful extraction of `mining_loop::pos_seal_if_leader` — same
    /// buried-block beacon, same slot (= chain height), same
    /// `is_valid_proposer` election, same `MIN_VALIDATOR_STAKE`
    /// permissionless-bootstrap fallback, same emission bounds enforced
    /// later at validation. It reads **no** wall clock: both the block
    /// `timestamp` and the fallback `elapsed` derive from `now_ms`. The
    /// validator set is an **injected input** (the production shell
    /// builds it from the reputation engine; the simulator from its scenario),
    /// so no consensus RULE changes — only the inputs are injected
    /// (sans-IO). Hence the C7 🛑 was evaluated and **not** triggered.
    pub fn propose_block_at(&mut self, now_ms: u64, validators: &[Validator]) -> Option<Effect> {
        // Keep virtual time monotonic, then use the authoritative `self.now_ms`
        // for the block timestamp, the fallback `elapsed`, AND the envelope
        // timestamp (via `sign_broadcast`) — so all three agree.
        if now_ms > self.now_ms {
            self.now_ms = now_ms;
        }
        let now = self.now_ms;
        // PQ-MIG-3B: the **value** identity — the miner-reward target and the
        // proposer-election key — is this node's ML-DSA address (`from`/`to` are
        // addresses everywhere now, and `validator_stakes()` is keyed by
        // address). The envelope carrying the block is still signed by the
        // Ed25519 **transport** key (see `sign_envelope`), which is deferred.
        let pk = self.identity.as_ref()?.pq_address_hex()?;
        if !self.is_elected_proposer_at(now, &pk, validators) {
            return None;
        }
        let block = self
            .ledger
            .seal_if_pending_at(&pk, 0.0, millis_to_rfc3339(now))?;
        let block_json = serde_json::to_string(&block).ok()?;
        self.sign_broadcast(GossipMessage::NewBlock { block_json })
    }

    /// Eligibility check mirroring `pos_seal_if_leader`: buried-block beacon,
    /// slot = chain height, **injected** `elapsed` for the fallback
    /// timeout. Returns `true` in the permissionless bootstrap phase (no
    /// validator meets `MIN_VALIDATOR_STAKE`), exactly like production.
    fn is_elected_proposer_at(&self, now_ms: u64, pk: &str, validators: &[Validator]) -> bool {
        let height = self.ledger.chain_height();
        if height == 0 {
            return true; // bootstrap (genesis is always present, so
                         // unreachable)
        }
        let tip_index = height - 1;
        let buried_index = tip_index.saturating_sub(pos_consensus::LEADER_ENTROPY_LOOKBACK);
        let beacon = match self.ledger.block_at(buried_index) {
            Some(b) => pos_consensus::leader_beacon(&b.hash, height),
            None => return true, // bootstrap
        };
        // Permissionless seal while nobody has staked the minimum (bootstrap).
        let has_eligible = validators
            .iter()
            .any(|v| v.stake >= pos_consensus::MIN_VALIDATOR_STAKE);
        if !has_eligible {
            return true;
        }
        // Injected elapsed since the tip's timestamp drives the fallback timeout.
        let tip_time = self
            .ledger
            .block_at(tip_index)
            .and_then(|b| chrono::DateTime::parse_from_rfc3339(&b.timestamp).ok())
            .map(|t| t.timestamp() as u64)
            .unwrap_or(0);
        let elapsed = (now_ms / 1_000).saturating_sub(tip_time);
        let (is_valid, _is_primary) =
            pos_consensus::is_valid_proposer(pk, &beacon, height, elapsed, validators);
        is_valid
    }

    /// Liveness response: a signed `Pong` echoing the ping nonce — mirrors the
    /// production `handle_ping` (which broadcasts a Pong). Emits nothing if
    /// this node has no signing identity (observer mode).
    fn on_ping(&mut self, nonce: u64) -> Vec<Effect> {
        self.sign_broadcast(GossipMessage::Pong { nonce })
            .into_iter()
            .collect()
    }

    /// Sign `msg` into a gossip envelope and return its bytes.
    ///
    /// The envelope timestamp comes from the **injected** virtual clock (no
    /// system-clock read), the outgoing nonce is the core's own monotonic
    /// counter, and the signature is deterministic Ed25519 — so the produced
    /// bytes are fully reproducible from the seed. Returns `None` (consuming no
    /// nonce) when there is no identity or any step fails.
    fn sign_envelope(&mut self, msg: GossipMessage) -> Option<Vec<u8>> {
        let nonce = self.out_nonce;
        let timestamp = millis_to_rfc3339(self.now_ms);
        let crypto = self.identity.as_ref()?;
        let pk = crypto.get_identity().ok()?.public_key_hex;
        let signable = GossipRouter::signable_envelope_bytes(&pk, nonce, &timestamp, &msg);
        let sig = crypto.sign(&signable).ok()?;
        let env = GossipRouter::build_signed_envelope(pk, msg, nonce, timestamp, &sig).ok()?;
        let bytes = serde_json::to_vec(&env).ok()?;
        // Consume the nonce only once the message is fully built.
        self.out_nonce = self.out_nonce.saturating_add(1);
        Some(bytes)
    }

    /// Sign `msg` and wrap it as an [`Effect::Broadcast`] (to all peers).
    fn sign_broadcast(&mut self, msg: GossipMessage) -> Option<Effect> {
        self.sign_envelope(msg)
            .map(|bytes| Effect::Broadcast { bytes })
    }

    /// Sign `msg` and wrap it as an [`Effect::Send`] targeted at one peer
    /// (chain-sync replies).
    fn sign_send(&mut self, to: PeerId, msg: GossipMessage) -> Option<Effect> {
        self.sign_envelope(msg)
            .map(|bytes| Effect::Send { to, bytes })
    }

    /// Advance virtual time and run time-driven maintenance.
    fn on_tick(&mut self, now_ms: u64) -> Vec<Effect> {
        // Monotonic: never let a stale or duplicate tick move time backwards.
        if now_ms > self.now_ms {
            self.now_ms = now_ms;
        }
        // Deterministic mempool eviction using INJECTED time (Unix seconds).
        let now_secs = (self.now_ms / 1_000) as i64;
        let _evicted = self.ledger.prune_mempool_at(now_secs);
        // T0.4 tr.2: arm the periodic seal timer once, but ONLY for a node that
        // can actually propose — it has a signing identity AND a validator set.
        // Gating on a non-empty set keeps observer/unconfigured nodes (and every
        // existing test) effect-free here.
        if self.identity.is_some() && !self.validators.is_empty() && !self.seal_timer_armed {
            self.seal_timer_armed = true;
            self.next_seal_at_ms = self.now_ms.saturating_add(SEAL_INTERVAL_MS);
            return vec![Effect::SetTimer {
                id: SEAL_TIMER_ID,
                fire_at_ms: self.next_seal_at_ms,
            }];
        }
        Vec::new()
    }

    /// T0.4 tr.2: the periodic seal timer fired → try to PRODUCE a block as the
    /// elected leader (event-driven, replacing the orchestrated kick), then
    /// **re-arm** for the next slot. Reuses [`Node::propose_block_at`] against
    /// the held validator set; a non-leader, or an empty mempool, simply yields
    /// no block. Re-arming is unconditional, like production's periodic seal.
    fn on_seal_timer(&mut self) -> Vec<Effect> {
        // Take the validators out so `propose_block_at` can borrow `&mut self`
        // (it never touches `self.validators`); restore them right after.
        let validators = std::mem::take(&mut self.validators);
        let mut effects: Vec<Effect> = self
            .propose_block_at(self.now_ms, &validators)
            .into_iter()
            .collect();
        self.validators = validators;
        // Advance the schedule MONOTONICALLY (not from frozen `now_ms`) so the
        // next fire is strictly later — otherwise the simulator loops forever.
        self.next_seal_at_ms = self.next_seal_at_ms.saturating_add(SEAL_INTERVAL_MS);
        effects.push(Effect::SetTimer {
            id: SEAL_TIMER_ID,
            fire_at_ms: self.next_seal_at_ms,
        });
        effects
    }
}

impl Default for Node {
    fn default() -> Self {
        Self::new()
    }
}

/// Format virtual time (ms since the Unix epoch) as an RFC3339 string. Pure
/// formatting — **no system-clock read**. Out-of-range inputs yield an empty
/// string (which a receiver's freshness check rejects), never a panic.
fn millis_to_rfc3339(now_ms: u64) -> String {
    chrono::DateTime::from_timestamp_millis(now_ms as i64)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{ConsensusTelemetry, Node};
    use crate::{
        p2p::{
            gossip::{GossipMessage, GossipRouter},
            ledger::{Block, Ledger, MICRO},
            pos_consensus::{Validator, MIN_VALIDATOR_STAKE},
        },
        security::CryptoEngine,
        sm::{
            event::{PeerId, TimerId},
            Blake3Rng, Effect, Event,
        },
    };

    #[test]
    fn tick_advances_virtual_time_monotonically() {
        let mut node = Node::new();
        let mut rng = Blake3Rng::from_seed(1);
        assert_eq!(node.now_ms(), 0);
        node.handle(Event::Tick { now_ms: 5_000 }, &mut rng);
        assert_eq!(node.now_ms(), 5_000);
        // A stale tick must not move time backwards.
        node.handle(Event::Tick { now_ms: 1_000 }, &mut rng);
        assert_eq!(node.now_ms(), 5_000);
    }

    #[test]
    fn inert_events_are_no_ops_for_now() {
        let mut node = Node::new();
        let mut rng = Blake3Rng::from_seed(2);
        let height_before = node.ledger().chain.len();
        let effects = node.handle(Event::TimerFired { id: TimerId(1) }, &mut rng);
        assert!(effects.is_empty());
        assert_eq!(node.ledger().chain.len(), height_before);
    }

    /// Untrusted inbound bytes must never panic the core and must leave state
    /// untouched when they fail the stateless security gate (Constitution §3:
    /// raw bytes are never trusted).
    #[test]
    fn message_received_drops_untrusted_bytes_without_panic() {
        let mut node = Node::new();
        let mut rng = Blake3Rng::from_seed(4);
        node.handle(Event::Tick { now_ms: 1_000_000 }, &mut rng);
        let height_before = node.ledger().chain.len();
        let hostile: Vec<Vec<u8>> = vec![
            vec![],
            vec![0xff; 16],
            b"{not valid json".to_vec(),
            vec![0u8; 2048],
        ];
        for bytes in hostile {
            let effects = node.handle(
                Event::MessageReceived {
                    from: PeerId("peer".into()),
                    bytes,
                },
                &mut rng,
            );
            assert!(effects.is_empty(), "garbage input must produce no effects");
        }
        assert_eq!(node.ledger().chain.len(), height_before);
    }

    /// Conservation INVARIANT, now checked **through the core**: minting plus
    /// transfers keep `Σ balances + burned == minted`, and driving `Tick`s
    /// (which prune the mempool) never breaks it.
    #[test]
    fn conservation_holds_through_node_ticks() {
        let mut crypto = CryptoEngine::new();
        let _ = crypto.generate_keypair();
        crypto.generate_pq_identity().expect("ml-dsa primary"); // PQ-MIG-3
        let acc = [
            "a".repeat(64),
            "b".repeat(64),
            "c".repeat(64),
            "d".repeat(64),
        ];

        let mut node = Node::new();
        let minted_each = 100 * MICRO;
        for a in &acc {
            node.ledger_mut().mine_tx(a, minted_each, 0.0);
        }
        let minted_total = minted_each * acc.len() as u64;

        // A couple of transfers with burn.
        let _ = node
            .ledger_mut()
            .transfer_with_burn(&acc[0], &acc[1], 5 * MICRO, &crypto);
        let _ = node
            .ledger_mut()
            .transfer_with_burn(&acc[1], &acc[2], 3 * MICRO, &crypto);

        // Drive virtual time forward across several ticks (exercises prune).
        let mut rng = Blake3Rng::from_seed(3);
        for t in 1..=5u64 {
            node.handle(Event::Tick { now_ms: t * 60_000 }, &mut rng);
        }

        let sum: u64 = node.ledger().all_balances().values().sum();
        let burned = node.ledger().total_burned();
        assert_eq!(
            sum + burned,
            minted_total,
            "conservation through core: Σ balances ({sum}) + burned ({burned}) != minted \
             ({minted_total})"
        );
    }

    /// Determinism: two cores fed the SAME event sequence reach identical
    /// observable state (virtual time, height, balances). The foundation of
    /// byte-for-byte replayability.
    #[test]
    fn two_nodes_same_events_converge() {
        let build = || {
            let mut node = Node::new();
            let mut rng = Blake3Rng::from_seed(7);
            for (k, a) in ["x".repeat(64), "y".repeat(64)].iter().enumerate() {
                node.ledger_mut().mine_tx(a, (k as u64 + 1) * MICRO, 0.0);
            }
            for t in 1..=3u64 {
                node.handle(Event::Tick { now_ms: t * 30_000 }, &mut rng);
            }
            let mut bals: Vec<(String, u64)> = node.ledger().all_balances().into_iter().collect();
            bals.sort();
            (node.now_ms(), node.ledger().chain.len(), bals)
        };
        assert_eq!(build(), build());
    }

    /// A verified `BroadcastTx` arriving over the wire is admitted to the
    /// core's authoritative ledger: the recipient is credited. Proves the
    /// first Event→state path that mutates the ledger from network input.
    #[test]
    fn broadcast_tx_is_applied_to_ledger_through_core() {
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        crypto.generate_pq_identity().expect("ml-dsa primary"); // PQ-MIG-3
        // PQ-MIG-3B: the **value** identity (`from`/balance key) is the ML-DSA
        // address; the **transport** identity (gossip envelope sender) stays the
        // Ed25519 key.
        let addr = crypto.pq_address_hex().expect("ml-dsa address");
        let sender = id.public_key_hex.clone();
        let recipient = "r".repeat(64);

        // Origin builds a SIGNED transfer (sender funded so it's valid).
        let mut origin = Ledger::new();
        origin.mine_tx(&addr, 50 * MICRO, 0.0);
        let (tx, _burn, _amt) = origin
            .transfer_with_burn(&addr, &recipient, 10 * MICRO, &crypto)
            .expect("transfer builds");
        let credited = tx.amount; // net after the 1% burn
        let tx_json = serde_json::to_string(&tx).unwrap();

        // Wrap the tx in a freshly-signed gossip envelope (sender's key).
        let msg = GossipMessage::BroadcastTx { tx_json };
        let timestamp = "2026-03-01T12:00:00+00:00".to_string();
        let nonce = 0_u64;
        let signable = GossipRouter::signable_envelope_bytes(&sender, nonce, &timestamp, &msg);
        let sig = crypto.sign(&signable).unwrap();
        let env = GossipRouter::build_signed_envelope(
            sender.clone(),
            msg,
            nonce,
            timestamp.clone(),
            &sig,
        )
        .unwrap();
        let bytes = serde_json::to_vec(&env).unwrap();

        // A fresh receiver core admits it at an injected time inside the window.
        let mut node = Node::new();
        let t0 = chrono::DateTime::parse_from_rfc3339(&timestamp)
            .unwrap()
            .timestamp();
        let mut rng = Blake3Rng::from_seed(5);
        node.handle(
            Event::Tick {
                now_ms: (t0 as u64) * 1_000,
            },
            &mut rng,
        );
        assert_eq!(
            node.ledger().balance_of(&recipient),
            0,
            "recipient starts empty"
        );

        node.handle(
            Event::MessageReceived {
                from: PeerId(sender.clone()),
                bytes: bytes.clone(),
            },
            &mut rng,
        );
        assert_eq!(
            node.ledger().balance_of(&recipient),
            credited,
            "verified BroadcastTx must credit the recipient on the receiving core"
        );

        // Idempotent: replaying the same envelope changes nothing (dedup).
        node.handle(
            Event::MessageReceived {
                from: PeerId(sender),
                bytes,
            },
            &mut rng,
        );
        assert_eq!(
            node.ledger().balance_of(&recipient),
            credited,
            "replay is a no-op"
        );
    }

    // ── Tranche 5: outgoing Effect (signed Pong) ──────────────────────────────

    /// Deterministic signing identity derived from a seed (Ed25519 accepts any
    /// 32 bytes as the secret). Same seed ⇒ same key ⇒ reproducible signatures.
    ///
    /// PQ-MIG-3: also establishes the **independent ML-DSA primary** (the
    /// tx-authority key the ledger binds), its seed derived from the Ed25519 seed
    /// via a domain-separated BLAKE3 hash — reproducible (C1) yet structurally
    /// independent of the Ed25519 seed bytes.
    fn seeded_identity(seed: u64) -> CryptoEngine {
        let mut sk = [0x11u8; 32];
        sk[..8].copy_from_slice(&seed.to_le_bytes());
        let mut c = CryptoEngine::new();
        c.import_keypair(&sk).expect("valid 32-byte ed25519 secret");
        let mut pq_seed = [0u8; 32];
        pq_seed.copy_from_slice(&CryptoEngine::blake3_hash(
            &[b"QUANTA-PQ-PRIMARY-sim:".as_ref(), &sk].concat(),
        ));
        c.import_pq_identity(&pq_seed).expect("ml-dsa primary");
        c
    }

    /// PQ-MIG-3B: the **value** identity of an engine — its ML-DSA *address*
    /// (`BLAKE3(ADDR_DOMAIN ‖ pq_pubkey)`), the key under which balance, stake,
    /// reward and `from`/`to` are recorded. Distinct from `public_key_hex`, the
    /// Ed25519 **transport** identity used only to sign gossip envelopes.
    fn addr_of(crypto: &CryptoEngine) -> String {
        crypto.pq_address_hex().expect("ml-dsa address")
    }

    /// A signed gossip envelope wrapping `msg` from `crypto`'s identity,
    /// timestamped at `now_ms` (envelope nonce 0).
    fn signed_envelope(crypto: &CryptoEngine, msg: GossipMessage, now_ms: u64) -> Vec<u8> {
        let pk = crypto.get_identity().unwrap().public_key_hex;
        let ts = chrono::DateTime::from_timestamp_millis(now_ms as i64)
            .unwrap()
            .to_rfc3339();
        let signable = GossipRouter::signable_envelope_bytes(&pk, 0, &ts, &msg);
        let sig = crypto.sign(&signable).unwrap();
        let env = GossipRouter::build_signed_envelope(pk, msg, 0, ts, &sig).unwrap();
        serde_json::to_vec(&env).unwrap()
    }

    /// A signed `Ping` envelope from `crypto`'s identity, timestamped at
    /// `now_ms`.
    fn signed_ping_envelope(crypto: &CryptoEngine, ping_nonce: u64, now_ms: u64) -> Vec<u8> {
        signed_envelope(crypto, GossipMessage::Ping { nonce: ping_nonce }, now_ms)
    }

    /// A Ping yields exactly one Broadcast effect carrying a Pong that is
    /// itself a VALID, FRESH, correctly-signed gossip envelope (signed by
    /// us).
    #[test]
    fn ping_yields_a_valid_signed_pong_broadcast() {
        let now_ms = 1_800_000_000_000_u64; // fixed virtual time (ms)
        let mut node = Node::with_identity(seeded_identity(1));
        let mut rng = Blake3Rng::from_seed(9);
        node.handle(Event::Tick { now_ms }, &mut rng);

        let pinger = seeded_identity(2);
        let ping = signed_ping_envelope(&pinger, 1234, now_ms);
        let effects = node.handle(
            Event::MessageReceived {
                from: PeerId(pinger.get_identity().unwrap().public_key_hex),
                bytes: ping,
            },
            &mut rng,
        );

        assert_eq!(
            effects.len(),
            1,
            "a Ping must yield exactly one Pong broadcast"
        );
        let Effect::Broadcast { bytes } = &effects[0] else {
            panic!("expected an Effect::Broadcast, got {:?}", effects[0]);
        };
        // Our own Pong must pass the same gate any peer would apply to it.
        let now_secs = (now_ms / 1_000) as i64;
        let env = crate::p2p::dispatcher::validate_envelope_at(bytes, now_secs)
            .expect("our Pong must be a valid, fresh, signed envelope");
        assert!(matches!(env.payload, GossipMessage::Pong { nonce: 1234 }));
        assert_eq!(
            env.sender,
            seeded_identity(1).get_identity().unwrap().public_key_hex,
            "the Pong must be signed by us, the responder"
        );
    }

    #[test]
    fn ping_without_identity_emits_nothing() {
        let now_ms = 1_800_000_000_000_u64;
        let mut node = Node::new(); // observer — no signing identity
        let mut rng = Blake3Rng::from_seed(9);
        node.handle(Event::Tick { now_ms }, &mut rng);
        let ping = signed_ping_envelope(&seeded_identity(2), 7, now_ms);
        let effects = node.handle(
            Event::MessageReceived {
                from: PeerId("p".into()),
                bytes: ping,
            },
            &mut rng,
        );
        assert!(effects.is_empty(), "an observer node cannot sign a Pong");
    }

    /// Outgoing Pong bytes are byte-for-byte reproducible: deterministic
    /// Ed25519 signing + injected timestamp + deterministic outgoing nonce.
    #[test]
    fn pong_emission_is_deterministic() {
        let now_ms = 1_800_000_000_000_u64;
        let build = || {
            let mut node = Node::with_identity(seeded_identity(1));
            let mut rng = Blake3Rng::from_seed(9);
            node.handle(Event::Tick { now_ms }, &mut rng);
            let ping = signed_ping_envelope(&seeded_identity(2), 55, now_ms);
            node.handle(
                Event::MessageReceived {
                    from: PeerId("p".into()),
                    bytes: ping,
                },
                &mut rng,
            )
        };
        assert_eq!(build(), build(), "Pong emission must be byte-deterministic");
    }

    // ── Tranche 6: NewBlock integration (consensus-critical) ──────────────────

    /// A sealed block arriving over the wire is validated and integrated into
    /// the core's chain via the shared `integrate_remote_block` path.
    #[test]
    fn new_block_is_integrated_through_core() {
        let now_ms = 1_800_000_000_000_u64;
        let sealer = seeded_identity(3);
        let pk = sealer.get_identity().unwrap().public_key_hex;

        // Origin seals a block at height 1 (shared fixed genesis ⇒ links to B).
        let mut origin = Ledger::new();
        origin.mine_tx(&pk, 50 * MICRO, 0.0);
        let block = origin.seal_block(&pk, 0.0);
        let tip_hash = block.hash.clone();
        let bytes = signed_envelope(
            &sealer,
            GossipMessage::NewBlock {
                block_json: serde_json::to_string(&block).unwrap(),
            },
            now_ms,
        );

        let mut node = Node::new();
        let mut rng = Blake3Rng::from_seed(11);
        node.handle(Event::Tick { now_ms }, &mut rng);
        let height_before = node.ledger().chain.len();
        node.handle(
            Event::MessageReceived {
                from: PeerId(pk),
                bytes,
            },
            &mut rng,
        );

        assert_eq!(
            node.ledger().chain.len(),
            height_before + 1,
            "the sealed block must extend the chain"
        );
        assert_eq!(
            node.ledger().chain.last().map(|b| b.hash.clone()),
            Some(tip_hash),
            "the tip must be the integrated block"
        );
    }

    /// Consensus validation is enforced through the core: a block whose
    /// `prev_hash` does not link to our tip is rejected, leaving the chain
    /// unchanged.
    #[test]
    fn new_block_with_broken_link_is_rejected_by_core() {
        let now_ms = 1_800_000_000_000_u64;
        let sealer = seeded_identity(3);
        let pk = sealer.get_identity().unwrap().public_key_hex;

        let mut origin = Ledger::new();
        origin.mine_tx(&pk, 50 * MICRO, 0.0);
        let mut block = origin.seal_block(&pk, 0.0);
        block.prev_hash = "0".repeat(64); // break the chain linkage
        let bytes = signed_envelope(
            &sealer,
            GossipMessage::NewBlock {
                block_json: serde_json::to_string(&block).unwrap(),
            },
            now_ms,
        );

        let mut node = Node::new();
        let mut rng = Blake3Rng::from_seed(11);
        node.handle(Event::Tick { now_ms }, &mut rng);
        let height_before = node.ledger().chain.len();
        node.handle(
            Event::MessageReceived {
                from: PeerId(pk),
                bytes,
            },
            &mut rng,
        );

        assert_eq!(
            node.ledger().chain.len(),
            height_before,
            "a block with a broken prev_hash must be rejected"
        );
    }

    // ── C1: transitive determinism — canonical fingerprint + meta-test ────────

    /// A canonical, order-independent fingerprint of the ledger's observable
    /// state. Balances are read from a `HashMap`, so they are **sorted** before
    /// hashing — the fingerprint depends only on the stored *values*, never on
    /// `HashMap` iteration order. Any transitive non-determinism (a clock read,
    /// `OsRng`, or an order-dependent decision) that perturbs the stored state
    /// makes this fingerprint diverge across runs.
    fn fingerprint(node: &Node) -> [u8; 32] {
        let l = node.ledger();
        let s = l.stats();
        let mut h = blake3::Hasher::new();
        h.update(b"QUANTA-FP-1");
        h.update(&node.now_ms().to_le_bytes());
        // Chain is an ordered Vec — hash block hashes in order.
        for b in &l.chain {
            h.update(b.hash.as_bytes());
        }
        // Balances come from a HashMap — sort to a canonical order first.
        let mut bals: Vec<(String, u64)> = l.all_balances().into_iter().collect();
        bals.sort();
        for (k, v) in &bals {
            h.update(k.as_bytes());
            h.update(&v.to_le_bytes());
        }
        // Scalar aggregates (all order-independent sums).
        h.update(&l.total_burned().to_le_bytes());
        h.update(&s.total_mined.to_le_bytes());
        h.update(&s.pending.to_le_bytes());
        h.update(&s.total_txs.to_le_bytes());
        h.update(&s.total_blocks.to_le_bytes());
        *h.finalize().as_bytes()
    }

    /// **C1 meta-test (the centrepiece).** A FIXED sequence of events — built
    /// once, frozen into bytes — is replayed through a fresh core `N = 128`
    /// times. Every run must produce byte-identical [`Effect`]s AND an
    /// identical ledger fingerprint. This guards determinism *transitively*
    /// across the whole call graph reached by [`Node::handle`]
    /// (`validate_envelope_at` → `verify_tx`, `apply_remote_tx_checked`,
    /// `integrate_remote_block`, `sign_broadcast` → `CryptoEngine::sign`,
    /// `prune_mempool_at`), not just the surface of `sm/`. A clock read,
    /// `OsRng`, or a `HashMap`-order dependence anywhere in that tree would
    /// diverge at least one run.
    #[test]
    fn determinism_meta_test_128_runs_are_byte_identical() {
        // Fixed virtual time for the whole sequence (so signed Effects are
        // byte-stable). Far from the real clock on purpose — proves nothing in
        // the validation path depends on "now ≈ real now".
        let now0 = 1_800_000_000_000_u64;

        // ── Build all input envelopes ONCE (frozen bytes) ──
        let alice = seeded_identity(2);
        let alice_pk = addr_of(&alice); // PQ-MIG-3B: value identity = ML-DSA address
        let carol = seeded_identity(4);
        let dave = seeded_identity(5);
        let bob = "b".repeat(64);

        // Block #1 funds alice via a NETWORK mining tx (chain-backed funds are
        // never pruned), sealed in a throwaway origin sharing our fixed genesis.
        let mut origin = Ledger::new();
        origin.mine_tx(&alice_pk, 50 * MICRO, 0.0);
        let funding_block = origin.seal_block(&alice_pk, 0.0);
        let funding_env = signed_envelope(
            &dave,
            GossipMessage::NewBlock {
                block_json: serde_json::to_string(&funding_block).unwrap(),
            },
            now0,
        );

        // alice signs a transfer alice→bob (valid: origin funded her on-chain).
        let (xfer, _burn, _amt) = origin
            .transfer_with_burn(&alice_pk, &bob, 10 * MICRO, &alice)
            .expect("alice transfer builds");
        let credited = xfer.amount;
        let xfer_env = signed_envelope(
            &alice,
            GossipMessage::BroadcastTx {
                tx_json: serde_json::to_string(&xfer).unwrap(),
            },
            now0,
        );

        // carol pings us (drives the only outgoing Effect: a signed Pong).
        let ping_env = signed_ping_envelope(&carol, 777, now0);
        // hostile bytes the gate must drop without effect.
        let garbage = b"{not a valid envelope".to_vec();

        // ── Replay the frozen sequence through a fresh core, N times ──
        // Returns: every step's Effects, the credit observed at admission time,
        // the credit after the pruning ticks, and the final fingerprint.
        let dave_pk = dave.get_identity().unwrap().public_key_hex;
        let carol_pk = carol.get_identity().unwrap().public_key_hex;
        let run = || -> (Vec<Vec<Effect>>, u64, u64, [u8; 32]) {
            let mut node = Node::with_identity(seeded_identity(1));
            let mut rng = Blake3Rng::from_seed(0xC1);
            let mut fx: Vec<Vec<Effect>> = Vec::new();

            fx.push(node.handle(Event::Tick { now_ms: now0 }, &mut rng));
            fx.push(node.handle(
                Event::MessageReceived {
                    from: PeerId(dave_pk.clone()),
                    bytes: funding_env.clone(),
                },
                &mut rng,
            ));
            fx.push(node.handle(
                Event::MessageReceived {
                    from: PeerId(alice_pk.clone()),
                    bytes: xfer_env.clone(),
                },
                &mut rng,
            ));
            // Credit observed at admission, BEFORE the pruning ticks.
            let credited_at_admission = node.ledger().balance_of(&bob);
            fx.push(node.handle(
                Event::MessageReceived {
                    from: PeerId(carol_pk.clone()),
                    bytes: ping_env.clone(),
                },
                &mut rng,
            ));
            fx.push(node.handle(
                Event::MessageReceived {
                    from: PeerId("garbage".into()),
                    bytes: garbage.clone(),
                },
                &mut rng,
            ));
            // These ticks are far past the (frozen, real-clock) tx timestamp, so
            // prune_mempool_at evicts alice's still-pending transfer — exercising
            // the injected-time eviction + cache-revert path deterministically.
            fx.push(node.handle(
                Event::Tick {
                    now_ms: now0 + 60_000,
                },
                &mut rng,
            ));
            fx.push(node.handle(
                Event::Tick {
                    now_ms: now0 + 120_000,
                },
                &mut rng,
            ));
            let credited_after_prune = node.ledger().balance_of(&bob);

            (
                fx,
                credited_at_admission,
                credited_after_prune,
                fingerprint(&node),
            )
        };

        let first = run();
        // Sanity — the sequence really exercised the paths, so the determinism
        // assertion below is not vacuously true:
        assert_eq!(
            first.1, credited,
            "bob must be credited through the core at admission"
        );
        assert_eq!(
            first.2, 0,
            "the still-pending transfer must be evicted by the injected-time prune"
        );
        assert_eq!(
            first.0[3].len(),
            1,
            "the Ping must have produced exactly one Pong Effect"
        );

        for i in 1..128 {
            let next = run();
            assert_eq!(
                next, first,
                "run {i} diverged from run 0 — a transitive non-determinism (clock, OsRng, or \
                 HashMap order) leaked into the core"
            );
        }
    }

    /// **C1 teeth.** The determinism check is only meaningful if its comparison
    /// can actually tell states apart. This proves both channels discriminate:
    /// the fingerprint detects a 1-µQTA divergence, and the Effect comparison
    /// detects two different Pongs. So a real non-determinism would be caught,
    /// not silently passed.
    #[test]
    fn determinism_comparison_has_teeth() {
        // Fingerprint detects a one-microQTA difference.
        let mut a = Node::new();
        let b = Node::new();
        a.ledger_mut().mine_tx(&"z".repeat(64), 1, 0.0);
        assert_ne!(
            fingerprint(&a),
            fingerprint(&b),
            "fingerprint must detect a 1-µQTA state divergence"
        );

        // Effect bytes distinguish two different pings.
        let now = 1_800_000_000_000_u64;
        let pong_for = |ping_nonce: u64| -> Vec<Effect> {
            let mut n = Node::with_identity(seeded_identity(1));
            let mut rng = Blake3Rng::from_seed(1);
            n.handle(Event::Tick { now_ms: now }, &mut rng);
            n.handle(
                Event::MessageReceived {
                    from: PeerId("p".into()),
                    bytes: signed_ping_envelope(&seeded_identity(2), ping_nonce, now),
                },
                &mut rng,
            )
        };
        assert_ne!(
            pong_for(1),
            pong_for(2),
            "distinct pings must yield distinct Pong Effects"
        );
    }

    // ── C2: sync-replay — historical blocks/txs validate at a far-future now ──

    /// **C2 (the missing test).** A block sealed at `t0` must still integrate
    /// when validated at an injected `now` HOURS later — the exact case chain
    /// sync hits (RequestChain → ChainSegment replays historical blocks). Block
    /// validation reads no wall clock (C1 audit), so the block's age relative
    /// to `now` is irrelevant; only the carrying envelope must be fresh,
    /// and it is (re-signed at send time). Proven at two far-apart
    /// validation times so it is not a "just inside a window" fluke. The
    /// injected `now` is derived from the block's own seal time + N h, so
    /// the test is date-independent.
    #[test]
    fn sync_replay_historical_block_integrates_at_far_future_now() {
        let sealer = seeded_identity(3);
        let pk = sealer.get_identity().unwrap().public_key_hex;

        let mut origin = Ledger::new();
        origin.mine_tx(&pk, 50 * MICRO, 0.0);
        let block = origin.seal_block(&pk, 0.0);
        let tip_hash = block.hash.clone();
        let sealed_at = chrono::DateTime::parse_from_rfc3339(&block.timestamp)
            .expect("block timestamp parses")
            .timestamp();
        let block_json = serde_json::to_string(&block).unwrap();

        // Validate the SAME historical block at now = seal + 6 h, then + 48 h.
        for hours_later in [6_i64, 48] {
            let now_ms = ((sealed_at + hours_later * 3600) as u64) * 1_000;
            let env = signed_envelope(
                &sealer,
                GossipMessage::NewBlock {
                    block_json: block_json.clone(),
                },
                now_ms,
            );
            let mut node = Node::new();
            let mut rng = Blake3Rng::from_seed(13);
            node.handle(Event::Tick { now_ms }, &mut rng);
            node.handle(
                Event::MessageReceived {
                    from: PeerId(pk.clone()),
                    bytes: env,
                },
                &mut rng,
            );
            assert_eq!(
                node.ledger().chain.last().map(|b| b.hash.clone()),
                Some(tip_hash.clone()),
                "a block sealed {hours_later}h before `now` must still integrate (sync replay)"
            );
        }
    }

    /// **C2 (tx side).** A signed transfer built at `t0` must still be admitted
    /// when received at an injected `now` hours later: remote-tx admission
    /// (`apply_remote_tx_checked` → `verify_tx`) verifies the signature only
    /// and reads no wall clock, so an old-but-valid tx is not rejected for
    /// age. Only the carrying envelope must be fresh (it is). Guards
    /// against a future edit adding a wall-clock freshness gate to the
    /// remote-admission path.
    #[test]
    fn remote_tx_admitted_regardless_of_tx_age() {
        let alice = seeded_identity(2);
        let alice_pk = addr_of(&alice); // PQ-MIG-3B: value identity = ML-DSA address
        let bob = "b".repeat(64);

        let mut origin = Ledger::new();
        origin.mine_tx(&alice_pk, 50 * MICRO, 0.0);
        let funding_block = origin.seal_block(&alice_pk, 0.0);
        let (xfer, _burn, _amt) = origin
            .transfer_with_burn(&alice_pk, &bob, 10 * MICRO, &alice)
            .expect("alice transfer builds");
        let credited = xfer.amount;
        let built_at = chrono::DateTime::parse_from_rfc3339(&xfer.timestamp)
            .expect("tx timestamp parses")
            .timestamp();

        // Receive the old tx 6 h after it was built.
        let now_ms = ((built_at + 6 * 3600) as u64) * 1_000;
        let funding_env = signed_envelope(
            &alice,
            GossipMessage::NewBlock {
                block_json: serde_json::to_string(&funding_block).unwrap(),
            },
            now_ms,
        );
        let xfer_env = signed_envelope(
            &alice,
            GossipMessage::BroadcastTx {
                tx_json: serde_json::to_string(&xfer).unwrap(),
            },
            now_ms,
        );

        let mut node = Node::new();
        let mut rng = Blake3Rng::from_seed(17);
        node.handle(Event::Tick { now_ms }, &mut rng);
        node.handle(
            Event::MessageReceived {
                from: PeerId(alice_pk.clone()),
                bytes: funding_env,
            },
            &mut rng,
        );
        node.handle(
            Event::MessageReceived {
                from: PeerId(alice_pk),
                bytes: xfer_env,
            },
            &mut rng,
        );
        assert_eq!(
            node.ledger().balance_of(&bob),
            credited,
            "an old-but-validly-signed tx must still be admitted at a far-future injected now"
        );
    }

    // ── C3: consensus-decision observability at the core boundary ─────────────

    /// **C3 (the core claim).** An integrated block and a rejected block both
    /// return an EMPTY effect stream — so they are indistinguishable to
    /// anything watching only `Vec<Effect>` (or `chain.len()` from
    /// outside). The telemetry channel makes the *decision* observable: the
    /// harness can assert "this block was rejected" at the core boundary.
    #[test]
    fn consensus_outcome_distinguishes_integrated_from_rejected_at_core_boundary() {
        let now0 = 1_800_000_000_000_u64;
        let sealer = seeded_identity(3);
        let pk = sealer.get_identity().unwrap().public_key_hex;

        let mut origin = Ledger::new();
        origin.mine_tx(&pk, 50 * MICRO, 0.0);
        let good = origin.seal_block(&pk, 0.0);
        let mut bad = good.clone();
        bad.prev_hash = "0".repeat(64); // breaks linkage → rejected by validation

        let deliver = |block: &Block| -> (Vec<Effect>, ConsensusTelemetry) {
            let env = signed_envelope(
                &sealer,
                GossipMessage::NewBlock {
                    block_json: serde_json::to_string(block).unwrap(),
                },
                now0,
            );
            let mut node = Node::new();
            let mut rng = Blake3Rng::from_seed(21);
            node.handle(Event::Tick { now_ms: now0 }, &mut rng);
            let fx = node.handle(
                Event::MessageReceived {
                    from: PeerId(pk.clone()),
                    bytes: env,
                },
                &mut rng,
            );
            (fx, node.telemetry().clone())
        };

        let (fx_good, tele_good) = deliver(&good);
        let (fx_bad, tele_bad) = deliver(&bad);

        // Indistinguishable through the effect stream (both empty)…
        assert_eq!(fx_good, fx_bad, "both outcomes emit no effects");
        assert!(fx_good.is_empty());
        // …but fully distinguishable through telemetry (C3).
        assert_eq!(tele_good.blocks_integrated, 1);
        assert_eq!(tele_good.blocks_rejected, 0);
        assert_eq!(tele_bad.blocks_integrated, 0);
        assert_eq!(tele_bad.blocks_rejected, 1);
    }

    /// **C3 (full counter coverage).** A single core records every consensus
    /// outcome: block integrated, block duplicate, tx admitted, tx dropped.
    #[test]
    fn consensus_telemetry_counts_duplicates_and_tx_outcomes() {
        let now0 = 1_800_000_000_000_u64;
        let alice = seeded_identity(2);
        let alice_pk = addr_of(&alice); // PQ-MIG-3B: value identity = ML-DSA address
        let bob = "b".repeat(64);

        let mut origin = Ledger::new();
        origin.mine_tx(&alice_pk, 50 * MICRO, 0.0);
        let block = origin.seal_block(&alice_pk, 0.0);
        let (xfer, _burn, _amt) = origin
            .transfer_with_burn(&alice_pk, &bob, 10 * MICRO, &alice)
            .expect("alice transfer builds");

        let block_env = signed_envelope(
            &alice,
            GossipMessage::NewBlock {
                block_json: serde_json::to_string(&block).unwrap(),
            },
            now0,
        );
        let tx_env = signed_envelope(
            &alice,
            GossipMessage::BroadcastTx {
                tx_json: serde_json::to_string(&xfer).unwrap(),
            },
            now0,
        );

        let mut node = Node::new();
        let mut rng = Blake3Rng::from_seed(23);
        node.handle(Event::Tick { now_ms: now0 }, &mut rng);
        let msg = |bytes: Vec<u8>| Event::MessageReceived {
            from: PeerId(alice_pk.clone()),
            bytes,
        };
        node.handle(msg(block_env.clone()), &mut rng); // integrated
        node.handle(msg(block_env), &mut rng); // duplicate
        node.handle(msg(tx_env.clone()), &mut rng); // admitted
        node.handle(msg(tx_env), &mut rng); // dropped (duplicate hash)

        let t = node.telemetry();
        assert_eq!(t.blocks_integrated, 1, "one block integrated");
        assert_eq!(t.blocks_duplicate, 1, "the re-sent block is a duplicate");
        assert_eq!(t.blocks_rejected, 0, "no block rejected");
        assert_eq!(t.txs_admitted, 1, "the transfer is admitted once");
        assert_eq!(t.txs_dropped, 1, "the re-sent transfer is dropped");
    }

    // ── C4: reorg + rejection patterns exercised THROUGH the core ─────────────

    /// Build a valid block at height 1: alice (seed 2) is funded by a NETWORK
    /// mining tx of `mint` µQTA and signs a 10-QUANTA transfer to `recipient`,
    /// all sealed into one block extending the shared genesis.
    fn sealed_block_with_transfer(recipient: &str, mint: u64) -> Block {
        let alice = seeded_identity(2);
        let alice_pk = addr_of(&alice); // PQ-MIG-3B: value identity = ML-DSA address
        let mut o = Ledger::new();
        o.mine_tx(&alice_pk, mint, 0.0);
        let _ = o.transfer_with_burn(&alice_pk, recipient, 10 * MICRO, &alice);
        o.seal_block(&alice_pk, 0.0)
    }

    /// Deliver `block` (wrapped in a fresh signed `NewBlock` envelope) to a
    /// brand new core at injected time `now_ms`, and return the core for
    /// inspection.
    fn integrate_block_via_core(block: &Block, now_ms: u64) -> Node {
        let sealer = seeded_identity(6); // signs the ENVELOPE (not the block)
        let env = signed_envelope(
            &sealer,
            GossipMessage::NewBlock {
                block_json: serde_json::to_string(block).unwrap(),
            },
            now_ms,
        );
        let mut node = Node::new();
        let mut rng = Blake3Rng::from_seed(31);
        node.handle(Event::Tick { now_ms }, &mut rng);
        node.handle(
            Event::MessageReceived {
                from: PeerId(sealer.get_identity().unwrap().public_key_hex),
                bytes: env,
            },
            &mut rng,
        );
        node
    }

    /// **C4 (reorg — the most audited path).** A same-height fork resolved by
    /// the deterministic hash tie-break (higher hash wins) must, through
    /// the core, (1) switch the tip to the winner and (2) **re-queue the
    /// loser's exclusive txs** so no validated tx is lost (AUDIT-BLK-1). We
    /// integrate the lower-hash block first, then the higher-hash block
    /// (which triggers the reorg). Both transfers survive: the winner's in
    /// the chain, the loser's in the mempool — so both recipients stay
    /// credited.
    ///
    /// NB: the implemented rule is a single-block **same-height** tie-break,
    /// not a multi-block heaviest-chain reorg — this test exercises the
    /// real code.
    #[test]
    fn fork_reorg_through_core_preserves_all_txs() {
        let now0 = 1_800_000_000_000_u64;
        let bob = "b".repeat(64);
        let carol = "c".repeat(64);
        let block_bob = sealed_block_with_transfer(&bob, 50 * MICRO);
        let block_carol = sealed_block_with_transfer(&carol, 50 * MICRO);

        // Lower hash integrated first; higher hash arrives second and wins.
        let (first, second) = if block_bob.hash < block_carol.hash {
            (&block_bob, &block_carol)
        } else {
            (&block_carol, &block_bob)
        };
        let winner_hash = second.hash.clone();

        let sealer = seeded_identity(6);
        let sealer_pk = sealer.get_identity().unwrap().public_key_hex;
        let envelope = |b: &Block| {
            signed_envelope(
                &sealer,
                GossipMessage::NewBlock {
                    block_json: serde_json::to_string(b).unwrap(),
                },
                now0,
            )
        };

        let mut node = Node::new();
        let mut rng = Blake3Rng::from_seed(33);
        node.handle(Event::Tick { now_ms: now0 }, &mut rng);
        node.handle(
            Event::MessageReceived {
                from: PeerId(sealer_pk.clone()),
                bytes: envelope(first),
            },
            &mut rng,
        );
        node.handle(
            Event::MessageReceived {
                from: PeerId(sealer_pk),
                bytes: envelope(second),
            },
            &mut rng,
        );

        // Tip switched to the higher-hash winner.
        assert_eq!(
            node.ledger().chain.last().map(|b| b.hash.clone()),
            Some(winner_hash),
            "the higher-hash block must win the tie-break"
        );
        // Both transfers survive — neither bob nor carol is zeroed out.
        let net = 10 * MICRO - (10 * MICRO / 100); // amount net of the 1% burn
        assert_eq!(
            node.ledger().balance_of(&bob),
            net,
            "bob's transfer must survive the reorg (chain or re-queued mempool)"
        );
        assert_eq!(
            node.ledger().balance_of(&carol),
            net,
            "carol's transfer must survive the reorg"
        );
        // The loser's exclusive USER txs (transfer + burn) are re-queued; its
        // synthetic mining reward is NOT (EMIT-1 §4.1 — re-queuing it would
        // double-mint). So 2 re-queued, not 3.
        assert_eq!(
            node.ledger().stats().pending,
            2,
            "the loser's transfer + burn are re-queued (AUDIT-BLK-1); its mining \
             reward is excluded (EMIT-1 §4.1)"
        );
        assert_eq!(node.telemetry().blocks_integrated, 2);
        assert_eq!(node.telemetry().blocks_rejected, 0);
    }

    /// **C4 (rejection — inconsistent tx set vs committed root).** Dropping a
    /// tx from a sealed block (without re-sealing) leaves the committed
    /// block hash covering a tx set that no longer matches → the recomputed
    /// Merkle/hash mismatches and the core rejects it, chain unchanged.
    #[test]
    fn core_rejects_block_with_inconsistent_merkle_root() {
        let now0 = 1_800_000_000_000_u64;
        let mut block = sealed_block_with_transfer(&"r".repeat(64), 50 * MICRO);
        block.transactions.pop(); // tx set no longer matches the committed hash
        let node = integrate_block_via_core(&block, now0);
        assert_eq!(node.telemetry().blocks_rejected, 1);
        assert_eq!(node.telemetry().blocks_integrated, 0);
        assert_eq!(node.ledger().chain.len(), 1, "chain must stay at genesis");
    }

    /// **C4 (rejection — emission cap).** A block minting far above the
    /// per-block emission bound is refused at the consensus guard
    /// (`validate_block_emission`) through the core, chain unchanged.
    #[test]
    fn core_rejects_block_exceeding_emission_cap() {
        let now0 = 1_800_000_000_000_u64;
        // 500 QUANTA minted in one block ≫ the per-block bound (~128 at genesis).
        let block = sealed_block_with_transfer(&"r".repeat(64), 500 * MICRO);
        let node = integrate_block_via_core(&block, now0);
        assert_eq!(node.telemetry().blocks_rejected, 1);
        assert_eq!(node.telemetry().blocks_integrated, 0);
        assert_eq!(node.ledger().chain.len(), 1, "over-emission block rejected");
    }

    /// **C4 (rejection — bad contained-tx signature).** Corrupting the
    /// signature of a signed tx inside an otherwise-valid block makes
    /// `verify_tx` fail during block validation → the core rejects the
    /// whole block.
    #[test]
    fn core_rejects_block_with_invalid_contained_tx_signature() {
        let now0 = 1_800_000_000_000_u64;
        let mut block = sealed_block_with_transfer(&"r".repeat(64), 50 * MICRO);
        // Corrupt the first signed (non-synthetic) tx's signature.
        if let Some(tx) = block
            .transactions
            .iter_mut()
            .find(|t| !t.signature.is_empty())
        {
            tx.signature = "0".repeat(128); // valid hex, wrong bytes → verify
                                            // fails
        }
        let node = integrate_block_via_core(&block, now0);
        assert_eq!(node.telemetry().blocks_rejected, 1);
        assert_eq!(node.telemetry().blocks_integrated, 0);
        assert_eq!(node.ledger().chain.len(), 1, "chain must stay at genesis");
    }

    // ── C5: single signature-gated tx-admission entry point (typestate) ───────

    /// **C5.** The `VerifiedTx` token can only be minted by verifying the
    /// signature, and `apply_verified_remote_tx` (the single authoritative
    /// admission entry point, shared by core and shell) *requires* that token —
    /// so an unverified tx cannot reach the linear ledger by mistake. A valid
    /// signature mints the token; a tampered one yields `None`. (The "can't
    /// apply without a token" guarantee is enforced at COMPILE time by the
    /// type, hence nothing to assert at runtime — that is the point.)
    #[test]
    fn verified_tx_typestate_gates_admission_on_signature() {
        use crate::p2p::ledger::VerifiedTx;
        let alice = seeded_identity(2);
        let alice_pk = addr_of(&alice); // PQ-MIG-3B: value identity = ML-DSA address
        let mut origin = Ledger::new();
        origin.mine_tx(&alice_pk, 50 * MICRO, 0.0);
        let (xfer, _burn, _amt) = origin
            .transfer_with_burn(&alice_pk, &"r".repeat(64), 10 * MICRO, &alice)
            .expect("alice transfer builds");

        // A validly-signed tx mints the verification token.
        assert!(
            VerifiedTx::new(xfer.clone()).is_some(),
            "a valid signature must mint a VerifiedTx token"
        );
        // Tampering the **authority** signature makes the token un-mintable — the
        // single gate. PQ-MIG-3B: authority is PURE ML-DSA (`pq_signature`); the
        // Ed25519 `signature` is a vestigial co-factor off the authority path, so
        // corrupting it would NOT reject. We corrupt the ML-DSA signature (same
        // length ⇒ still valid hex, wrong bytes) — that is what the gate checks.
        let mut tampered = xfer;
        if let Some(sig) = tampered.pq_signature.as_mut() {
            *sig = "0".repeat(sig.len());
        }
        assert!(
            VerifiedTx::new(tampered).is_none(),
            "a tampered ML-DSA authority signature must be rejected by the single admission gate"
        );
    }

    // ── C7: deterministic, leader-gated block production in the core ──────────

    /// **C7.** An elected PoS leader, with pending txs, seals a valid block at
    /// **injected** time and emits it as a signed `NewBlock` broadcast — the
    /// core's first PRODUCED (not just validated) block. The sealed block's
    /// timestamp and the carrying envelope both come from `now_ms`; the block
    /// is a valid, fresh, signed envelope any peer would accept.
    #[test]
    fn elected_leader_seals_a_valid_block_at_injected_time() {
        let mut node = Node::with_identity(seeded_identity(1));
        // PQ-MIG-3B: the validator identity, the elected-proposer key, and the
        // block's miner-reward target are all the ML-DSA **address** now
        // (`validator_stakes()` is address-keyed). The envelope stays Ed25519-signed.
        let pk = addr_of(&seeded_identity(1));
        node.ledger_mut().mine_tx(&pk, 5 * MICRO, 0.0); // something to seal

        // We are the only staked validator → we are the elected leader.
        let validators = vec![Validator {
            pk: pk.clone(),
            stake: MIN_VALIDATOR_STAKE,
            reputation: 0,
        }];
        let now_ms = 1_800_000_000_000_u64;
        let height_before = node.ledger().chain.len();

        let effect = node.propose_block_at(now_ms, &validators);
        let Some(Effect::Broadcast { bytes }) = effect else {
            panic!("an elected leader with pending txs must emit a NewBlock broadcast");
        };
        // The chain grew by one block.
        assert_eq!(node.ledger().chain.len(), height_before + 1);
        // The broadcast is a valid, fresh, signed NewBlock at the injected time.
        let now_secs = (now_ms / 1_000) as i64;
        let env = crate::p2p::dispatcher::validate_envelope_at(&bytes, now_secs)
            .expect("the sealed NewBlock must be a valid signed envelope");
        let GossipMessage::NewBlock { block_json } = env.payload else {
            panic!("expected a NewBlock payload");
        };
        let block: Block = serde_json::from_str(&block_json).unwrap();
        assert_eq!(block.miner, pk, "we sealed it");
        assert_eq!(
            block.timestamp,
            super::millis_to_rfc3339(now_ms),
            "the block timestamp is the injected time"
        );
    }

    /// **C7.** A node that is NOT the elected leader (someone else holds the
    /// stake) does not seal, even with pending txs — the chain is untouched.
    /// A small injected `elapsed` keeps us out of the fallback/after-timeout
    /// windows, so only the primary may propose.
    #[test]
    fn non_leader_does_not_seal() {
        let mut node = Node::with_identity(seeded_identity(1));
        // PQ-MIG-3B: address-keyed identities (value path) — see C7 above.
        let pk = addr_of(&seeded_identity(1));
        node.ledger_mut().mine_tx(&pk, 5 * MICRO, 0.0);

        // Someone else is the sole staked validator → they are the leader.
        let other = addr_of(&seeded_identity(2));
        let validators = vec![Validator {
            pk: other,
            stake: MIN_VALIDATOR_STAKE,
            reputation: 0,
        }];
        // Just after genesis → small elapsed → no fallback / after-timeout path.
        let genesis_secs = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00+00:00")
            .unwrap()
            .timestamp() as u64;
        let now_ms = (genesis_secs + 5) * 1_000;
        let height_before = node.ledger().chain.len();

        let effect = node.propose_block_at(now_ms, &validators);
        assert!(effect.is_none(), "a non-leader must not seal");
        assert_eq!(
            node.ledger().chain.len(),
            height_before,
            "the chain must be untouched when we are not the leader"
        );
    }

    /// **C7.** Block production is byte-deterministic: identical seed, pending,
    /// and injected time give identical sealed-block broadcast bytes via a
    /// deterministic timestamp, Ed25519 signature, and out-nonce. Uses a FROZEN
    /// pending tx so the pending content is identical across runs.
    #[test]
    fn block_proposal_is_byte_deterministic() {
        // PQ-MIG-3B: address-keyed validator + miner target (see C7 above).
        let pk = addr_of(&seeded_identity(1));
        // One frozen mining tx → identical pending content in every run.
        let frozen_tx = {
            let mut o = Ledger::new();
            o.mine_tx(&pk, 5 * MICRO, 0.0)
        };
        let validators = vec![Validator {
            pk: pk.clone(),
            stake: MIN_VALIDATOR_STAKE,
            reputation: 0,
        }];
        let now_ms = 1_800_000_000_000_u64;

        let build = || {
            let mut node = Node::with_identity(seeded_identity(1));
            node.ledger_mut().replay_remote_tx(frozen_tx.clone());
            node.propose_block_at(now_ms, &validators)
        };
        assert!(build().is_some(), "the elected leader must seal");
        assert_eq!(
            build(),
            build(),
            "block proposal must be byte-deterministic"
        );
    }

    // ── BLK-HASH-1: reward theft rejected through the core ────────────────────

    /// **BLK-HASH-1 T2.** Take a valid block, redirect the mining reward AND
    /// the `miner` field to an attacker but leave the hash stale (the old
    /// scheme committed neither `miner` nor tx content, so it wouldn't
    /// notice). The core must now **reject** it (hash mismatch); the
    /// attacker gets nothing.
    #[test]
    fn blk_hash_1_reward_theft_is_rejected_by_core() {
        let now0 = 1_800_000_000_000_u64;
        let honest = seeded_identity(3);
        let honest_pk = honest.get_identity().unwrap().public_key_hex;

        let mut origin = Ledger::new();
        origin.mine_tx(&honest_pk, 50 * MICRO, 0.0);
        let mut block = origin.seal_block(&honest_pk, 0.0);

        // ATTACK: steal the reward + miner credit, keep the (now stale) hash.
        let attacker = "9".repeat(64);
        if let Some(tx) = block.transactions.iter_mut().find(|t| t.from == "NETWORK") {
            tx.to = attacker.clone();
        }
        block.miner = attacker.clone();

        let bytes = signed_envelope(
            &honest,
            GossipMessage::NewBlock {
                block_json: serde_json::to_string(&block).unwrap(),
            },
            now0,
        );
        let mut node = Node::new();
        let mut rng = Blake3Rng::from_seed(99);
        node.handle(Event::Tick { now_ms: now0 }, &mut rng);
        node.handle(
            Event::MessageReceived {
                from: PeerId("p".into()),
                bytes,
            },
            &mut rng,
        );

        assert_eq!(
            node.telemetry().blocks_rejected,
            1,
            "a reward-theft block must be rejected"
        );
        assert_eq!(node.ledger().chain.len(), 1, "chain unchanged");
        assert_eq!(
            node.ledger().balance_of(&attacker),
            0,
            "the attacker stole nothing"
        );
    }

    // ── EMIT-1: one reward per block, enforced through the core ───────────────

    /// **EMIT-1 E2 — two mining rewards in one block, rejected by the core.** A
    /// malicious sealer forges a height-1 block with TWO `NETWORK→miner` mining
    /// txs whose sum slips under the per-block emission bound (which only checks
    /// the total). The block hash is CORRECT (the attacker can compute it), so
    /// only the new count rule (§4.2) can stop it — and it must.
    #[test]
    fn emit_1_two_mining_rewards_in_one_block_is_rejected_by_core() {
        let now0 = 1_800_000_000_000_u64;
        let pk = seeded_identity(3).get_identity().unwrap().public_key_hex;

        // Two small NETWORK→pk mining txs (sum 10 QUANTA ≪ the ~128 bound).
        let mut o = Ledger::new();
        let m1 = o.mine_tx(&pk, 5 * MICRO, 0.0);
        let m2 = o.mine_tx(&pk, 5 * MICRO, 0.0);
        let genesis_hash = o.block_at(0).unwrap().hash.clone();
        let bad = Ledger::forge_block_at(
            1,
            &genesis_hash,
            &super::millis_to_rfc3339(now0),
            &pk,
            vec![m1, m2],
        );

        let node = integrate_block_via_core(&bad, now0);
        assert_eq!(
            node.telemetry().blocks_rejected,
            1,
            "two mining rewards in one block must be rejected (count rule)"
        );
        assert_eq!(node.telemetry().blocks_integrated, 0);
        assert_eq!(node.ledger().chain.len(), 1, "chain stays at genesis");
    }

    /// **EMIT-1 E3 — reward credited to a non-miner, rejected by the core.** A
    /// block with exactly ONE mining tx, but crediting an attacker while
    /// `block.miner` is someone else, is rejected by the recipient rule
    /// (`to == block.miner`). Belt-and-suspenders with BLK-HASH-1.
    #[test]
    fn emit_1_mining_reward_to_non_miner_is_rejected_by_core() {
        let now0 = 1_800_000_000_000_u64;
        let honest_pk = seeded_identity(3).get_identity().unwrap().public_key_hex;
        let attacker = "9".repeat(64);

        // ONE mining tx NETWORK→attacker, but the block claims miner = honest.
        let mut o = Ledger::new();
        let stolen = o.mine_tx(&attacker, 5 * MICRO, 0.0);
        let genesis_hash = o.block_at(0).unwrap().hash.clone();
        let bad = Ledger::forge_block_at(
            1,
            &genesis_hash,
            &super::millis_to_rfc3339(now0),
            &honest_pk,
            vec![stolen],
        );

        let node = integrate_block_via_core(&bad, now0);
        assert_eq!(
            node.telemetry().blocks_rejected,
            1,
            "a reward crediting someone other than the block miner must be rejected"
        );
        assert_eq!(node.telemetry().blocks_integrated, 0);
        assert_eq!(
            node.ledger().balance_of(&attacker),
            0,
            "the attacker is credited nothing"
        );
        assert_eq!(node.ledger().chain.len(), 1, "chain stays at genesis");
    }
}
