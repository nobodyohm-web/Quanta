// p2p/ledger/slash.rs — accountable-safety slashing on the live ledger (LIVE-3/3B)
//
// Split out of ledger.rs (organizational only — pure move, no logic change).
// Houses: slash amount/consumption planning (expected_slash_consumption),
// slashable_stakes_by_pubkey, build/verify of Slash txs, the in-block
// invalid_slash_indices guard, queue_slash and the pending-slash eviction.

use super::*;

impl Ledger {

    // ─── LIVE-3: accountable-safety slashing on the live ledger ──────────────

    /// LIVE-3 — the slash amount for an offender bonding `staked` µQTA, per the
    /// ratified policy (ADR-009: `SLASH_NUM / SLASH_DEN` = full). Pure integer math.
    /// **Clamped to `staked`** so a future retune of the fraction to `> 1` can never
    /// make the amount exceed the bonded stake (which would desync the STAKE-sink
    /// debit from the `staked` debit and break conservation) — mirrors
    /// `finality_slashing::slash_amount`'s `.min(stake)`.
    pub fn slash_amount_for(staked: u64) -> u64 {
        use crate::sm::finality_slashing::{SLASH_DEN, SLASH_NUM};
        // full slash (1/1) by default; the fraction is graven-generic (u128 to
        // avoid overflow on the multiply, though staked ≤ 100M QUANTA ≪ u64).
        (((staked as u128 * SLASH_NUM as u128) / SLASH_DEN.max(1) as u128) as u64).min(staked)
    }

    /// LIVE-3B — the **deterministic consumption plan** of a slash against this
    /// ledger's current state: `(amount, bonded_take, consumed_unbonding)`.
    ///
    /// The slash base is `staked + unbonding` (Casper semantics: a validator is
    /// slashable until its withdrawal completes — this is what closes the
    /// *unstake-and-run* escape, audit 837). The ratified fraction (ADR-009,
    /// `SLASH_NUM/SLASH_DEN`) applies to that base; **bonded stake is consumed
    /// first**, then unbonding entries in `(unlock_height, tx_hash)` order (the
    /// soonest-to-mature coins burn first — they are the flight risk), the last
    /// entry possibly partially. Pure over `&self` → every node computes the
    /// SAME plan from the same chain, which is what lets `slash_tx_valid`
    /// require the tx's carried breakdown to match **exactly**.
    fn expected_slash_consumption(&self, addr: &str) -> (u64, u64, Vec<ConsumedUnbond>) {
        let bonded = self.staked_of(addr);
        let unbonding = self.unbonding_of(addr);
        let amount = Self::slash_amount_for(bonded.saturating_add(unbonding));
        let bonded_take = amount.min(bonded);
        let mut rest = amount - bonded_take;
        let mut entries = Vec::new();
        if rest > 0 {
            if let Some(list) = self.unbonding.get(addr) {
                let mut sorted: Vec<&UnbondEntry> = list.iter().collect();
                sorted.sort_by(|a, b| {
                    (a.unlock_height, &a.tx_hash).cmp(&(b.unlock_height, &b.tx_hash))
                });
                for e in sorted {
                    if rest == 0 {
                        break;
                    }
                    let take = rest.min(e.amount);
                    entries.push(ConsumedUnbond {
                        tx_hash: e.tx_hash.clone(),
                        unlock_height: e.unlock_height,
                        amount: take,
                    });
                    rest -= take;
                }
            }
        }
        (amount, bonded_take, entries)
    }

    /// LIVE-3B — the **slashable** weight per revealed ML-DSA pubkey:
    /// `staked + unbonding` (vs [`Self::validator_stakes_by_pubkey`], the
    /// **voting** weight = bonded only). Two maps, two purposes: an unbonding
    /// validator must NOT vote (its weight left the active set) but MUST remain
    /// punishable until withdrawal completes — passing this map to
    /// `verify_proof` is what keeps a fully-unstaked equivocator slashable.
    /// Same chain-walk binding as the voting map (each `Stake` tx reveals its
    /// `pq_public_key`, graven to the address by `verify_tx`).
    pub fn slashable_stakes_by_pubkey(&self) -> HashMap<String, u64> {
        let mut out = HashMap::new();
        for block in &self.chain {
            for tx in &block.transactions {
                if tx.tx_type != TxType::Stake {
                    continue;
                }
                let Some(pk) = tx.pq_public_key.as_ref() else {
                    continue;
                };
                let weight = self
                    .staked_of(&tx.from)
                    .saturating_add(self.unbonding_of(&tx.from));
                if weight > 0 {
                    out.insert(pk.clone(), weight);
                }
            }
        }
        out
    }

