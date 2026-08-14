// p2p/ledger/reorg.rs — block production, integration & fork reconciliation
//
// Split out of ledger.rs (organizational only — pure move, no logic change).
// Houses: block sealing (seal_block[_at] / coalesce_block_rewards /
// seal_if_pending[_at]), forge_block_at, remote-block integration
// (integrate_remote_block), GADGET-5B reorg_to_fork and pop_above.

use super::*;

impl Ledger {

    /// Seal pending transactions into a new block.
    /// Invariant: `self.chain` always has at least the genesis block (enforced
    /// by `new()`).
    pub fn seal_block(&mut self, miner: &str, energy_kwh: f64) -> Block {
        // Production reads the wall clock at the boundary and delegates to the
        // injected-time core (Phase 0, Constitution §3).
        self.seal_block_at(miner, energy_kwh, Utc::now().to_rfc3339())
    }

    /// Seal pending transactions into a new block with an **injected** RFC3339
    /// `timestamp`, so the deterministic core (`sm::Node`) can PRODUCE blocks
    /// reproducibly without reading the wall clock (C7 / Phase 0). Pure
    /// extraction of `seal_block`: identical Merkle root, hash pre-image, and
    /// block structure — only the timestamp source changes.
    pub fn seal_block_at(&mut self, miner: &str, energy_kwh: f64, timestamp: String) -> Block {
        // Resolve prev (recover genesis if the chain is somehow empty), capturing
        // only OWNED data so no borrow of `self` outlives the mutations below.
        if self.chain.last().is_none() {
            log::error!("◈ [Ledger] CRITICAL: chain is empty — this should never happen");
            *self = Self::new(); // Re-create genesis to recover
        }
        let (prev_index, prev_hash) = {
            let prev = self.chain.last().expect("genesis exists after recovery");
            (prev.index, prev.hash.clone())
        };
        let index = prev_index + 1;
        let ts = timestamp;
        // COVER-2 §1–§2: on-chain spendable balances BEFORE this block, via the
        // COVER-1 helper (single source of truth) — chain-only, deterministic.
        let onchain_before = {
            let prev = self.chain.last().expect("genesis exists");
            self.onchain_spendable_before(prev)
        };

        // EMIT-1 (Option A — one reward per block): fold the pending mining
        // rewards into a SINGLE coalesced `NETWORK→miner` tx before sealing.
        // MINT-EXACT-1 : la récompense a été posée dans le mempool par
        // `mint_block_reward` (production) — un montant CANONIQUE dérivé de la
        // chaîne, que `validate_block_emission_against` recalcule à la réception.
        // Le scellement ne fait donc que coalescer, sans jamais créer de monnaie
        // de sa propre initiative : le cœur déterministe (`sm/`) garde exactement
        // la sémantique qu'il avait.
        let candidate = Self::coalesce_block_rewards(std::mem::take(&mut self.pending), index, &ts);

        // COVER-2 §2: build a VALID-BY-CONSTRUCTION block — EXCLUDE any uncovered
        // tx (the SAME sequential rule validation uses) rather than refusing to
        // seal. An excluded tx already applied its balance effect at admission
        // (`cache_apply_tx`), so we REVERT it to keep the `cache == chain+pending`
        // invariant, then DROP it (§4: evicted — it stays in `seen_tx_hashes`, so
        // it is not re-admitted; if its funding later lands on-chain it re-enters
        // this node via block sync, not the mempool). The sealed block thus holds
        // ONLY covered txs and always passes `validate_block_against_prev` (§3).
        // PQ-MIG-3 §4 (COVER-2 symmetry): also exclude any tx that VIOLATES the
        // ML-DSA key binding (a foreign/forged key for an already-bound account),
        // using the SAME rule `validate_block_against_prev` rejects with — so a
        // self-sealed block is binding-valid by construction. A node's own txs all
        // carry its consistent primary key, so this only fires on a mempool that
        // somehow holds another account's mismatched tx.
        let bindings_before = {
            let prev = self.chain.last().expect("genesis exists");
            self.pq_bindings_before(prev)
        };
        let uncovered = Self::uncovered_tx_indices(&onchain_before, &candidate);
        let unbound = Self::binding_violations(&bindings_before, &candidate);
        // LIVE-3 (COVER-2 symmetry): also exclude any `Slash` whose embedded proof
        // no longer verifies or whose amount no longer matches the offender's current
        // bonded stake (e.g. the stake changed since the slash was queued). Same rule
        // `verify_block_slashes` rejects a received block with — so a self-sealed block
        // is slash-valid by construction and peers never reject the honest leader's block.
        let bad_slashes = self.invalid_slash_indices(&candidate);
        // C2 (AUDIT-2026-07-25, COVER-2 symmetry): exclude any tx from a synthetic
        // sender that is not the one legitimate coinbase — the same rule
        // `validate_block_against_prev` now rejects with, so a self-sealed block
        // stays valid by construction and an honest leader is never rejected by
        // its peers.
        let bad_synthetic = Self::illegal_synthetic_indices(&candidate, miner);
        // C3 (AUDIT-2026-07-25, COVER-2 symmetry): exclude any Unstake exceeding the
        // sender's bonded stake as of the parent — the same rule the shared
        // validator now rejects with. `validator_stakes()` is the live bonded map,
        // which at seal time is exactly the as-of-parent state (this block is not
        // applied yet).
        let bonded_before_seal = self.validator_stakes();
        let overdrawn = Self::overdrawn_unstake_indices(&bonded_before_seal, &candidate);
        // C-01 (NONCE-ONCHAIN-1, COVER-2 symmetry): exclude any tx that is not the
        // exact next transaction of its sender — the SAME rule
        // `validate_block_against_prev` now rejects a received block with, so a
        // self-sealed block is nonce-valid by construction and honest peers never
        // reject our block.
        //
        // The two fault classes are treated differently, and that difference is the
        // whole point: a STALE tx (nonce already consumed on-chain) is a replay and
        // dies here like any other invalid tx; an AHEAD tx (its predecessor has not
        // landed yet) is perfectly valid and is **put back in the mempool**, so
        // unordered gossip costs one block of latency instead of a lost payment.
        let nonces_before_seal = {
            let prev = self.chain.last().expect("genesis exists");
            self.account_nonces_before(prev)
        };
        let nonce_faults = Self::nonce_faults(&nonces_before_seal, &candidate);
        let stale_nonce: Vec<usize> = nonce_faults
            .iter()
            .filter(|(_, f)| *f == NonceFault::Stale)
            .map(|(i, _)| *i)
            .collect();
        let defer_nonce: HashSet<usize> = nonce_faults
            .iter()
            .filter(|(_, f)| *f == NonceFault::Ahead)
            .map(|(i, _)| *i)
            .collect();
        let txs = if uncovered.is_empty()
            && unbound.is_empty()
            && bad_slashes.is_empty()
            && bad_synthetic.is_empty()
            && overdrawn.is_empty()
            && nonce_faults.is_empty()
        {
            candidate
        } else {
            let drop_idx: HashSet<usize> = uncovered
                .into_iter()
                .chain(unbound)
                .chain(bad_slashes)
                .chain(bad_synthetic)
                .chain(overdrawn)
                .chain(stale_nonce)
                .collect();
            let mut kept = Vec::with_capacity(candidate.len());
            let mut deferred = Vec::new();
            for (i, tx) in candidate.into_iter().enumerate() {
                if drop_idx.contains(&i) {
                    log::warn!(
                        "◈ [Ledger] COVER-2/PQ-MIG-3: excluding uncovered-or-unbound tx {} (from {}) from sealed block #{}",
                        short(&tx.hash, 12),
                        short(&tx.from, 12),
                        index
                    );
                    self.cache_revert_tx(&tx); // undo its admission-time cache effect
                } else if defer_nonce.contains(&i) {
                    // Early, not invalid: keep it pending, cache effect untouched
                    // (it is still counted by `cache == chain + pending`).
                    log::debug!(
                        "◈ [Ledger] NONCE-ONCHAIN-1: tx {} (from {}, nonce {}) différée — \
                         sa transaction précédente n'est pas encore scellée",
                        short(&tx.hash, 12),
                        short(&tx.from, 12),
                        tx.nonce
                    );
                    deferred.push(tx);
                } else {
                    kept.push(tx);
                }
            }
            // `pending` was drained by `std::mem::take` above; the deferred txs go
            // back in, in their original relative order, for the next seal.
            self.pending.extend(deferred);
            kept
        };

        // BLK-HASH-1: the block hash commits to tx CONTENT via the Merkle root
        // (content+signature leaves) AND to the `miner` — via the shared
        // `block_hash_hex` (PQ-MIG-5), used verbatim by genesis and validation so
        // the hashings can never drift.
        let hash = Self::block_hash_hex(index, &prev_hash, &ts, miner, &txs, energy_kwh);
        let block = Block {
            index,
            timestamp: ts,
            transactions: txs,
            prev_hash,
            hash,
            miner: miner.into(),
            energy_kwh,
        };
        self.chain.push(block.clone());
        // ONCHAIN-STAKE-1: apply this block's Stake/Unstake + height-triggered
        // maturation now that it is the chain tip (block-index-anchored).
        self.apply_block_stake_effects(&block);
        // LIVE-3 (audit 2318): no `evict_stale_pending_slashes` needed HERE — `pending`
        // was drained by `std::mem::take` above and COVER-2 DROPS excluded txs (never
        // re-queues), so it is empty now. The stale-pending-slash race is integrate-only
        // (a *remote* block slashing an offender we still hold a pending slash for); this
        // node just sealed its OWN pending slash into the block, leaving nothing stale.
        block
    }

