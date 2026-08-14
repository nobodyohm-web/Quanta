//! # LIVE-4 — Deep-fork reconciliation on the live network (partition heal)
//!
//! `Ledger::integrate_remote_block` handles the linear happy path and the
//! **1-deep** same-height fork competition. Anything deeper — two partitions
//! that each sealed ≥2 blocks and then heal — used to end in an endless
//! `"block index out of range"` loop: the GADGET-5B machinery
//! ([`Ledger::reorg_to_fork`], validated multi-block reorg on a trial clone,
//! finality-floor-vetoed) existed and was DST-proven, but **nothing on the
//! network path ever invoked it**. Two healed partitions never converged —
//! a direct violation of the fork-convergence promise.
//!
//! This module is the missing live caller. [`ForkReconciler`] is a **bounded,
//! deterministic orphan-branch buffer** fed with every block that fails linear
//! integration (from `handle_new_block` / `handle_chain_segment`). It:
//!
//! 1. **Buffers** candidate blocks (dedup by hash, hard cap
//!    [`FORK_BUFFER_MAX_BLOCKS`], deterministic eviction of the *highest*
//!    index first — blocks near the fork root are the scarce resource);
//! 2. **Assembles** the longest contiguous run rooted at a block **we hold**
//!    (parent lookup by `prev_hash` at `index - 1`);
//! 3. **Decides** with the deterministic *live win rule*: adopt iff the run's
//!    tip is **higher** than ours, or **equal-height with the
//!    lexicographically greater hash** — the exact N-block generalization of
//!    the 1-deep tie-break `integrate_remote_block` already applies, so both
//!    sides of a heal converge to the SAME chain (exactly one side adopts);
//! 4. **Applies** through [`Ledger::reorg_to_fork`] — full validation on a
//!    trial clone (coverage, emission cap, signatures, slashes), loser user
//!    txs re-queued, synthetic/slash txs dropped, and the **finality floor is
//!    absolute** (a fork rooted below `finalized_floor_index` can never win —
//!    LIVE-2 irreversibility is enforced on this path too);
//! 5. **Probes** for the common ancestor when the buffered branch does not
//!    yet root in our chain: asks for the window *below* the lowest buffered
//!    index (descends one [`PROBE_WINDOW`] per round, clamped above the
//!    floor — terminating), via the existing paginated `RequestChain` flow.
//!    No new wire messages.
//!
//! As a free by-product the buffer also heals **out-of-order** `ChainSegment`
//! windows (NET-6 fan-out answers can arrive high-window-first): linear
//! orphans wait in the buffer and [`ForkReconciler::resolve`] drains them the
//! moment their predecessor lands.
//!
//! ## Honest scoping (fork-choice)
//! The live win rule is **longest-above-floor with the lexicographic
//! tie-break** — the network-path extension of the existing deterministic
//! rule, *not* stake-weighted LMD-GHOST. Live votes are per-epoch checkpoint
//! votes (they do not attribute weight to every interior block), so GHOST
//! weighting stays where it is exact: the `sm/` core's checkpoint-level
//! anchor selection (`ghost_head`, GADGET-5A) and the finality floor that
//! anchors this rule. Everything at or below the floor is untouchable here.
//!
//! ## Determinism & bounds
//! - Buffer = `BTreeMap<u64, Vec<Block>>` with per-index vectors kept sorted
//!   by hash → iteration order and candidate choice are deterministic.
//! - At most one reorg attempt per assembled run; every attempt **purges**
//!   the attempted run from the buffer (success or failure), so `resolve`
//!   strictly shrinks the buffer and always terminates.
//! - An invalid branch fails `reorg_to_fork`'s trial clone and is purged —
//!   a garbage-spamming peer costs one bounded trial, never a retry loop.
//!
//! ## Locking
//! The reconciler is IO-layer state (never consensus state — it is not
//! persisted, not hashed, and its loss is harmless). Lock order:
//! **ledger → fork_heal** (both write-acquired together in the dispatcher),
//! released before any gossip broadcast (probes) — consistent with the
//! project-wide `crypto → reputation → ledger → gossip` ordering.

use std::collections::BTreeMap;

use super::ledger::{Block, Ledger};