    /// LIVE-3 — build the network-authorized **Slash** tx that destroys the
    /// offender's **slashable** stake (bonded + unbonding, LIVE-3B — Casper
    /// semantics: punishable until the withdrawal completes), carrying `proof`
    /// (GADGET-4) as its authority. The offender is taken from the proof; the
    /// amount is the ratified fraction of their current slashable base, and the
    /// tx carries the exact consumed-unbonding breakdown (hash-bound, verified
    /// by every node, restored exactly on reorg). `None` if the proof names no
    /// slashable validator (nothing anywhere) or its offender key is malformed.
    /// The tx is **unsigned** (no offender signature — a slash is authorized by
    /// the proof, which every node re-verifies via [`Self::verify_block_slashes`]).
    pub fn build_slash_tx(&self, proof: &crate::sm::finality_slashing::FaultProof) -> Option<Transaction> {
        let offender_pk = proof.offender();
        let pk_bytes = hex::decode(offender_pk).ok()?;
        let offender_addr = CryptoEngine::ml_dsa_address_hex(&pk_bytes);
        // LIVE-3B: the slash base is bonded + unbonding (unstake-and-run closed);
        // the consumption plan is the shared deterministic computation every
        // verifier re-runs (`slash_tx_valid`).
        let (amount, _bonded_take, consumed) = self.expected_slash_consumption(&offender_addr);
        if amount == 0 {
            return None; // nothing bonded OR unbonding to slash
        }
        let proof_json = serde_json::to_string(proof).ok()?;
        let ts = Self::GENESIS_TIMESTAMP; // deterministic; the hash binds the content
        // The id/hash bind the offender + amount + the FULL proof deterministically
        // (not a truncation — so two distinct proofs never collide on the tx hash).
        // LIVE-3B: the consumed-unbonding breakdown is bound too (appended ONLY
        // when non-empty, so a purely-bonded slash hashes byte-identically to
        // pre-LIVE-3B — zero wire drift on the existing flow).
        let id = format!("slash_{}", short(&offender_addr, 16));
        let payload = if consumed.is_empty() {
            format!("slash:{offender_addr}:{amount}:{proof_json}")
        } else {
            let consumed_json = serde_json::to_string(&consumed).ok()?;
            format!("slash:{offender_addr}:{amount}:{proof_json}:{consumed_json}")
        };
        let hash = hex::encode(blake3::hash(payload.as_bytes()).as_bytes());
        Some(Transaction {
            id,
            from: offender_addr,
            to: "BURN".into(),
            amount,
            tx_type: TxType::Slash,
            timestamp: ts.to_string(),
            signature: String::new(),
            hash,
            nonce: 0,
            pq_signature: None,
            pq_public_key: None,
            fault_proof: Some(proof_json),
            slash_unbonding: (!consumed.is_empty()).then_some(consumed),
        })
    }

    /// LIVE-3 — whether ONE `Slash` tx is legitimate against this ledger's on-chain
    /// stake (pure over `&self` + the tx + a precomputed pubkey-keyed stake map). A
    /// slash is authorized NOT by a signature but by its embedded [`FaultProof`], so
    /// the checks are: (1) the proof verifies against the on-chain **slashable**
    /// stake (a real double-vote/surround, valid ML-DSA sigs, offender bonded OR
    /// unbonding — LIVE-3B); (2) `from` is exactly the offender key's address
    /// `BLAKE3(ADDR_DOMAIN ‖ pk)`; (3) the amount is the ratified fraction of the
    /// offender's current slashable base AND the carried consumed-unbonding
    /// breakdown matches this node's own deterministic plan exactly; (4) the
    /// destination is `BURN`. A non-Slash tx is trivially "valid" here.
    fn slash_tx_valid(&self, tx: &Transaction, stakes_by_pk: &HashMap<String, u64>) -> bool {
        use crate::sm::finality::EPOCH_LENGTH_BLOCKS;
        use crate::sm::finality_slashing::{verify_proof, FaultProof};
        if tx.tx_type != TxType::Slash {
            return true;
        }
        let Some(proof_json) = tx.fault_proof.as_deref() else {
            return false;
        };
        let Ok(proof) = serde_json::from_str::<FaultProof>(proof_json) else {
            return false;
        };
        if !verify_proof(&proof, stakes_by_pk, EPOCH_LENGTH_BLOCKS) {
            return false; // no real fault / forged / not slashable
        }
        let Ok(pk_bytes) = hex::decode(proof.offender()) else {
            return false;
        };
        let offender_addr = CryptoEngine::ml_dsa_address_hex(&pk_bytes);
        // LIVE-3B: re-run the SAME deterministic consumption plan this node would
        // build, and require the tx to match it EXACTLY — total amount AND the
        // per-entry unbonding breakdown. A proposer can neither over-slash, nor
        // under-slash, nor lie about WHICH unbonding entries were destroyed
        // (the breakdown is what a reorg restores, so it must be beyond dispute).
        let (expected_amount, _bonded_take, expected_consumed) =
            self.expected_slash_consumption(&offender_addr);
        let carried: &[ConsumedUnbond] = tx.slash_unbonding.as_deref().unwrap_or(&[]);
        tx.from == offender_addr
            && tx.to == "BURN"
            && expected_amount > 0
            && tx.amount == expected_amount
            && carried == expected_consumed.as_slice()
    }