    /// EMIT-1 (Option A — one reward per block): collapse every pending
    /// `Mining` tx into a SINGLE coalesced `NETWORK→miner` reward (amount = Σ),
    /// preserving every non-mining tx in its original order (the coalesced
    /// reward leads). A block with ≤1 mining tx is returned **unchanged** —
    /// byte-identical to the pre-EMIT-1 seal — so only the genuinely
    /// multi-reward case is rewritten.
    ///
    /// MINT-EXACT-1 : la somme n'est plus un montant *choisi* — en production le
    /// seul contributeur du mempool est [`Ledger::mint_block_reward`], qui pose la
    /// récompense **canonique** dérivée de la chaîne. Coalescer reste utile comme
    /// défense en profondeur (un mempool corrompu, ou un producteur qui frappe
    /// deux fois, produit une somme unique que `validate_block_emission_against`
    /// compare ensuite à la canonique et rejette).
    ///
    /// The merged tx is fully deterministic: its id derives from the block
    /// `index`, its timestamp is the **injected** block `ts` (no wall-clock
    /// read), and its content/hash follow the same scheme as `next_tx`.
    pub(super) fn coalesce_block_rewards(
        txs: Vec<Transaction>,
        index: u64,
        ts: &str,
    ) -> Vec<Transaction> {
        // MINT-GUARD-2 (defense in depth for the critical mint vector): `Mining` is
        // NETWORK-only. ONLY genuine `NETWORK` rewards are coalesced; a `Mining` tx
        // from any other sender is a forgery (MINT-GUARD-1 already rejects it at
        // admission — this ensures a corrupted mempool could never mint either) and
        // is DROPPED here, never summed into the reward.
        let has_forged = txs
            .iter()
            .any(|t| t.tx_type == TxType::Mining && t.from != "NETWORK");
        // REWARD-SHARE-1 : un bloc porte désormais UNE récompense PAR bénéficiaire
        // (producteur + participants récents). La coalescence se fait donc **par
        // destinataire**, pas vers le mineur : fusionner tout vers lui écraserait
        // le partage que `validate_block_reward_plan` va recalculer.
        let mut per_payee: BTreeMap<String, u64> = BTreeMap::new();
        let mut duplicated = false;
        for tx in txs.iter() {
            if tx.tx_type == TxType::Mining && tx.from == "NETWORK" {
                let slot = per_payee.entry(tx.to.clone()).or_insert(0);
                if *slot > 0 {
                    duplicated = true;
                }
                *slot = slot.saturating_add(tx.amount);
            }
        }
        // Cas de production : au plus une récompense par bénéficiaire et aucune
        // forgerie ⇒ on rend le vecteur **inchangé**, byte-identique.
        if !duplicated && !has_forged {
            return txs;
        }
        let mut rest: Vec<Transaction> = Vec::with_capacity(txs.len());
        for tx in txs {
            if tx.tx_type != TxType::Mining {
                rest.push(tx);
            }
            // sinon : récompense NETWORK déjà sommée ci-dessus, ou forgerie —
            // une `Mining` d'un autre expéditeur n'est JAMAIS frappée (MINT-GUARD-2).
        }
        let payees: Vec<(String, u64)> =
            per_payee.into_iter().filter(|(_, amount)| *amount > 0).collect();
        if payees.is_empty() {
            return rest; // rien à émettre ; les forgeries ont été écartées
        }
        let mut out = Vec::with_capacity(rest.len() + payees.len());
        for (k, (to, total)) in payees.into_iter().enumerate() {
            let id = if k == 0 {
                format!("tx_mint_b{index}")
            } else {
                format!("tx_mint_b{index}_{k}")
            };
            let payload = Self::tx_signing_preimage(&id, "NETWORK", &to, total, ts, &TxType::Mining, 0, "");
            let hash = hex::encode(blake3::hash(&payload).as_bytes());
            out.push(Transaction {
                id,
                from: "NETWORK".into(),
                to,
                amount: total,
                tx_type: TxType::Mining,
                timestamp: ts.into(),
                signature: String::new(),
                hash,
                nonce: 0,
                pq_signature: None,
                pq_public_key: None,
                fault_proof: None,
                slash_unbonding: None,
            });
        }
        out.extend(rest);
        out
    }


