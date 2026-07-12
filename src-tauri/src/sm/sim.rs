//! `Sim` — the **deterministic simulation shell** (DST harness, T0.4).
//!
//! Mono-thread event loop over a virtual clock and a **seeded** RNG. Events are
//! totally ordered by `(time_ms, seq)` where `seq` is a monotonic insertion
//! counter — the tie-break that makes the whole run reproducible
//! (`QUANTA_T0_DST_HARNESS.md` §3.1). The simulation shell never reads the real
//! clock and never does real I/O: it drives [`Node`]s purely through
//! `Event`/`Effect`, so two runs with the same seed produce an identical trace.
//!
//! Virtual network: the **transport-flood** model decided in C8 (iroh-gossip
//! floods at the transport level) — an [`Effect::Broadcast`] is delivered as
//! `Event::MessageReceived` to **every other** node, [`Effect::Send`] to one —
//! through a [`NetFaults`] profile (T0.5): drop, duplicate, variable delay
//! (⇒ reorder), graph **partition**, and directed **withhold** (retention), all
//! drawn from the seeded RNG so any fault pattern is reproducible. T0.6 adds
//! **node crash/restart** and **byzantine** scenarios (equivocation + a
//! detection primitive), each scenarizable and reproducible.
//!
//! Block production is **event-driven** (tr.2): a node arms a consensus seal
//! timer on `Tick` and proposes on `TimerFired`. [`Sim::propose`] remains as an
//! orchestrated kick for scenarios that want to place a block
//! deterministically.

use super::{
    effect::Effect,
    event::{Event, PeerId},
    finality::{Checkpoint, EPOCH_LENGTH_BLOCKS},
    rng::{Blake3Rng, Rng},
    Node,
};
use crate::p2p::{ledger::TxType, pos_consensus::Validator};
use std::{
    cmp::{Ordering, Reverse},
    collections::{BTreeMap, BTreeSet, BinaryHeap},
};

/// Default link delay (ms) for a lossless link.
const NET_DELAY_MS: u64 = 5;

/// T0.5: injectable network faults — drop, duplication, variable delay (which
/// also produces **reorder**), and **graph partition** — all driven by the
/// simulator's seeded RNG so any fault pattern is **reproducible by seed**.
/// Probabilities are integer parts-per-million for cross-platform determinism
/// (no float). The default is a lossless `NET_DELAY_MS` link.
#[derive(Clone)]
struct NetFaults {
    /// P(drop a delivery) = `drop_ppm / 1_000_000`.
    drop_ppm: u64,
    /// P(also deliver a duplicate) = `dup_ppm / 1_000_000`.
    dup_ppm: u64,
    /// Inclusive delay range [min, max] ms; `max > min` ⇒ variable ⇒ reorder.
    min_delay_ms: u64,
    max_delay_ms: u64,
    /// A graph partition: nodes on opposite sides cannot reach each other.
    partition: Option<(BTreeSet<PeerId>, BTreeSet<PeerId>)>,
    /// T0.6: **directed** cuts `(from → to)` that are dropped — models
    /// byzantine **block retention** (a node delivers to some peers but
    /// withholds from others), distinct from the symmetric `partition`.
    withheld: BTreeSet<(PeerId, PeerId)>,
}

impl Default for NetFaults {
    fn default() -> Self {
        // Lossless: fixed delay, no drop/dup/partition (the tr.1/2 behaviour).
        Self {
            drop_ppm: 0,
            dup_ppm: 0,
            min_delay_ms: NET_DELAY_MS,
            max_delay_ms: NET_DELAY_MS,
            partition: None,
            withheld: BTreeSet::new(),
        }
    }
}

impl NetFaults {
    /// Is a delivery `a → b` cut — by a symmetric partition OR a directed
    /// withhold (retention)?
    fn blocked(&self, a: &PeerId, b: &PeerId) -> bool {
        if self.withheld.contains(&(a.clone(), b.clone())) {
            return true;
        }
        match &self.partition {
            None => false,
            Some((s1, s2)) => {
                (s1.contains(a) && s2.contains(b)) || (s2.contains(a) && s1.contains(b))
            }
        }
    }

    /// Sample a delivery delay in `[min, max]` from the seeded RNG.
    fn sample_delay(&self, rng: &mut Blake3Rng) -> u64 {
        if self.max_delay_ms <= self.min_delay_ms {
            return self.min_delay_ms;
        }
        let span = self.max_delay_ms - self.min_delay_ms + 1;
        self.min_delay_ms + (rng.next_u64() % span)
    }
}

/// One scheduled event. Ordered **only** by `(time_ms, seq)` so the heap gives
/// a total, reproducible order even when many events share a virtual instant.
struct Scheduled {
    time_ms: u64,
    seq: u64,
    node: PeerId,
    event: Event,
}

impl PartialEq for Scheduled {
    fn eq(&self, other: &Self) -> bool {
        self.time_ms == other.time_ms && self.seq == other.seq
    }
}
impl Eq for Scheduled {}
impl Ord for Scheduled {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.time_ms, self.seq).cmp(&(other.time_ms, other.seq))
    }
}
impl PartialOrd for Scheduled {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// An invariant violation found by the harness (T0.7). Carries the `seed` so
/// the exact run is replayable, plus enough detail to pinpoint the failure.
/// `Clone` lets the T0.8 sweep collect violations into a report without moving
/// them out of the per-seed outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Violation {
    /// **Safety/agreement broken**: two nodes hold a *different* block at the
    /// same chain index — correct nodes disagreed on a committed value.
    Safety {
        seed: u64,
        index: u64,
        node_a: PeerId,
        hash_a: String,
        node_b: PeerId,
        hash_b: String,
    },
    /// **Finality safety broken** (GADGET-1): two nodes hold a *conflicting*
    /// finalized checkpoint at the **same epoch** — they disagree on history
    /// that is supposed to be **irreversible**. This is the dual of `Safety` one
    /// layer up: `Safety` is per-block agreement on the still-reorderable chain;
    /// this is per-epoch agreement on the **finalized** set. The two-step
    /// justify/finalize rule that makes this reachable on a real run is GADGET-3;
    /// in GADGET-1 only genesis is finalized, so only a **planted** conflict can
    /// trip it today — which the teeth test does, proving the checker bites
    /// before finalization exists. Carries the `seed` (replay) and the epoch.
    /// Mirrors [`Violation::Safety`] with `epoch` in place of `index`; the
    /// boundary height is derivable (`epoch × E`) so it is not duplicated.
    FinalitySafety {
        seed: u64,
        epoch: u64,
        node_a: PeerId,
        hash_a: String,
        node_b: PeerId,
        hash_b: String,
    },
    /// **Conservation broken**: `Σ spendable + locked_stake + burned != minted`
    /// for a node — µQTA appeared or vanished (e.g. a negative balance, a phantom
    /// credit). ONCHAIN-STAKE-1 §5: coins locked in stake (the `STAKE` sink:
    /// bonded + unbonding + any pending stake) are **locked, not destroyed**, so
    /// they MUST be counted on the balances side — otherwise staking would *look*
    /// like burning and trip this falsely.
    Conservation {
        seed: u64,
        node: PeerId,
        sum_balances: u64,
        locked_stake: u64,
        burned: u64,
        minted: u64,
    },
    /// **Emission broken** (EMIT-1): a chain block carries more than one mining
    /// reward — a double-mint (a re-queued losing reward) or a forged
    /// multi-reward block. Production seals exactly one coalesced reward per
    /// block, so any block with >1 is illegitimate. Conservation is blind to
    /// this (the extra µQTA is backed by a real, if illegal, mining tx), so
    /// this is the invariant that makes the illegitimate mint visible.
    Emission {
        seed: u64,
        node: PeerId,
        index: u64,
        mining_count: usize,
    },
    /// **Emission AMOUNT broken** (FORK-CAP-1): a chain block mints MORE than the
    /// legitimate per-block bound / pushes total supply past the hard cap —
    /// checked with the **same** production rule
    /// ([`Ledger::validate_block_emission_against`]) against the supply that
    /// existed *before* the block. The count-only [`Violation::Emission`] above
    /// is blind to a single over-sized reward; this is the backstop that makes
    /// a regression of the fork-reorg emission gate scream in the sweep.
    EmissionAmount {
        seed: u64,
        node: PeerId,
        index: u64,
        block_minted: u64,
        prior_mined: u64,
        reason: String,
    },
}

/// The deterministic simulator: virtual clock, seeded RNG, `(time_ms, seq)`
/// priority queue, and a set of nodes wired by a flood network.
struct Sim {
    /// The seed this run was built from — echoed in any [`Violation`] so a
    /// failure is replayable (harness T0.7: "show the seed and the trace").
    seed: u64,
    clock_ms: u64,
    rng: Blake3Rng,
    /// Min-heap on `(time_ms, seq)` via `Reverse`.
    queue: BinaryHeap<Reverse<Scheduled>>,
    seq: u64,
    nodes: BTreeMap<PeerId, Node>,
    /// T0.5: the network fault profile (default lossless).
    faults: NetFaults,
    /// T0.6: nodes that are currently **crashed/offline** — every event
    /// targeted at them is dropped (their state is retained for restart).
    crashed: BTreeSet<PeerId>,
    /// Per-step record `(time_ms, node, effects)` — the byte-comparable trace.
    trace: Vec<(u64, PeerId, Vec<Effect>)>,
}

impl Sim {
    fn new(seed: u64, nodes: Vec<(PeerId, Node)>) -> Self {
        Self {
            seed,
            clock_ms: 0,
            rng: Blake3Rng::from_seed(seed),
            queue: BinaryHeap::new(),
            seq: 0,
            nodes: nodes.into_iter().collect(),
            faults: NetFaults::default(),
            crashed: BTreeSet::new(),
            trace: Vec::new(),
        }
    }

    /// Install a network fault profile (T0.5).
    fn set_faults(&mut self, faults: NetFaults) {
        self.faults = faults;
    }

    /// Partition the graph into two sides that cannot reach each other.
    fn partition(&mut self, side_a: &[PeerId], side_b: &[PeerId]) {
        self.faults.partition = Some((
            side_a.iter().cloned().collect(),
            side_b.iter().cloned().collect(),
        ));
    }

    /// Heal any partition (cross-side delivery resumes).
    fn heal(&mut self) {
        self.faults.partition = None;
    }

    /// T0.6: a byzantine `from` withholds deliveries to `to` (directed cut).
    fn withhold(&mut self, from: &PeerId, to: &PeerId) {
        self.faults.withheld.insert((from.clone(), to.clone()));
    }

    /// T0.6: crash a node — it stops processing events (its state is kept).
    fn crash(&mut self, id: &PeerId) {
        self.crashed.insert(id.clone());
    }

    /// T0.6: restart a crashed node — it resumes processing (behind, so it must
    /// catch up via chain sync).
    fn restart(&mut self, id: &PeerId) {
        self.crashed.remove(id);
    }

    fn node_mut(&mut self, id: &PeerId) -> &mut Node {
        self.nodes.get_mut(id).expect("node exists (test scenario)")
    }

    /// Add a node mid-run (a late joiner). It starts at genesis and only sees
    /// traffic scheduled after it joined — so it must catch up via chain sync.
    fn add_node(&mut self, id: PeerId, node: Node) {
        self.nodes.insert(id, node);
    }

    /// Enqueue `event` for `node` at virtual time `at_ms` (monotonic `seq`).
    fn schedule(&mut self, at_ms: u64, node: PeerId, event: Event) {
        self.seq += 1;
        self.queue.push(Reverse(Scheduled {
            time_ms: at_ms,
            seq: self.seq,
            node,
            event,
        }));
    }

    /// Deliver a `Tick { now_ms }` to every node (sorted key order →
    /// deterministic).
    fn tick_all(&mut self, now_ms: u64) {
        let ids: Vec<PeerId> = self.nodes.keys().cloned().collect();
        for id in ids {
            self.schedule(now_ms, id, Event::Tick { now_ms });
        }
    }

    /// Route the `effects` a node produced through the virtual network.
    /// Records the trace first (so even dropped/ignored effects are
    /// observable), then applies the transport-flood model.
    fn route(&mut self, from: PeerId, now_ms: u64, effects: Vec<Effect>) {
        self.trace.push((now_ms, from.clone(), effects.clone()));
        let peers: Vec<PeerId> = self.nodes.keys().cloned().collect();
        for e in effects {
            match e {
                // C8 transport-flood: deliver to every OTHER node (through the
                // fault model: partition / drop / delay / duplicate).
                Effect::Broadcast { bytes } => {
                    for peer in &peers {
                        if *peer != from {
                            self.deliver(&from, peer, now_ms, &bytes);
                        }
                    }
                }
                Effect::Send { to, bytes } => {
                    if self.nodes.contains_key(&to) {
                        self.deliver(&from, &to, now_ms, &bytes);
                    }
                }
                Effect::SetTimer { id, fire_at_ms } => {
                    self.schedule(fire_at_ms, from.clone(), Event::TimerFired { id });
                }
                // Slice 1: timers-cancel / persistence / UI aren't modelled yet
                // (disk recovery = T0.5+/§3.4). They stay in the trace above.
                Effect::CancelTimer { .. } | Effect::Persist { .. } | Effect::Emit(_) => {}
            }
        }
    }

    /// Deliver `bytes` from `from` to `to` through the **fault model** (T0.5):
    /// partition cut, then drop, then a delay-scheduled `MessageReceived`, plus
    /// an optional duplicate — every decision drawn from the seeded RNG so the
    /// fault pattern is reproducible. RNG draws happen in a fixed order
    /// (sorted-key flood, effect order), keeping the whole run deterministic.
    fn deliver(&mut self, from: &PeerId, to: &PeerId, now_ms: u64, bytes: &[u8]) {
        // 1. Partition: opposite sides cannot communicate.
        if self.faults.blocked(from, to) {
            return;
        }
        // 2. Drop.
        if self.faults.drop_ppm > 0 && (self.rng.next_u64() % 1_000_000) < self.faults.drop_ppm {
            return;
        }
        // 3. Variable delay (⇒ reorder when max > min).
        let delay = self.faults.sample_delay(&mut self.rng);
        self.schedule(
            now_ms.saturating_add(delay),
            to.clone(),
            Event::MessageReceived {
                from: from.clone(),
                bytes: bytes.to_vec(),
            },
        );
        // 4. Duplicate (idempotency in the core absorbs it).
        if self.faults.dup_ppm > 0 && (self.rng.next_u64() % 1_000_000) < self.faults.dup_ppm {
            let delay2 = self.faults.sample_delay(&mut self.rng);
            self.schedule(
                now_ms.saturating_add(delay2),
                to.clone(),
                Event::MessageReceived {
                    from: from.clone(),
                    bytes: bytes.to_vec(),
                },
            );
        }
    }

    /// Run the event loop until the queue drains or `max_steps` is reached.
    /// Each step advances the virtual clock to the event's time and feeds it to
    /// the target node; the node's effects re-enter the network.
    fn run(&mut self, max_steps: u64) {
        let mut steps = 0;
        while let Some(Reverse(s)) = self.queue.pop() {
            if steps >= max_steps {
                break;
            }
            steps += 1;
            self.clock_ms = s.time_ms;
            // T0.6: a crashed node processes nothing — drop the event.
            if self.crashed.contains(&s.node) {
                continue;
            }
            // Direct field access (not `node_mut`) so `self.nodes` and `self.rng`
            // are borrowed as disjoint fields.
            let node = self
                .nodes
                .get_mut(&s.node)
                .expect("node exists (test scenario)");
            let effects = node.handle(s.event, &mut self.rng);
            self.route(s.node, s.time_ms, effects);
        }
    }

    /// Run the loop but stop **before** any event scheduled after `horizon_ms`.
    /// Lets a periodic timer (e.g. the seal timer) fire within a bounded window
    /// without churning indefinitely past it.
    fn run_until(&mut self, horizon_ms: u64) {
        while let Some(next) = self.queue.peek().map(|Reverse(s)| s.time_ms) {
            if next > horizon_ms {
                break;
            }
            let Reverse(s) = self.queue.pop().expect("peeked above");
            self.clock_ms = s.time_ms;
            // T0.6: a crashed node processes nothing — drop the event.
            if self.crashed.contains(&s.node) {
                continue;
            }
            let node = self
                .nodes
                .get_mut(&s.node)
                .expect("node exists (test scenario)");
            let effects = node.handle(s.event, &mut self.rng);
            self.route(s.node, s.time_ms, effects);
        }
    }