    /// LIVE-3 — indices of the **invalid** `Slash` txs in `txs` (the single source
    /// of truth, COVER-2 style): [`Self::verify_block_slashes`] rejects a received
    /// block if any exist, and [`Self::seal_block_at`] **excludes** exactly these so
    /// a self-sealed block is slash-valid by construction — the two can never disagree.
    ///
    /// The pass is **sequential** (like COVER-1's coverage), which is what keeps
    /// slashing conservation-exact under an adversarial block:
    /// - **at most ONE slash per offender per block** — a second slash of the same
    ///   offender is rejected. (Both would pass the stateless `amount == staked`
    ///   check against the same pre-block stake, then the sink would be debited
    ///   twice while `staked` saturates at 0 → a permanent conservation break. This
    ///   is the CRITICAL adversarial case: a leader self-equivocates once, then
    ///   duplicates the slash tx K times.)
    /// - **a slashed offender may not also Stake/Unstake in the same block** — an
    ///   `Unstake` in the same block would move the offender's coins into an
    ///   unbonding entry that later matures and **returns the slashed coins**
    ///   (double-count at maturation). Reject the slash so the two never coexist.
    pub(super) fn invalid_slash_indices(&self, txs: &[Transaction]) -> Vec<usize> {
        // Any Slash present ⇒ compute the pubkey-keyed stake once (it's the costly bit).
        if !txs.iter().any(|t| t.tx_type == TxType::Slash) {
            return Vec::new();
        }
        // LIVE-3B: proofs verify against the SLASHABLE weight (bonded + unbonding)
        // — an offender who fully unstaked remains punishable until withdrawal
        // completes (the voting map would drop them and let them run).
        let stakes_by_pk = self.slashable_stakes_by_pubkey();
        // Addresses that move stake (Stake/Unstake) in THIS block — a slash of any
        // of them is refused (the unbonding-escape / double-move hazard above).
        let stake_movers: HashSet<&str> = txs
            .iter()
            .filter(|t| matches!(t.tx_type, TxType::Stake | TxType::Unstake))
            .map(|t| t.from.as_str())
            .collect();
        let mut already_slashed: HashSet<String> = HashSet::new();
        let mut out = Vec::new();
        for (i, tx) in txs.iter().enumerate() {
            if tx.tx_type != TxType::Slash {
                continue;
            }
            let valid = self.slash_tx_valid(tx, &stakes_by_pk)
                && !already_slashed.contains(&tx.from) // one slash per offender per block
                && !stake_movers.contains(tx.from.as_str()); // no concurrent stake move
            if valid {
                already_slashed.insert(tx.from.clone());
            } else {
                out.push(i);
            }
        }
        out
    }

    /// LIVE-3 — **verify every `Slash` tx in a received block** against this
    /// ledger's on-chain stake. A malicious proposer cannot punish an innocent
    /// validator: the embedded proof must show a real fault and the amount must
    /// equal the ratified fraction of the offender's bonded stake. Rejects the
    /// whole block on the first invalid slash. Shares [`Self::invalid_slash_indices`]
    /// with the seal path (one rule, both directions).
    pub fn verify_block_slashes(&self, block: &Block) -> Result<(), String> {
        if let Some(&i) = self.invalid_slash_indices(&block.transactions).first() {
            return Err(format!(
                "bloc rejeté : slash invalide en position {} (preuve/offender/montant) — LIVE-3",
                i
            ));
        }
        Ok(())
    }