    /// Seal only if there are pending transactions. Returns the block if one
    /// was sealed.
    pub fn seal_if_pending(&mut self, miner: &str, energy_kwh: f64) -> Option<Block> {
        if self.pending.is_empty() {
            return None;
        }
        Some(self.seal_block(miner, energy_kwh))
    }

    /// Injected-time `seal_if_pending` (see `seal_block_at`) — the
    /// deterministic core's block-production entry point.
    pub fn seal_if_pending_at(
        &mut self,
        miner: &str,
        energy_kwh: f64,
        timestamp: String,
    ) -> Option<Block> {
        if self.pending.is_empty() {
            return None;
        }
        Some(self.seal_block_at(miner, energy_kwh, timestamp))
    }

    /// Test-only forge: build a block over an arbitrary tx set with a CORRECT
    /// hash (real Merkle root + real pre-image), the way a malicious peer would
    /// — a valid hash over *invalid content*. Lets EMIT-1 E2/E3/E4 craft
    /// adversarial blocks (two mining txs; a mis-credited reward) that the
    /// consensus rule — not a stale hash — must reject. Shares
    /// `compute_merkle_root` + the pre-image with the real seal, so it can
    /// never drift from production. Never compiled into the shipped binary.
    #[cfg(test)]
    pub fn forge_block_at(
        index: u64,
        prev_hash: &str,
        timestamp: &str,
        miner: &str,
        txs: Vec<Transaction>,
    ) -> Block {
        let tx_root = Self::compute_merkle_root(&txs);
        // CANON-1: shares the canonical header encoding with production, so the
        // forge helper can never drift from `block_hash_hex`.
        let payload =
            Self::block_header_preimage(index, prev_hash, timestamp, miner, txs.len(), &tx_root, 0.0);
        let hash = hex::encode(blake3::hash(&payload).as_bytes());
        Block {
            index,
            timestamp: timestamp.into(),
            transactions: txs,
            prev_hash: prev_hash.into(),
            hash,
            miner: miner.into(),
            energy_kwh: 0.0,
        }
    }