    /// T0.7 + EMIT-1: check the consensus invariants over the CURRENT state of
    /// all nodes.
    /// - **Safety**: no two nodes hold a different block at the same chain
    ///   index (genesis at index 0 is shared by construction).
    /// - **Emission**: every block carries **at most** one mining reward.
    ///   EMIT-1-VERIFY fixed the form as `≤`, *not* `== height−1`: a block may
    ///   legitimately carry **zero** rewards — a user-transfer-only block
    ///   (`coalesce_block_rewards` injects nothing when none is pending, and
    ///   `validate_block_against_prev` bounds the reward only *if present*; cf.
    ///   `int2`'s reward-free block). So this catches only **over**-emission
    ///   (the double-mint); a *missing* reward is invisible here but harmless —
    ///   under-emission only deepens scarcity, never breaching the 100M cap.
    ///   Checked **before** conservation: an illegitimate mint (a re-queued or
    ///   forged reward) is backed by a real mining tx, so
    ///   `Σ balances + burned == minted` still holds — only this structural
    ///   check sees the extra reward.
    /// - **Conservation**: `Σ balances + burned == minted` for every node.
    ///
    /// Runs in deterministic (sorted-key) order so the *first* violation found
    /// is itself reproducible.
    fn check_invariants(&self) -> Result<(), Violation> {
        // Safety: index → (first node seen there, its block hash).
        let mut by_index: BTreeMap<u64, (PeerId, String)> = BTreeMap::new();
        for (id, node) in &self.nodes {
            for (i, block) in node.ledger().chain.iter().enumerate() {
                let idx = i as u64;
                match by_index.get(&idx) {
                    Some((other, hash)) if *hash != block.hash => {
                        return Err(Violation::Safety {
                            seed: self.seed,
                            index: idx,
                            node_a: other.clone(),
                            hash_a: hash.clone(),
                            node_b: id.clone(),
                            hash_b: block.hash.clone(),
                        });
                    }
                    None => {
                        by_index.insert(idx, (id.clone(), block.hash.clone()));
                    }
                    _ => {}
                }
            }
        }
        // Finality safety (GADGET-1): across nodes, never two CONFLICTING
        // finalized checkpoints at the same epoch — the finalized sets must
        // coincide on every shared epoch. Mirrors the `Safety` scan above, but
        // keyed by EPOCH over each node's finalized set instead of by chain
        // index. Nothing finalizes past genesis until GADGET-3, so on the honest
        // path every node only holds epoch-0 genesis (which agrees) — today this
        // can only fire on a PLANTED conflict, and the teeth test proves it does.
        // `nodes` is a `BTreeMap` and `finalized().iter()` is epoch-ordered, so
        // the first violation found is reproducible (spec §3, §4 determinism).
        let mut finalized_by_epoch: BTreeMap<u64, (PeerId, Checkpoint)> = BTreeMap::new();
        for (id, node) in &self.nodes {
            for cp in node.finalized().iter() {
                match finalized_by_epoch.get(&cp.epoch) {
                    // Same epoch ⇒ same boundary height by construction, so a
                    // mismatch on the full checkpoint reduces to a hash conflict
                    // (a different block finalized for that epoch).
                    Some((other, seen)) if seen != cp => {
                        return Err(Violation::FinalitySafety {
                            seed: self.seed,
                            epoch: cp.epoch,
                            node_a: other.clone(),
                            hash_a: seen.hash.clone(),
                            node_b: id.clone(),
                            hash_b: cp.hash.clone(),
                        });
                    }
                    None => {
                        finalized_by_epoch.insert(cp.epoch, (id.clone(), cp.clone()));
                    }
                    _ => {}
                }
            }
        }
        // Emission (EMIT-1 §4.3): at MOST one mining reward per block — zero is
        // legitimate (a user-only block), so the `> 1` test is the intended `≤`
        // form, not `==` (EMIT-1-VERIFY; see the method doc). A block with >1
        // `Mining` tx is a double-mint (a re-queued losing reward) or a forged
        // multi-reward block — illegitimate, even though conservation can't see
        // it. Checked before conservation so it surfaces first.
        for (id, node) in &self.nodes {
            for (i, block) in node.ledger().chain.iter().enumerate() {
                let mining = block
                    .transactions
                    .iter()
                    .filter(|t| t.tx_type == TxType::Mining)
                    .count();
                if mining > 1 {
                    return Err(Violation::Emission {
                        seed: self.seed,
                        node: id.clone(),
                        index: i as u64,
                        mining_count: mining,
                    });
                }
            }
        }
        // FORK-CAP-1: emission AMOUNT backstop. The count check above is blind
        // to a single OVER-sized reward; here every block is re-validated with
        // the SAME production rule (`validate_block_emission_against`) against
        // the supply that existed BEFORE it (Σ mining in earlier blocks). A
        // block production accepted (happy path or fork-reorg) passes
        // identically; if the fork-reorg emission gate ever regressed and let an
        // over-emitter in, this fires with the seed. Integer-only prior
        // accumulation, sorted-key node iteration (§4 determinism).
        for (id, node) in &self.nodes {
            let mut prior_mined: u64 = 0;
            for (i, block) in node.ledger().chain.iter().enumerate() {
                let block_minted: u64 = block
                    .transactions
                    .iter()
                    .filter(|t| t.tx_type == TxType::Mining)
                    .map(|t| t.amount)
                    .sum();
                if let Err(reason) = Ledger::validate_block_emission_against(block, prior_mined) {
                    return Err(Violation::EmissionAmount {
                        seed: self.seed,
                        node: id.clone(),
                        index: i as u64,
                        block_minted,
                        prior_mined,
                        reason,
                    });
                }
                prior_mined = prior_mined.saturating_add(block_minted);
            }
        }
        // Conservation: Σ spendable + locked_stake + burned == minted
        // (pending-aware). ONCHAIN-STAKE-1 §5: `all_balances` is the SPENDABLE side
        // only — a Stake moves coins into the synthetic `STAKE` sink — so the
        // locked-stake pool (`locked_stake_total`: bonded + unbonding + pending
        // stake), which is locked-but-not-destroyed, is added back here. Without
        // it, a single Stake would look like a burn and trip this check; with it, a
        // full Stake → Unstake → maturation cycle conserves at every step.
        for (id, node) in &self.nodes {
            let l = node.ledger();
            let sum: u64 = l.all_balances().values().sum();
            let locked_stake = l.locked_stake_total();
            let burned = l.total_burned();
            let minted = l.total_minted();
            if sum
                .saturating_add(locked_stake)
                .saturating_add(burned)
                != minted
            {
                return Err(Violation::Conservation {
                    seed: self.seed,
                    node: id.clone(),
                    sum_balances: sum,
                    locked_stake,
                    burned,
                    minted,
                });
            }
        }
        Ok(())
    }

    /// Like [`Sim::run`], but **verifies the invariants after every step** and
    /// bails out at the first [`Violation`]. This is the harness with teeth: a
    /// scenario that breaks safety or conservation is caught, with the seed.
    fn run_checked(&mut self, max_steps: u64) -> Result<(), Violation> {
        self.run_checked_steps(max_steps)
            .map(|_| ())
            .map_err(|e| e.1)
    }

    /// As [`Sim::run_checked`], but returns the **number of event steps
    /// executed** — on success the total drained, on failure the step index of
    /// the **first faulty step** alongside the [`Violation`]. The T0.8 executor
    /// sums these across a move timeline to report *where* a sweep failure first
    /// appeared (spec §4: "enregistre le premier pas fautif, pas seulement
    /// l'état final"). `run_checked` is the value-only wrapper, so existing
    /// callers are unchanged. The `Err` is boxed: `Violation` carries full
    /// snapshots, large enough to trip `clippy::result_large_err` under the CI
    /// `--all-targets -D warnings` gate.
    fn run_checked_steps(&mut self, max_steps: u64) -> Result<u64, Box<(u64, Violation)>> {
        let mut steps = 0;
        while let Some(Reverse(s)) = self.queue.pop() {
            if steps >= max_steps {
                break;
            }
            steps += 1;
            self.clock_ms = s.time_ms;
            if self.crashed.contains(&s.node) {
                continue;
            }
            let node = self
                .nodes
                .get_mut(&s.node)
                .expect("node exists (test scenario)");
            let effects = node.handle(s.event, &mut self.rng);
            self.route(s.node, s.time_ms, effects);
            if let Err(v) = self.check_invariants() {
                return Err(Box::new((steps, v)));
            }
        }
        Ok(steps)
    }

    /// Orchestrated proposal kick (alternative to the event-driven seal timer):
    /// make `node` seal+sign a block as the elected leader at the current
    /// virtual time, and inject the resulting broadcast into the network.
    fn propose(&mut self, node: &PeerId, validators: &[Validator]) {
        let now = self.clock_ms;
        let effects: Vec<Effect> = self
            .node_mut(node)
            .propose_block_at(now, validators)
            .into_iter()
            .collect();
        self.route(node.clone(), now, effects);
    }

    /// Each node's current chain tip hash (the convergence observable).
    fn tips(&self) -> BTreeMap<PeerId, Option<String>> {
        self.nodes
            .iter()
            .map(|(id, n)| (id.clone(), n.ledger().chain.last().map(|b| b.hash.clone())))
            .collect()
    }