    /// LIVE-3 — **queue a slash** from a verified fault proof: build the network-
    /// authorized `Slash` tx (amount = the ratified fraction of the offender's
    /// current **slashable** stake — bonded + unbonding, LIVE-3B), apply its
    /// STAKE-sink debit to the cache (admission, like any tx), and push it to the
    /// mempool so the next seal includes it. Idempotent via `seen_tx_hashes`.
    /// Returns the queued tx, or `None` if there is nothing to slash (offender
    /// neither bonded nor unbonding) or the slash is already pending/applied.
    pub fn queue_slash(&mut self, proof: &crate::sm::finality_slashing::FaultProof) -> Option<Transaction> {
        let tx = self.build_slash_tx(proof)?;
        // **Per-offender mempool guard (audit CRITICAL/HIGH).** `seen_tx_hashes`
        // dedups by tx HASH, but a slash hash binds the full fault proof, and TWO
        // distinct-but-valid proofs can incriminate the SAME offender — e.g. two
        // separate equivocations, or the order-symmetric `FaultProof(a,b)` vs `(b,a)`
        // (both verify; both name the same `offender()`), which a live node reaches
        // when two peers observe one equivocation in opposite vote-arrival order.
        // Each would build a distinct-hash Slash tx whose amount = the offender's
        // FULL still-uncommitted bonded stake, and each `cache_apply_tx` debits the
        // shared STAKE sink → the sink goes negative and `total_burned` double-counts
        // → a **permanent** conservation break on a non-sealing node (a queued Slash is
        // now TTL-exempt from `prune_mempool`, audit 788, so it never self-heals). The
        // in-block guard (`invalid_slash_indices`) only fires at seal, so it does not
        // protect a node that never seals. Refuse a second slash
        // of an offender that already has one pending (keyed on the offender address —
        // every proof variant for one fault shares it). An **already-applied** slash
        // needs no guard: the offender's bonded stake is then 0, so `build_slash_tx`
        // already returns `None` (`slash_amount_for(0) == 0`).
        if self
            .pending
            .iter()
            .any(|t| t.tx_type == TxType::Slash && t.from == tx.from)
        {
            return None; // this offender already has a pending slash
        }
        if !self.seen_tx_hashes.insert(tx.hash.clone()) {
            return None; // already queued or applied (dedup)
        }
        self.cache_apply_tx(&tx); // STAKE sink debit at admission (mirrors other txs)
        self.pending.push(tx.clone());
        Some(tx)
    }

    /// Test-only: number of `Slash` txs currently in the mempool.
    #[cfg(test)]
    pub(crate) fn pending_slash_count_for_test(&self) -> usize {
        self.pending.iter().filter(|t| t.tx_type == TxType::Slash).count()
    }

    /// LIVE-3 (audit) — after a block is applied, drop any pending `Slash` that is
    /// now **redundant** against the new on-chain state — chiefly one whose offender
    /// was just slashed by the block (its bonded stake is now 0, so the pending slash
    /// would over-debit the STAKE sink and inflate `total_burned` against a stake
    /// that no longer exists). Reverts each dropped slash's cache effect so
    /// conservation stays **exact at block time** (not merely self-healing at the next
    /// mempool prune). Uses the SAME `invalid_slash_indices` rule the seal/receive
    /// paths use, so "redundant" means exactly "would be excluded/rejected now".
    /// Cheap no-op when no slash is pending.
    pub(super) fn evict_stale_pending_slashes(&mut self) {
        if !self.pending.iter().any(|t| t.tx_type == TxType::Slash) {
            return;
        }
        let snapshot = self.pending.clone();
        let bad_hashes: Vec<String> = self
            .invalid_slash_indices(&snapshot)
            .into_iter()
            .filter_map(|i| snapshot.get(i).map(|t| t.hash.clone()))
            .collect();
        for h in bad_hashes {
            if let Some(pos) = self.pending.iter().position(|t| t.hash == h) {
                let tx = self.pending.remove(pos);
                self.cache_revert_tx(&tx); // undo its admission-time STAKE-sink debit
                self.seen_tx_hashes.remove(&tx.hash);
            }
        }
    }
}