    /// D1.3: Integrate a validated remote block into the local chain.
    ///
    /// Returns:
    /// - `Ok(true)` — block was new and integrated
    /// - `Ok(false)` — block already known (duplicate)
    /// - `Err(reason)` — block invalid or fork detected
    ///
    /// Fork resolution: if the remote block's index matches our tip (same
    /// height, different hash), we keep the block with the higher hash
    /// (deterministic tie-break). This is a simplified longest-chain rule
    /// suitable for PoC.
    pub fn integrate_remote_block(&mut self, block: Block) -> Result<bool, String> {
        let tip = self.chain.last().ok_or("empty chain")?;

        // Already have this block (same index + same hash)
        if block.index <= tip.index && self.chain.iter().any(|b| b.hash == block.hash) {
            return Ok(false); // duplicate, nothing to do
        }

        // Block extends our chain (happy path)
        if block.index == tip.index + 1 && block.prev_hash == tip.hash {
            self.validate_remote_block(&block)?;
            // LIVE-3: re-verify every embedded slash proof against OUR on-chain
            // stake (pre-block state = `&self`) before accepting the block. A
            // proposer cannot punish an innocent validator — the proof must show a
            // real fault and the amount must equal the offender's bonded stake.
            self.verify_block_slashes(&block)?;

            // Dedup transactions we haven't seen
            for tx in &block.transactions {
                self.seen_tx_hashes.insert(tx.hash.clone());
            }

            // Remove any pending txs that are now in this block.
            // Their cache effects are already applied, so no cache update needed.
            // BLK-HASH-1: match on `tx.hash` (content-bearing, carried across
            // nodes), NOT the local counter `tx.id` — otherwise a remote tx and
            // an unrelated local pending tx sharing a counter would falsely
            // match, dropping pending / skipping a cache effect → balance drift.
            let block_tx_hashes: HashSet<String> = block
                .transactions
                .iter()
                .map(|tx| tx.hash.clone())
                .collect();
            let pending_tx_hashes: HashSet<String> =
                self.pending.iter().map(|tx| tx.hash.clone()).collect();
            self.pending
                .retain(|tx| !block_tx_hashes.contains(&tx.hash));

            // PERF-1: Apply cache effects for remote txs that weren't in our pending.
            // (Remote mining/transfer txs we've never seen locally.)
            for tx in &block.transactions {
                if !pending_tx_hashes.contains(&tx.hash) {
                    self.cache_apply_tx(tx);
                }
            }

            log::info!(
                "◈ [Ledger] Integrated remote block #{} ({} txs, miner={})",
                block.index,
                block.transactions.len(),
                short(&block.miner, 12)
            );
            // ONCHAIN-STAKE-1: apply the integrated block's stake effects before
            // it is moved into the chain (block-index-anchored, identical to seal).
            self.apply_block_stake_effects(&block);
            self.chain.push(block);
            // LIVE-3 (audit 2318): a remote block that slashed an offender makes our
            // own pending slash for the same offender redundant — evict it so its
            // STAKE-sink debit is reverted and conservation stays exact at block time.
            self.evict_stale_pending_slashes();
            return Ok(true);
        }

        // Fork detected: same height, different block
        if block.index == tip.index && block.hash != tip.hash {
            // LIVE-2 — FINALITY FLOOR (absolute veto). If the tip we'd replace is at
            // or below the last finalized block, it is IRREVERSIBLE (GADGET-3
            // finalization + GADGET-4 accountable safety: two conflicting finalized
            // branches require ⅓ slashable). Refuse the reorg regardless of hash —
            // the free lexicographic tie-break only applies ABOVE the floor (Gasper:
            // fork-choice is free above finality, frozen at/below it). Pure safety
            // guard: we return without mutating any balance, so conservation and the
            // cache are untouched. `reorg_to_fork` (GADGET-5B) enforces the same floor
            // on the multi-block path; this closes it on the live single-block path.
            if tip.index <= self.finalized_floor_index {
                log::warn!(
                    "◈ [Ledger] FORK at height {} REFUSED — tip is finalized (floor={}), \
                     finalized history is irreversible",
                    block.index,
                    self.finalized_floor_index
                );
                return Ok(false); // keep the finalized block
            }
            // **C-04 (AUDIT-2026-08-13) — FORK-RANK-1.** Le départage était « le
            // plus grand hash gagne », donc gagnable par n'importe qui pour le
            // prix d'un broyage. Il passe par le rang d'élection pondéré par
            // l'enjeu ; le hash ne tranche plus que ce que le rang ne sépare pas.
            // L'ensemble bondé est pris **as-of-le-parent-du-fork**, c'est-à-dire
            // `chain[tip.index - 1]`, pour être identique chez les deux nœuds.
            if self.prefers_same_height(&block, tip) {
                log::warn!(
                    "◈ [Ledger] FORK at height {} — remote block wins (proposeur mieux élu ou hash supérieur: {}... vs {}...)",
                    block.index,
                    short(&block.hash, 12),
                    short(&tip.hash, 12)
                );
                // AUDIT-BLK-2: Validate FIRST, pop SECOND. The previous order
                // popped our tip before validate, so a malformed remote block
                // would leave the chain truncated by one. We now compute the
                // post-pop prev (= chain[tip.index - 1]) and validate against
                // that without mutating state.
                let prev_idx = tip
                    .index
                    .checked_sub(1)
                    .ok_or("cannot reorg below genesis")?;
                let prev_for_remote = self
                    .chain
                    .get(prev_idx as usize)
                    .ok_or("chain too short for fork reorg")?
                    .clone();
                if block.prev_hash != prev_for_remote.hash {
                    return Err("fork block prev_hash doesn't match the would-be new tip".into());
                }
                // Standalone validation (sigs + merkle root + recomputed hash).
                // COVER-1: the reorg winner REPLACES the tip, so its coverage is
                // checked against the chain WITHOUT that tip (`prev_for_remote` =
                // chain[tip.index-1]) — the exact same shared validator the linear
                // path uses, proving the check lives on both paths, not one.
                let onchain_before = self.onchain_spendable_before(&prev_for_remote);
                let bindings_before = self.pq_bindings_before(&prev_for_remote);
                // PROPOSER-1: on the 1-block fork tie-break `self` is one block
                // ahead of the parent (its tip is the block being replaced), so the
                // bonded set as of the parent must be replayed (`staked_before`),
                // NOT read from the live cache. Rare path (only on a fork), so the
                // O(chain) replay is fine; `staked_before(prev) == validator_stakes()`
                // at the parent is locked by test, so both paths agree.
                let bonded_before = self.staked_before(&prev_for_remote);
                // C-01: same as-of-parent nonce expectation on the fork path.
                let nonces_before = self.account_nonces_before(&prev_for_remote);
                // BLOCK-TIME-2 : médiane as-of-parent, comme sur le chemin linéaire.
                let median_before = self.median_time_before(&prev_for_remote);
                Self::validate_block_against_prev(
                    &block,
                    &prev_for_remote,
                    &onchain_before,
                    &bindings_before,
                    &bonded_before,
                    &nonces_before,
                    median_before,
                )?;
                // LIVE-3: a fork winner's slashes are re-verified too (same guard as
                // the linear path). Slashes essentially never appear in a competing
                // height-1 fork — the honest leader includes them on the canonical
                // chain — and LIVE-2's floor freezes finalized history; this is the
                // belt-and-suspenders check so no integration path admits a forged slash.
                self.verify_block_slashes(&block)?;

                // FORK-CAP-1: the incoming block REPLACES the tip, so it must
                // clear the SAME emission validation as the linear path — the
                // 100M hard cap + the per-block bound. Without this, the
                // fork-reorg branch let a network adversary mint past the hard
                // cap (the one monetary safety property that must never cede).
                // Validate BEFORE popping (AUDIT-BLK-2), against the supply that
                // would exist WITHOUT the tip we are about to replace, so the
                // popped tip's own reward is not double-counted.
                let our_tip_mining: u64 = tip
                    .transactions
                    .iter()
                    .filter(|t| t.tx_type == TxType::Mining)
                    .map(|t| t.amount)
                    .sum();
                let prior_mined = self.stats().total_mined.saturating_sub(our_tip_mining);
                Self::validate_block_emission_against(&block, prior_mined)?;
                // REWARD-SHARE-1 : la répartition passe par le même chemin que le
                // total. `self` est encore au tip que ce bloc remplace, et
                // `expected_block_rewards` ne regarde que les blocs d'index
                // STRICTEMENT inférieur — la fenêtre de participation exclut donc
                // bien le tip abandonné.
                self.validate_block_reward_plan(&block, prior_mined)?;

                // Now it's safe to mutate state.
                let our_tip = self.chain.pop().ok_or("chain unexpectedly empty")?;

                // PERF-1: Revert balance effects of the old tip's txs.
                for tx in &our_tip.transactions {
                    self.cache_revert_tx(tx);
                }
                // ONCHAIN-STAKE-1: revert the popped tip's Stake/Unstake stake-state
                // effects (single-block reorg; maturation is height-keyed and the
                // replacement sits at the same height, so it is left intact).
                self.revert_block_stake_effects(&our_tip);
                // C-01 (NONCE-ONCHAIN-1): the sealed nonce expectation is a `max`
                // over the chain, so it cannot be un-done incrementally — recompute
                // it from the truncated chain. Without this, a reorg that replaces a
                // tip carrying nonce 5 with one carrying nonce 3 would leave the
                // expectation at 6 and freeze the account.
                self.rebuild_chain_nonces();

                // AUDIT-BLK-1: Re-queue txs from our popped tip that are NOT in
                // the winning remote block, so they aren't lost in the reorg.
                // The previous `!seen_tx_hashes.contains` guard always returned
                // false (popped txs are still in `seen_tx_hashes`) and silently
                // discarded them — a real data-loss bug for any tx exclusive
                // to the loser fork.
                let remote_tx_hashes: HashSet<String> = block
                    .transactions
                    .iter()
                    .map(|tx| tx.hash.clone())
                    .collect();
                for tx in our_tip.transactions {
                    if remote_tx_hashes.contains(&tx.hash) {
                        // In the winning block — no re-queue; its cache effect
                        // is re-applied below.
                        continue;
                    }
                    // EMIT-1 §4.1: re-queue ONLY real user transfers. Synthetic
                    // senders (`NETWORK` mining reward, `ESCROW` release) belong
                    // to a block, never the mempool. Re-queuing the loser's
                    // mining reward would let it be sealed AGAIN into a later
                    // block — a double-mint the height-1 competition never owed.
                    // Its cache effect was reverted above and stays reverted.
                    // LIVE-3 (audit): a `Slash` is likewise **network-authored** (its
                    // authority is an embedded proof, not a signature) and belongs to
                    // a block, not the mempool. Re-queuing a popped slash would
                    // re-debit the STAKE sink and could double-slash if the winner
                    // re-slashes — skip it (its cache effect was reverted above).
                    if Self::is_synthetic_sender(&tx.from) || matches!(tx.tx_type, TxType::Slash) {
                        continue;
                    }
                    self.cache_apply_tx(&tx);
                    self.pending.push(tx);
                }

                // Apply the winning block's tx effects to cache + seen set.
                for tx in &block.transactions {
                    self.seen_tx_hashes.insert(tx.hash.clone());
                    self.cache_apply_tx(tx);
                }
                // ONCHAIN-STAKE-1: apply the winner's stake effects (its unlock
                // heights anchor to the same index the loser used).
                self.apply_block_stake_effects(&block);
                self.chain.push(block);
                // LIVE-3 (audit 2318): the winning fork may slash an offender for whom
                // we hold a pending slash — evict the now-redundant pending one.
                self.evict_stale_pending_slashes();
                return Ok(true);
            } else {
                log::info!(
                    "◈ [Ledger] FORK at height {} — our block wins ({}... vs {}...)",
                    block.index,
                    short(&tip.hash, 12),
                    short(&block.hash, 12)
                );
                return Ok(false); // keep ours
            }
        }

        // Block too far ahead or behind — ignore
        Err(format!(
            "block index {} out of range (our tip: {})",
            block.index, tip.index
        ))
    }