    /// Each node's chain height.
    fn heights(&self) -> BTreeMap<PeerId, usize> {
        self.nodes
            .iter()
            .map(|(id, n)| (id.clone(), n.ledger().chain.len()))
            .collect()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────

use crate::{
    p2p::{
        gossip::{GossipMessage, GossipRouter},
        ledger::{Block, Ledger, Transaction, MICRO},
        pos_consensus::MIN_VALIDATOR_STAKE,
    },
    security::CryptoEngine,
};

/// Deterministic, seed-derived signing identity (Ed25519 accepts any 32 bytes).
///
/// PQ-MIG-3: the identity also establishes its **independent ML-DSA primary**
/// (the tx-authority key the ledger binds). Its seed is derived from the Ed25519
/// seed via a **domain-separated** BLAKE3 hash, so it is byte-reproducible (C1)
/// yet structurally **independent** of the Ed25519 seed bytes — mirroring the
/// production rule that a quantum break of Ed25519 must not yield the ML-DSA key.
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
/// reward, validator weight and `from`/`to` are recorded. Distinct from
/// `public_key_hex`, the Ed25519 **transport** identity that only signs gossip
/// envelopes (`signed_msg_bytes`). The simulator's PeerId labels (`node-A`) are
/// a third, independent namespace.
fn addr_of(crypto: &CryptoEngine) -> String {
    crypto.pq_address_hex().expect("ml-dsa address")
}

/// Build a signed gossip envelope wrapping `msg` from `crypto`'s identity,
/// timestamped at `now_ms` (envelope nonce 0) — used to inject requests.
fn signed_msg_bytes(crypto: &CryptoEngine, msg: GossipMessage, now_ms: u64) -> Vec<u8> {
    let pk = crypto.get_identity().unwrap().public_key_hex;
    let ts = chrono::DateTime::from_timestamp_millis(now_ms as i64)
        .unwrap()
        .to_rfc3339();
    let signable = GossipRouter::signable_envelope_bytes(&pk, 0, &ts, &msg);
    let sig = crypto.sign(&signable).unwrap();
    let env = GossipRouter::build_signed_envelope(pk, msg, 0, ts, &sig).unwrap();
    serde_json::to_vec(&env).unwrap()
}

/// Build a 3-node sim where `A` carries a seed-derived signing identity (the
/// proposer) and `B`, `C` are observers. `now0` is a fixed virtual time.
fn three_node_sim(seed: u64) -> (Sim, PeerId, PeerId, PeerId, String) {
    let a_id = seeded_identity(1);
    let a_pk = addr_of(&a_id); // PQ-MIG-3B: value identity = ML-DSA address
    let a = PeerId("node-A".into());
    let b = PeerId("node-B".into());
    let c = PeerId("node-C".into());
    let sim = Sim::new(
        seed,
        vec![
            (a.clone(), Node::with_identity(a_id)),
            (b.clone(), Node::new()),
            (c.clone(), Node::new()),
        ],
    );
    (sim, a, b, c, a_pk)
}

/// **T0.4.** A block proposed by the leader propagates over the flood network
/// and all three nodes converge to the same chain tip — multi-node consensus
/// convergence, driven entirely by the deterministic event loop.
#[test]
fn sim_three_nodes_converge_on_proposed_block() {
    let now0 = 1_800_000_000_000_u64;
    let (mut sim, a, b, c, a_pk) = three_node_sim(0xC0FFEE);

    // Advance every node's virtual clock to now0 (so envelope freshness holds).
    sim.tick_all(now0);
    sim.run(10_000);

    // Give the leader something to seal (frozen mining tx → reproducible block).
    let frozen = {
        let mut origin = Ledger::new();
        origin.mine_tx(&a_pk, 50 * MICRO, 0.0)
    };
    sim.node_mut(&a).ledger_mut().replay_remote_tx(frozen);

    // A is the sole staked validator → the elected leader.
    let validators = vec![Validator {
        pk: a_pk,
        stake: MIN_VALIDATOR_STAKE,
        reputation: 0,
    }];
    sim.propose(&a, &validators);
    sim.run(10_000);

    // Convergence: every node has the SAME non-genesis tip.
    let tips = sim.tips();
    let leader_tip = tips[&a].clone();
    assert!(leader_tip.is_some(), "leader must have sealed a block");
    for id in [&a, &b, &c] {
        assert_eq!(
            &tips[id], &leader_tip,
            "node {id:?} must converge on the leader's tip"
        );
    }
    // All chains extended by exactly one block (genesis → height 2).
    for (id, h) in sim.heights() {
        assert_eq!(h, 2, "node {id:?} chain height");
    }
}

/// **T0.4 acceptance.** Same seed ⇒ identical run: the byte-comparable effect
/// trace AND the final tips match across two independent executions.
#[test]
fn sim_run_is_byte_deterministic() {
    let now0 = 1_800_000_000_000_u64;
    let a_pk = addr_of(&seeded_identity(1)); // PQ-MIG-3B: value identity = ML-DSA address
    // Frozen mining tx built ONCE → identical pending content in both runs.
    let frozen = {
        let mut origin = Ledger::new();
        origin.mine_tx(&a_pk, 50 * MICRO, 0.0)
    };
    let validators = vec![Validator {
        pk: a_pk,
        stake: MIN_VALIDATOR_STAKE,
        reputation: 0,
    }];

    let run = || {
        let (mut sim, a, _b, _c, _pk) = three_node_sim(0x5EED);
        sim.tick_all(now0);
        sim.run(10_000);
        sim.node_mut(&a)
            .ledger_mut()
            .replay_remote_tx(frozen.clone());
        sim.propose(&a, &validators);
        sim.run(10_000);
        (sim.trace.clone(), sim.tips())
    };

    let first = run();
    // Sanity: the run actually did something (a block sealed + propagated).
    assert!(
        first.1.values().all(|t| t.is_some()),
        "all nodes should have a tip"
    );
    assert_eq!(run(), first, "same seed ⇒ byte-identical trace + tips");
}

/// A different seed must NOT change the *outcome* here (the core ignores the
/// RNG so far) — but the harness must still treat the seed as a real input:
/// the determinism guarantee is per-seed, and the trace is seed-labelled.
#[test]
fn sim_scheduler_total_order_is_deterministic_under_many_events() {
    // Many same-instant events across nodes must pop in a fixed (time, seq)
    // order — the property the whole simulator rests on.
    let now0 = 1_800_000_000_000_u64;
    let build = || {
        let (mut sim, _a, _b, _c, _pk) = three_node_sim(0xABCD);
        for t in 0..50u64 {
            sim.tick_all(now0 + t); // 3 events per instant, 150 total
        }
        sim.run(10_000);
        sim.trace
            .iter()
            .map(|(t, n, _)| (*t, n.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(build(), build(), "scheduler order must be reproducible");
}

/// **T0.4 tr.2 — event-driven proposal.** The leader produces a block from its
/// **consensus seal timer** (`Tick` arms it → `TimerFired` fires it), with
/// **no** orchestrated `sim.propose` kick. The block then floods and all nodes
/// converge — proving block production now lives in the event loop.
#[test]
fn sim_leader_proposes_via_consensus_timer() {
    let now0 = 1_800_000_000_000_u64;
    let (mut sim, a, b, c, a_pk) = three_node_sim(0x7117E2);

    // Give the leader the validator set so its seal timer arms on the first tick.
    sim.node_mut(&a).set_validators(vec![Validator {
        pk: a_pk.clone(),
        stake: MIN_VALIDATOR_STAKE,
        reputation: 0,
    }]);

    // Advance clocks to now0 (arms A's seal timer for now0 + SEAL_INTERVAL).
    sim.tick_all(now0);
    sim.run_until(now0);

    // Give the leader something to seal, AFTER the tick so prune leaves it.
    let frozen = {
        let mut origin = Ledger::new();
        origin.mine_tx(&a_pk, 50 * MICRO, 0.0)
    };
    sim.node_mut(&a).ledger_mut().replay_remote_tx(frozen);

    // Run across one seal interval (120 s); horizon stops before the NEXT fire.
    // No `sim.propose` — the timer drives production entirely.
    sim.run_until(now0 + 200_000);

    let tips = sim.tips();
    let leader_tip = tips[&a].clone();
    assert!(
        leader_tip.is_some(),
        "the leader must have produced a block via its consensus timer"
    );
    for id in [&a, &b, &c] {
        assert_eq!(&tips[id], &leader_tip, "node {id:?} must converge");
    }
    for (id, h) in sim.heights() {
        assert_eq!(h, 2, "node {id:?} chain height (genesis + 1)");
    }
}

/// **T0.4 tr.2 — chain sync.** A node that joins AFTER a block was produced
/// (so it missed the flood) catches up via `RequestChain → Effect::Send(
/// ChainSegment)`: it asks the leader for blocks from its height, the leader
/// answers with a **targeted** signed segment, and the late joiner integrates
/// it. This is the recovery path partitions (T0.5) will lean on.
#[test]
fn sim_late_joiner_syncs_via_request_chain() {
    let now0 = 1_800_000_000_000_u64;
    let (mut sim, a, _b, _c, a_pk) = three_node_sim(0x5A1C);

    // Leader A produces block #1 (orchestrated, keeps this test focused on sync).
    sim.tick_all(now0);
    sim.run(10_000);
    let frozen = {
        let mut origin = Ledger::new();
        origin.mine_tx(&a_pk, 50 * MICRO, 0.0)
    };
    sim.node_mut(&a).ledger_mut().replay_remote_tx(frozen);
    let validators = vec![Validator {
        pk: a_pk,
        stake: MIN_VALIDATOR_STAKE,
        reputation: 0,
    }];
    sim.propose(&a, &validators);
    sim.run(10_000);
    let a_tip = sim.tips()[&a].clone();
    assert!(a_tip.is_some(), "A built a block");

    // A late joiner D appears at genesis — it missed the earlier flood.
    // `seeded_identity` is deterministic, so the signer and the node share a key.
    let d_id = seeded_identity(9);
    let d = PeerId("node-D".into());
    sim.add_node(d.clone(), Node::with_identity(seeded_identity(9)));
    assert_eq!(sim.heights()[&d], 1, "D starts at genesis (behind)");

    // Set D's clock (freshness), then D asks A for the chain from height 1.
    sim.schedule(now0, d.clone(), Event::Tick { now_ms: now0 });
    sim.run(10_000);
    let req = GossipMessage::RequestChain {
        from_height: 1,
        max_blocks: 50,
    };
    let req_bytes = signed_msg_bytes(&d_id, req, now0);
    sim.schedule(
        now0,
        a.clone(),
        Event::MessageReceived {
            from: d.clone(),
            bytes: req_bytes,
        },
    );
    sim.run(10_000);

    // D caught up: same tip and height as the leader.
    assert_eq!(sim.tips()[&d], a_tip, "late joiner D synced to A's tip");
    assert_eq!(sim.heights()[&d], 2, "D height after sync");
    assert_eq!(
        sim.node_mut(&d).telemetry().blocks_integrated,
        1,
        "D integrated exactly the missing block via the segment"
    );
}

// ─── T0.5: network faults + partition ─────────────────────────────────────

/// Build a 2-node sim where both `A` and `B` can sign (so either can request a
/// sync). `A` is the chain producer.
fn two_signing_nodes(seed: u64) -> (Sim, PeerId, PeerId, String) {
    let a_pk = addr_of(&seeded_identity(1)); // PQ-MIG-3B: value identity = ML-DSA address
    let a = PeerId("node-A".into());
    let b = PeerId("node-B".into());
    let sim = Sim::new(
        seed,
        vec![
            (a.clone(), Node::with_identity(seeded_identity(1))),
            (b.clone(), Node::with_identity(seeded_identity(2))),
        ],
    );
    (sim, a, b, a_pk)
}

/// **T0.5 — partition + recovery.** A graph partition isolates `A` from `B`, so
/// `A`'s block never reaches `B`. After the partition **heals**, `B` catches up
/// via the tr.2 chain sync — the canonical "split-brain then reconcile" path.
#[test]
fn sim_partition_isolates_then_sync_recovers() {
    let now0 = 1_800_000_000_000_u64;
    let (mut sim, a, b, a_pk) = two_signing_nodes(770_077);

    // Cut the link between A and B.
    sim.partition(std::slice::from_ref(&a), std::slice::from_ref(&b));
    sim.tick_all(now0);
    sim.run(10_000);

    // A produces a block; the broadcast toward B is blocked by the partition.
    let frozen = {
        let mut origin = Ledger::new();
        origin.mine_tx(&a_pk, 50 * MICRO, 0.0)
    };
    sim.node_mut(&a).ledger_mut().replay_remote_tx(frozen);
    let validators = vec![Validator {
        pk: a_pk,
        stake: MIN_VALIDATOR_STAKE,
        reputation: 0,
    }];
    sim.propose(&a, &validators);
    sim.run(10_000);
    assert_eq!(sim.heights()[&a], 2, "A advanced");
    assert_eq!(
        sim.heights()[&b],
        1,
        "B is isolated by the partition — still at genesis"
    );

    // Heal, then B reconciles by requesting the chain from A.
    sim.heal();
    let b_id = seeded_identity(2);
    let req = GossipMessage::RequestChain {
        from_height: 1,
        max_blocks: 50,
    };
    let req_bytes = signed_msg_bytes(&b_id, req, now0);
    sim.schedule(
        now0,
        a.clone(),
        Event::MessageReceived {
            from: b.clone(),
            bytes: req_bytes,
        },
    );
    sim.run(10_000);
    assert_eq!(sim.heights()[&b], 2, "B recovered after heal + sync");
    assert_eq!(sim.tips()[&b], sim.tips()[&a], "B converged to A's tip");
}

/// **T0.5 acceptance — reproducible by seed.** With drop + duplicate + variable
/// delay all active, two runs of the SAME seed produce an identical outcome
/// (tips + heights). The fault pattern is a pure function of the seed.
#[test]
fn sim_network_faults_are_reproducible_by_seed() {
    let now0 = 1_800_000_000_000_u64;
    let a_pk = addr_of(&seeded_identity(1)); // PQ-MIG-3B: value identity = ML-DSA address
    let frozen = {
        let mut origin = Ledger::new();
        origin.mine_tx(&a_pk, 50 * MICRO, 0.0)
    };
    let validators = vec![Validator {
        pk: a_pk,
        stake: MIN_VALIDATOR_STAKE,
        reputation: 0,
    }];
    let faulty = NetFaults {
        drop_ppm: 400_000, // 40% drop
        dup_ppm: 150_000,  // 15% duplicate
        min_delay_ms: 1,   // variable delay 1..30 ms ⇒ reorder
        max_delay_ms: 30,
        ..NetFaults::default()
    };

    let run = |seed: u64| {
        let (mut sim, a, _b, _c, _pk) = three_node_sim(seed);
        sim.set_faults(faulty.clone());
        sim.tick_all(now0);
        sim.run(10_000);
        sim.node_mut(&a)
            .ledger_mut()
            .replay_remote_tx(frozen.clone());
        sim.propose(&a, &validators);
        sim.run(10_000);
        (sim.tips(), sim.heights())
    };

    assert_eq!(
        run(0xF00D_BEEF),
        run(0xF00D_BEEF),
        "the same seed must reproduce the same fault pattern + outcome"
    );
}

/// **T0.5 — drop is activatable.** A 100% drop link delivers nothing; the same
/// scenario with a lossless link delivers the block. Deterministic both ways.
#[test]
fn sim_total_drop_prevents_delivery() {
    let now0 = 1_800_000_000_000_u64;
    let a_pk = addr_of(&seeded_identity(1)); // PQ-MIG-3B: value identity = ML-DSA address
    let frozen = {
        let mut origin = Ledger::new();
        origin.mine_tx(&a_pk, 50 * MICRO, 0.0)
    };
    let validators = vec![Validator {
        pk: a_pk,
        stake: MIN_VALIDATOR_STAKE,
        reputation: 0,
    }];

    let height_of_b = |drop_ppm: u64| -> usize {
        let (mut sim, a, b, _c, _pk) = three_node_sim(0xD0_D0_D0);
        sim.set_faults(NetFaults {
            drop_ppm,
            ..NetFaults::default()
        });
        sim.tick_all(now0);
        sim.run(10_000);
        sim.node_mut(&a)
            .ledger_mut()
            .replay_remote_tx(frozen.clone());
        sim.propose(&a, &validators);
        sim.run(10_000);
        sim.heights()[&b]
    };

    assert_eq!(height_of_b(1_000_000), 1, "100% drop → B receives nothing");
    assert_eq!(height_of_b(0), 2, "lossless → B receives the block");
}

// ─── T0.6: node faults + byzantine variants ───────────────────────────────

/// Craft two CONFLICTING blocks at height 1 (different mining reward ⇒
/// different content/hash), each wrapped in a `NewBlock` envelope signed by the
/// **same** byzantine key — i.e. an equivocation. Returns `(hash1, hash2, env1,
/// env2)`.
fn equivocating_blocks(byz: &CryptoEngine, now_ms: u64) -> (String, String, Vec<u8>, Vec<u8>) {
    let pk = addr_of(byz); // PQ-MIG-3B: block miner = ML-DSA address (envelope stays Ed25519)
    let seal_one = |amount: u64| {
        let mut origin = Ledger::new();
        origin.mine_tx(&pk, amount, 0.0);
        origin.seal_block(&pk, 0.0) // a block at index 1, extending genesis
    };
    let b1 = seal_one(50 * MICRO);
    let b2 = seal_one(40 * MICRO); // different reward ⇒ different block
    let (h1, h2) = (b1.hash.clone(), b2.hash.clone());
    let env1 = signed_msg_bytes(
        byz,
        GossipMessage::NewBlock {
            block_json: serde_json::to_string(&b1).unwrap(),
        },
        now_ms,
    );
    let env2 = signed_msg_bytes(
        byz,
        GossipMessage::NewBlock {
            block_json: serde_json::to_string(&b2).unwrap(),
        },
        now_ms,
    );
    (h1, h2, env1, env2)
}

/// Detect block equivocation: two VALID envelopes from the **same** sender,
/// each a `NewBlock` at the same height, with **different** block hashes. This
/// is the evidence a future slashing rule ([[ADR-003]]) would consume — the
/// detection primitive, with no punishment wired (ADR-003 is still open).
fn detect_equivocation(env1: &[u8], env2: &[u8], now_secs: i64) -> bool {
    use crate::p2p::dispatcher::validate_envelope_at;
    let (Ok(e1), Ok(e2)) = (
        validate_envelope_at(env1, now_secs),
        validate_envelope_at(env2, now_secs),
    ) else {
        return false; // both must be valid, signature-verified envelopes
    };
    if e1.sender != e2.sender {
        return false; // different authors → not equivocation
    }
    let block_of = |p: &GossipMessage| -> Option<Block> {
        match p {
            GossipMessage::NewBlock { block_json } => serde_json::from_str(block_json).ok(),
            _ => None,
        }
    };
    match (block_of(&e1.payload), block_of(&e2.payload)) {
        (Some(b1), Some(b2)) => b1.index == b2.index && b1.hash != b2.hash,
        _ => false,
    }
}

/// **T0.6 — equivocation: detected, and safety holds.** A byzantine proposer
/// signs TWO different blocks at the same height and broadcasts both. Honest
/// nodes still **converge** (deterministic fork tie-break → both keep the same
/// block), and the pair of envelopes is a **provable** equivocation.
#[test]
fn sim_equivocation_is_detected_and_honest_nodes_stay_safe() {
    let now0 = 1_800_000_000_000_u64;
    let a = PeerId("node-A".into());
    let c = PeerId("node-C".into());
    let mut sim = Sim::new(
        0xE0_71_F0,
        vec![(a.clone(), Node::new()), (c.clone(), Node::new())],
    );
    sim.tick_all(now0);
    sim.run(10_000);

    let byz = seeded_identity(66);
    let (h1, h2, env1, env2) = equivocating_blocks(&byz, now0);
    let byz_peer = PeerId("byz".into());
    for honest in [&a, &c] {
        sim.schedule(
            now0,
            honest.clone(),
            Event::MessageReceived {
                from: byz_peer.clone(),
                bytes: env1.clone(),
            },
        );
        sim.schedule(
            now0,
            honest.clone(),
            Event::MessageReceived {
                from: byz_peer.clone(),
                bytes: env2.clone(),
            },
        );
    }
    sim.run(10_000);

    // SAFETY: both honest nodes keep the SAME block (the higher-hash one) — they
    // never split on the conflicting pair.
    let winner = if h1 > h2 { h1.clone() } else { h2.clone() };
    assert_eq!(
        sim.tips()[&a],
        Some(winner.clone()),
        "A keeps the tie-break winner"
    );
    assert_eq!(
        sim.tips()[&c],
        sim.tips()[&a],
        "A and C agree despite equivocation"
    );

    // DETECTION: the two envelopes prove the equivocation; a non-conflict doesn't.
    let now_secs = (now0 / 1000) as i64;
    assert!(
        detect_equivocation(&env1, &env2, now_secs),
        "two conflicting signed blocks at the same height = equivocation"
    );
    assert!(
        !detect_equivocation(&env1, &env1, now_secs),
        "the same block twice is NOT an equivocation"
    );
}

/// **T0.6 — crash/restart.** A node that is **crashed** while a block is
/// produced misses it (events to it are dropped); after **restart** it catches
/// up via chain sync.
#[test]
fn sim_crashed_node_misses_blocks_then_recovers_on_restart() {
    let now0 = 1_800_000_000_000_u64;
    let (mut sim, a, d, a_pk) = two_signing_nodes(0xC2A54);
    sim.tick_all(now0);
    sim.run(10_000);

    sim.crash(&d); // D goes offline
    let frozen = {
        let mut origin = Ledger::new();
        origin.mine_tx(&a_pk, 50 * MICRO, 0.0)
    };
    sim.node_mut(&a).ledger_mut().replay_remote_tx(frozen);
    sim.propose(
        &a,
        &[Validator {
            pk: a_pk,
            stake: MIN_VALIDATOR_STAKE,
            reputation: 0,
        }],
    );
    sim.run(10_000);
    assert_eq!(sim.heights()[&a], 2, "A advanced");
    assert_eq!(sim.heights()[&d], 1, "crashed D missed the block");

    // Restart D → it reconciles by syncing from A.
    sim.restart(&d);
    let d_id = seeded_identity(2);
    let req = signed_msg_bytes(
        &d_id,
        GossipMessage::RequestChain {
            from_height: 1,
            max_blocks: 50,
        },
        now0,
    );
    sim.schedule(
        now0,
        a.clone(),
        Event::MessageReceived {
            from: d.clone(),
            bytes: req,
        },
    );
    sim.run(10_000);
    assert_eq!(sim.heights()[&d], 2, "D recovered after restart + sync");
    assert_eq!(sim.tips()[&d], sim.tips()[&a]);
}

/// **T0.6 — byzantine block retention.** A leader delivers its block to one
/// peer but **withholds** it from another (directed cut). The victim detects it
/// is behind only implicitly and recovers by syncing from the honest peer that
/// did receive the block.
#[test]
fn sim_byzantine_retention_then_victim_syncs_from_honest_peer() {
    let now0 = 1_800_000_000_000_u64;
    let l = PeerId("node-A".into()); // leader/retainer
    let s = PeerId("node-B".into()); // honest peer that receives the block
    let r = PeerId("node-C".into()); // victim, withheld from
    let l_pk = addr_of(&seeded_identity(1)); // PQ-MIG-3B: value identity = ML-DSA address
    let mut sim = Sim::new(
        0x5E7E7,
        vec![
            (l.clone(), Node::with_identity(seeded_identity(1))),
            (s.clone(), Node::with_identity(seeded_identity(2))),
            (r.clone(), Node::with_identity(seeded_identity(3))),
        ],
    );
    sim.withhold(&l, &r); // L retains its block from R
    sim.tick_all(now0);
    sim.run(10_000);

    let frozen = {
        let mut origin = Ledger::new();
        origin.mine_tx(&l_pk, 50 * MICRO, 0.0)
    };
    sim.node_mut(&l).ledger_mut().replay_remote_tx(frozen);
    sim.propose(
        &l,
        &[Validator {
            pk: l_pk,
            stake: MIN_VALIDATOR_STAKE,
            reputation: 0,
        }],
    );
    sim.run(10_000);
    assert_eq!(sim.heights()[&s], 2, "S received the block");
    assert_eq!(sim.heights()[&r], 1, "R was withheld from (retention)");

    // R recovers by syncing from the honest S (not from the byzantine leader).
    let r_id = seeded_identity(3);
    let req = signed_msg_bytes(
        &r_id,
        GossipMessage::RequestChain {
            from_height: 1,
            max_blocks: 50,
        },
        now0,
    );
    sim.schedule(
        now0,
        s.clone(),
        Event::MessageReceived {
            from: r.clone(),
            bytes: req,
        },
    );
    sim.run(10_000);
    assert_eq!(sim.heights()[&r], 2, "R recovered via an honest peer");
    assert_eq!(sim.tips()[&r], sim.tips()[&s]);
}

// ─── T0.7: invariant checkers (safety + conservation), with teeth ─────────

/// A frozen mining tx crediting `pk` — deterministic pending content.
fn frozen_mining(pk: &str, amount: u64) -> crate::p2p::ledger::Transaction {
    let mut origin = Ledger::new();
    origin.mine_tx(pk, amount, 0.0)
}

/// **T0.7 — invariants hold + liveness.** A healthy run (propose → flood →
/// integrate) keeps **safety and conservation** true at every step, and the
/// chain actually **advances** (liveness): all nodes reach height 2.
#[test]
fn sim_invariants_hold_through_a_healthy_run() {
    let now0 = 1_800_000_000_000_u64;
    let (mut sim, a, _b, _c, a_pk) = three_node_sim(0x600D_5EED);
    sim.tick_all(now0);
    assert_eq!(
        sim.run_checked(10_000),
        Ok(()),
        "invariants hold during ticks"
    );

    sim.node_mut(&a)
        .ledger_mut()
        .replay_remote_tx(frozen_mining(&a_pk, 50 * MICRO));
    sim.propose(
        &a,
        &[Validator {
            pk: a_pk,
            stake: MIN_VALIDATOR_STAKE,
            reputation: 0,
        }],
    );
    assert_eq!(
        sim.run_checked(10_000),
        Ok(()),
        "safety + conservation hold through propagation"
    );
    // Liveness: the chain advanced and everyone converged.
    for (id, h) in sim.heights() {
        assert_eq!(h, 2, "node {id:?} advanced (liveness)");
    }
}

/// **T0.7 — teeth (safety).** A partition lets two sides each seal a DIFFERENT
/// block at height 1, with no way to reconcile → correct nodes disagree on a
/// committed value. `run_checked` catches it as a `Safety` violation **carrying
/// the seed** (so the failure is replayable). This is also an honest mirror of
/// the PoC's known weakness that the finality gadget ([[ADR-001]]) will fix.
#[test]
fn sim_partition_fork_breaks_safety_and_is_detected() {
    let now0 = 1_800_000_000_000_u64;
    let seed = 0xF02C_BAD5;
    let a = PeerId("node-A".into());
    let b = PeerId("node-B".into());
    let a_pk = addr_of(&seeded_identity(1)); // PQ-MIG-3B: value identity = ML-DSA address
    let b_pk = addr_of(&seeded_identity(2)); // PQ-MIG-3B: value identity = ML-DSA address
    let mut sim = Sim::new(
        seed,
        vec![
            (a.clone(), Node::with_identity(seeded_identity(1))),
            (b.clone(), Node::with_identity(seeded_identity(2))),
        ],
    );
    // Each is the sole validator of its own side → each will seal on its timer.
    sim.node_mut(&a).set_validators(vec![Validator {
        pk: a_pk.clone(),
        stake: MIN_VALIDATOR_STAKE,
        reputation: 0,
    }]);
    sim.node_mut(&b).set_validators(vec![Validator {
        pk: b_pk.clone(),
        stake: MIN_VALIDATOR_STAKE,
        reputation: 0,
    }]);
    sim.partition(std::slice::from_ref(&a), std::slice::from_ref(&b));

    // BLK-HASH-1 T4: NO timestamp band-aid. The two height-1 blocks differ by
    // **content** (different miner + mining recipient), so the hash commits them
    // distinctly even at the SAME timestamp. If they stopped forking here, `miner`
    // would be mis-bound — a fix bug to investigate, not a band-aid to restore.
    sim.tick_all(now0);
    sim.run_until(now0); // arm both seal timers
    sim.node_mut(&a)
        .ledger_mut()
        .replay_remote_tx(frozen_mining(&a_pk, 50 * MICRO));
    sim.node_mut(&b)
        .ledger_mut()
        .replay_remote_tx(frozen_mining(&b_pk, 50 * MICRO));

    // Both sides seal at their next slot; the partition blocks reconciliation.
    let result = sim.run_checked(10_000);
    match result {
        Err(Violation::Safety { seed: s, index, .. }) => {
            assert_eq!(index, 1, "the conflict is at the first non-genesis block");
            assert_eq!(s, seed, "the violation carries the run's seed for replay");
        }
        other => panic!("expected a Safety violation, got {other:?}"),
    }
}

/// **GADGET-1 — teeth (finality safety).** Nothing finalizes past genesis yet
/// (the justify/finalize rule is GADGET-3), so the finality-safety invariant
/// would be **vacuously true** — exactly the "passes no matter what" trap the
/// spec §4 warns against. So we **plant** the violation: inject two CONFLICTING
/// finalized checkpoints at the SAME epoch directly into two nodes' state
/// (outside any real finalization path, as the T0.8 teeth tests do), and assert
/// the checker BITES, carrying the seed + epoch. This proves the verifier has
/// teeth NOW, before finalization exists — the invariant is not decoration.
#[test]
fn gadget_1_finality_safety_invariant_has_teeth() {
    let seed = 0x6AD9_E7F1;
    let a = PeerId("node-A".into());
    let b = PeerId("node-B".into());
    let mut sim = Sim::new(seed, vec![(a.clone(), Node::new()), (b.clone(), Node::new())]);
    // Clean baseline: both nodes start with {genesis} finalized → epoch 0 agrees,
    // so the invariant holds. NOT a vacuous pass — the matching-checkpoints test
    // below finalizes a SHARED epoch-1 checkpoint and still holds.
    assert_eq!(sim.check_invariants(), Ok(()));

    // Plant a conflict at epoch 1: A finalizes one checkpoint, B another, both at
    // the same epoch-1 boundary height but with DIFFERENT hashes. Hand-built via
    // the test hook — there is no finalization RULE yet (that is GADGET-3).
    let boundary = EPOCH_LENGTH_BLOCKS; // height of epoch 1's boundary block
    sim.node_mut(&a).record_finalized_for_test(Checkpoint {
        epoch: 1,
        height: boundary,
        hash: "a".repeat(64),
    });
    sim.node_mut(&b).record_finalized_for_test(Checkpoint {
        epoch: 1,
        height: boundary,
        hash: "b".repeat(64),
    });

    match sim.check_invariants() {
        Err(Violation::FinalitySafety { seed: s, epoch, .. }) => {
            assert_eq!(epoch, 1, "the planted conflict is at epoch 1");
            assert_eq!(s, seed, "the violation carries the run's seed for replay");
        }
        other => panic!("expected a FinalitySafety violation, got {other:?}"),
    }
}

/// **GADGET-1 — anti-false-positive.** The finality-safety invariant must fire on
/// a *conflict*, not on the mere presence of finalized checkpoints. Two nodes
/// that finalize the **same** epoch-1 checkpoint agree, so the checker must stay
/// green — this pins down that the teeth test above isn't tripping on any two
/// finalized entries, only on genuine disagreement.
#[test]
fn gadget_1_finality_safety_accepts_matching_checkpoints() {
    let a = PeerId("node-A".into());
    let b = PeerId("node-B".into());
    let mut sim = Sim::new(0x6AD9_E7F2, vec![(a.clone(), Node::new()), (b.clone(), Node::new())]);
    let cp = Checkpoint {
        epoch: 1,
        height: EPOCH_LENGTH_BLOCKS,
        hash: "c".repeat(64),
    };
    sim.node_mut(&a).record_finalized_for_test(cp.clone());
    sim.node_mut(&b).record_finalized_for_test(cp);
    assert_eq!(
        sim.check_invariants(),
        Ok(()),
        "agreement on a finalized checkpoint must NOT trip finality safety"
    );
}

/// **GADGET-3 — FinalitySafety becomes REAL (harness).** GADGET-1 left the
/// finality-safety invariant quasi-vacuous: only genesis was ever finalized, so
/// it could fire only on a *planted* conflict. GADGET-3's justify/finalize rule
/// now grows the finalized set past genesis. Here two nodes apply the **same**
/// honest certificate chain `g→c1→c2→c3` through
/// [`Node::apply_finality_certificate`]; both finalize the **same** real
/// checkpoints `{g, c1, c2}`, and the cross-node finality-safety invariant stays
/// **green** — proving it now guards REAL finalized history (not the vacuous
/// genesis-only state) and that an honest path never finalizes conflicting
/// checkpoints. The planted-conflict teeth test
/// ([`gadget_1_finality_safety_invariant_has_teeth`]) still bites, so the checker
/// is neither vacuous nor a rubber stamp.
#[test]
fn gadget_3_finality_safety_guards_real_finalized_checkpoints() {
    use crate::security::hybrid_crypto::{derive_ml_dsa, ml_dsa_sign_deterministic};
    use crate::sm::{MlDsaCertificate, Vote};
    use std::collections::HashMap;

    const E: u64 = EPOCH_LENGTH_BLOCKS;

    let a = PeerId("node-A".into());
    let b = PeerId("node-B".into());

    // Two fresh nodes share the same deterministic genesis hash — the genesis
    // checkpoint both start with already justified + finalized. The honest chain
    // of certificates must root at THIS hash, else `is_justified(source)` fails.
    let genesis_hash = Node::new()
        .ledger()
        .chain
        .first()
        .map(|blk| blk.hash.clone())
        .expect("genesis block");
    let cp = |epoch: u64, hash: &str| Checkpoint {
        epoch,
        height: epoch * E,
        hash: hash.to_string(),
    };
    let g = cp(0, &genesis_hash);
    let c1 = cp(1, "c1-hash");
    let c2 = cp(2, "c2-hash");
    let c3 = cp(3, "c3-hash");

    // Two equal-stake ML-DSA validators (100 each, total 200, ⅔ = 134): the
    // whole committee signing each link clears the quorum. The stake snapshot is
    // SUPPLIED to the rule (its provenance is the open GADGET-2 §4 reconciliation,
    // not resolved here).
    let validator = |seed: u8| derive_ml_dsa(&[seed; 32]).expect("derive").1;
    let stakes: HashMap<String, u64> = [validator(1), validator(2)]
        .into_iter()
        .map(|pk| (pk, 100u64))
        .collect();
    let signed_vote = |seed: u8, source: &Checkpoint, target: &Checkpoint| -> Vote {
        let (sk, pk) = derive_ml_dsa(&[seed; 32]).expect("derive");
        let mut v = Vote {
            source: source.clone(),
            target: target.clone(),
            voting_epoch: target.epoch,
            validator: pk,
            signature: Vec::new(),
        };
        v.signature = ml_dsa_sign_deterministic(&sk, &v.signable_bytes()).expect("sign");
        v
    };
    let cert = |source: &Checkpoint, target: &Checkpoint| {
        MlDsaCertificate::new(
            source.clone(),
            target.clone(),
            vec![signed_vote(1, source, target), signed_vote(2, source, target)],
        )
    };

    // Both nodes apply the identical honest chain through the real rule.
    let chain = [(&g, &c1), (&c1, &c2), (&c2, &c3)];
    let mut node_a = Node::new();
    let mut node_b = Node::new();
    for (s, t) in chain {
        node_a.apply_finality_certificate(&cert(s, t), &stakes);
        node_b.apply_finality_certificate(&cert(s, t), &stakes);
    }

    // The rule GREW the finalized set past genesis: g + c1 + c2 (c3 is justified
    // but its direct child was never linked ⇒ not finalized — two-step).
    assert_eq!(
        node_a.finalized().iter().count(),
        3,
        "genesis + c1 + c2 finalized — the set grew, no longer vacuous"
    );
    assert_eq!(node_a.finalized().get(1).map(|c| c.hash.as_str()), Some("c1-hash"));
    assert_eq!(node_a.finalized().get(2).map(|c| c.hash.as_str()), Some("c2-hash"));

    // Cross-node finality safety holds on REAL (non-genesis) finalized points.
    let sim = Sim::new(0x6AD9_E7F3, vec![(a, node_a), (b, node_b)]);
    assert_eq!(
        sim.check_invariants(),
        Ok(()),
        "two nodes finalizing the SAME honest chain agree — FinalitySafety guards real points"
    );
}

/// **GADGET-4 — accountable safety made executable.** This is the §4 test that
/// proves the slashing theorem: if a **byzantine committee** finalizes two
/// **conflicting** checkpoints, then (i) the broken finality is **observable**
/// (the cross-node `FinalitySafety` invariant fires), and (ii) the validators'
/// **own signed votes** leave a proof covering **≥ ⅓** of the stake. Breaking
/// irreversible history is not free — it is **answerable**.
///
/// Construction: 3 equal-stake validators (100 each, total 300, ⅔ = 200). Two
/// ⅔-quorums back conflicting epoch-1 forks — `{1,2}` finalize fork A on node A,
/// `{2,3}` finalize fork B on node B (each via GADGET-3's two-step rule). The
/// two quorums must intersect in ≥ ⅓ (pigeonhole: `⅔ + ⅔ − 1 = ⅓`); here the
/// intersection is validator 2, who signed **both** forks ⇒ a double vote.
/// [`slashable_weight`] recovers exactly its 100 µQTA = ⅓ of 300.
#[test]
fn gadget_4_accountable_safety_finalized_conflict_leaves_a_third_slashable() {
    use crate::security::hybrid_crypto::{derive_ml_dsa, ml_dsa_sign_deterministic};
    use crate::sm::finality_slashing::slashable_weight;
    use crate::sm::{MlDsaCertificate, Vote};
    use std::collections::HashMap;

    const E: u64 = EPOCH_LENGTH_BLOCKS;

    let a = PeerId("node-A".into());
    let b = PeerId("node-B".into());

    // Shared deterministic genesis hash — the root both forks build on (else the
    // genesis source is not justified and nothing finalizes).
    let genesis_hash = Node::new()
        .ledger()
        .chain
        .first()
        .map(|blk| blk.hash.clone())
        .expect("genesis block");
    let cp = |epoch: u64, hash: &str| Checkpoint {
        epoch,
        height: epoch * E,
        hash: hash.to_string(),
    };
    let g = cp(0, &genesis_hash);
    // Two CONFLICTING epoch-1 checkpoints and their epoch-2 children (one per fork).
    let c1a = cp(1, "epoch1-fork-A");
    let c2a = cp(2, "epoch2-fork-A");
    let c1b = cp(1, "epoch1-fork-B");
    let c2b = cp(2, "epoch2-fork-B");

    // 3 equal-stake ML-DSA validators (100 each). The stake snapshot is SUPPLIED
    // to the rule (provenance = the open GADGET-2 §4 reconciliation, not resolved).
    let v = |seed: u8| derive_ml_dsa(&[seed; 32]).expect("derive").1;
    let stakes: HashMap<String, u64> = [v(1), v(2), v(3)]
        .into_iter()
        .map(|pk| (pk, 100u64))
        .collect();
    let signed_vote = |seed: u8, source: &Checkpoint, target: &Checkpoint| -> Vote {
        let (sk, pk) = derive_ml_dsa(&[seed; 32]).expect("derive");
        let mut vt = Vote {
            source: source.clone(),
            target: target.clone(),
            voting_epoch: target.epoch,
            validator: pk,
            signature: Vec::new(),
        };
        vt.signature = ml_dsa_sign_deterministic(&sk, &vt.signable_bytes()).expect("sign");
        vt
    };
    let cert = |source: &Checkpoint, target: &Checkpoint, signers: &[u8]| {
        let votes: Vec<Vote> = signers.iter().map(|s| signed_vote(*s, source, target)).collect();
        MlDsaCertificate::new(source.clone(), target.clone(), votes)
    };

    // Fork A finalized on node A by validators {1,2}; fork B on node B by {2,3}.
    // Validator 2 backs BOTH forks — the equivocation that breaks finality.
    let cert_g_c1a = cert(&g, &c1a, &[1, 2]);
    let cert_c1a_c2a = cert(&c1a, &c2a, &[1, 2]);
    let cert_g_c1b = cert(&g, &c1b, &[2, 3]);
    let cert_c1b_c2b = cert(&c1b, &c2b, &[2, 3]);

    let mut node_a = Node::new();
    node_a.apply_finality_certificate(&cert_g_c1a, &stakes); // justify c1a
    node_a.apply_finality_certificate(&cert_c1a_c2a, &stakes); // finalize c1a
    let mut node_b = Node::new();
    node_b.apply_finality_certificate(&cert_g_c1b, &stakes); // justify c1b
    node_b.apply_finality_certificate(&cert_c1b_c2b, &stakes); // finalize c1b

    // Each node finalized its fork's epoch-1 checkpoint — CONFLICTING across nodes.
    assert_eq!(
        node_a.finalized().get(1).map(|c| c.hash.as_str()),
        Some("epoch1-fork-A"),
        "node A finalized fork A at epoch 1"
    );
    assert_eq!(
        node_b.finalized().get(1).map(|c| c.hash.as_str()),
        Some("epoch1-fork-B"),
        "node B finalized fork B at epoch 1"
    );

    // (i) The broken finality is OBSERVABLE: the cross-node FinalitySafety
    // invariant fires at the conflicting epoch — the catastrophe someone must
    // answer for.
    let sim = Sim::new(0x6AD9_E7F4, vec![(a, node_a), (b, node_b)]);
    match sim.check_invariants() {
        Err(Violation::FinalitySafety { epoch, .. }) => {
            assert_eq!(epoch, 1, "the conflicting checkpoints are finalized at epoch 1");
        }
        other => panic!("expected a FinalitySafety violation from the byzantine finalization, got {other:?}"),
    }

    // (ii) The votes LEAVE A PROOF: across every vote both forks cast, validator
    // 2 double-voted at epoch 1 (g→c1a AND g→c1b) — its 100 µQTA is provably
    // slashable, exactly ⅓ of 300. Honest validators 1 and 3 voted once per epoch
    // and stay clean. Accountable safety: breaking finality costs ≥ a third.
    let all_votes: Vec<Vote> = [&cert_g_c1a, &cert_c1a_c2a, &cert_g_c1b, &cert_c1b_c2b]
        .into_iter()
        .flat_map(|c| c.votes().iter().cloned())
        .collect();
    let total: u64 = stakes.values().sum();
    let slashable = slashable_weight(&all_votes, &stakes, E);
    assert_eq!(slashable, 100, "validator 2's equivocation is provably slashable");
    assert!(
        slashable * 3 >= total,
        "accountable safety: ≥ ⅓ of stake ({slashable}/{total}) is provably slashable"
    );
}

/// **T0.7 — teeth (conservation).** Phantom µQTA (an ESCROW release with no
/// matching lock) makes `Σ balances + burned != minted`; the checker catches
/// it.
#[test]
fn sim_conservation_violation_is_detected() {
    let n = PeerId("node-N".into());
    let mut sim = Sim::new(0xC0FE7EA, vec![(n.clone(), Node::new())]);
    // Conserved before tampering.
    assert_eq!(sim.check_invariants(), Ok(()));
    // Inject phantom money: release from ESCROW that was never locked.
    sim.node_mut(&n)
        .ledger_mut()
        .escrow_release_to(&"v".repeat(64), 7 * MICRO);
    match sim.check_invariants() {
        Err(Violation::Conservation { node, minted, .. }) => {
            assert_eq!(node, n);
            assert_eq!(minted, 0, "no mint backs the phantom credit");
        }
        other => panic!("expected a Conservation violation, got {other:?}"),
    }
}

/// **BLK-HASH-1 T5 — conservation through a fork reorg.** A node integrates two
/// conflicting height-1 blocks (lower hash first, forcing a reorg to the higher
/// one, which re-queues the loser's mining tx). `run_checked` asserts
/// `Σ balances + burned == minted` at **every** step — the long-suspected
/// double-mint on the loser's re-queued reward. If this violates, it's a real
/// bug to report, not a test to soften.
#[test]
fn blk_hash_1_conservation_holds_through_reorg() {
    let now0 = 1_800_000_000_000_u64;
    let n = PeerId("node-N".into());
    let mut sim = Sim::new(0x5EED_F02C, vec![(n.clone(), Node::new())]);
    sim.tick_all(now0);
    assert_eq!(sim.run_checked(10_000), Ok(()));

    // Two conflicting blocks at height 1 (same miner, different reward ⇒
    // different content ⇒ different hash). Deliver the LOWER hash first so the
    // HIGHER one wins and the reorg's re-queue path is exercised.
    let byz = seeded_identity(7);
    let (h1, h2, env1, env2) = equivocating_blocks(&byz, now0);
    let (first, second) = if h1 < h2 { (env1, env2) } else { (env2, env1) };
    let from = PeerId("src".into());
    sim.schedule(
        now0,
        n.clone(),
        Event::MessageReceived {
            from: from.clone(),
            bytes: first,
        },
    );
    sim.schedule(
        now0,
        n.clone(),
        Event::MessageReceived {
            from,
            bytes: second,
        },
    );

    assert_eq!(
        sim.run_checked(10_000),
        Ok(()),
        "Σ balances + burned == minted must hold through the fork reorg + re-queue"
    );
    assert_eq!(sim.heights()[&n], 2, "N converged on one height-1 block");
}

/// **ONCHAIN-STAKE-1 §5 — the harness conservation checker counts locked stake.**
/// A node holding bonded + unbonding stake must satisfy the harness invariant
/// (staked/unbonding are locked, **not** destroyed). Teeth: the OLD formula
/// (`spendable + burned == minted`) FALSELY flags this state, proving the new
/// `staked`/`unbonding` terms are load-bearing — staking is not burning.
#[test]
fn onchain_stake_harness_conservation_counts_locked_stake() {
    let a = PeerId("node-A".into());
    let mut crypto = CryptoEngine::new();
    let _ = crypto.generate_keypair();
    crypto.generate_pq_identity().expect("ml-dsa primary"); // PQ-MIG-3: bind authority key
    let pk = addr_of(&crypto); // PQ-MIG-3B: value identity (stake/mine/seal) = ML-DSA address

    let mut node = Node::new();
    node.ledger_mut().mine_tx(&pk, 100 * MICRO, 0.0);
    node.ledger_mut().seal_block(&pk, 0.0);
    node.ledger_mut()
        .stake_tx(&pk, 40 * MICRO, &crypto)
        .expect("stake builds");
    node.ledger_mut().seal_block(&pk, 0.0);
    node.ledger_mut()
        .unstake_tx(&pk, 25 * MICRO, &crypto)
        .expect("unstake builds");
    node.ledger_mut().seal_block(&pk, 0.0);

    let sim = Sim::new(0x57A6E, vec![(a.clone(), node)]);

    // NEW checker: the locked-stake sink (bonded 15 + unbonding 25 = 40) is
    // counted ⇒ conserves.
    assert_eq!(
        sim.check_invariants(),
        Ok(()),
        "locked stake (bonded + unbonding) must satisfy harness conservation"
    );

    // TEETH: the pre-ONCHAIN-STAKE-1 formula would have FALSELY flagged it.
    let l = sim.nodes[&a].ledger();
    let spendable: u64 = l.all_balances().values().sum();
    assert_eq!(l.locked_stake_total(), 40 * MICRO, "40 locked (15 bonded + 25 unbonding)");
    assert_ne!(
        spendable + l.total_burned(),
        l.total_minted(),
        "old formula (spendable + burned) MUST mismatch — locked stake is load-bearing"
    );
    assert_eq!(
        spendable + l.locked_stake_total() + l.total_burned(),
        l.total_minted(),
        "new formula conserves"
    );
}

// ─── EMIT-1: no double-mint at reorg + emission invariant ──────────────────

/// **EMIT-1 E1 — the question T5 could not settle.** After a same-height reorg
/// (lower hash delivered first, higher wins), the LOSING block's mining reward
/// must NOT land in `pending`: it belongs to a block, and re-queuing it would
/// let it be sealed AGAIN — the double-mint conservation is blind to. T5 passed
/// identically whether or not the reward was re-queued; this asserts the
/// discriminating fact directly: `total_minted` (chain + pending mint) equals
/// the chain-only mined total, i.e. **zero pending mint**.
#[test]
fn emit_1_losing_block_reward_is_not_requeued() {
    let now0 = 1_800_000_000_000_u64;
    let n = PeerId("node-N".into());
    let mut sim = Sim::new(0x105E_B10C, vec![(n.clone(), Node::new())]);
    sim.tick_all(now0);
    assert_eq!(sim.run_checked(10_000), Ok(()));

    // Two conflicting height-1 blocks (same miner, different reward ⇒ different
    // hash). Deliver the LOWER hash first so the HIGHER one wins and the reorg's
    // re-queue path runs.
    let byz = seeded_identity(7);
    let (h1, h2, env1, env2) = equivocating_blocks(&byz, now0);
    let (first, second) = if h1 < h2 { (env1, env2) } else { (env2, env1) };
    let from = PeerId("src".into());
    sim.schedule(
        now0,
        n.clone(),
        Event::MessageReceived {
            from: from.clone(),
            bytes: first,
        },
    );
    sim.schedule(
        now0,
        n.clone(),
        Event::MessageReceived { from, bytes: second },
    );

    // Through the reorg, safety + conservation + EMISSION all hold.
    assert_eq!(
        sim.run_checked(10_000),
        Ok(()),
        "all invariants hold through the fork reorg + re-queue"
    );
    assert_eq!(sim.heights()[&n], 2, "converged on one height-1 block");

    let l = sim.node_mut(&n).ledger();
    assert_eq!(
        l.total_minted(),
        l.stats().total_mined,
        "the loser's mining reward must NOT be re-queued into pending (EMIT-1 §4.1)"
    );
}

/// **EMIT-1 E4 — the emission invariant has teeth.** A chain state with one
/// mining reward too many (a height-1 block carrying TWO mining txs) makes
/// `run_checked`/`check_invariants` return `Violation::Emission`, carrying the
/// seed. The block is injected straight into the chain (bypassing validation,
/// which would now reject it) so the *checker* — not the validator — is what is
/// under test.
#[test]
fn emit_1_emission_invariant_has_teeth() {
    let now0 = 1_800_000_000_000_u64;
    let seed = 0xE315_7EE7;
    let n = PeerId("node-N".into());
    let mut sim = Sim::new(seed, vec![(n.clone(), Node::new())]);
    sim.tick_all(now0);
    assert_eq!(sim.run_checked(10_000), Ok(()));

    // Forge a height-1 block with TWO NETWORK→pk mining txs and inject it.
    let pk = addr_of(&seeded_identity(1)); // PQ-MIG-3B: value identity = ML-DSA address
    let mut o = Ledger::new();
    let m1 = o.mine_tx(&pk, 5 * MICRO, 0.0);
    let m2 = o.mine_tx(&pk, 5 * MICRO, 0.0);
    let genesis_hash = o.block_at(0).unwrap().hash.clone();
    let bad = Ledger::forge_block_at(1, &genesis_hash, "2026-01-01T00:00:00+00:00", &pk, vec![m1, m2]);
    sim.node_mut(&n).ledger_mut().chain.push(bad);

    match sim.check_invariants() {
        Err(Violation::Emission {
            seed: s,
            index,
            mining_count,
            ..
        }) => {
            assert_eq!(s, seed, "the violation carries the run's seed for replay");
            assert_eq!(index, 1, "the offending block is the height-1 block");
            assert_eq!(mining_count, 2, "two mining rewards in one block");
        }
        other => panic!("expected an Emission violation, got {other:?}"),
    }
}

/// **FORK-CAP-1 — the emission AMOUNT backstop has teeth.** A single mining
/// reward far above the per-block bound (a height-1 block minting 1M QUANTA,
/// which the count-only check `mining == 1` cannot see) is injected straight
/// into a node's chain (bypassing validation). `check_invariants` MUST return
/// `Violation::EmissionAmount`, carrying the seed and the offending amounts.
/// Without this backstop a regression of the fork-reorg emission gate would let
/// an over-emitter through and the sweep would stay green.
#[test]
fn forkcap_emission_amount_invariant_has_teeth() {
    let now0 = SIM_EPOCH_MS;
    let seed = 0xF0_4C_CA_77;
    let n = PeerId("node-N".into());
    let mut sim = Sim::new(seed, vec![(n.clone(), Node::new())]);
    sim.tick_all(now0);
    assert_eq!(sim.run_checked(10_000), Ok(()));

    // ONE mining reward, but 1M QUANTA ≫ the per-block bound → an over-emission
    // the count check (mining == 1) is blind to.
    let pk = addr_of(&seeded_identity(1)); // PQ-MIG-3B: value identity = ML-DSA address
    let mut o = Ledger::new();
    let evil = o.mine_tx(&pk, 1_000_000 * MICRO, 0.0);
    let genesis_hash = o.block_at(0).unwrap().hash.clone();
    let bad =
        Ledger::forge_block_at(1, &genesis_hash, "2026-01-01T00:00:00+00:00", &pk, vec![evil]);
    sim.node_mut(&n).ledger_mut().chain.push(bad);

    match sim.check_invariants() {
        Err(Violation::EmissionAmount {
            seed: s,
            index,
            block_minted,
            prior_mined,
            ..
        }) => {
            assert_eq!(s, seed, "the violation carries the run's seed for replay");
            assert_eq!(index, 1, "the offending block is the height-1 block");
            assert_eq!(block_minted, 1_000_000 * MICRO, "the over-emitted amount");
            assert_eq!(prior_mined, 0, "genesis minted nothing");
        }
        other => panic!("expected an EmissionAmount violation, got {other:?}"),
    }
}

/// **FORK-CAP-1 — the byzantine over-emit reorg is rejected by the sweep's
/// production path.** Drive the `OverEmitReorg` archetype through the SAME
/// `run_plan` executor as the sweep: node 0 adopts a legit height-1 block, then
/// the byzantine peer delivers an over-cap block at the same height (ground to a
/// higher hash, so it reaches the reorg tie-break). Production
/// `validate_block_emission_against` MUST reject it on the reorg branch, so:
/// (1) no invariant violation surfaces, (2) node 0 keeps the LEGIT tip (its
/// height-1 reward is 50 QUANTA, never the 1M over-emitter), (3) supply ≤ cap.
#[test]
fn forkcap_sweep_over_emit_reorg_is_rejected() {
    let t0 = SIM_EPOCH_MS;
    let plan = ScenarioPlan {
        seed: 0xF0_4C_CA_91,
        n_nodes: 2,
        id_seeds: vec![1, 2],
        moves: vec![
            Move::TickAll { at_ms: t0 },
            Move::Run { steps: 1_000 },
            Move::OverEmitReorg {
                byz_seed: 77,
                legit_reward: 50 * MICRO,
                evil_reward: 1_000_000 * MICRO,
                at_ms: t0,
            },
            Move::Run { steps: 4_000 },
            Move::Sync {
                from: 1,
                to: 0,
                at_ms: t0,
            },
            Move::Run { steps: 4_000 },
        ],
        max_steps: 40_000,
    };
    let (mut sim, violation, _) = run_plan(&plan);
    assert_eq!(
        violation, None,
        "production must REJECT the over-emitter on the reorg branch → no invariant violation"
    );

    let node0 = sim.node_mut(&nid(0)).ledger();
    assert_eq!(
        node0.chain.len(),
        2,
        "node 0 holds genesis + the legit height-1 block (the over-emitter was rejected)"
    );
    let tip_minted: u64 = node0.chain[1]
        .transactions
        .iter()
        .filter(|t| t.tx_type == TxType::Mining)
        .map(|t| t.amount)
        .sum();
    assert_eq!(
        tip_minted,
        50 * MICRO,
        "the 1M over-emitter must NOT have replaced the legit 50 QUANTA tip"
    );
    assert!(
        node0.total_supply() <= crate::p2p::reputation::MAX_SUPPLY_MICRO,
        "la masse en circulation ne dépasse JAMAIS 100M"
    );
}

/// **EMIT-1 E5 — a healthy run keeps all three.** Propose → flood → integrate
/// keeps safety, conservation, AND emission true at every step (`run_checked`),
/// the chain advances (liveness), and every node's chain holds exactly one
/// mining reward per non-genesis block — the one-reward-per-block property the
/// validation rule and the invariant jointly guarantee.
#[test]
fn emit_1_healthy_run_keeps_emission_safety_conservation() {
    let now0 = 1_800_000_000_000_u64;
    let (mut sim, a, b, c, a_pk) = three_node_sim(0x600D_0E05);
    sim.tick_all(now0);
    assert_eq!(sim.run_checked(10_000), Ok(()), "invariants hold during ticks");

    sim.node_mut(&a)
        .ledger_mut()
        .replay_remote_tx(frozen_mining(&a_pk, 50 * MICRO));
    sim.propose(
        &a,
        &[Validator {
            pk: a_pk,
            stake: MIN_VALIDATOR_STAKE,
            reputation: 0,
        }],
    );
    assert_eq!(
        sim.run_checked(10_000),
        Ok(()),
        "safety + conservation + EMISSION hold through propagation"
    );

    for id in [&a, &b, &c] {
        let l = sim.node_mut(id).ledger();
        assert_eq!(l.chain.len(), 2, "node {id:?} advanced (liveness)");
        let mining = l
            .chain
            .iter()
            .flat_map(|blk| blk.transactions.iter())
            .filter(|t| t.tx_type == TxType::Mining)
            .count() as u64;
        assert_eq!(
            mining,
            l.chain_height() - 1,
            "node {id:?}: exactly one mining reward per non-genesis block"
        );
    }
}

// ─── T0.8: multi-seed sweep + replay (the Phase 0 global gate) ─────────────
//
// Take the whole T0.4→T0.7 machinery and turn it through N pseudo-random,
// **seed-derived** scenarios, checking **all three invariants** (safety,
// conservation, emission) at a per-step cadence, with every failure replayable
// **byte-for-byte** from its seed.
//
// **Honest framing — a falsifier, not a proof.** Passing N seeds buys
// confidence proportional to N *and* to the fault coverage actually exercised,
// never a correctness proof — hence the two non-negotiable anti-vacuity teeth
// below (`t0_8_sweep_exercises_faults` §5.1, `t0_8_sweep_catches_planted_violation`
// §5.2).
//
// **One generator, two callers (§2).** [`scenario`] is the single pure
// derivation; the sweep AND the replay both go through it, so seed `S` always
// rebuilds the same [`ScenarioPlan`]. [`execute_scenario`] is the single
// executor. If the two derived the scenario by different paths, replay would
// reproduce nothing and all of T0.8 would be worthless.
//
// **The modelled space is the *recoverable* fault space** the protocol is
// designed to survive: a single sealing leader under drop/dup/delay, partitions
// that heal + sync, crash/restart that re-syncs, and byzantine equivocation
// honest nodes tie-break (delivered highest-hash-first so reconciliation is
// monotone — no transient split). The one KNOWN-unrecoverable case — two honest
// validators sealing on opposite sides of a partition (the PoC's documented
// ADR-001 split-brain) — is deliberately **not** in the random space: it has
// its own dedicated teeth ([`planted_fork_scenario`] +
// `sim_partition_fork_breaks_safety_and_is_detected`). Excluding a
// separately-tested *known* limitation is scoping, not masking (§7): a **new**
// violation in the modelled space is a LIVRABLE and makes the clean sweep fail
// loudly with its seed, invariant, and first faulty step.

/// Fixed virtual epoch shared by every scenario (a constant, never a wall-clock
/// read) — matches the existing T0.4+ tests' `now0`.
const SIM_EPOCH_MS: u64 = 1_800_000_000_000;

/// Default seed count for the suite-time sweep — sized for a few seconds, like
/// C1. `QUANTA_SIM_SEEDS` pushes a deeper (e.g. nightly) run. Reading the env is
/// a *test-harness* concern; the sans-IO core is untouched and the per-seed
/// outcome is still a pure function of the seed.
fn default_seed_count() -> u64 {
    std::env::var("QUANTA_SIM_SEEDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(64)
}

/// `lo..=hi` inclusive, drawn from the seeded stream (integer math, no float,
/// no `OsRng`, no `HashMap` iteration — spec §4 determinism).
fn sd_range(r: &mut Blake3Rng, lo: u64, hi: u64) -> u64 {
    if hi <= lo {
        return lo;
    }
    lo + r.next_u64() % (hi - lo + 1)
}

/// Deterministic RFC3339 from injected millis (mirrors [`signed_msg_bytes`]) —
/// a pure conversion of an injected value, NOT a clock read.
fn det_rfc3339(ts_ms: u64) -> String {
    chrono::DateTime::from_timestamp_millis(ts_ms as i64)
        .unwrap_or_default()
        .to_rfc3339()
}

/// A **deterministic** `NETWORK → pk` mining reward with an INJECTED timestamp
/// and a `seq`-unique hash. Unlike [`Ledger::mine_tx`], whose `Utc::now()`
/// makes it non-reproducible across runs (the existing tests dodge this by
/// building a `frozen` tx once and cloning), this stays byte-identical between
/// the sweep and the replay. `seq` keeps successive rewards' stored hashes
/// distinct so the mempool dedup (`seen_tx_hashes`) admits each. The merkle
/// leaf binds `tx_content_bytes` (from/to/amount/nonce/type/ts) + signature, so
/// this reward roots into a block exactly as a real one would; `NETWORK` is a
/// synthetic sender, so it needs no signature (`verify_tx` exempts it).
fn det_mining_tx(pk: &str, amount: u64, ts_ms: u64, seq: u64) -> Transaction {
    let ts = det_rfc3339(ts_ms);
    let hash = hex::encode(
        blake3::hash(format!("mine:{seq}:{pk}:{amount}:{ts}").as_bytes()).as_bytes(),
    );
    Transaction {
        id: format!("tx_mine_{seq}"),
        from: "NETWORK".into(),
        to: pk.into(),
        amount,
        tx_type: TxType::Mining,
        timestamp: ts,
        signature: String::new(),
        hash,
        nonce: 0,
        pq_signature: None,
        pq_public_key: None,
    }
}

/// Two conflicting height-1 blocks signed by the SAME byzantine key (an
/// equivocation), built with INJECTED timestamps via `forge_block_at` so the
/// pair is byte-reproducible (the existing `equivocating_blocks` seals with the
/// wall clock — fine for its non-determinism tests, unusable for replay). Both
/// pass `validate_block_against_prev` (coinbase `NETWORK → miner`, correct
/// hash), so honest nodes integrate the higher-hash one and reject the other.
/// Returns the two signed `NewBlock` envelopes **ordered highest-hash first**,
/// so delivering them in that order drives every node monotonically to the same
/// tip with no transient disagreement (which the per-step safety check would —
/// correctly — flag). `reward_a` must differ from `reward_b` or the two blocks
/// are identical and there is no equivocation.
fn forge_equivocation(
    byz: &CryptoEngine,
    now_ms: u64,
    reward_a: u64,
    reward_b: u64,
) -> (Vec<u8>, Vec<u8>) {
    let pk = addr_of(byz); // PQ-MIG-3B: forged-block miner = ML-DSA address (envelope stays Ed25519)
    let genesis_hash = Ledger::new()
        .block_at(0)
        .expect("genesis present")
        .hash
        .clone();
    let ts = det_rfc3339(now_ms);
    let forge = |amount: u64, seq: u64| {
        let tx = det_mining_tx(&pk, amount, now_ms, seq);
        Ledger::forge_block_at(1, &genesis_hash, &ts, &pk, vec![tx])
    };
    let ba = forge(reward_a, 9_001);
    let bb = forge(reward_b, 9_002);
    let (high, low) = if ba.hash >= bb.hash { (ba, bb) } else { (bb, ba) };
    let envelope = |b: &Block| {
        signed_msg_bytes(
            byz,
            GossipMessage::NewBlock {
                block_json: serde_json::to_string(b).expect("block json"),
            },
            now_ms,
        )
    };
    (envelope(&high), envelope(&low))
}

/// FORK-CAP-1: forge a LEGIT height-1 block and a same-height OVER-EMITTING
/// block whose hash is ground to EXCEED the legit one, so the over-emitter wins
/// the reorg tie-break (`block.hash > tip.hash`) and reaches
/// `validate_block_emission_against` — which rejects it, so it is never
/// integrated. Both are byte-reproducible (injected ts, deterministic seq
/// grind), so the sweep and the replay rebuild them identically. Returned LEGIT
/// first, EVIL second: deliver in that order so the legit block becomes node 0's
/// height-1 tip (happy path), then the over-emitter lands on the reorg branch.
fn forge_over_emit_reorg(
    byz: &CryptoEngine,
    now_ms: u64,
    legit_reward: u64,
    evil_reward: u64,
) -> (Vec<u8>, Vec<u8>) {
    let pk = addr_of(byz); // PQ-MIG-3B: forged-block miner = ML-DSA address (envelope stays Ed25519)
    let genesis_hash = Ledger::new()
        .block_at(0)
        .expect("genesis present")
        .hash
        .clone();
    let ts = det_rfc3339(now_ms);
    let legit_tx = det_mining_tx(&pk, legit_reward, now_ms, 7_001);
    let legit = Ledger::forge_block_at(1, &genesis_hash, &ts, &pk, vec![legit_tx]);
    // Grind the evil block's TIMESTAMP — which IS in the block-hash pre-image —
    // until the over-emitter beats the legit tip and so WINS the reorg tie-break
    // (`block.hash > tip.hash`), reaching the emission gate. NB: the tx id/hash
    // are NOT in the block hash (only tx CONTENT is, via the Merkle root), so
    // varying a sequence number alone would never move the hash. BLAKE3 is
    // ~uniform → a few tries; deterministic, so the sweep and the replay rebuild
    // the same over-emitter.
    let evil = (1..=1_000_000u64)
        .find_map(|k| {
            let ts_k = det_rfc3339(now_ms + k);
            let tx = det_mining_tx(&pk, evil_reward, now_ms + k, 7_002);
            let b = Ledger::forge_block_at(1, &genesis_hash, &ts_k, &pk, vec![tx]);
            (b.hash > legit.hash).then_some(b)
        })
        .expect("FORK-CAP sweep: could not grind an over-emitter hash above the legit tip");
    let envelope = |b: &Block| {
        signed_msg_bytes(
            byz,
            GossipMessage::NewBlock {
                block_json: serde_json::to_string(b).expect("block json"),
            },
            now_ms,
        )
    };
    (envelope(&legit), envelope(&evil))
}

/// Network fault knobs as plain **seed-derived data** (no live RNG handle), so a
/// [`ScenarioPlan`] is a pure value the sweep and the replay rebuild
/// identically. Materialised to [`NetFaults`] at execution time.
#[derive(Clone, PartialEq, Eq, Debug)]
struct FaultProfile {
    drop_ppm: u64,
    dup_ppm: u64,
    min_delay_ms: u64,
    max_delay_ms: u64,
}

impl FaultProfile {
    /// The lossless link (fixed delay, no drop/dup) — used to "heal" before
    /// sync recovery.
    fn lossless() -> Self {
        Self {
            drop_ppm: 0,
            dup_ppm: 0,
            min_delay_ms: NET_DELAY_MS,
            max_delay_ms: NET_DELAY_MS,
        }
    }
    fn to_net(&self) -> NetFaults {
        NetFaults {
            drop_ppm: self.drop_ppm,
            dup_ppm: self.dup_ppm,
            min_delay_ms: self.min_delay_ms,
            max_delay_ms: self.max_delay_ms,
            partition: None,
            withheld: BTreeSet::new(),
        }
    }
    /// Does this profile actually perturb delivery (drop / dup / variable
    /// delay)? The §5.1 coverage proof keys on this.
    fn is_faulty(&self) -> bool {
        self.drop_ppm > 0 || self.dup_ppm > 0 || self.max_delay_ms > self.min_delay_ms
    }
}

/// One orchestrated step of a scenario timeline. A [`ScenarioPlan`] is a `Vec`
/// of these, derived **purely** from the seed. The executor interprets them
/// against a fresh [`Sim`]; every orchestrated mutation is followed by an
/// invariant check, and every [`Move::Run`] checks invariants **per event
/// step** (the §4 cadence that catches transient divergence).
#[derive(Clone, PartialEq, Eq, Debug)]
enum Move {
    /// Tick all nodes' clocks to `at_ms` (envelope freshness + maintenance).
    TickAll { at_ms: u64 },
    /// Install a network fault profile (drop/dup/delay). Apply BEFORE
    /// `Partition` (it resets `NetFaults`, clearing any partition).
    SetFaults(FaultProfile),
    /// Partition the graph: node indices `a` vs `b` cannot communicate.
    Partition { a: Vec<usize>, b: Vec<usize> },
    /// Heal any partition.
    Heal,
    /// Crash node `idx` (its events are dropped; state retained).
    Crash { idx: usize },
    /// Restart node `idx`.
    Restart { idx: usize },
    /// Node `idx`, the elected leader, seals a block crediting itself `reward`.
    Propose { idx: usize, reward: u64 },
    /// An external byzantine equivocator broadcasts TWO conflicting height-1
    /// blocks (`reward_a != reward_b`). `low_first=false`: HIGH-hash first to
    /// EVERY node (monotone — each jumps straight to the tip). `low_first=true`:
    /// LOW then HIGH to node 0 ONLY (the reorg *sense* — adopt low, flip to
    /// high), isolated so no cross-node transient trips the per-step safety
    /// check; the archetype reconciles the rest via `Sync`.
    Equivocate {
        byz_seed: u64,
        reward_a: u64,
        reward_b: u64,
        at_ms: u64,
        low_first: bool,
    },
    /// FORK-CAP-1 byzantine archetype: an OVER-EMITTING reorg attempt. Node 0
    /// first adopts a LEGIT height-1 block (happy path), then the byzantine peer
    /// delivers a SECOND height-1 block at the same height whose mining reward
    /// breaks the emission bound, ground to a HIGHER hash so it reaches the
    /// reorg tie-break. Production validation (`validate_block_emission_against`)
    /// MUST reject it on the reorg branch, so node 0 keeps the legit tip and the
    /// supply never exceeds the cap. Without the FORK-CAP fix the over-emitter
    /// would be integrated and the amount backstop ([`Violation::EmissionAmount`])
    /// would scream — that is how the sweep now guards the gate.
    OverEmitReorg {
        byz_seed: u64,
        legit_reward: u64,
        evil_reward: u64,
        at_ms: u64,
    },
    /// Node `from` requests the chain from node `to` (recovery after a
    /// partition / crash). `from` must hold a signing identity.
    Sync { from: usize, to: usize, at_ms: u64 },
    /// A confirmed user transfer: leader `from` sends `amount` µQTA to `to`,
    /// the signed transfer + 1 % burn (timestamped at the **injected** virtual
    /// `at_ms` via `transfer_with_burn_at`, so it is byte-reproducible) entering
    /// `from`'s pending to ride in its next sealed block — conservation under
    /// the burn path. Underfunded ⇒ a no-op.
    Transfer {
        from: usize,
        to: usize,
        amount: u64,
        at_ms: u64,
    },
    /// Drive the event loop (checked) for up to `steps` events.
    Run { steps: u64 },
}

/// A fully seed-derived, bounded scenario. **Pure data**: [`scenario`] builds
/// it, [`execute_scenario`] runs it. ONE generator, two callers (sweep +
/// replay) — the property all of T0.8 rests on (§2).
#[derive(Clone, PartialEq, Eq, Debug)]
struct ScenarioPlan {
    seed: u64,
    /// Number of nodes (bounded 2..=3).
    n_nodes: usize,
    /// Per-node identity seed: node `i` signs as `seeded_identity(id_seeds[i])`.
    id_seeds: Vec<u64>,
    /// The orchestrated timeline.
    moves: Vec<Move>,
    /// Global step budget — guarantees the executor (hence the sweep)
    /// terminates regardless of the move list (spec §4 bornes).
    max_steps: u64,
}

impl ScenarioPlan {
    fn has_partition(&self) -> bool {
        self.moves.iter().any(|m| matches!(m, Move::Partition { .. }))
    }
    fn has_net_faults(&self) -> bool {
        self.moves
            .iter()
            .any(|m| matches!(m, Move::SetFaults(f) if f.is_faulty()))
    }
    fn has_equivocation(&self) -> bool {
        self.moves
            .iter()
            .any(|m| matches!(m, Move::Equivocate { .. }))
    }
    /// FORK-CAP-1: the over-emitting reorg archetype. Guarded by
    /// `t0_8_sweep_exercises_faults` so it can never silently vanish from the
    /// sweep (which would leave the fork-reorg emission gate untested while the
    /// clean sweep stayed green — the precise vacuity this anti-vacuity proof
    /// exists to prevent).
    fn has_over_emit_reorg(&self) -> bool {
        self.moves
            .iter()
            .any(|m| matches!(m, Move::OverEmitReorg { .. }))
    }
    fn has_crash(&self) -> bool {
        self.moves.iter().any(|m| matches!(m, Move::Crash { .. }))
    }
    fn has_transfer(&self) -> bool {
        self.moves.iter().any(|m| matches!(m, Move::Transfer { .. }))
    }
}

/// The outcome of running one scenario: the first invariant violation (if any),
/// the step it appeared at, the final convergence observables, and the full
/// byte-comparable effect trace. `PartialEq` makes "replay is byte-identical" a
/// single assertion.
#[derive(Clone, PartialEq, Eq, Debug)]
struct ScenarioOutcome {
    violation: Option<Violation>,
    first_faulty_step: Option<u64>,
    tips: BTreeMap<PeerId, Option<String>>,
    heights: BTreeMap<PeerId, usize>,
    trace: Vec<(u64, PeerId, Vec<Effect>)>,
}

/// Stable node label for index `i`.
fn nid(i: usize) -> PeerId {
    PeerId(format!("n{i}"))
}

/// The public key of the identity seeded by `id_seed`.
fn pk_of(id_seed: u64) -> String {
    // PQ-MIG-3B: the scenario **value** identity (validator weight, mining-reward
    // target, transfer `from`/`to`) is the ML-DSA address — election in
    // `propose_block_at` is address-keyed. Transport (envelope) stays Ed25519.
    addr_of(&seeded_identity(id_seed))
}

fn build_scenario_nodes(plan: &ScenarioPlan) -> Vec<(PeerId, Node)> {
    (0..plan.n_nodes)
        .map(|i| (nid(i), Node::with_identity(seeded_identity(plan.id_seeds[i]))))
        .collect()
}

/// **T0.8 §2 — the single derivation function.** Build a bounded scenario
/// purely from `seed`. Called by BOTH the sweep and the replay, so a failure at
/// seed `S` is reproducible byte-for-byte. The archetype (and all its knobs)
/// come from a seeded [`Blake3Rng`] — never `OsRng`, the wall clock, or
/// `HashMap` order. Each archetype is a *recoverable* fault family the protocol
/// is designed to survive; across the default range every family appears
/// (asserted by `t0_8_sweep_exercises_faults`).
fn scenario(seed: u64) -> ScenarioPlan {
    let mut r = Blake3Rng::from_seed(seed);
    match r.next_u64() % 4 {
        0 => scenario_faulty_link(seed, &mut r),
        1 => scenario_partition_heal(seed, &mut r),
        2 => scenario_crash_restart(seed, &mut r),
        _ => scenario_equivocation(seed, &mut r),
    }
}

/// Archetype 0 — a lossy link (drop guaranteed `> 0`, so this archetype always
/// contributes drop/dup/delay coverage), an optional second block, then heal +
/// sync recovery. A single leader, so honest nodes never fork.
fn scenario_faulty_link(seed: u64, r: &mut Blake3Rng) -> ScenarioPlan {
    let t0 = SIM_EPOCH_MS;
    let faults = FaultProfile {
        drop_ppm: sd_range(r, 100_000, 500_000),
        dup_ppm: sd_range(r, 0, 200_000),
        min_delay_ms: 1,
        max_delay_ms: sd_range(r, 2, 40),
    };
    let mut moves = vec![
        Move::SetFaults(faults),
        Move::TickAll { at_ms: t0 },
        Move::Run { steps: 1_000 },
        Move::Propose {
            idx: 0,
            reward: 50 * MICRO,
        },
        Move::Run { steps: 2_000 },
    ];
    // Optionally a confirmed user transfer (leader → peer, injected ts) riding
    // in a second block: exercises the 1 % burn and conservation-under-burn
    // across the sweep. The distinct second reward also keeps block 2 distinct.
    if r.next_u64().is_multiple_of(2) {
        moves.push(Move::Transfer {
            from: 0,
            to: 1,
            amount: sd_range(r, 1, 9) * MICRO,
            at_ms: t0,
        });
        moves.push(Move::Propose {
            idx: 0,
            reward: 49 * MICRO,
        });
        moves.push(Move::Run { steps: 2_000 });
    }
    moves.push(Move::SetFaults(FaultProfile::lossless()));
    moves.push(Move::Heal);
    moves.push(Move::Sync {
        from: 1,
        to: 0,
        at_ms: t0,
    });
    moves.push(Move::Sync {
        from: 2,
        to: 0,
        at_ms: t0,
    });
    moves.push(Move::Run { steps: 4_000 });
    ScenarioPlan {
        seed,
        n_nodes: 3,
        id_seeds: vec![1, 2, 3],
        moves,
        max_steps: 40_000,
    }
}

/// Archetype 1 — a graph partition isolates the leader; the far side lags, then
/// reconciles after heal via chain sync. A single leader, so no honest fork.
fn scenario_partition_heal(seed: u64, r: &mut Blake3Rng) -> ScenarioPlan {
    let t0 = SIM_EPOCH_MS;
    let n = sd_range(r, 2, 3) as usize;
    // Optional non-dropping perturbation (dup/delay) layered on; the partition
    // is the headline fault.
    let faults = FaultProfile {
        drop_ppm: 0,
        dup_ppm: sd_range(r, 0, 100_000),
        min_delay_ms: 1,
        max_delay_ms: sd_range(r, 1, 20),
    };
    let mut moves = vec![
        Move::SetFaults(faults),
        Move::Partition {
            a: vec![0],
            b: (1..n).collect(),
        },
        Move::TickAll { at_ms: t0 },
        Move::Run { steps: 1_000 },
        Move::Propose {
            idx: 0,
            reward: 50 * MICRO,
        },
        Move::Run { steps: 2_000 },
        Move::SetFaults(FaultProfile::lossless()),
        Move::Heal,
    ];
    for behind in 1..n {
        moves.push(Move::Sync {
            from: behind,
            to: 0,
            at_ms: t0,
        });
    }
    moves.push(Move::Run { steps: 4_000 });
    ScenarioPlan {
        seed,
        n_nodes: n,
        id_seeds: (1..=n as u64).collect(),
        moves,
        max_steps: 40_000,
    }
}

/// Archetype 2 — a non-leader crashes while a block is produced (misses it),
/// then re-syncs on restart. A single leader, so no honest fork.
fn scenario_crash_restart(seed: u64, r: &mut Blake3Rng) -> ScenarioPlan {
    let t0 = SIM_EPOCH_MS;
    let n = sd_range(r, 2, 3) as usize;
    let faults = FaultProfile {
        drop_ppm: 0,
        dup_ppm: sd_range(r, 0, 100_000),
        min_delay_ms: 1,
        max_delay_ms: sd_range(r, 1, 20),
    };
    let victim = 1usize;
    let mut moves = vec![
        Move::SetFaults(faults),
        Move::TickAll { at_ms: t0 },
        Move::Run { steps: 1_000 },
        Move::Crash { idx: victim },
        Move::Propose {
            idx: 0,
            reward: 50 * MICRO,
        },
        Move::Run { steps: 2_000 },
        Move::Restart { idx: victim },
        Move::SetFaults(FaultProfile::lossless()),
        Move::Sync {
            from: victim,
            to: 0,
            at_ms: t0,
        },
        Move::Run { steps: 4_000 },
    ];
    if n == 3 {
        moves.push(Move::Sync {
            from: 2,
            to: 0,
            at_ms: t0,
        });
        moves.push(Move::Run { steps: 2_000 });
    }
    ScenarioPlan {
        seed,
        n_nodes: n,
        id_seeds: (1..=n as u64).collect(),
        moves,
        max_steps: 40_000,
    }
}

/// Archetype 3 — a byzantine equivocator broadcasts two conflicting height-1
/// blocks to every honest node, delivered highest-hash-first so reconciliation
/// is monotone and all nodes converge on the same tip (no transient fork). The
/// equivocating envelopes are direct-scheduled, so the network fault profile is
/// intentionally left lossless here.
fn scenario_equivocation(seed: u64, r: &mut Blake3Rng) -> ScenarioPlan {
    let t0 = SIM_EPOCH_MS;
    let n = sd_range(r, 2, 3) as usize;
    // A byzantine key well outside the honest range 1..=3.
    let byz_seed = 50 + (r.next_u64() % 50);
    // FORK-CAP-1: a fraction (~1/3) of the byzantine archetype is an
    // OVER-EMITTING reorg — node 0 adopts a legit height-1 block, then the
    // byzantine peer pushes an over-cap block on the REORG branch that
    // production MUST reject. This exercises the fork-reorg emission gate AND
    // the amount backstop continuously across the sweep; until the archetype
    // could generate this, the sweep was blind to the class.
    let over_emit = r.next_u64().is_multiple_of(3);
    let mut moves = vec![Move::TickAll { at_ms: t0 }, Move::Run { steps: 1_000 }];
    if over_emit {
        moves.push(Move::OverEmitReorg {
            byz_seed,
            legit_reward: 50 * MICRO,
            evil_reward: 1_000_000 * MICRO, // ≫ per-block bound → rejected
            at_ms: t0,
        });
        moves.push(Move::Run { steps: 4_000 });
        // node 0 kept the LEGIT tip (over-emitter rejected); the rest converge
        // by syncing it.
        for behind in 1..n {
            moves.push(Move::Sync {
                from: behind,
                to: 0,
                at_ms: t0,
            });
        }
        moves.push(Move::Run { steps: 4_000 });
    } else {
        let reward_a = 50 * MICRO;
        let reward_b = reward_a - MICRO; // distinct ⇒ the two blocks truly differ
        // Cover BOTH delivery senses across the sweep (T0.8-HARDEN §2a):
        // monotone (high-first to all) and the reorg flip (low-then-high,
        // isolated to node 0).
        let low_first = r.next_u64().is_multiple_of(2);
        moves.push(Move::Equivocate {
            byz_seed,
            reward_a,
            reward_b,
            at_ms: t0,
            low_first,
        });
        moves.push(Move::Run { steps: 4_000 });
        if low_first {
            // Node 0 reorged in isolation; the rest converge by syncing from it.
            for behind in 1..n {
                moves.push(Move::Sync {
                    from: behind,
                    to: 0,
                    at_ms: t0,
                });
            }
            moves.push(Move::Run { steps: 4_000 });
        }
    }
    ScenarioPlan {
        seed,
        n_nodes: n,
        id_seeds: (1..=n as u64).collect(),
        moves,
        max_steps: 40_000,
    }
}

/// **§5.2 — the planted, KNOWN-bad scenario** (NOT in the random space): two
/// staked validators seal different height-1 blocks on opposite sides of a
/// partition that never heals — the documented ADR-001 split-brain. Run through
/// the SAME [`execute_scenario`] as the sweep, it MUST surface a `Safety`
/// violation carrying `seed`; replaying it reproduces the violation
/// byte-for-byte. A runner that cannot catch a planted violation validates
/// everything by construction.
fn planted_fork_scenario(seed: u64) -> ScenarioPlan {
    let t0 = SIM_EPOCH_MS;
    ScenarioPlan {
        seed,
        n_nodes: 2,
        id_seeds: vec![1, 2],
        moves: vec![
            Move::Partition {
                a: vec![0],
                b: vec![1],
            },
            Move::TickAll { at_ms: t0 },
            Move::Run { steps: 1_000 },
            // Both sides seal a DIFFERENT block at height 1 (different miner ⇒
            // different content ⇒ different hash); the partition blocks
            // reconciliation, so correct nodes disagree at a shared index.
            Move::Propose {
                idx: 0,
                reward: 50 * MICRO,
            },
            Move::Propose {
                idx: 1,
                reward: 50 * MICRO,
            },
            Move::Run { steps: 1_000 },
        ],
        max_steps: 10_000,
    }
}

/// **T0.8 §3 — the single executor core.** Build a fresh [`Sim`] from `plan`
/// and interpret its move timeline, checking the three invariants after every
/// orchestrated mutation AND per event step inside each [`Move::Run`]. Stops at
/// the first violation, recording the cumulative step at which it appeared.
/// Returns the final [`Sim`] so tests can inspect the resulting chains
/// (coverage / conservation-under-burn). Fully deterministic given the plan
/// (itself a pure function of the seed), so two runs are byte-identical.
fn run_plan(plan: &ScenarioPlan) -> (Sim, Option<Violation>, Option<u64>) {
    let mut sim = Sim::new(plan.seed, build_scenario_nodes(plan));
    let mut steps: u64 = 0;
    let mut mint_seq: u64 = 0;
    let mut violation: Option<Violation> = None;
    let mut first_faulty_step: Option<u64> = None;

    for mv in &plan.moves {
        if violation.is_some() {
            break;
        }
        // A non-`Run` orchestrated mutation can itself break an invariant (e.g.
        // the planted second proposal). Check right after, attributing it to the
        // current cumulative step (spec §4 "premier pas fautif").
        let mut check_after = true;
        match mv {
            Move::TickAll { at_ms } => sim.tick_all(*at_ms),
            Move::SetFaults(f) => sim.set_faults(f.to_net()),
            Move::Partition { a, b } => {
                let ids_a: Vec<PeerId> = a.iter().map(|i| nid(*i)).collect();
                let ids_b: Vec<PeerId> = b.iter().map(|i| nid(*i)).collect();
                sim.partition(&ids_a, &ids_b);
            }
            Move::Heal => sim.heal(),
            Move::Crash { idx } => sim.crash(&nid(*idx)),
            Move::Restart { idx } => sim.restart(&nid(*idx)),
            Move::Propose { idx, reward } => {
                let pk = pk_of(plan.id_seeds[*idx]);
                mint_seq += 1;
                let reward_tx = det_mining_tx(&pk, *reward, SIM_EPOCH_MS, mint_seq);
                sim.node_mut(&nid(*idx)).ledger_mut().replay_remote_tx(reward_tx);
                let validators = vec![Validator {
                    pk: pk.clone(),
                    stake: MIN_VALIDATOR_STAKE,
                    reputation: 0,
                }];
                sim.propose(&nid(*idx), &validators);
            }
            Move::Equivocate {
                byz_seed,
                reward_a,
                reward_b,
                at_ms,
                low_first,
            } => {
                let byz = seeded_identity(*byz_seed);
                let (env_high, env_low) = forge_equivocation(&byz, *at_ms, *reward_a, *reward_b);
                let byz_peer = PeerId("byz".into());
                let deliver = |sim: &mut Sim, i: usize, bytes: Vec<u8>| {
                    sim.schedule(
                        *at_ms,
                        nid(i),
                        Event::MessageReceived {
                            from: byz_peer.clone(),
                            bytes,
                        },
                    );
                };
                if *low_first {
                    // Reorg sense, isolated to node 0: adopt LOW, then flip to
                    // HIGH. Others stay at genesis and reconcile via Sync, so no
                    // cross-node transient at the same index.
                    deliver(&mut sim, 0, env_low.clone());
                    deliver(&mut sim, 0, env_high.clone());
                } else {
                    // Monotone: HIGH first to every node.
                    for i in 0..plan.n_nodes {
                        deliver(&mut sim, i, env_high.clone());
                        deliver(&mut sim, i, env_low.clone());
                    }
                }
                check_after = false; // nothing applied yet — the Run checks it
            }
            Move::OverEmitReorg {
                byz_seed,
                legit_reward,
                evil_reward,
                at_ms,
            } => {
                let byz = seeded_identity(*byz_seed);
                let (env_legit, env_evil) =
                    forge_over_emit_reorg(&byz, *at_ms, *legit_reward, *evil_reward);
                let byz_peer = PeerId("byz".into());
                // Legit first → node 0 adopts it at height 1 (happy path); then
                // the over-emitter at the same height → reorg branch → rejected
                // by `validate_block_emission_against` (never integrated). The
                // following `Run` checks invariants per event step.
                sim.schedule(
                    *at_ms,
                    nid(0),
                    Event::MessageReceived {
                        from: byz_peer.clone(),
                        bytes: env_legit,
                    },
                );
                sim.schedule(
                    *at_ms,
                    nid(0),
                    Event::MessageReceived {
                        from: byz_peer.clone(),
                        bytes: env_evil,
                    },
                );
                check_after = false; // applied during the following Run
            }
            Move::Sync { from, to, at_ms } => {
                let req = signed_msg_bytes(
                    &seeded_identity(plan.id_seeds[*from]),
                    GossipMessage::RequestChain {
                        from_height: 1,
                        max_blocks: 50,
                    },
                    *at_ms,
                );
                sim.schedule(
                    *at_ms,
                    nid(*to),
                    Event::MessageReceived {
                        from: nid(*from),
                        bytes: req,
                    },
                );
                check_after = false;
            }
            Move::Transfer {
                from,
                to,
                amount,
                at_ms,
            } => {
                let crypto = seeded_identity(plan.id_seeds[*from]);
                let from_pk = addr_of(&crypto); // PQ-MIG-3B: transfer `from` = ML-DSA address
                let to_pk = pk_of(plan.id_seeds[*to]);
                // Injected ts ⇒ byte-reproducible. Best-effort: an underfunded
                // transfer is a no-op (conservation holds either way). The
                // signed transfer + burn sit in `from`'s pending to be sealed
                // into its next block.
                let _ = sim.node_mut(&nid(*from)).ledger_mut().transfer_with_burn_at(
                    &from_pk,
                    &to_pk,
                    *amount,
                    &crypto,
                    det_rfc3339(*at_ms),
                    true, // deterministic ML-DSA signing ⇒ byte-reproducible
                );
            }
            Move::Run { steps: budget } => {
                check_after = false; // run_checked_steps already checks per step
                let remaining = plan.max_steps.saturating_sub(steps);
                let run_budget = (*budget).min(remaining);
                match sim.run_checked_steps(run_budget) {
                    Ok(ran) => steps += ran,
                    Err(e) => {
                        let (ran, v) = *e;
                        steps += ran;
                        violation = Some(v);
                        first_faulty_step = Some(steps);
                    }
                }
            }
        }
        if check_after && violation.is_none() {
            if let Err(v) = sim.check_invariants() {
                violation = Some(v);
                first_faulty_step = Some(steps);
            }
        }
    }

    (sim, violation, first_faulty_step)
}

/// **T0.8 §3 — the single executor.** [`run_plan`] reduced to its
/// [`ScenarioOutcome`] (first violation + step, final tips/heights, byte trace).
fn execute_scenario(plan: &ScenarioPlan) -> ScenarioOutcome {
    let (sim, violation, first_faulty_step) = run_plan(plan);
    ScenarioOutcome {
        violation,
        first_faulty_step,
        tips: sim.tips(),
        heights: sim.heights(),
        trace: sim.trace,
    }
}

/// Tally `(transfers, burns, mining)` txs across every node's **sealed** chain
/// after running `plan` — the observable the Phase 1 coverage / burn tests key
/// on (a user transfer that never reaches a block, or a burn that never fires,
/// would mean the load is generated but not exercised).
fn chain_tx_tally(plan: &ScenarioPlan) -> (usize, usize, usize) {
    let (sim, _, _) = run_plan(plan);
    let (mut transfers, mut burns, mut mining) = (0usize, 0usize, 0usize);
    for node in sim.nodes.values() {
        for block in &node.ledger().chain {
            for tx in &block.transactions {
                match tx.tx_type {
                    TxType::Transfer => transfers += 1,
                    TxType::Burn => burns += 1,
                    TxType::Mining => mining += 1,
                    _ => {}
                }
            }
        }
    }
    (transfers, burns, mining)
}

/// **T0.8 §3 — replay.** Reconstruct seed `S`'s scenario through the SAME
/// [`scenario`] generator and re-run it. Byte-identical to that seed's pass in
/// the sweep (same plan, same deterministic executor).
fn replay(seed: u64) -> ScenarioOutcome {
    execute_scenario(&scenario(seed))
}

/// **T0.8.1 — `clean_default_sweep`.** The default seed range produces **zero**
/// invariant violations, within the suite time budget. Per §7 a violation is a
/// LIVRABLE, not a failure to paper over: this panics with the offending seed,
/// invariant, and first faulty step so the bug is reported, never masked.
#[test]
fn t0_8_clean_default_sweep() {
    let n = default_seed_count();
    let mut violations: Vec<(u64, Violation, Option<u64>)> = Vec::new();
    for seed in 0..n {
        let outcome = execute_scenario(&scenario(seed));
        if let Some(v) = outcome.violation {
            violations.push((seed, v, outcome.first_faulty_step));
        }
    }
    assert!(
        violations.is_empty(),
        "T0.8 sweep over {n} seeds found invariant violation(s) — each is a \
         LIVRABLE (report seed + invariant + first faulty step; replay with \
         QUANTA_SIM_SEED=<seed>); do NOT mask: {violations:#?}"
    );
}

/// **T0.8.2 — `sweep_is_reproducible`.** The same seed range run twice yields
/// **byte-identical** outcomes (hence an identical faulty-seed set) — the direct
/// consequence of per-seed determinism, asserted on the full outcome, not just
/// the violation set, so it is meaningful even on a clean (violation-free) run.
#[test]
fn t0_8_sweep_is_reproducible() {
    let run_range = || -> Vec<ScenarioOutcome> {
        (0..32u64).map(|s| execute_scenario(&scenario(s))).collect()
    };
    assert_eq!(
        run_range(),
        run_range(),
        "same seed range ⇒ byte-identical outcomes (incl. the faulty-seed set)"
    );
}

/// First seed in `0..256` whose scenario actually drives faults/byzantine
/// behaviour — used to anchor the replay-fidelity test on a *rich* plan rather
/// than a happy path. Guaranteed to exist (the coverage test proves every
/// family appears in the default range).
fn first_rich_seed() -> u64 {
    (0..256u64)
        .find(|&s| {
            let p = scenario(s);
            p.has_equivocation() || p.has_partition() || p.has_crash() || p.has_net_faults()
        })
        .expect("a faulty scenario exists in 0..256")
}

/// **T0.8.3 — `replay_is_byte_identical`.** A seed whose scenario exercises real
/// faults/byzantine behaviour, replayed twice, is identical down to the effect
/// trace — extending C1 from the happy path to the full fault plan.
#[test]
fn t0_8_replay_is_byte_identical() {
    let seed = first_rich_seed();
    let plan = scenario(seed);
    assert!(
        plan.has_equivocation() || plan.has_partition() || plan.has_crash() || plan.has_net_faults(),
        "the replayed seed must drive real faults, else this only re-checks the happy path"
    );
    assert_eq!(
        replay(seed),
        replay(seed),
        "replay of seed {seed} reproduces the rich scenario byte-for-byte"
    );
}

/// **T0.8.4 — `sweep_exercises_faults` (§5.1 anti-vacuity).** Over the default
/// range the generator really produces **all four** fault families — partitions,
/// drops/dups/delays, byzantine equivocators, crash/restart. Without this a
/// sweep of happy paths would "pass" by testing the void.
#[test]
fn t0_8_sweep_exercises_faults() {
    // A FIXED range (not `default_seed_count()`): the coverage proof is about
    // the generator, so it must not weaken under a small `QUANTA_SIM_SEEDS`.
    const COVERAGE_SEEDS: u64 = 128;
    let (mut partition, mut net, mut equivocation, mut crash, mut over_emit) =
        (false, false, false, false, false);
    for seed in 0..COVERAGE_SEEDS {
        let p = scenario(seed);
        partition |= p.has_partition();
        net |= p.has_net_faults();
        equivocation |= p.has_equivocation();
        crash |= p.has_crash();
        over_emit |= p.has_over_emit_reorg();
    }
    assert!(partition, "generator must produce partitions");
    assert!(net, "generator must produce drops/dups/delays");
    assert!(equivocation, "generator must produce byzantine equivocators");
    assert!(crash, "generator must produce crash/restart");
    // FORK-CAP-1: the over-emitting reorg archetype MUST appear, else the
    // fork-reorg emission gate is untested by the sweep while it stays green.
    assert!(
        over_emit,
        "generator must produce over-emitting reorg attempts (FORK-CAP-1)"
    );
}

/// **T0.8.5 — `sweep_catches_planted_violation` (§5.2 teeth + replay
/// fidelity).** A planted, known-bad scenario (the two-validator split-brain) is
/// flagged by the executor as a `Safety` violation **carrying its seed**, with a
/// recorded first faulty step — AND replaying that seed reproduces the violation
/// byte-for-byte. One test proves both the teeth and the replay fidelity.
#[test]
fn t0_8_sweep_catches_planted_violation() {
    let seed = 0x0BAD_F02C_u64;
    let outcome = execute_scenario(&planted_fork_scenario(seed));
    match &outcome.violation {
        Some(Violation::Safety {
            seed: s, index, ..
        }) => {
            assert_eq!(*s, seed, "the violation must carry the planted seed (replayable)");
            assert_eq!(*index, 1, "the split-brain conflicts at the first non-genesis block");
        }
        other => panic!("planted fork must surface a Safety violation, got {other:?}"),
    }
    assert!(
        outcome.first_faulty_step.is_some(),
        "the first faulty step must be recorded"
    );
    assert_eq!(
        execute_scenario(&planted_fork_scenario(seed)),
        outcome,
        "replay of the planted seed reproduces the violation byte-for-byte"
    );
}

/// **T0.8 §3 — single-seed replay via `QUANTA_SIM_SEED`.** Set the env var to
/// replay one seed with its outcome surfaced for debugging (`--nocapture`);
/// unset (the suite default) it is a no-op. The replayed run uses the SAME
/// [`scenario`] as the sweep, so it reproduces that seed's behaviour exactly.
#[test]
fn t0_8_replay_from_env_seed() {
    let Some(seed) = std::env::var("QUANTA_SIM_SEED")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
    else {
        return; // default: nothing to replay
    };
    let a = replay(seed);
    let b = replay(seed);
    assert_eq!(a, b, "replay must be deterministic for seed {seed}");
    eprintln!(
        "replay seed={seed}: violation={:?} first_faulty_step={:?} heights={:?}",
        a.violation, a.first_faulty_step, a.heights
    );
}

// ─── T0.8-HARDEN Phase 1: tx load restored (injected time + deterministic sig) ─

/// First seed in `0..256` whose scenario actually seals a user transfer —
/// anchors the transfer tests on a real transfer load (guaranteed to exist:
/// `coverage_transfers_and_burns` proves transfers appear in the default range).
fn first_transfer_seed() -> u64 {
    (0..256u64)
        .find(|&s| scenario(s).has_transfer())
        .expect("a transfer-bearing scenario exists in 0..256")
}

/// **Phase 1.4 — `determinism_with_transfers`.** A seed whose load includes a
/// signed user transfer — now timestamped by the **virtual** clock (C7) AND
/// ML-DSA-signed **deterministically** — replayed twice is byte-identical. This
/// is exactly the regression that caught the original `Utc::now()` leak (and the
/// ML-DSA `OsRng` leak); both are now closed, so it must pass. C1 stays green.
#[test]
fn t0_8_determinism_with_transfers() {
    let seed = first_transfer_seed();
    assert!(
        scenario(seed).has_transfer(),
        "the chosen seed must carry a user transfer"
    );
    assert_eq!(
        replay(seed),
        replay(seed),
        "a transfer-bearing seed replays byte-for-byte (injected ts + deterministic ML-DSA)"
    );
}

/// **Phase 1.4 — `conservation_under_burn`.** Across the default sweep, with
/// real burns sealed into blocks, `Σ balances + burned == minted` holds at every
/// step — conservation now exercises the **burn** path, not just minting.
#[test]
fn t0_8_conservation_under_burn() {
    let mut total_burns = 0usize;
    let mut violations = 0usize;
    for seed in 0..default_seed_count() {
        let plan = scenario(seed);
        let (_t, burns, _m) = chain_tx_tally(&plan);
        total_burns += burns;
        if execute_scenario(&plan).violation.is_some() {
            violations += 1;
        }
    }
    assert!(
        total_burns > 0,
        "the sweep must seal real burns, else conservation-under-burn is vacuous"
    );
    assert_eq!(
        violations, 0,
        "conservation (Σ balances + burned == minted, with burns present) holds across the sweep"
    );
}

/// **Phase 1.4 — `coverage_transfers_and_burns` (anti-vacuity).** Over the
/// default range, user transfers are REALLY sealed into blocks and burns REALLY
/// fire — otherwise the burn path is generated but never exercised.
#[test]
fn t0_8_coverage_transfers_and_burns() {
    let (mut transfers, mut burns) = (0usize, 0usize);
    for seed in 0..default_seed_count() {
        let (t, b, _m) = chain_tx_tally(&scenario(seed));
        transfers += t;
        burns += b;
    }
    assert!(transfers > 0, "sweep must seal user transfers into blocks");
    assert!(burns > 0, "sweep must produce burns (1 % per transfer)");
}

/// **Phase 1.4 — `prod_tx_still_timestamps_at_edge`.** The production wrapper
/// `transfer_with_burn` (the path the Tauri command at `lib.rs` calls) still
/// stamps the tx at the REAL wall clock and signs with hedged `OsRng` — the
/// Phase 1 refactor moved the read point to a boundary wrapper, not the
/// behaviour. Asserts the resulting tx timestamp is within seconds of now.
#[test]
fn t0_8_prod_tx_still_timestamps_at_edge() {
    let mut ledger = Ledger::new();
    let crypto = seeded_identity(1);
    let pk = addr_of(&crypto); // PQ-MIG-3B: value identity = ML-DSA address
    ledger.mine_tx(&pk, 100 * MICRO, 0.0);
    let to = "b".repeat(64);
    let (tx, burn, _amt) = ledger
        .transfer_with_burn(&pk, &to, 10 * MICRO, &crypto)
        .expect("prod transfer succeeds");
    let tx_secs = chrono::DateTime::parse_from_rfc3339(&tx.timestamp)
        .expect("tx timestamp is rfc3339")
        .timestamp();
    let now_secs = chrono::Utc::now().timestamp();
    assert!(
        (now_secs - tx_secs).abs() < 120,
        "prod transfer must timestamp at the real wall clock (drift {}s)",
        now_secs - tx_secs
    );
    // And it stays hybrid-signed (a real PQ layer), unlike a classical-only tx.
    assert!(
        burn.is_some_and(|b| b.pq_signature.is_some_and(|s| !s.is_empty())),
        "prod path keeps the hybrid ML-DSA layer"
    );
}

// ─── PQ-MIG-5 : genèse post-quantique — les dents §5 (côté harnais) ───────────

/// **PQ-MIG-5 §5 — adresses de genèse liées à leurs graines.** Chaque adresse de
/// genèse figée EST l'adresse PQ-MIG-2 `BLAKE3(ADDR_DOMAIN ‖ clé ML-DSA)` d'une
/// clé de test dérivée d'une graine connue — donc les constantes câblées ne sont
/// pas du « hex magique » mais des adresses ML-DSA reproductibles (PQ-MIG-2), ce
/// que la dent « enchaînement » exploite pour signer depuis un compte de genèse.
#[test]
fn pqmig5_genesis_addresses_bind_their_seeds() {
    assert_eq!(addr_of(&seeded_identity(5_000_001)), Ledger::GENESIS_ADDR_0);
    assert_eq!(addr_of(&seeded_identity(5_000_002)), Ledger::GENESIS_ADDR_1);
    assert_eq!(addr_of(&seeded_identity(5_000_003)), Ledger::GENESIS_ADDR_2);
}

/// **PQ-MIG-5 §5 — enchaînement.** Un premier bloc bâti sur la genèse PQ
/// (allocation DEV) est **valide** — couverture (COVER-1), émission (FORK-CAP-1)
/// et binding ML-DSA (PQ-MIG-3B) — et **conserve**. G0 (compte de genèse, 50 QTA
/// dépensables) mine une petite récompense et **dépense une partie de sa valeur de
/// genèse** via un transfert signé ; un récepteur frais sur la **même** genèse PQ
/// l'intègre (`Ok(true)`), prouvant que la validation reçue passe. Après
/// enchaînement, `Σ dépensable + enjeu-verrouillé + brûlé == miné` des deux côtés,
/// le miné n'ayant grandi que de la récompense.
#[test]
fn pqmig5_first_block_on_pq_genesis_validates_and_conserves() {
    let now0 = SIM_EPOCH_MS;
    let ts = det_rfc3339(now0);
    let crypto = seeded_identity(5_000_001); // la clé de GENESIS_ADDR_0
    let g0 = addr_of(&crypto);
    assert_eq!(g0, Ledger::GENESIS_ADDR_0);

    let mut l = Ledger::genesis_with_allocation(Ledger::DEV_GENESIS_ALLOCATION);
    // Récompense de bloc déterministe + transfert signé de 10 QTA pris sur les
    // 50 QTA de genèse de G0 (exerce émission + couverture + binding).
    l.replay_remote_tx(det_mining_tx(&g0, 7 * MICRO, now0, 1));
    l.transfer_tx_at(&g0, Ledger::GENESIS_ADDR_2, 10 * MICRO, &crypto, ts.clone(), true)
        .expect("transfert signé depuis un compte de genèse");
    let block1 = l.seal_block_at(&g0, 0.0, ts.clone());
    assert_eq!(block1.index, 1, "premier bloc après la genèse");
    assert_eq!(
        block1.prev_hash,
        l.block_at(0).unwrap().hash,
        "il s'enchaîne sur la genèse PQ"
    );

    // Un récepteur frais sur la MÊME genèse PQ valide entièrement le bloc.
    let mut recv = Ledger::genesis_with_allocation(Ledger::DEV_GENESIS_ALLOCATION);
    assert_eq!(
        recv.integrate_remote_block(block1.clone()),
        Ok(true),
        "premier bloc valide sur la genèse PQ (couverture/émission/binding)"
    );

    for led in [&l, &recv] {
        let spendable: u64 = led.all_balances().values().sum();
        assert_eq!(
            spendable + led.locked_stake_total() + led.total_burned(),
            led.total_minted(),
            "conservation après le premier bloc"
        );
        assert_eq!(led.total_minted(), 107 * MICRO, "100 genèse + 7 récompense");
    }
    assert_eq!(
        l.balance_of(Ledger::GENESIS_ADDR_2),
        20 * MICRO,
        "G2 reçoit 10 en plus de ses 10 de genèse"
    );
}

/// **PQ-MIG-5 §5 — déterminisme global (C1).** Une chaîne partant de la genèse PQ
/// est **byte-identique** entre deux exécutions : même genèse (constante) + même
/// récompense déterministe + même transfert signé déterministe ⇒ même hash de
/// bloc #1. C1 tient de la genèse au premier bloc.
#[test]
fn pqmig5_pq_genesis_chain_is_deterministic() {
    let now0 = SIM_EPOCH_MS;
    let ts = det_rfc3339(now0);
    let build = || {
        let crypto = seeded_identity(5_000_001);
        let g0 = addr_of(&crypto);
        let mut l = Ledger::genesis_with_allocation(Ledger::DEV_GENESIS_ALLOCATION);
        l.replay_remote_tx(det_mining_tx(&g0, 7 * MICRO, now0, 1));
        l.transfer_tx_at(&g0, Ledger::GENESIS_ADDR_2, 10 * MICRO, &crypto, ts.clone(), true)
            .expect("transfert signé");
        l.seal_block_at(&g0, 0.0, ts.clone())
    };
    let a = build();
    let b = build();
    assert_eq!(
        a.hash, b.hash,
        "C1 : chaîne depuis la genèse PQ byte-identique"
    );
    assert_eq!(
        Ledger::genesis_with_allocation(Ledger::DEV_GENESIS_ALLOCATION)
            .block_at(0)
            .unwrap()
            .hash,
        Ledger::genesis_with_allocation(Ledger::DEV_GENESIS_ALLOCATION)
            .block_at(0)
            .unwrap()
            .hash,
        "C1 : la genèse PQ elle-même est byte-identique"
    );
}

// ─── T0.8-HARDEN Phase 2a: lowest-hash-first reorg (the reorg sense, tested) ──

/// **Phase 2a — `single_block_reorg_lowest_hash_first_reconciles`.** The reorg
/// *sense* the monotone (highest-first) sweep never exercises: a node adopts the
/// LOW-hash block at an index, then the HIGH-hash block arrives and **forces a
/// flip**. The loser carries a signed **user transfer** (+ burn) and a mining
/// reward; after the flip the node must (a) converge on the winner, (b) re-queue
/// the loser's **user** txs (EMIT-1 §4.1 / AUDIT-BLK-1), (c) NOT re-queue the
/// loser's **mining** reward — no double-mint — with all three invariants held
/// at every step. Delivered to one node (isolated) so no cross-node transient
/// trips the per-step safety check; a peer then converges via sync.
///
/// **Expected GREEN.** Red (safety, a lost transfer, or a double-mint) is a real
/// bug in a SUPPORTED case ⇒ STOP and report the seed (T0.8 §7) — do not green.
#[test]
fn t0_8_single_block_reorg_lowest_hash_first_reconciles() {
    let now0 = SIM_EPOCH_MS;
    let ts = det_rfc3339(now0);
    let n = PeerId("n0".into());
    let p = PeerId("n1".into());
    let n_crypto = seeded_identity(1);
    let pk = addr_of(&n_crypto); // PQ-MIG-3B: miner + transfer sender = ML-DSA address
    let y = "d".repeat(64); // transfer recipient

    let mut sim = Sim::new(
        0x2A5EED,
        vec![
            (n.clone(), Node::with_identity(seeded_identity(1))),
            (p.clone(), Node::with_identity(seeded_identity(2))),
        ],
    );
    sim.tick_all(now0);
    assert_eq!(sim.run_checked(10_000), Ok(()));

    // index 1: a block crediting `pk` (so it can fund the transfer). Both nodes
    // integrate it directly (test setup; the reorg under test is delivered).
    let genesis = Ledger::new().block_at(0).unwrap().hash.clone();
    let b1 = Ledger::forge_block_at(1, &genesis, &ts, &pk, vec![det_mining_tx(&pk, 100 * MICRO, now0, 1)]);
    for node in [&n, &p] {
        assert_eq!(
            sim.node_mut(node).ledger_mut().integrate_remote_block(b1.clone()),
            Ok(true),
            "index 1 integrated"
        );
    }

    // The signed transfer (+ burn), built deterministically on a scratch ledger
    // where `pk` holds the index-1 reward.
    let mut scratch = Ledger::new();
    scratch.replay_remote_tx(det_mining_tx(&pk, 100 * MICRO, now0, 1));
    let (xfer, burn_opt, _) = scratch
        .transfer_with_burn_at(&pk, &y, 10 * MICRO, &n_crypto, ts.clone(), true)
        .expect("scratch transfer succeeds");
    let burn = burn_opt.expect("burn leg present");

    // The LOSER (index 2): user transfer + burn + a mining reward.
    let loser = Ledger::forge_block_at(
        2,
        &b1.hash,
        &ts,
        &pk,
        vec![xfer.clone(), burn.clone(), det_mining_tx(&pk, 7 * MICRO, now0, 2)],
    );
    // The WINNER (index 2): mining only, reward tuned (deterministically) so its
    // hash exceeds the loser's — so the loser is adopted first, then loses.
    let mut reward = 8 * MICRO;
    let mut winner = Ledger::forge_block_at(2, &b1.hash, &ts, &pk, vec![det_mining_tx(&pk, reward, now0, 100)]);
    while winner.hash <= loser.hash {
        reward += 1;
        winner = Ledger::forge_block_at(2, &b1.hash, &ts, &pk, vec![det_mining_tx(&pk, reward, now0, 100)]);
    }
    assert!(winner.hash > loser.hash, "winner must out-hash the loser");

    // Deliver LOW (loser) first, then HIGH (winner), to N ONLY.
    let src = PeerId("src".into());
    let env = |b: &Block| {
        signed_msg_bytes(
            &seeded_identity(9),
            GossipMessage::NewBlock {
                block_json: serde_json::to_string(b).unwrap(),
            },
            now0,
        )
    };
    sim.schedule(now0, n.clone(), Event::MessageReceived { from: src.clone(), bytes: env(&loser) });
    sim.schedule(now0, n.clone(), Event::MessageReceived { from: src, bytes: env(&winner) });
    assert_eq!(
        sim.run_checked(10_000),
        Ok(()),
        "the low→high flip keeps safety + conservation + emission at every step"
    );

    // (a) converged on the winner; (b) loser's USER txs re-queued; (c) no mint.
    assert_eq!(sim.tips()[&n], Some(winner.hash.clone()), "N flipped to the higher-hash winner");
    let l = sim.node_mut(&n).ledger();
    let pending = l.pending_txs();
    assert!(
        pending.iter().any(|t| t.id == xfer.id && t.tx_type == TxType::Transfer),
        "the loser's user transfer must be re-queued (AUDIT-BLK-1)"
    );
    assert!(
        pending.iter().any(|t| t.id == burn.id && t.tx_type == TxType::Burn),
        "the loser's burn (a user tx) must be re-queued"
    );
    assert!(
        pending.iter().all(|t| t.tx_type != TxType::Mining),
        "the loser's mining reward must NOT be re-queued — no double-mint (EMIT-1 §4.1)"
    );
    assert_eq!(
        l.total_minted(),
        l.stats().total_mined,
        "no double-mint: pending carries zero mint"
    );

    // A peer that never saw the fork converges via chain sync (no transient).
    let req = signed_msg_bytes(
        &seeded_identity(2),
        GossipMessage::RequestChain { from_height: 1, max_blocks: 50 },
        now0,
    );
    sim.schedule(now0, n.clone(), Event::MessageReceived { from: p.clone(), bytes: req });
    assert_eq!(sim.run_checked(10_000), Ok(()));
    assert_eq!(sim.tips()[&p], sim.tips()[&n], "peer converged on the winner via sync");
}

// ─── T0.8-HARDEN Phase 2b: multi-block partition (the ADR-001 gap — CLOSED) ───

/// **Phase 2b — `multiblock_partition_reconciles_at_heal` (GADGET-5B).**
///
/// The `partition_heal` archetype converged only because a SINGLE leader sealed
/// while the other side just re-synced. Here BOTH sides advance **multi-block**
/// on competing forks, then heal — the case the old **single-block** fork-choice
/// could not reconcile (a height-1 block arriving when the tip is height 2 fails
/// the prev-hash link). That was the documented ADR-001 finality gap, asserted
/// for sessions as a `…gadget_deferred` divergence.
///
/// **GADGET-5B closes it.** At heal each node feeds the competing fork to the
/// GADGET-5A GHOST engine ([`Node::reconcile_fork`]): the union block tree is
/// walked from the last justified checkpoint to the heaviest branch (here,
/// vote-free, the deterministic smallest-hash tie-break), and the loser reorgs
/// to it with full conservation. Seeing the same blocks, the two nodes converge
/// on the **same** head — so this test now asserts **convergence + global
/// conservation** (the inversion the gap was seeded as the target for).
///
/// **The teeth.** Convergence (`tips[a] == tips[b]`, determinism C1) is the
/// reconciliation tooth; the per-node + global conservation checks are the
/// load-bearing **conservation-at-heal** tooth — were the loser branch's
/// emission *not* reverted on reorg, `Σ balances` would exceed `minted` and
/// `check_invariants` would fire (the planted-break the spec demands bite).
#[test]
fn t0_8_multiblock_partition_reconciles_at_heal() {
    let now0 = SIM_EPOCH_MS;
    let a = PeerId("n0".into());
    let b = PeerId("n1".into());
    let a_pk = pk_of(1);
    let b_pk = pk_of(2);
    let mut sim = Sim::new(
        0x6AD9E7,
        vec![
            (a.clone(), Node::with_identity(seeded_identity(1))),
            (b.clone(), Node::with_identity(seeded_identity(2))),
        ],
    );
    // Two-sided partition: BOTH sides advance (unlike `partition_heal`, where
    // only the single leader seals and the far side merely re-syncs).
    sim.partition(std::slice::from_ref(&a), std::slice::from_ref(&b));
    sim.tick_all(now0);
    sim.run(10_000);

    // Each side seals TWO blocks on its own fork (multi-block divergence).
    let propose = |sim: &mut Sim, who: &PeerId, pk: &str, reward: u64, seq: u64| {
        sim.node_mut(who)
            .ledger_mut()
            .replay_remote_tx(det_mining_tx(pk, reward, now0, seq));
        sim.propose(
            who,
            &[Validator {
                pk: pk.into(),
                stake: MIN_VALIDATOR_STAKE,
                reputation: 0,
            }],
        );
    };
    propose(&mut sim, &a, &a_pk, 50 * MICRO, 1);
    propose(&mut sim, &a, &a_pk, 49 * MICRO, 2);
    propose(&mut sim, &b, &b_pk, 50 * MICRO, 3);
    propose(&mut sim, &b, &b_pk, 49 * MICRO, 4);
    sim.run(10_000);
    assert_eq!(sim.heights()[&a], 3, "A advanced two blocks on its fork");
    assert_eq!(sim.heights()[&b], 3, "B advanced two blocks on its fork");

    // Heal and attempt reconciliation BOTH ways.
    sim.heal();
    for (from, to, seed) in [(&a, &b, 1u64), (&b, &a, 2u64)] {
        let req = signed_msg_bytes(
            &seeded_identity(seed),
            GossipMessage::RequestChain { from_height: 1, max_blocks: 50 },
            now0,
        );
        sim.schedule(now0, to.clone(), Event::MessageReceived { from: from.clone(), bytes: req });
    }
    sim.run(10_000);

    // GADGET-5B §2: CONSERVATION + EMISSION hold per node at heal — checked
    // directly (and load-bearing: the loser branch's emission is reverted on
    // reorg, so `Σ balances + burned == minted` STILL holds; an un-reverted
    // emission would break exactly this).
    for (id, node) in &sim.nodes {
        let l = node.ledger();
        let sum: u64 = l.all_balances().values().sum();
        assert_eq!(
            sum.saturating_add(l.total_burned()),
            l.total_minted(),
            "conservation must hold on {id:?} after reconciliation (loser emission reverted)"
        );
        for (i, blk) in l.chain.iter().enumerate() {
            let mining = blk
                .transactions
                .iter()
                .filter(|t| t.tx_type == TxType::Mining)
                .count();
            assert!(
                mining <= 1,
                "no double-mint on {id:?} block {i} after reconciliation (EMIT-1 §4.1)"
            );
        }
    }

    // GADGET-5B §1/§3: the two multi-block forks **reconcile** — both nodes
    // converge on the SAME head (the gap, inverted). Determinism (C1): seeing the
    // same blocks, each node's GHOST engine picks the identical winner.
    assert_eq!(
        sim.tips()[&a],
        sim.tips()[&b],
        "GADGET-5B: the two multi-block forks reconcile to one common head at heal"
    );
    assert_eq!(sim.heights()[&a], 3, "the converged chain keeps its two-block height");
    assert_eq!(sim.heights()[&b], 3, "both nodes hold the same three-block chain");

    // Global safety + conservation across all nodes (no Safety/Conservation
    // violation anywhere — the reconciliation is real, not vacuous).
    assert_eq!(
        sim.check_invariants(),
        Ok(()),
        "after reconciliation: safety holds (one chain) and conservation is global"
    );

    // Conservation-at-heal, made surgical: exactly the winning miner is credited
    // the single fork's emission (50+49 = 99 QTA); the LOSER miner's branch — its
    // identical 99 QTA emission — was undone, not double-minted (a re-queue or a
    // missing revert would leave its reward in a balance and break the equality
    // above). Both forks mint 99, so whichever won, the total is one fork's worth.
    for (id, node) in &sim.nodes {
        let l = node.ledger();
        let won_a = l.balance_of(&a_pk);
        let won_b = l.balance_of(&b_pk);
        assert_eq!(won_a + won_b, 99 * MICRO, "only one fork's emission survives on {id:?}");
        assert!(
            won_a == 0 || won_b == 0,
            "the losing miner's dropped-branch emission is fully reverted on {id:?}"
        );
        assert_eq!(l.total_minted(), 99 * MICRO, "minted counts one fork only on {id:?}");
    }
}