/// Hard cap on buffered candidate blocks (DoS bound). 1024 blocks ≈ 34 h of
/// sealing at the 2-min cadence — far deeper than any plausible partition —
/// while costing at most a few MB of memory.
pub const FORK_BUFFER_MAX_BLOCKS: usize = 1024;

/// H5 (AUDIT-2026-07-25) — how many competing candidates one height may hold.
/// A real fork offers ONE block per height that can matter to us; a margin of 8
/// covers honest churn while stopping a single index from owning the buffer.
pub const FORK_BUFFER_MAX_PER_INDEX: usize = 8;

/// How far below the lowest unrooted orphan an ancestor probe asks, per
/// round. Matches the `ChainSegment` size cap (50): each probe round is one
/// full segment window, so the descent reaches any fork point in
/// `depth / 50` round-trips, clamped above the finality floor.
pub const PROBE_WINDOW: u64 = 50;

/// What [`ForkReconciler::resolve`] did — the dispatcher acts on this after
/// releasing the ledger + reconciler locks.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ResolveOutcome {
    /// Blocks adopted through a successful `reorg_to_fork` (partition heal).
    pub adopted: usize,
    /// Buffered orphans that turned out to extend the tip linearly and were
    /// integrated (out-of-order window heal).
    pub extended: usize,
    /// `Some((from, to))` when unrooted orphans remain and the common
    /// ancestor must be below `to`: issue `RequestChain` for `[from, to)`.
    pub probe: Option<(u64, u64)>,
}

/// Bounded deterministic orphan-branch buffer + the live fork-choice caller.
/// See the module docs for the full contract.
#[derive(Debug, Default)]
pub struct ForkReconciler {
    /// Candidate blocks by index. Per-index vectors are sorted by hash and
    /// deduped — deterministic iteration, deterministic candidate choice.
    buf: BTreeMap<u64, Vec<Block>>,
    /// Total buffered blocks across all indices (≤ [`FORK_BUFFER_MAX_BLOCKS`]).
    total: usize,
}