    /// **C-04 (AUDIT-2026-08-13) — FORK-RANK-1 : le départage cesse d'être gratuit.**
    ///
    /// Le fork-choice était « le plus grand hash gagne ». Un attaquant **sans un
    /// seul µQTA d'enjeu** rebroyait son bloc (l'ordre des transactions et le
    /// `timestamp` entrent dans le hash, et rien ne bornait ce dernier avant
    /// BLOCK-TIME-1) jusqu'à dépasser le hash honnête : il gagnait alors *tous*
    /// les départages, à volonté. Combiné à OPEN-DOOR-1, cela lui donnait la
    /// réécriture d'un bloc sur seize pour un coût de calcul dérisoire.
    ///
    /// Le correctif n'invente rien : l'élection pondérée par l'enjeu **existait
    /// déjà** (`pos_consensus::elect_leader`), elle ne servait simplement qu'au
    /// scellement. On la généralise en classement total ([`election_rank_of`]) et
    /// on en fait le **premier** critère de départage. Le broyage devient inutile
    /// : le rang d'un proposeur ne dépend que du beacon enterré, de la hauteur et
    /// de l'enjeu bondé chez le parent — trois choses qu'un bloc concurrent ne
    /// peut pas changer.
    ///
    /// Déterminisme inter-nœuds (sinon le correctif serait lui-même un fork) :
    /// les trois entrées sont communes aux deux nœuds — le beacon vient d'un bloc
    /// **enterré** de `LEADER_ENTROPY_LOOKBACK` hauteurs, donc sous le point de
    /// fork ; l'ensemble bondé est pris as-of-parent ; le proposeur est dans le
    /// bloc et lié à son hash par BLK-HASH-1.
    ///
    /// **Slot ouvert (OPEN-DOOR-1) : le rang ne s'applique pas.** Un slot ouvert
    /// existe précisément pour qu'une adresse sans enjeu puisse produire ; y faire
    /// gagner systématiquement le mieux bondé rendrait la porte décorative et
    /// refermerait le réseau, ce que OPEN-DOOR-1 avait ouvert exprès. Sur ces
    /// blocs — un sur seize, borne déjà concédée au trilemme Sybil — le départage
    /// reste le hash. Hors slot ouvert, PROPOSER-1 garantit que les deux
    /// concurrents sont bondés, donc les deux ont un rang.
    ///
    /// Départage deux blocs **de même hauteur** en dérivant l'ensemble bondé
    /// as-of-le-parent depuis la chaîne locale. C'est l'entrée unique du
    /// fork-choice : le départage vit à un seul endroit, donc le chemin vivant
    /// (`integrate_remote_block`) et le chemin de guérison de partition
    /// (`fork_heal`) ne peuvent pas diverger — deux règles de fork-choice
    /// différentes dans un même binaire, c'est un fork garanti.
    pub(crate) fn prefers_same_height(&self, challenger: &Block, incumbent: &Block) -> bool {
        let parent_idx = incumbent.index.saturating_sub(1);
        let bonded = match self.chain.get(parent_idx as usize) {
            Some(p) => self.staked_before(p),
            None => self.validator_stakes(),
        };
        self.fork_choice_prefers(challenger, incumbent, &bonded)
    }

