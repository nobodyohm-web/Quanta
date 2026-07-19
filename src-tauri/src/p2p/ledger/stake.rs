// p2p/ledger/stake.rs — on-chain stake state (ONCHAIN-STAKE-1)
//
// Split out of ledger.rs (organizational only — pure move, no logic change).
// Houses: the STAKE_SINK sink const, per-block bonded-stake apply/revert,
// unbonding maturation & queries, validator_stakes[_by_pubkey], and the
// Stake/Unstake tx builders.

use super::*;

impl Ledger {

    // ── ONCHAIN-STAKE-1: on-chain stake state (block-index-anchored) ─────────

    /// ONCHAIN-STAKE-1: the synthetic **locked-stake sink** address. A `Stake`
    /// moves coins `pk → STAKE` (a balance-NEUTRAL transfer, like any other), so
    /// the staked coins are debited from the staker's spendable balance the moment
    /// the tx is admitted to the mempool — **locking** them (HARDEN-STAKE-1: this
    /// is what prevents a pending Stake's coins from being double-spent by a
    /// concurrent transfer, and what keeps conservation exact: nothing is created
    /// or destroyed, value just sits in the sink). The per-account *bonded* weight
    /// that consensus reads (`staked`) is committed separately at block time. Like
    /// `BURN`/`ESCROW`, `STAKE` is excluded from `all_balances` (it is not a
    /// spendable holder); unlike `BURN`, it is **not** synthetic in the cache, so
    /// the move is balance-neutral and the locked coins are conserved, not burned.
    pub(crate) const STAKE_SINK: &str = "STAKE";

    /// Commit a freshly-applied block's per-account **bonded stake** (the consensus
    /// weight), then mature any unbonding entry the block's height has reached.
    /// Called right after a block is pushed to the chain (seal, happy-path
    /// integrate, fork-reorg winner) and during `rebuild_cache`. Every quantity is
    /// derived from the chain and **anchored to `block.index`**, so two nodes that
    /// hold the same chain compute byte-identical `staked`/`unbonding` maps — the
    /// property §7 verifies.
    ///
    /// The **spendable debit** for a Stake (and its conserving credit to the
    /// `STAKE` sink) already happened at mempool time via `cache_apply_tx` — here
    /// we only commit the bonded weight, so a pending Stake locks funds immediately
    /// but only weighs consensus once sealed.
    ///
    /// - **Stake**: bond `amount` into `staked` (consensus weight).
    /// - **Unstake**: move `amount` out of `staked` into a new unbonding entry that
    ///   unlocks at `block.index + UNBONDING_PERIOD_BLOCKS` (the coins stay in the
    ///   `STAKE` sink — still locked — until maturation returns them).
    /// - **maturation**: see [`Self::mature_unbonding`].
    pub(super) fn apply_block_stake_effects(&mut self, block: &Block) {
        for tx in &block.transactions {
            match tx.tx_type {
                TxType::Stake => {
                    *self.staked.entry(tx.from.clone()).or_insert(0) += tx.amount;
                }
                TxType::Unstake => {
                    let bonded = self.staked.entry(tx.from.clone()).or_insert(0);
                    *bonded = bonded.saturating_sub(tx.amount);
                    if *bonded == 0 {
                        self.staked.remove(&tx.from);
                    }
                    self.unbonding.entry(tx.from.clone()).or_default().push(UnbondEntry {
                        amount: tx.amount,
                        unlock_height: block.index.saturating_add(UNBONDING_PERIOD_BLOCKS),
                        tx_hash: tx.hash.clone(),
                    });
                }
                // LIVE-3: a Slash **destroys** the offender's stake — NO unbonding
                // entry created, NO spendable credit (the coins are burned). The
                // matching STAKE-sink debit happened in `cache_apply_tx`, so
                // sink == Σstaked+Σunbonding stays consistent.
                // LIVE-3B: the tx's verified breakdown says exactly what dies —
                // `amount - Σ(consumed unbonding)` from the bonded weight, plus the
                // listed unbonding entries (matched by their Unstake tx hash). The
                // breakdown was checked == this node's own deterministic plan by
                // `verify_block_slashes`/COVER-2, so the arithmetic is exact; the
                // saturating ops are pure defense-in-depth.
                TxType::Slash => {
                    let consumed_unbonding: u64 =
                        tx.slash_unbonding.iter().flatten().map(|e| e.amount).sum();
                    let bonded_take = tx.amount.saturating_sub(consumed_unbonding);
                    if bonded_take > 0 {
                        let bonded = self.staked.entry(tx.from.clone()).or_insert(0);
                        *bonded = bonded.saturating_sub(bonded_take);
                    }
                    if self.staked.get(&tx.from).copied().unwrap_or(0) == 0 {
                        self.staked.remove(&tx.from);
                    }
                    if let Some(consumed) = &tx.slash_unbonding {
                        if let Some(list) = self.unbonding.get_mut(&tx.from) {
                            for c in consumed {
                                if let Some(e) =
                                    list.iter_mut().find(|e| e.tx_hash == c.tx_hash)
                                {
                                    e.amount = e.amount.saturating_sub(c.amount);
                                }
                            }
                            list.retain(|e| e.amount > 0);
                        }
                        if self.unbonding.get(&tx.from).is_some_and(|l| l.is_empty()) {
                            self.unbonding.remove(&tx.from);
                        }
                    }
                }
                _ => {}
            }
        }
        self.mature_unbonding(block.index);
    }