impl ForkReconciler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of blocks currently buffered.
    pub fn len(&self) -> usize {
        self.total
    }

    pub fn is_empty(&self) -> bool {
        self.total == 0
    }

    /// Offer one block that failed linear integration. Filters what can never
    /// matter (at/below the finality floor, already on our chain, duplicate in
    /// the buffer) and enforces the hard cap with deterministic eviction
    /// (highest index first — root-adjacent blocks are the scarce resource).
    /// Returns `true` if the block was buffered.
    pub fn offer(&mut self, block: Block, ledger: &Ledger) -> bool {
        let floor = ledger.finalized_floor_index();
        // Genesis (index 0) can never be replaced; ≤ floor is irreversible.
        if block.index == 0 || block.index <= floor {
            return false;
        }
        // Already ours at that height → not a candidate.
        if ledger
            .block_at(block.index)
            .is_some_and(|b| b.hash == block.hash)
        {
            return false;
        }
        let entry = self.buf.entry(block.index).or_default();
        // Dedup by hash (sorted vector → binary search).
        let Err(pos) = entry.binary_search_by(|b| b.hash.cmp(&block.hash)) else {
            return false; // duplicate
        };
        // H5 (AUDIT-2026-07-25) — per-index occupancy cap. A competing branch has
        // ONE block per height that matters to us; anything beyond a small margin
        // at a single index is an attacker minting hashes, and without this cap a
        // single index could own the entire buffer.
        if entry.len() >= FORK_BUFFER_MAX_PER_INDEX {
            return false;
        }
        // H5 — evict by USEFULNESS, not by index.
        //
        // The old rule evicted the highest buffered index and refused any newcomer
        // at or above it, on the reasoning that root-adjacent blocks are scarce.
        // That holds for honest peers and inverts under attack: blocks far below
        // our tip are the *cheapest* to forge, they never assemble a winning run,
        // nothing ever removes them — so a buffer stuffed with 1024 low-index junk
        // blocks was STABLE, and from then on every genuine competing block was
        // refused. That switches off LIVE-4, the only live caller of
        // `reorg_to_fork`, i.e. deep partition healing stops working entirely.
        //
        // A real competing branch diverges *near* our tip, so distance to the tip
        // is the honest signal. Ties break on the highest hash, keeping eviction
        // deterministic across nodes.
        if self.total >= FORK_BUFFER_MAX_BLOCKS {
            let tip = ledger.chain_height().saturating_sub(1);
            let newcomer_distance = tip.abs_diff(block.index);
            let victim = self
                .buf
                .iter()
                .flat_map(|(idx, v)| v.iter().map(move |b| (*idx, b.hash.clone())))
                .max_by_key(|(idx, hash)| (tip.abs_diff(*idx), hash.clone()));
            let Some((victim_idx, victim_hash)) = victim else {
                return false;
            };
            // The newcomer is itself the least useful block on offer → refuse it
            // rather than churn the buffer.
            if tip.abs_diff(victim_idx) < newcomer_distance {
                return false;
            }
            if let Some(v) = self.buf.get_mut(&victim_idx) {
                if let Some(p) = v.iter().position(|b| b.hash == victim_hash) {
                    v.remove(p);
                    self.total -= 1;
                }
                if v.is_empty() {
                    self.buf.remove(&victim_idx);
                }
            }
        }
        // NB: `entry` may have been invalidated by the eviction borrow — re-fetch.
        let entry = self.buf.entry(block.index).or_default();
        let pos = entry
            .binary_search_by(|b| b.hash.cmp(&block.hash))
            .err()
            .unwrap_or(pos);
        entry.insert(pos, block);
        self.total += 1;
        true
    }

    /// Try to make progress against `ledger`: purge dead candidates, adopt any
    /// assembled winning branch via [`Ledger::reorg_to_fork`], drain linear
    /// extensions, and compute the next ancestor-probe window if unrooted
    /// orphans remain. Strictly shrinks the buffer per reorg attempt →
    /// always terminates.
    pub fn resolve(&mut self, ledger: &mut Ledger) -> ResolveOutcome {
        let mut out = ResolveOutcome::default();
        self.purge_dead(ledger);

        // Adopt every assembled winning run (usually at most one).
        while let Some(run) = self.assemble_winning_run(ledger) {
            let floor = ledger.finalized_floor_index();
            match ledger.reorg_to_fork(&run, floor) {
                Ok(true) => {
                    log::warn!(
                        "◈ [LIVE-4] PARTITION HEALED — adopted a {}-block competing branch \
                         (new tip #{}, floor {})",
                        run.len(),
                        ledger.chain_height().saturating_sub(1),
                        floor
                    );
                    out.adopted += run.len();
                }
                Ok(false) => {
                    log::info!(
                        "◈ [LIVE-4] competing branch of {} block(s) legitimately kept out \
                         (floor / not a clean fork)",
                        run.len()
                    );
                }
                Err(e) => {
                    log::warn!(
                        "◈ [LIVE-4] competing branch of {} block(s) INVALID — discarded: {}",
                        run.len(),
                        e
                    );
                }
            }
            // Success or failure: the attempted run leaves the buffer, so the
            // loop strictly progresses (an invalid branch is never retried).
            for b in &run {
                self.remove(b);
            }
            self.purge_dead(ledger);
        }

        // Drain linear extensions (out-of-order windows now in order).
        out.extended = self.drain_linear(ledger);

        // Ancestor probe: only when orphans remain whose parent we don't hold.
        out.probe = self.probe_window(ledger);
        out
    }

    /// Drop everything that can no longer matter: blocks at/below the floor,
    /// or already on our chain (same hash at same height).
    fn purge_dead(&mut self, ledger: &Ledger) {
        let floor = ledger.finalized_floor_index();
        let mut removed = 0usize;
        self.buf.retain(|idx, v| {
            v.retain(|b| {
                let dead = *idx <= floor
                    || ledger.block_at(*idx).is_some_and(|ours| ours.hash == b.hash);
                if dead {
                    removed += 1;
                }
                !dead
            });
            !v.is_empty()
        });
        self.total -= removed;
    }

    /// Remove one specific block (by index + hash) from the buffer.
    fn remove(&mut self, block: &Block) {
        if let Some(v) = self.buf.get_mut(&block.index) {
            if let Ok(pos) = v.binary_search_by(|b| b.hash.cmp(&block.hash)) {
                v.remove(pos);
                self.total -= 1;
            }
            if v.is_empty() {
                self.buf.remove(&block.index);
            }
        }
    }

    /// Assemble the first (deterministically ordered) contiguous buffered run
    /// that roots at a block we hold AND beats our tip under the live win
    /// rule: strictly higher, or equal-height with the lexicographically
    /// greater tip hash. Returns the run tip-exclusive of our chain (ascending
    /// order, ready for [`Ledger::reorg_to_fork`]).
    fn assemble_winning_run(&self, ledger: &Ledger) -> Option<Vec<Block>> {
        let our_tip = ledger.chain.last()?;
        let floor = ledger.finalized_floor_index();
        for (idx, blocks) in &self.buf {
            let parent_idx = idx.checked_sub(1)?;
            // The fork point (= parent) must be at/above the floor for the
            // reorg to be admissible (reorg_to_fork re-checks — belt and
            // suspenders, LIVE-2).
            if parent_idx < floor {
                continue;
            }
            // A run rooted AT our tip is a plain linear extension, not a fork —
            // that's `drain_linear`'s job (no reorg, no "partition healed" log).
            if parent_idx >= our_tip.index {
                continue;
            }
            for root in blocks {
                let Some(parent) = ledger.block_at(parent_idx) else {
                    continue;
                };
                if parent.hash != root.prev_hash {
                    continue;
                }
                // Marche avant dans le tampon. **C-04 (AUDIT-2026-08-13) —
                // FORK-RANK-1** : quand plusieurs enfants concourent, on prenait
                // le plus grand hash, donc l'assemblage préférait la branche la
                // plus broyée. On prend désormais celui que le fork-choice
                // préfère, c'est-à-dire d'abord le proposeur le mieux élu.
                //
                // Les deux `expect("run is non-empty")` d'origine ont sauté au
                // passage : `run` est non vide par construction, mais un
                // invariant vrai n'est pas une raison de laisser une panique
                // atteignable dans un chemin nourri par le réseau.
                let mut run = vec![root.clone()];
                while let Some(cur) = run.last().cloned() {
                    let next_idx = cur.index + 1;
                    let Some(cands) = self.buf.get(&next_idx) else {
                        break;
                    };
                    let mut best: Option<&Block> = None;
                    for cand in cands.iter().filter(|c| c.prev_hash == cur.hash) {
                        best = match best {
                            None => Some(cand),
                            Some(b) if ledger.prefers_same_height(cand, b) => Some(cand),
                            keep => keep,
                        };
                    }
                    match best {
                        Some(n) => run.push(n.clone()),
                        None => break,
                    }
                }
                let Some(run_tip) = run.last() else { continue };
                let wins = run_tip.index > our_tip.index
                    || (run_tip.index == our_tip.index
                        && ledger.prefers_same_height(run_tip, our_tip));
                if wins {
                    return Some(run);
                }
            }
        }
        None
    }

    /// Integrate buffered blocks that extend the tip **linearly** (the
    /// out-of-order-window heal). Returns how many were integrated. A block
    /// that fails integration is purged (never retried).
    fn drain_linear(&mut self, ledger: &mut Ledger) -> usize {
        let mut n = 0usize;
        while let Some((tip_index, tip_hash)) =
            ledger.chain.last().map(|t| (t.index, t.hash.clone()))
        {
            let Some(cands) = self.buf.get(&(tip_index + 1)) else {
                break;
            };
            let Some(next) = cands
                .iter()
                .filter(|c| c.prev_hash == tip_hash)
                .max_by(|a, b| a.hash.cmp(&b.hash))
                .cloned()
            else {
                break;
            };
            self.remove(&next);
            match ledger.integrate_remote_block(next) {
                Ok(true) => n += 1,
                // Duplicate / lost competition → just purged; invalid → purged
                // with a log (the ledger already logged the reason).
                Ok(false) => {}
                Err(e) => {
                    log::warn!("◈ [LIVE-4] buffered linear extension invalid — dropped: {e}");
                }
            }
        }
        n
    }

    /// The next ancestor-probe window, if unrooted orphans remain: descend one
    /// [`PROBE_WINDOW`] below the lowest buffered index, clamped above the
    /// finality floor. `None` when the buffer is empty, the lowest orphan
    /// already roots (or forks) against our chain, or there is no room left
    /// above the floor to descend into.
    fn probe_window(&self, ledger: &Ledger) -> Option<(u64, u64)> {
        let (&lowest, blocks) = self.buf.iter().next()?;
        let floor = ledger.finalized_floor_index();
        // If some lowest-index orphan's parent is a block we HOLD (same hash →
        // it roots; different hash → the fork point is found and the run simply
        // does not win), descending further gains nothing.
        let parent_idx = lowest.checked_sub(1)?;
        if blocks.iter().any(|b| {
            ledger
                .block_at(parent_idx)
                .is_some_and(|p| p.hash == b.prev_hash)
        }) {
            return None;
        }
        // Also nothing to fetch when the parent height is above our tip — the
        // regular linear catch-up (`request_chain_range(our_height, …)`)
        // already covers that gap.
        if parent_idx >= ledger.chain_height() {
            return None;
        }
        let from = lowest.saturating_sub(PROBE_WINDOW).max(floor + 1);
        (from < lowest).then_some((from, lowest))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extend `ledger` linearly with `n` forged empty blocks whose timestamps
    /// embed `tag` (so two branches sealed from the same parent differ), and
    /// return the appended blocks.
    ///
    /// **C-02 (BLOCK-TIME-1)** — le timestamp d'un bloc est désormais un champ
    /// *validé* : RFC3339 et non décroissant le long de la chaîne. Le marqueur de
    /// branche voyage donc dans une **avance en secondes** dérivée du tag, au
    /// lieu d'être collé après le fuseau — ce qui produisait `…+00:00-a`, une
    /// chaîne que rien ne parsait et que rien ne refusait.
    fn extend(ledger: &mut Ledger, n: usize, tag: &str) -> Vec<Block> {
        // Avance dérivée du tag, bornée sous l'heure : deux branches issues du même
        // parent restent distinctes, et chaque bloc reste postérieur au sien.
        let lane = i64::from(blake3::hash(tag.as_bytes()).as_bytes()[0]);
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            let tip = ledger.chain.last().expect("tip").clone();
            let ts = (chrono::DateTime::parse_from_rfc3339(&tip.timestamp)
                .expect("le timestamp du parent est RFC3339")
                + chrono::Duration::seconds(3600 + lane))
            .to_rfc3339();
            let b = Ledger::forge_block_at(
                tip.index + 1,
                &tip.hash,
                &ts,
                "miner",
                vec![],
            );
            assert_eq!(
                ledger.integrate_remote_block(b.clone()),
                Ok(true),
                "forged linear block must integrate"
            );
            out.push(b);
        }
        out
    }

    /// A pair of ledgers partitioned after a shared prefix of `shared` blocks:
    /// A seals `a_len` further blocks, B seals `b_len` (different tags → forks).
    fn partitioned(shared: usize, a_len: usize, b_len: usize) -> (Ledger, Ledger, Vec<Block>, Vec<Block>) {
        let mut a = Ledger::new();
        extend(&mut a, shared, "shared");
        let mut b = a.clone();
        let a_branch = extend(&mut a, a_len, "branch-a");
        let b_branch = extend(&mut b, b_len, "branch-b");
        (a, b, a_branch, b_branch)
    }

    fn conserves(l: &Ledger) -> bool {
        let spendable: u64 = l.all_balances().values().sum();
        spendable + l.locked_stake_total() + l.total_burned() == l.total_minted()
    }

    #[test]
    fn live4_two_deep_partition_heals_and_converges_symmetrically() {
        // The core promise: after a partition where A sealed 2 and B sealed 3,
        // feeding each side the other's branch converges BOTH to B's chain —
        // exactly one side reorgs (the shorter one), the other keeps.
        let (mut a, mut b, a_branch, b_branch) = partitioned(2, 2, 3);
        assert_ne!(
            a.chain.last().unwrap().hash,
            b.chain.last().unwrap().hash,
            "genuinely diverged"
        );

        // A receives B's branch (out of order, even — buffer must not care).
        let mut ra = ForkReconciler::new();
        for blk in b_branch.iter().rev() {
            ra.offer(blk.clone(), &a);
        }
        let out_a = ra.resolve(&mut a);
        assert_eq!(out_a.adopted, 3, "A adopts B's longer 3-block branch");
        assert_eq!(
            a.chain.last().unwrap().hash,
            b.chain.last().unwrap().hash,
            "A converged to B's tip"
        );
        assert!(conserves(&a), "A conserves after the heal");

        // B receives A's branch → shorter → keeps its own chain.
        let mut rb = ForkReconciler::new();
        for blk in &a_branch {
            rb.offer(blk.clone(), &b);
        }
        let out_b = rb.resolve(&mut b);
        assert_eq!(out_b.adopted, 0, "B keeps its longer chain");
        assert_eq!(
            a.chain.last().unwrap().hash,
            b.chain.last().unwrap().hash,
            "both sides now share one tip"
        );
    }

    #[test]
    fn live4_equal_length_fork_resolves_by_tip_hash_exactly_one_side_adopts() {
        // Equal-depth partition (2 vs 2): the deterministic tie-break (greater
        // tip hash) must make EXACTLY ONE side adopt — same rule as the 1-deep
        // in-ledger competition, generalized.
        let (mut a, mut b, a_branch, b_branch) = partitioned(1, 2, 2);
        let a_tip = a.chain.last().unwrap().hash.clone();
        let b_tip = b.chain.last().unwrap().hash.clone();
        assert_ne!(a_tip, b_tip);

        let mut ra = ForkReconciler::new();
        for blk in &b_branch {
            ra.offer(blk.clone(), &a);
        }
        let adopted_by_a = ra.resolve(&mut a).adopted;

        let mut rb = ForkReconciler::new();
        for blk in &a_branch {
            rb.offer(blk.clone(), &b);
        }
        let adopted_by_b = rb.resolve(&mut b).adopted;

        assert_eq!(
            (adopted_by_a > 0) as u8 + (adopted_by_b > 0) as u8,
            1,
            "exactly one side adopts on an equal-length fork"
        );
        assert_eq!(
            a.chain.last().unwrap().hash,
            b.chain.last().unwrap().hash,
            "and both converge to the lexicographically greater tip"
        );
        let winner = if b_tip > a_tip { b_tip } else { a_tip };
        assert_eq!(a.chain.last().unwrap().hash, winner);
    }

    #[test]
    fn live4_fork_below_finality_floor_is_never_adopted() {
        // LIVE-2 on the heal path: a longer branch rooted BELOW the finalized
        // floor must never displace finalized history, no matter its length.
        let (mut a, _b, _ab, b_branch) = partitioned(2, 2, 4);
        // Finalize A's height 3 (inside its own branch, above the fork point 2).
        let h3 = a.block_at(3).unwrap().hash.clone();
        a.set_finalized_floor(3, &h3);
        assert_eq!(a.finalized_floor_index(), 3);

        let mut ra = ForkReconciler::new();
        let offered: usize = b_branch
            .iter()
            .map(|blk| ra.offer(blk.clone(), &a) as usize)
            .sum();
        let out = ra.resolve(&mut a);
        assert_eq!(out.adopted, 0, "finalized history is irreversible on the heal path");
        assert_eq!(
            a.finalized_floor_index(),
            3,
            "floor untouched"
        );
        // The would-be branch roots at height 2 < floor: its blocks at 3,4 are
        // ≤/at floor-adjacent and unrootable above it — buffer must not grow
        // unboundedly either (offers at index ≤ floor are refused outright).
        assert!(offered <= b_branch.len());
        assert!(conserves(&a));
    }

    #[test]
    fn live4_out_of_order_windows_drain_linearly() {
        // NET-6 fan-out answers can land high-window-first. Blocks [4,5] arrive
        // before [3]: they wait in the buffer; once 3 lands linearly, resolve
        // drains 4 and 5 without any re-request.
        let mut a = Ledger::new();
        extend(&mut a, 2, "base"); // heights 1,2
        let mut donor = a.clone();
        let rest = extend(&mut donor, 3, "rest"); // heights 3,4,5

        let mut r = ForkReconciler::new();
        // High windows first (4,5) — unrooted for now.
        r.offer(rest[1].clone(), &a);
        r.offer(rest[2].clone(), &a);
        let out = r.resolve(&mut a);
        assert_eq!(out.adopted + out.extended, 0, "nothing integrable yet");
        // The missing window (3) arrives; the dispatcher integrates it linearly
        // — here we just offer it too (resolve's drain path covers both).
        r.offer(rest[0].clone(), &a);
        let out2 = r.resolve(&mut a);
        assert_eq!(out2.extended, 3, "the buffered suffix drained in order");
        assert_eq!(a.chain_height(), 6, "genesis + 5");
        assert!(r.is_empty(), "buffer fully consumed");
    }

    #[test]
    fn live4_invalid_branch_is_purged_and_never_retried() {
        // A structurally-corrupt competing branch must cost exactly one failed
        // trial: purged from the buffer, chain untouched, no retry loop.
        let (mut a, _b, _ab, mut b_branch) = partitioned(2, 1, 3);
        b_branch[1].transactions.push(crate::p2p::ledger_types::Transaction {
            id: "forged".into(),
            from: "attacker".into(),
            to: "victim".into(),
            amount: 1,
            tx_type: crate::p2p::ledger_types::TxType::Transfer,
            timestamp: "ts".into(),
            signature: String::new(),
            hash: "forged-hash".into(),
            nonce: 0,
            pq_signature: None,
            pq_public_key: None,
            fault_proof: None,
            slash_unbonding: None,
        }); // merkle/hash no longer match → trial reorg must reject
        let tip_before = a.chain.last().unwrap().hash.clone();

        let mut r = ForkReconciler::new();
        for blk in &b_branch {
            r.offer(blk.clone(), &a);
        }
        let out = r.resolve(&mut a);
        assert_eq!(out.adopted, 0, "corrupt branch rejected");
        assert_eq!(a.chain.last().unwrap().hash, tip_before, "chain untouched");
        // The attempted run was purged; the leftover unrooted tail (if any)
        // cannot loop: a second resolve makes no progress and no adoption.
        let out2 = r.resolve(&mut a);
        assert_eq!(out2.adopted, 0);
        assert!(conserves(&a));
    }

    #[test]
    fn h5_low_index_junk_cannot_pin_the_buffer_against_a_real_branch() {
        // H5 (AUDIT-2026-07-25): the old bound evicted the HIGHEST buffered index
        // and refused any newcomer at or above it. Blocks far below our tip are the
        // cheapest to forge, never assemble a winning run, and nothing ever removed
        // them — so a buffer stuffed with low-index junk was STABLE and every
        // genuine competing block was refused from then on. That switches off
        // LIVE-4, the only live caller of `reorg_to_fork`: deep partition healing
        // simply stops. Eviction now goes by distance to the tip.
        let mut a = Ledger::new();
        extend(&mut a, 200, "base");
        let mut r = ForkReconciler::new();

        // Saturate with junk far below the tip, spread wide enough to defeat the
        // per-index cap (150 indices x 8 > FORK_BUFFER_MAX_BLOCKS).
        for i in 0..(FORK_BUFFER_MAX_BLOCKS * 2) {
            let idx = 1 + (i as u64 % 150);
            let junk =
                Ledger::forge_block_at(idx, &format!("junk-parent-{i}"), "ts", "m", vec![]);
            r.offer(junk, &a);
        }
        assert_eq!(r.len(), FORK_BUFFER_MAX_BLOCKS, "the buffer is saturated with junk");

        // A genuine competing block at our tip height must still find a slot.
        let tip = a.chain.last().expect("tip").clone();
        let parent = a.block_at(tip.index - 1).expect("parent").clone();
        let rival = Ledger::forge_block_at(
            tip.index,
            &parent.hash,
            "2026-07-25T00:00:00+00:00-rival",
            "miner",
            vec![],
        );
        assert!(
            r.offer(rival, &a),
            "a real competing block near our tip must always displace distant junk"
        );
        assert!(r.len() <= FORK_BUFFER_MAX_BLOCKS, "hard cap still holds");
    }

    #[test]
    fn h5_one_index_cannot_own_the_whole_buffer() {
        // H5: without a per-index cap, an attacker mints unlimited distinct hashes
        // at a single convenient height and owns every slot.
        let mut a = Ledger::new();
        extend(&mut a, 5, "base");
        let mut r = ForkReconciler::new();
        for i in 0..200 {
            let junk = Ledger::forge_block_at(3, &format!("same-height-{i}"), "ts", "m", vec![]);
            r.offer(junk, &a);
        }
        assert!(
            r.len() <= FORK_BUFFER_MAX_PER_INDEX,
            "one height may not exceed the per-index cap, got {}",
            r.len()
        );
    }

    #[test]
    fn live4_buffer_is_bounded_with_deterministic_eviction() {
        // DoS: an attacker floods far-ahead junk. The buffer never exceeds the
        // cap, and root-adjacent (low-index) candidates are never displaced by
        // higher-index junk.
        let mut a = Ledger::new();
        extend(&mut a, 1, "base");
        let mut r = ForkReconciler::new();
        // One precious low candidate (index 2).
        let low = Ledger::forge_block_at(2, "unknown-parent", "ts", "m", vec![]);
        assert!(r.offer(low.clone(), &a));
        // Flood with junk at ascending indices.
        for i in 0..(FORK_BUFFER_MAX_BLOCKS * 2) {
            let junk = Ledger::forge_block_at(
                1000 + i as u64,
                &format!("junk-parent-{i}"),
                "ts",
                "m",
                vec![],
            );
            r.offer(junk, &a);
        }
        assert!(r.len() <= FORK_BUFFER_MAX_BLOCKS, "hard cap holds");
        // The low candidate survived the flood.
        let probe = r.resolve(&mut a).probe;
        assert!(
            probe.is_some(),
            "low unrooted candidate still present → probe suggested"
        );
    }

    #[test]
    fn live4_probe_descends_by_window_and_clamps_above_floor() {
        let mut a = Ledger::new();
        extend(&mut a, 30, "base"); // tip = 30
        let mut r = ForkReconciler::new();
        // An orphan at height 25 whose parent we don't hold (a fork block).
        let orphan = Ledger::forge_block_at(25, "not-our-24", "ts", "m", vec![]);
        assert!(r.offer(orphan, &a));
        let out = r.resolve(&mut a);
        // Probe = [25-50 clamped to floor+1, 25) = [1, 25).
        assert_eq!(out.probe, Some((1, 25)), "descend one window, clamp at floor+1");

        // With a floor at 20, the clamp rises.
        let h20 = a.block_at(20).unwrap().hash.clone();
        a.set_finalized_floor(20, &h20);
        let out2 = r.resolve(&mut a);
        assert_eq!(out2.probe, Some((21, 25)), "probe never descends below the floor");
    }

    #[test]
    fn live4_conservation_holds_through_a_heal_with_reward_blocks() {
        // A partition heal where BOTH branches minted rewards: the loser's
        // emission must be fully reverted, the winner's fully counted —
        // conservation exact after the swap (EMIT-1 at partition scale).
        let mut a = Ledger::new();
        a.mint_block_reward_of("miner-a", 3_000_000);
        a.seal_block("miner-a", 0.0); // shared height 1 (reward 5 QTA)
        let mut b = a.clone();

        // A seals one more reward block; B seals two (B wins on length).
        a.mint_block_reward_of("miner-a", 3_000_000);
        a.seal_block("miner-a", 0.0);
        b.mint_block_reward_of("miner-b", 2_000_000);
        b.seal_block("miner-b", 0.0);
        b.mint_block_reward_of("miner-b", 1_000_000);
        let b_tip = b.seal_block("miner-b", 0.0);

        let b_branch: Vec<Block> = vec![b.block_at(2).unwrap().clone(), b_tip];
        let mut r = ForkReconciler::new();
        for blk in b_branch {
            r.offer(blk, &a);
        }
        let out = r.resolve(&mut a);
        assert_eq!(out.adopted, 2, "A adopts B's longer reward branch");
        assert_eq!(
            a.total_minted(),
            b.total_minted(),
            "loser emission reverted, winner counted"
        );
        // REWARD-SHARE-1 : sur la branche gagnante, le second bloc de B partage sa
        // récompense avec miner-a (participant récent du bloc 1 partagé), donc B
        // encaisse 2 000 000 (bloc 2, seul participant = lui-même) + la moitié de
        // 1 000 000 (bloc 3) = 2 500 000... vérifié par recalcul plutôt que gravé :
        let expected_b: u64 = b
            .chain
            .iter()
            .flat_map(|blk| blk.transactions.iter())
            .filter(|t| t.tx_type == crate::p2p::ledger::TxType::Mining && t.to == "miner-b")
            .map(|t| t.amount)
            .sum();
        assert_eq!(a.balance_of("miner-b"), expected_b, "winner rewards live (part partagée incluse)");
        assert!(conserves(&a), "conservation exact after the heal");
        assert!(conserves(&b));
    }
}