    /// Renvoie `true` si `challenger` doit remplacer `incumbent`.
    pub(crate) fn fork_choice_prefers(
        &self,
        challenger: &Block,
        incumbent: &Block,
        bonded_before: &HashMap<String, u64>,
    ) -> bool {
        match self.election_rank_at(challenger.index, &challenger.miner, bonded_before) {
            Some(cr) => match self.election_rank_at(incumbent.index, &incumbent.miner, bonded_before) {
                // Rangs distincts : le mieux élu l'emporte, quel que soit le hash.
                Some(ir) if cr != ir => cr < ir,
                // Rangs égaux (même proposeur des deux côtés = auto-équivocation,
                // que le gadget de finalité punit) : le hash tranche, faute de
                // mieux, et le broyage n'achète alors qu'une faute slashable.
                Some(_) => challenger.hash > incumbent.hash,
                // L'un est classé, l'autre non : seul un slot ouvert permet un
                // proposeur non classé, et là le classement ne doit pas décider.
                None => challenger.hash > incumbent.hash,
            },
            None => challenger.hash > incumbent.hash,
        }
    }

    /// Rang d'élection d'un proposeur à une hauteur donnée, `None` s'il n'est pas
    /// classé. Le beacon est sourcé d'un bloc **enterré** (`LEADER_ENTROPY_LOOKBACK`)
    /// pour qu'un proposeur ne puisse pas se réélire en broyant le bloc qu'il vient
    /// de sceller ; sous cette profondeur on retombe sur la genèse, qui est fixe.
    pub(crate) fn election_rank_at(
        &self,
        index: u64,
        proposer: &str,
        bonded_before: &HashMap<String, u64>,
    ) -> Option<u32> {
        use crate::p2p::pos_consensus as pos;
        if pos::is_open_slot(index) && bonded_before.get(proposer).copied().unwrap_or(0) < pos::MIN_VALIDATOR_STAKE {
            return None;
        }
        let buried_idx = index.saturating_sub(pos::LEADER_ENTROPY_LOOKBACK);
        let buried = self.chain.get(buried_idx as usize).or_else(|| self.chain.first())?;
        let beacon = pos::leader_beacon(&buried.hash, index);
        let validators: Vec<pos::Validator> = bonded_before
            .iter()
            .filter(|(_, &s)| s >= pos::MIN_VALIDATOR_STAKE)
            .map(|(pk, &stake)| pos::Validator { pk: pk.clone(), stake, reputation: 0 })
            .collect();
        pos::election_rank_of(&beacon, index, &validators, proposer)
    }