    /// Return every unbonding entry whose `unlock_height <= height` from the
    /// `STAKE` sink to the owner's spendable balance (and drop it) — a conserving
    /// `STAKE → pk` move. Idempotent and **height-indexed** (never the wall clock):
    /// re-running at the same or a higher height releases each entry exactly once.
    /// The moves are additive, so the final cache is independent of `HashMap`
    /// iteration order (Constitution §3 determinism).
    pub(crate) fn mature_unbonding(&mut self, height: u64) {
        let mut matured: Vec<(String, u64)> = Vec::new();
        for (pk, entries) in self.unbonding.iter_mut() {
            entries.retain(|e| {
                if e.unlock_height <= height {
                    matured.push((pk.clone(), e.amount));
                    false
                } else {
                    true
                }
            });
        }
        self.unbonding.retain(|_, v| !v.is_empty());
        for (pk, amount) in matured {
            // Conserving move: the locked coins leave the sink and become spendable.
            *self.balance_cache.entry(Self::STAKE_SINK.to_string()).or_insert(0) -= amount as i128;
            *self.balance_cache.entry(pk).or_insert(0) += amount as i128;
        }
    }

    /// Reverse the **bonded-weight** effects of a block popped during a fork reorg.
    /// The spendable/sink balance of a Stake is reverted by `cache_revert_tx` (the
    /// reorg's cache-revert loop), so here we only undo the `staked`/`unbonding`
    /// bookkeeping. **Maturation is intentionally NOT un-done**: it is keyed on the
    /// chain *height*, and the replacement block sits at the **same** height, so a
    /// matured entry stays matured across the swap. This is sound because fork
    /// resolution here is single-block (≤1 deep) and `UNBONDING_PERIOD_BLOCKS ≫ 1`,
    /// so no reorg can ever span an unbonding maturation.
    pub(super) fn revert_block_stake_effects(&mut self, block: &Block) {
        for tx in &block.transactions {
            match tx.tx_type {
                TxType::Stake => {
                    let bonded = self.staked.entry(tx.from.clone()).or_insert(0);
                    *bonded = bonded.saturating_sub(tx.amount);
                    if *bonded == 0 {
                        self.staked.remove(&tx.from);
                    }
                }
                TxType::Unstake => {
                    *self.staked.entry(tx.from.clone()).or_insert(0) += tx.amount;
                    if let Some(entries) = self.unbonding.get_mut(&tx.from) {
                        if let Some(pos) = entries.iter().position(|e| e.tx_hash == tx.hash) {
                            entries.remove(pos);
                        }
                        if entries.is_empty() {
                            self.unbonding.remove(&tx.from);
                        }
                    }
                }
                // LIVE-3: reverse a Slash — restore the offender's stake (mirror
                // of the apply-side debit; the sink credit is done by
                // `cache_revert_tx` in the reorg's cache-revert loop).
                // LIVE-3B: the carried breakdown is what makes this EXACT — the
                // bonded portion returns to `staked`, and each consumed unbonding
                // entry is re-created (or topped back up, if partially consumed)
                // with its original amount + unlock height + origin tx hash. This
                // is the reversibility property the breakdown exists for: a popped
                // slash leaves the state byte-identical to pre-slash.
                TxType::Slash => {
                    let consumed_unbonding: u64 =
                        tx.slash_unbonding.iter().flatten().map(|e| e.amount).sum();
                    let bonded_take = tx.amount.saturating_sub(consumed_unbonding);
                    if bonded_take > 0 {
                        *self.staked.entry(tx.from.clone()).or_insert(0) += bonded_take;
                    }
                    if let Some(consumed) = &tx.slash_unbonding {
                        let list = self.unbonding.entry(tx.from.clone()).or_default();
                        for c in consumed {
                            if let Some(e) = list.iter_mut().find(|e| e.tx_hash == c.tx_hash) {
                                e.amount += c.amount; // partial consumption restored
                            } else {
                                list.push(UnbondEntry {
                                    amount: c.amount,
                                    unlock_height: c.unlock_height,
                                    tx_hash: c.tx_hash.clone(),
                                });
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// ONCHAIN-STAKE-1: bonded stake of an account (µQTA) — the consensus weight.
    pub fn staked_of(&self, pk: &str) -> u64 {
        self.staked.get(pk).copied().unwrap_or(0)
    }

    /// ONCHAIN-STAKE-1 §5: total bonded stake across all accounts (µQTA). Counts
    /// toward conservation as a **locked** balance (not destroyed).
    pub fn staked_total(&self) -> u64 {
        self.staked.values().sum()
    }

    /// ONCHAIN-STAKE-1 §5: total unbonding stake across all accounts (µQTA) —
    /// locked, not yet spendable, **not** destroyed. Also counts toward conservation.
    pub fn unbonding_total(&self) -> u64 {
        self.unbonding.values().flatten().map(|e| e.amount).sum()
    }

    /// ONCHAIN-STAKE-1: total amount this account currently has unbonding (µQTA).
    pub fn unbonding_of(&self, pk: &str) -> u64 {
        self.unbonding
            .get(pk)
            .map(|v| v.iter().map(|e| e.amount).sum())
            .unwrap_or(0)
    }

    /// Read view of this account's pending unlocks, sorted soonest-first:
    /// `(amount µQTA, unlock_height)`. Powers the wallet's unbonding timeline;
    /// pure read, no consensus effect.
    pub fn unbonding_entries_of(&self, pk: &str) -> Vec<(u64, u64)> {
        let mut v: Vec<(u64, u64)> = self
            .unbonding
            .get(pk)
            .map(|l| l.iter().map(|e| (e.amount, e.unlock_height)).collect())
            .unwrap_or_default();
        v.sort_by_key(|&(_, h)| h);
        v
    }

    /// ONCHAIN-STAKE-1 §4: the on-chain stake snapshot that feeds the validator
    /// set — `pk → bonded µQTA`, derived purely from the chain (zero locally
    /// measured input), so every node at the same chain state builds the **same**
    /// validator set. This is the source `build_validator_set` now reads, replacing
    /// the node-local reputation leaderboard and closing the fork vector.
    pub fn validator_stakes(&self) -> HashMap<String, u64> {
        self.staked
            .iter()
            .filter(|(_, &v)| v > 0)
            .map(|(k, &v)| (k.clone(), v))
            .collect()
    }

    /// LIVE-1 — the on-chain stake snapshot **re-keyed by the validator's ML-DSA
    /// public key** (hex), the identity a finality [`Vote`](crate::sm::finality_vote::Vote)
    /// carries. `validator_stakes()` is keyed by the ML-DSA **address**
    /// `BLAKE3(ADDR_DOMAIN ‖ pk)`; a finality vote's signature is verified against
    /// the **public key** itself, so weighing votes needs the map keyed that way.
    ///
    /// The bridge is a **pure function of the chain**: every `Stake` tx reveals the
    /// staker's `pq_public_key` (`verify_tx` requires it to hash to `from`), so
    /// scanning sealed blocks yields, for every currently-bonded validator, the
    /// `pk → stake` pair — **complete** (each staker revealed its key when it
    /// staked) and **identical on every node** (same chain ⇒ same map). The
    /// address→pk binding is unforgeable (BLAKE3), so no attacker can map a
    /// foreign account's stake onto its own key. Read-only; touches no mutation
    /// path, so conservation/coverage are unaffected. Non-Stake txs and the
    /// synthetic mint are skipped.
    pub fn validator_stakes_by_pubkey(&self) -> HashMap<String, u64> {
        let by_addr = self.validator_stakes();
        let mut out = HashMap::new();
        for block in &self.chain {
            for tx in &block.transactions {
                if tx.tx_type != TxType::Stake {
                    continue;
                }
                let Some(pk) = tx.pq_public_key.as_ref() else {
                    continue;
                };
                if let Some(&stake) = by_addr.get(&tx.from) {
                    // The address→pk binding is graven by the tx (verify_tx), so
                    // this maps each bonded address to the exact key it committed to.
                    out.insert(pk.clone(), stake);
                }
            }
        }
        out
    }

    // ── ONCHAIN-STAKE-1 §2 / §3: Stake & Unstake transactions ────────────────

    /// ONCHAIN-STAKE-1 §2: build a signed **Stake** tx locking `amount` µQTA of
    /// spendable balance into the bonded `staked` pool. The bonding (and the
    /// matching spendable debit) takes effect when the tx is **sealed into a
    /// block** — see `apply_block_stake_effects` — so the unlock arithmetic for a
    /// later `Unstake` is anchored to a chain height every node agrees on.
    ///
    /// `to` is the synthetic marker `"STAKE"`; the tx is still signed by `from`
    /// (it is **not** signature-exempt, unlike `NETWORK`/`ESCROW`), so no peer can
    /// forge a stake on someone else's behalf.
    pub fn stake_tx(
        &mut self,
        from: &str,
        amount: u64,
        crypto: &CryptoEngine,
    ) -> Result<Transaction, String> {
        self.stake_tx_at(from, amount, crypto, Utc::now().to_rfc3339(), false)
    }

    /// Injected-time / injected-entropy core of [`Self::stake_tx`] for the
    /// deterministic harness (mirrors `transfer_tx_at`).
    pub fn stake_tx_at(
        &mut self,
        from: &str,
        amount: u64,
        crypto: &CryptoEngine,
        ts: String,
        det_sign: bool,
    ) -> Result<Transaction, String> {
        if amount == 0 {
            return Err("Montant invalide".into());
        }
        // Lock only spendable funds. `balance_of` already nets pending outflows.
        let spendable = self.balance_of(from);
        if spendable < amount {
            return Err(format!(
                "Solde insuffisant pour staker: {:.6} QUANTA",
                spendable as f64 / MICRO as f64
            ));
        }
        let tx = self.build_signed_tx_at(from, "STAKE", amount, TxType::Stake, crypto, ts, det_sign)?;
        if !self.seen_tx_hashes.insert(tx.hash.clone()) {
            return Err("Transaction déjà traitée (replay détecté)".into());
        }
        // No spendable-balance effect at mempool time (applied on seal); this only
        // records the tx in the recent deque + mempool.
        self.cache_apply_tx(&tx);
        self.pending.push(tx.clone());
        Ok(tx)
    }

    /// ONCHAIN-STAKE-1 §3: build a signed **Unstake** tx beginning to unlock
    /// `amount` µQTA of bonded stake. On seal it moves `amount` from `staked` into
    /// an unbonding entry that becomes spendable only once the chain reaches
    /// `height_at_seal + UNBONDING_PERIOD_BLOCKS`. Rejected if the account does not
    /// currently have at least `amount` **bonded** (only sealed stake counts).
    pub fn unstake_tx(
        &mut self,
        from: &str,
        amount: u64,
        crypto: &CryptoEngine,
    ) -> Result<Transaction, String> {
        self.unstake_tx_at(from, amount, crypto, Utc::now().to_rfc3339(), false)
    }

    /// Injected-time / injected-entropy core of [`Self::unstake_tx`].
    pub fn unstake_tx_at(
        &mut self,
        from: &str,
        amount: u64,
        crypto: &CryptoEngine,
        ts: String,
        det_sign: bool,
    ) -> Result<Transaction, String> {
        if amount == 0 {
            return Err("Montant invalide".into());
        }
        // Only **sealed** bonded stake can be unbonded, and an account must not
        // queue Unstakes whose sum exceeds it (a pending Unstake has no effect
        // until sealed, so without this guard two pending Unstakes could each pass
        // the bonded check and over-draw `staked` at seal → fabricated unbonding).
        let bonded = self.staked_of(from);
        let pending_unstake: u64 = self
            .pending
            .iter()
            .filter(|t| t.tx_type == TxType::Unstake && t.from == from)
            .map(|t| t.amount)
            .sum();
        if bonded < amount.saturating_add(pending_unstake) {
            return Err(format!(
                "Enjeu insuffisant à délier: {:.6} QUANTA bondé ({:.6} déjà en attente de déliement)",
                bonded as f64 / MICRO as f64,
                pending_unstake as f64 / MICRO as f64
            ));
        }
        let tx =
            self.build_signed_tx_at(from, "STAKE", amount, TxType::Unstake, crypto, ts, det_sign)?;
        if !self.seen_tx_hashes.insert(tx.hash.clone()) {
            return Err("Transaction déjà traitée (replay détecté)".into());
        }
        self.cache_apply_tx(&tx);
        self.pending.push(tx.clone());
        Ok(tx)
    }
}