    /// GADGET-5B — **multi-block fork reconciliation** at partition heal. Adopt
    /// the competing fork `winners` (ascending index; `winners[0]` is a child of a
    /// block already on our chain — the *common ancestor*) that the GADGET-5A
    /// GHOST engine selected, reorganizing the chain with **full conservation**:
    ///
    /// - the abandoned local branch is **popped**, every block's balance-cache and
    ///   stake effects **reverted** — so its emission (block rewards) and applied
    ///   state are undone; no coin survives a dropped branch (EMIT-1 at partition
    ///   scale: `Σ(spendable+staked+unbonding)+burned == minted` still holds);
    /// - its **user** txs that the winning fork did not include are **re-queued**
    ///   (AUDIT-BLK-1), while its **synthetic** rewards are dropped — never
    ///   re-minted (EMIT-1 §4.1: re-queuing a `NETWORK` reward would double-mint);
    /// - the winning fork is applied through the SAME linear
    ///   [`Self::integrate_remote_block`] happy path, so each winner clears the
    ///   identical coverage + emission + signature + ML-DSA-binding validation a
    ///   directly-extended block does (no second, weaker validator).
    ///
    /// **Finality floor (absolute).** `floor_index` is the last finalized block's
    /// index; the reorg **refuses** to disturb any block at or below it — a fork
    /// that would undo finalized history can never win (GADGET-5A §3 / GADGET-3),
    /// and by accountable safety (GADGET-4) two conflicting finalized branches are
    /// impossible without ⅓ slashable, so finalized prefixes coincide.
    ///
    /// **Validate-before-commit** (AUDIT-BLK-2, generalized to N blocks): the
    /// reorg runs on a trial **clone**; a single invalid/duplicate winner aborts it
    /// whole, leaving the live chain byte-for-byte untouched (a malformed fork can
    /// never truncate us). Returns `Ok(true)` iff it actually reorganized,
    /// `Ok(false)` if it legitimately kept our chain (floor / unknown root / not a
    /// clean fork), `Err` only on an invalid winner block.
    pub fn reorg_to_fork(&mut self, winners: &[Block], floor_index: u64) -> Result<bool, String> {
        let first = winners.first().ok_or("empty winning fork")?;
        // Common ancestor = the chain block the fork roots at (its prev_hash).
        let fork_point = match self.chain.iter().find(|b| b.hash == first.prev_hash) {
            Some(b) => b.index,
            None => return Ok(false), // fork roots at a block we don't hold — keep ours
        };
        // Finality floor (absolute): never disturb a block at or below the last
        // finalized one — a fork diverging there contradicts finalized history.
        if fork_point < floor_index {
            return Ok(false);
        }
        // **C-03 (AUDIT-2026-08-13) — REORG-DEPTH-1.** Le plancher ci-dessus ne
        // protège que ce que la finalité a atteint ; tant que personne ne vote il
        // vaut 0, et une branche remontant à la genèse passait. On borne donc la
        // profondeur indépendamment de la finalité : c'est ce qui ferme
        // l'attaque *long-range* d'un ancien validateur qui n'a plus rien à
        // perdre. Placé avant tout clone d'essai, donc le rejet est O(1).
        let our_height = self.chain.last().map(|b| b.index).unwrap_or(0);
        let depth = our_height.saturating_sub(fork_point);
        if depth > MAX_REORG_DEPTH {
            log::warn!(
                "◈ [Ledger] REORG de {} blocs REFUSÉ (maximum {}) — une branche aussi \
                 profonde est une attaque ou un incident qui demande une resynchronisation \
                 explicite (REORG-DEPTH-1)",
                depth,
                MAX_REORG_DEPTH
            );
            return Ok(false);
        }
        // Trial reorg on a clone — commit only if the WHOLE winning fork integrates
        // cleanly (no half-applied reorg, no truncation on a bad winner).
        let mut trial = self.clone();
        let popped = trial.pop_above(fork_point);
        for w in winners {
            match trial.integrate_remote_block(w.clone()) {
                Ok(true) => {}
                // A duplicate/rejected winner means the supplied blocks are not a
                // clean linear fork off the common ancestor → abort, keep our chain.
                Ok(false) => return Ok(false),
                Err(e) => return Err(e),
            }
        }
        // Re-queue the loser's user txs absent from the winning fork (AUDIT-BLK-1);
        // drop synthetic-sender rewards so a popped emission is never re-minted.
        let winner_tx_hashes: HashSet<String> = winners
            .iter()
            .flat_map(|b| b.transactions.iter().map(|t| t.hash.clone()))
            .collect();
        for blk in popped {
            for tx in blk.transactions {
                // LIVE-3 (audit): a `Slash` is network-authored (belongs to a block,
                // not the mempool) — never re-queue a popped one (it would re-debit
                // the STAKE sink / risk a double-slash). Same rule as synthetic senders.
                if winner_tx_hashes.contains(&tx.hash)
                    || Self::is_synthetic_sender(&tx.from)
                    || matches!(tx.tx_type, TxType::Slash)
                {
                    continue;
                }
                trial.cache_apply_tx(&tx);
                trial.pending.push(tx);
            }
        }
        *self = trial;
        Ok(true)
    }

    /// Pop every block **strictly above** `keep_index` (tip-first), reverting each
    /// one's balance-cache and stake effects; returns the popped blocks (tip-first)
    /// for the caller's re-queue pass. Genesis (index 0) is never popped, since the
    /// caller always passes `keep_index >= floor_index >= 0`.
    fn pop_above(&mut self, keep_index: u64) -> Vec<Block> {
        let mut popped = Vec::new();
        while self.chain.last().map(|b| b.index > keep_index).unwrap_or(false) {
            let Some(blk) = self.chain.pop() else { break };
            for tx in &blk.transactions {
                self.cache_revert_tx(tx);
            }
            self.revert_block_stake_effects(&blk);
            popped.push(blk);
        }
        // C-01: same reason as the 1-block path — a `max` cannot be reverted, so
        // the sealed nonce expectation is recomputed from what is left.
        self.rebuild_chain_nonces();
        self.resync_account_nonces();
        popped
    }
}
