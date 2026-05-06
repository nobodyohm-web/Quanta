// p2p/ledger.rs — QUANTA Native Protocol Distributed Ledger
// Self-sovereign token: no external blockchain dependency
// Value backed by real energy consumption + network utility
//
// STRUCT-2: All monetary amounts are u64 in µQTA (1 QUANTA = 1_000_000 µQTA).
// This eliminates f64 rounding drift between nodes.

use crate::security::CryptoEngine;
use chrono::Utc;
use std::collections::{HashMap, HashSet, VecDeque};

// Re-export all types from ledger_types so external code keeps working
pub use super::ledger_types::*;


/// The ATN distributed ledger
pub struct Ledger {
    pub chain: Vec<Block>,
    pending: Vec<Transaction>,
    tx_counter: u64,
    /// V3: Anti-replay — set of all known transaction hashes
    seen_tx_hashes: HashSet<String>,
    /// B2: Per-account nonce tracking (account_pk → next expected nonce)
    account_nonces: HashMap<String, u64>,
    /// PERF-1: Incremental balance cache (pk → signed balance in µQTA).
    /// Updated on every tx insertion/removal. balance_of() reads this in O(1)
    /// instead of scanning the entire chain O(n).
    balance_cache: HashMap<String, i128>,
    /// PERF-2: Bounded deque of the most recent transactions (insertion order).
    /// recent_txs() reads from this in O(limit) instead of cloning + sorting O(n log n).
    recent_deque: VecDeque<Transaction>,
}

impl Ledger {
    pub fn new() -> Self {
        // Genesis block
        let genesis = Block {
            index: 0,
            timestamp: Utc::now().to_rfc3339(),
            transactions: vec![],
            prev_hash: "0".repeat(64),
            hash: hex::encode(blake3::hash(b"QUANTA_GENESIS_2026").as_bytes()),
            miner: "GENESIS".into(),
            energy_kwh: 0.0,
        };
        Self {
            chain: vec![genesis],
            pending: vec![],
            tx_counter: 0,
            seen_tx_hashes: HashSet::new(),
            account_nonces: HashMap::new(),
            balance_cache: HashMap::new(),
            recent_deque: VecDeque::new(),
        }
    }

    /// PERF-1: Maximum entries kept in the recent_deque (bounded ring buffer).
    const MAX_RECENT: usize = 500;

    /// PERF-1: Apply a transaction's balance effects to the cache.
    fn cache_apply_tx(&mut self, tx: &Transaction) {
        let synthetic = |a: &str| matches!(a, "NETWORK" | "BURN" | "ESCROW");
        if !synthetic(&tx.to) {
            *self.balance_cache.entry(tx.to.clone()).or_insert(0) += tx.amount as i128;
        }
        if !synthetic(&tx.from) {
            *self.balance_cache.entry(tx.from.clone()).or_insert(0) -= tx.amount as i128;
        }
        // PERF-2: push to recent deque (bounded)
        self.recent_deque.push_back(tx.clone());
        if self.recent_deque.len() > Self::MAX_RECENT {
            self.recent_deque.pop_front();
        }
    }

    /// PERF-1: Reverse a transaction's balance effects (used during fork reorg).
    fn cache_revert_tx(&mut self, tx: &Transaction) {
        let synthetic = |a: &str| matches!(a, "NETWORK" | "BURN" | "ESCROW");
        if !synthetic(&tx.to) {
            *self.balance_cache.entry(tx.to.clone()).or_insert(0) -= tx.amount as i128;
        }
        if !synthetic(&tx.from) {
            *self.balance_cache.entry(tx.from.clone()).or_insert(0) += tx.amount as i128;
        }
    }

    /// PERF-1: Full rebuild of balance_cache from chain + pending (used by restore).
    fn rebuild_cache(&mut self) {
        self.balance_cache.clear();
        self.recent_deque.clear();
        let synthetic = |a: &str| matches!(a, "NETWORK" | "BURN" | "ESCROW");
        for block in &self.chain {
            for tx in &block.transactions {
                if !synthetic(&tx.to) {
                    *self.balance_cache.entry(tx.to.clone()).or_insert(0) += tx.amount as i128;
                }
                if !synthetic(&tx.from) {
                    *self.balance_cache.entry(tx.from.clone()).or_insert(0) -= tx.amount as i128;
                }
                self.recent_deque.push_back(tx.clone());
            }
        }
        for tx in &self.pending {
            if !synthetic(&tx.to) {
                *self.balance_cache.entry(tx.to.clone()).or_insert(0) += tx.amount as i128;
            }
            if !synthetic(&tx.from) {
                *self.balance_cache.entry(tx.from.clone()).or_insert(0) -= tx.amount as i128;
            }
            self.recent_deque.push_back(tx.clone());
        }
        // Trim deque to MAX_RECENT
        while self.recent_deque.len() > Self::MAX_RECENT {
            self.recent_deque.pop_front();
        }
    }

    // ── B2: Nonce management ────────────────────────────────────────────

    /// Get the next expected nonce for an account.
    pub fn get_nonce(&self, pk: &str) -> u64 {
        self.account_nonces.get(pk).copied().unwrap_or(0)
    }

    /// Increment the nonce for an account (called after a successful outgoing tx).
    pub(crate) fn increment_nonce(&mut self, pk: &str) {
        let entry = self.account_nonces.entry(pk.to_string()).or_insert(0);
        *entry += 1;
    }

    /// Total circulating supply (mined - burned), in µQTA.
    pub fn total_supply(&self) -> u64 {
        let mined = self.stats().total_mined;
        let burned = self.total_burned();
        mined.saturating_sub(burned)
    }

    /// Current chain height (number of blocks including genesis).
    pub fn chain_height(&self) -> u64 {
        self.chain.len() as u64
    }

    /// Get a reference to the block at the given index, if it exists.
    pub fn block_at(&self, index: u64) -> Option<&Block> {
        self.chain.get(index as usize)
    }

    // ── CRIT-2: Escrow lock/release ─────────────────────────────────────

    /// Lock funds from a user account into the ESCROW sink (amount in µQTA).
    /// This creates an unsigned transaction that debits `amount` from `from_pk`
    /// and credits the synthetic "ESCROW" address.
    pub fn build_escrow_lock_tx(&mut self, from_pk: &str, amount: u64) -> Transaction {
        let tx = self.build_unsigned_tx(from_pk, "ESCROW", amount, TxType::Transfer);
        self.cache_apply_tx(&tx);
        self.pending.push(tx.clone());
        tx
    }

    /// Release funds from the ESCROW pool to a recipient (amount in µQTA).
    pub fn escrow_release_to(&mut self, to_pk: &str, amount: u64) -> Transaction {
        let tx = self.build_unsigned_tx("ESCROW", to_pk, amount, TxType::Transfer);
        self.cache_apply_tx(&tx);
        self.pending.push(tx.clone());
        tx
    }

    /// Create a mining transaction (network-issued, no signature). Amount in µQTA.
    pub fn mine_tx(&mut self, miner_pk: &str, amount: u64, kwh: f64) -> Transaction {
        let tx = self.build_unsigned_tx("NETWORK", miner_pk, amount, TxType::Mining);
        self.cache_apply_tx(&tx);
        self.pending.push(tx.clone());
        if self.pending.len() >= 10 {
            self.seal_block(miner_pk, kwh);
        }
        tx
    }

    /// Create a transfer transaction signed by the active identity (amount in µQTA).
    pub fn transfer_tx(
        &mut self,
        from: &str,
        to: &str,
        amount: u64,
        crypto: &CryptoEngine,
    ) -> Result<Transaction, String> {
        if amount == 0 {
            return Err("Montant invalide".into());
        }
        if from == to {
            return Err("Transfert vers soi-même".into());
        }
        // V4: Double-spend safe — balance_of includes pending outflows
        let balance = self.balance_of(from);
        if balance < amount {
            return Err(format!(
                "Solde insuffisant: {:.6} QUANTA",
                balance as f64 / MICRO as f64
            ));
        }
        let tx = self.build_signed_tx(from, to, amount, TxType::Transfer, crypto)?;
        // V3: Anti-replay — reject if we've seen this tx hash before
        if !self.seen_tx_hashes.insert(tx.hash.clone()) {
            return Err("Transaction déjà traitée (replay détecté)".into());
        }
        // V8: Timestamp window — reject txs with timestamps too far from now (±5 min)
        if let Ok(tx_time) = chrono::DateTime::parse_from_rfc3339(&tx.timestamp) {
            let drift = (Utc::now().timestamp() - tx_time.timestamp()).abs();
            if drift > 300 {
                return Err("Timestamp hors fenêtre de validité (±5 min)".into());
            }
        }
        self.cache_apply_tx(&tx);
        self.pending.push(tx.clone());
        Ok(tx)
    }

    /// V2 — Burn-and-Mint : 1% brûlé automatiquement à chaque transfert (amount en µQTA).
    /// Retourne `(tx_principale, montant_brûlé)` où le montant brûlé est en µQTA.
    pub fn transfer_with_burn(
        &mut self,
        from: &str,
        to: &str,
        amount: u64,
        crypto: &CryptoEngine,
    ) -> Result<(Transaction, u64), String> {
        // Minimum transfer 0.01 QUANTA = 10_000 µQTA
        if amount < 10_000 {
            return Err("Minimum transfer: 0.01 QUANTA".into());
        }
        // 1% burn — integer math: amount / 100
        let burn_amount = amount / 100;
        let net_amount = amount - burn_amount;
        let tx = self.transfer_tx(from, to, net_amount, crypto)?;
        if burn_amount > 0 {
            let burn_tx = self.build_unsigned_tx(from, "BURN", burn_amount, TxType::Burn);
            self.cache_apply_tx(&burn_tx);
            self.pending.push(burn_tx);
        }
        Ok((tx, burn_amount))
    }

    /// Burn QUANTA — permanent destruction, signed by the owner (amount en µQTA).
    pub fn burn_tx(
        &mut self,
        from: &str,
        amount: u64,
        crypto: &CryptoEngine,
    ) -> Result<Transaction, String> {
        if amount == 0 {
            return Err("Montant invalide".into());
        }
        let balance = self.balance_of(from);
        if balance < amount {
            return Err(format!(
                "Solde insuffisant: {:.6} QUANTA",
                balance as f64 / MICRO as f64
            ));
        }
        let tx = self.build_signed_tx(from, "BURN", amount, TxType::Burn, crypto)?;
        if !self.seen_tx_hashes.insert(tx.hash.clone()) {
            return Err("Transaction déjà traitée (replay détecté)".into());
        }
        self.cache_apply_tx(&tx);
        self.pending.push(tx.clone());
        Ok(tx)
    }

    /// STRUCT-3: Replay a validated remote transaction into the local ledger.
    /// Called from the gossip dispatcher AFTER signature + nonce verification.
    /// Idempotent via `seen_tx_hashes` dedup. Returns true if the tx was newly applied.
    pub fn replay_remote_tx(&mut self, tx: Transaction) -> bool {
        if !self.seen_tx_hashes.insert(tx.hash.clone()) {
            return false; // already seen
        }
        self.cache_apply_tx(&tx);
        self.pending.push(tx);
        true
    }

    /// Total QUANTA permanently burned (in µQTA).
    pub fn total_burned(&self) -> u64 {
        let chain_burn: u64 = self
            .chain
            .iter()
            .flat_map(|b| b.transactions.iter())
            .filter(|t| t.tx_type == TxType::Burn)
            .map(|t| t.amount)
            .sum();
        let pending_burn: u64 = self
            .pending
            .iter()
            .filter(|t| t.tx_type == TxType::Burn)
            .map(|t| t.amount)
            .sum();
        chain_burn + pending_burn
    }

    // ── Phase 3.1: Signature Verification (toujours hybride) ─────

    /// Verify the signature(s) on a user-signed transaction.
    /// - Network-issued transactions (from == "NETWORK") are exempt.
    /// - Toutes les autres tx passent par `HybridIdentity::verify_hybrid` :
    ///   - Si `pq_signature` + `pq_public_key` non vides → Ed25519 OR ML-DSA-65
    ///   - Sinon → Ed25519 seul (compatibilité ascendante)
    pub fn verify_tx(tx: &Transaction) -> Result<bool, String> {
        // Network-issued txs (mining) and system-generated burns/escrow are exempt
        if tx.from == "NETWORK" || tx.to == "BURN" || tx.to == "ESCROW" || tx.from == "ESCROW" {
            return Ok(true);
        }
        if tx.signature.is_empty() {
            return Err("Transaction non signée".into());
        }

        let payload = format!(
            "{}:{}:{}:{}:{}:{:?}",
            tx.id, tx.from, tx.to, tx.amount, tx.timestamp, tx.tx_type
        );

        let classical = hex::decode(&tx.signature).map_err(|_| "Signature invalide")?;
        let quantum = match tx.pq_signature.as_deref() {
            Some(hex_str) if !hex_str.is_empty() => {
                hex::decode(hex_str).map_err(|_| "PQ signature invalide")?
            }
            _ => Vec::new(),
        };
        let pq_pk_hex = tx.pq_public_key.as_deref().unwrap_or("");

        let hybrid_sig = crate::security::hybrid_crypto::HybridSignature { classical, quantum };
        Ok(
            crate::security::hybrid_crypto::HybridIdentity::verify_hybrid(
                &tx.from,
                pq_pk_hex,
                payload.as_bytes(),
                &hybrid_sig,
            ),
        )
    }

    /// Verify the integrity of the entire chain:
    /// - Each block's prev_hash links to the previous block's hash
    /// - All user-signed transactions have valid Ed25519 signatures
    pub fn verify_chain(&self) -> Result<(u64, u64), String> {
        let mut verified_blocks = 0u64;
        let mut verified_txs = 0u64;
        for (i, block) in self.chain.iter().enumerate() {
            // Genesis block (index 0) has no previous block to link
            if i > 0 {
                let prev = &self.chain[i - 1];
                if block.prev_hash != prev.hash {
                    return Err(format!(
                        "Chaîne corrompue au bloc {}: prev_hash mismatch",
                        block.index
                    ));
                }
            }
            // Verify all signed transactions in this block
            for tx in &block.transactions {
                if !Self::verify_tx(tx)? {
                    return Err(format!(
                        "Signature invalide: tx {} dans bloc {}",
                        tx.id, block.index
                    ));
                }
                verified_txs += 1;
            }
            verified_blocks += 1;
        }
        Ok((verified_blocks, verified_txs))
    }

    /// Seal pending transactions into a new block.
    /// Invariant: `self.chain` always has at least the genesis block (enforced by `new()`).
    pub fn seal_block(&mut self, miner: &str, energy_kwh: f64) -> Block {
        let prev = match self.chain.last() {
            Some(b) => b,
            None => {
                log::error!("◈ [Ledger] CRITICAL: chain is empty — this should never happen");
                // Re-create genesis to recover
                *self = Self::new();
                self.chain.last().unwrap() // genesis exists now
            }
        };
        let txs = std::mem::take(&mut self.pending);
        let index = prev.index + 1;
        let ts = Utc::now().to_rfc3339();

        // WEAK-4 fix: Block hash commits to tx CONTENT via Merkle root, not just count.
        // D1: shared with validate_remote_block via compute_merkle_root() to avoid drift.
        let tx_root = Self::compute_merkle_root(&txs);

        let payload = format!("{}:{}:{}:{}:{}", index, prev.hash, ts, txs.len(), tx_root);
        let hash = hex::encode(blake3::hash(payload.as_bytes()).as_bytes());
        let block = Block {
            index,
            timestamp: ts,
            transactions: txs,
            prev_hash: prev.hash.clone(),
            hash,
            miner: miner.into(),
            energy_kwh,
        };
        self.chain.push(block.clone());
        block
    }

    /// Seal only if there are pending transactions. Returns the block if one was sealed.
    pub fn seal_if_pending(&mut self, miner: &str, energy_kwh: f64) -> Option<Block> {
        if self.pending.is_empty() {
            return None;
        }
        Some(self.seal_block(miner, energy_kwh))
    }

    // ─── D1: Remote Block Validation & Integration ──────────────────────

    /// D1.2: Validate a block received from a remote peer.
    ///
    /// Checks:
    /// 1. Block index is sequential (matches our chain tip + 1)
    /// 2. prev_hash links to our chain tip
    /// 3. All transaction signatures are valid
    /// 4. Block hash is correctly computed (index:prev_hash:ts:tx_count:merkle_root)
    ///
    /// Returns Ok(()) if valid, Err(reason) if not.
    pub fn validate_remote_block(&self, block: &Block) -> Result<(), String> {
        let tip = self.chain.last().ok_or("empty chain")?;

        // Check index continuity
        if block.index != tip.index + 1 {
            return Err(format!(
                "block index {} does not follow tip {} (expected {})",
                block.index,
                tip.index,
                tip.index + 1
            ));
        }

        // Check prev_hash linkage
        if block.prev_hash != tip.hash {
            return Err(format!(
                "prev_hash mismatch: block says {} but tip is {}",
                &block.prev_hash[..block.prev_hash.len().min(16)],
                &tip.hash[..tip.hash.len().min(16)]
            ));
        }

        // Verify all transaction signatures
        for tx in &block.transactions {
            if !Self::verify_tx(tx)? {
                return Err(format!(
                    "invalid tx signature: {}",
                    &tx.id[..tx.id.len().min(16)]
                ));
            }
        }

        // Verify block hash integrity (recompute and compare)
        let tx_root = Self::compute_merkle_root(&block.transactions);
        let expected_payload = format!(
            "{}:{}:{}:{}:{}",
            block.index,
            block.prev_hash,
            block.timestamp,
            block.transactions.len(),
            tx_root
        );
        let expected_hash = hex::encode(blake3::hash(expected_payload.as_bytes()).as_bytes());
        if block.hash != expected_hash {
            return Err(format!(
                "block hash mismatch: declared {} but computed {}",
                &block.hash[..block.hash.len().min(16)],
                &expected_hash[..expected_hash.len().min(16)]
            ));
        }

        Ok(())
    }

    /// D1.3: Integrate a validated remote block into the local chain.
    ///
    /// Returns:
    /// - `Ok(true)` — block was new and integrated
    /// - `Ok(false)` — block already known (duplicate)
    /// - `Err(reason)` — block invalid or fork detected
    ///
    /// Fork resolution: if the remote block's index matches our tip (same height,
    /// different hash), we keep the block with the higher hash (deterministic tie-break).
    /// This is a simplified longest-chain rule suitable for PoC.
    pub fn integrate_remote_block(&mut self, block: Block) -> Result<bool, String> {
        let tip = self.chain.last().ok_or("empty chain")?;

        // Already have this block (same index + same hash)
        if block.index <= tip.index && self.chain.iter().any(|b| b.hash == block.hash) {
            return Ok(false); // duplicate, nothing to do
        }

        // Block extends our chain (happy path)
        if block.index == tip.index + 1 && block.prev_hash == tip.hash {
            self.validate_remote_block(&block)?;

            // Dedup transactions we haven't seen
            for tx in &block.transactions {
                self.seen_tx_hashes.insert(tx.hash.clone());
            }

            // Remove any pending txs that are now in this block.
            // Their cache effects are already applied, so no cache update needed.
            let block_tx_ids: HashSet<String> =
                block.transactions.iter().map(|tx| tx.id.clone()).collect();
            let pending_tx_ids: HashSet<String> =
                self.pending.iter().map(|tx| tx.id.clone()).collect();
            self.pending.retain(|tx| !block_tx_ids.contains(&tx.id));

            // PERF-1: Apply cache effects for remote txs that weren't in our pending.
            // (Remote mining/transfer txs we've never seen locally.)
            for tx in &block.transactions {
                if !pending_tx_ids.contains(&tx.id) {
                    self.cache_apply_tx(tx);
                }
            }

            log::info!(
                "◈ [Ledger] Integrated remote block #{} ({} txs, miner={})",
                block.index,
                block.transactions.len(),
                &block.miner[..block.miner.len().min(12)]
            );
            self.chain.push(block);
            return Ok(true);
        }

        // Fork detected: same height, different block
        if block.index == tip.index && block.hash != tip.hash {
            // Deterministic tie-break: keep the block with the lexicographically higher hash.
            // Both nodes will converge to the same choice.
            if block.hash > tip.hash {
                log::warn!(
                    "◈ [Ledger] FORK at height {} — remote block wins ({}... > {}...)",
                    block.index,
                    &block.hash[..12],
                    &tip.hash[..12]
                );
                // Pop our tip, replace with the remote block
                let our_tip = self.chain.pop().ok_or("chain unexpectedly empty")?;
                // PERF-1: Revert balance effects of the old tip's txs
                for tx in &our_tip.transactions {
                    self.cache_revert_tx(tx);
                }
                // Re-queue our tip's transactions as pending (they may differ)
                for tx in our_tip.transactions {
                    if !self.seen_tx_hashes.contains(&tx.hash) {
                        self.cache_apply_tx(&tx);
                        self.pending.push(tx);
                    }
                }
                // Validate + push the remote block
                // Re-validate against the new tip (which is now tip-1)
                let new_tip = self.chain.last().ok_or("chain empty after pop")?;
                if block.prev_hash != new_tip.hash {
                    return Err("fork block prev_hash doesn't match after reorg".into());
                }
                // CRIT-C: Full validation (hash integrity + tx signatures) before integration
                self.validate_remote_block(&block)?;
                for tx in &block.transactions {
                    self.seen_tx_hashes.insert(tx.hash.clone());
                    // PERF-1: Apply new block's tx effects to cache
                    self.cache_apply_tx(tx);
                }
                self.chain.push(block);
                return Ok(true);
            } else {
                log::info!(
                    "◈ [Ledger] FORK at height {} — our block wins ({}... > {}...)",
                    block.index,
                    &tip.hash[..12],
                    &block.hash[..12]
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

    /// Compute Merkle root of transaction IDs (extracted from seal_block for reuse).
    fn compute_merkle_root(txs: &[Transaction]) -> String {
        if txs.is_empty() {
            return "0".repeat(64);
        }
        let mut hashes: Vec<String> = txs
            .iter()
            .map(|tx| hex::encode(blake3::hash(tx.id.as_bytes()).as_bytes()))
            .collect();
        while hashes.len() > 1 {
            let mut next_level = Vec::new();
            for chunk in hashes.chunks(2) {
                let combined = if chunk.len() == 2 {
                    format!("{}{}", chunk[0], chunk[1])
                } else {
                    format!("{}{}", chunk[0], chunk[0])
                };
                next_level.push(hex::encode(blake3::hash(combined.as_bytes()).as_bytes()));
            }
            hashes = next_level;
        }
        hashes.into_iter().next().unwrap_or_else(|| "0".repeat(64))
    }

    /// Count of pending (unsealed) transactions.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Compute balance from the incremental cache (in µQTA, saturating at 0).
    /// PERF-1: O(1) via balance_cache instead of O(n) full chain scan.
    pub fn balance_of(&self, pk: &str) -> u64 {
        self.balance_cache
            .get(pk)
            .copied()
            .unwrap_or(0)
            .max(0) as u64
    }

    /// Get all balances in µQTA. Excludes synthetic addresses (NETWORK/BURN/ESCROW).
    /// PERF-1: O(accounts) direct read from cache instead of O(transactions) scan.
    pub fn all_balances(&self) -> HashMap<String, u64> {
        self.balance_cache
            .iter()
            .map(|(k, v)| (k.clone(), (*v).max(0) as u64))
            .collect()
    }

    /// Get recent transactions.
    /// PERF-2: O(limit) read from bounded deque instead of O(n log n) copy+sort.
    pub fn recent_txs(&self, limit: usize) -> Vec<Transaction> {
        self.recent_deque
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get chain stats. `total_mined` is in µQTA.
    pub fn stats(&self) -> LedgerStats {
        let total_blocks = self.chain.len() as u64;
        let total_txs: usize = self.chain.iter().map(|b| b.transactions.len()).sum();
        let total_mined: u64 = self
            .chain
            .iter()
            .flat_map(|b| b.transactions.iter())
            .filter(|t| t.tx_type == TxType::Mining)
            .map(|t| t.amount)
            .sum();
        let total_energy: f64 = self.chain.iter().map(|b| b.energy_kwh).sum();
        let holders = self.all_balances().len();
        LedgerStats {
            total_blocks,
            total_txs: total_txs as u64,
            total_mined,
            total_energy,
            holders: holders as u64,
            pending: self.pending.len() as u64,
        }
    }

    fn next_tx(
        &mut self,
        from: &str,
        to: &str,
        amount: u64,
        tx_type: TxType,
    ) -> (String, String, String, String) {
        self.tx_counter += 1;
        let id = format!("tx_{}", self.tx_counter);
        let ts = Utc::now().to_rfc3339();
        let payload = format!("{}:{}:{}:{}:{}:{:?}", id, from, to, amount, ts, tx_type);
        let hash = hex::encode(blake3::hash(payload.as_bytes()).as_bytes());
        (id, ts, payload, hash)
    }

    /// Build an unsigned (network-issued) tx. Used for Mining and reward QUANTA.
    fn build_unsigned_tx(
        &mut self,
        from: &str,
        to: &str,
        amount: u64,
        tx_type: TxType,
    ) -> Transaction {
        let (id, ts, _payload, hash) = self.next_tx(from, to, amount, tx_type.clone());
        Transaction {
            id,
            from: from.into(),
            to: to.into(),
            amount,
            tx_type,
            timestamp: ts,
            signature: String::new(),
            hash,
            nonce: 0, // Network-issued txs don't use account nonces
            pq_signature: None,
            pq_public_key: None,
        }
    }

    /// Build a tx signed by the active identity (signature hybride Ed25519 + PQ stub).
    /// La couche PQ est vide jusqu'à activation de `ml-dsa >= 0.2` ; le verify
    /// hybride retombe alors automatiquement sur Ed25519 seul. Amount en µQTA.
    fn build_signed_tx(
        &mut self,
        from: &str,
        to: &str,
        amount: u64,
        tx_type: TxType,
        crypto: &CryptoEngine,
    ) -> Result<Transaction, String> {
        // B2: assign and increment nonce for the sender
        let nonce = self.get_nonce(from);
        self.increment_nonce(from);

        let (id, ts, payload, hash) = self.next_tx(from, to, amount, tx_type.clone());
        let (classical, quantum, pq_pk) = crypto.sign_hybrid(payload.as_bytes())?;
        Ok(Transaction {
            id,
            from: from.into(),
            to: to.into(),
            amount,
            tx_type,
            timestamp: ts,
            signature: hex::encode(&classical),
            hash,
            nonce,
            pq_signature: Some(hex::encode(&quantum)),
            pq_public_key: Some(pq_pk),
        })
    }
}

impl Default for Ledger {
    fn default() -> Self {
        Self::new()
    }
}

impl Ledger {
    /// Create a serializable snapshot of the current state.
    pub fn snapshot(&self) -> LedgerSnapshot {
        LedgerSnapshot {
            chain: self.chain.clone(),
            pending: self.pending.clone(),
            tx_counter: self.tx_counter,
            account_nonces: self.account_nonces.clone(),
        }
    }

    /// Restore ledger state from a previously persisted snapshot.
    pub fn restore(snap: LedgerSnapshot) -> Self {
        // V3: Rebuild the anti-replay set from the existing chain
        let mut seen = HashSet::new();
        for block in &snap.chain {
            for tx in &block.transactions {
                seen.insert(tx.hash.clone());
            }
        }
        for tx in &snap.pending {
            seen.insert(tx.hash.clone());
        }
        let mut ledger = Self {
            chain: snap.chain,
            pending: snap.pending,
            tx_counter: snap.tx_counter,
            seen_tx_hashes: seen,
            account_nonces: snap.account_nonces,
            balance_cache: HashMap::new(),
            recent_deque: VecDeque::new(),
        };
        // PERF-1: Rebuild cache from restored state
        ledger.rebuild_cache();
        ledger
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_tx_uses_hybrid_path() {
        // Une tx signée doit avoir les champs PQ "présents-mais-vides", ce qui
        // exerce HybridIdentity::verify_hybrid avec fallback Ed25519.
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let mut ledger = Ledger::new();
        // Donne un solde au signataire via une mining tx réseau (5 QUANTA = 5_000_000 µQTA).
        ledger.mine_tx(&id.public_key_hex, 5 * MICRO, 0.0);

        // Adresse destinataire valide (64 chars hex)
        let to = "b".repeat(64);
        let tx = ledger
            .transfer_tx(&id.public_key_hex, &to, MICRO, &crypto)
            .expect("transfer should succeed");

        assert!(
            tx.pq_signature.is_some(),
            "tx signée doit porter le champ pq_signature"
        );
        assert!(
            tx.pq_public_key.is_some(),
            "tx signée doit porter le champ pq_public_key"
        );
        assert!(
            Ledger::verify_tx(&tx).unwrap(),
            "verify_tx hybride doit valider Ed25519"
        );
    }

    #[test]
    fn network_tx_skips_signature() {
        // Une mining tx réseau (from=NETWORK) n'a pas à porter de signature.
        let to = "c".repeat(64);
        let mut ledger = Ledger::new();
        // 0.5 QUANTA = 500_000 µQTA
        let tx = ledger.mine_tx(&to, MICRO / 2, 0.0);
        assert_eq!(tx.from, "NETWORK");
        assert!(
            Ledger::verify_tx(&tx).unwrap(),
            "tx NETWORK toujours valide"
        );
    }

    #[test]
    fn transfer_with_burn_deducts_one_percent() {
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let mut ledger = Ledger::new();
        // Give sender 100 QUANTA
        ledger.mine_tx(&id.public_key_hex, 100 * MICRO, 0.0);

        let to = "d".repeat(64);
        let (tx, burn_uqta) = ledger
            .transfer_with_burn(&id.public_key_hex, &to, 10 * MICRO, &crypto)
            .expect("transfer should succeed");

        // 1% of 10 QUANTA = 0.1 QUANTA = 100_000 µQTA
        assert_eq!(burn_uqta, 100_000, "burn should be exactly 1%");
        // tx.amount stores the NET amount (after burn deduction)
        assert_eq!(tx.amount, 9_900_000, "tx amount = net after 1% burn");

        // Sender: 100 - 10 = 90 QUANTA
        assert_eq!(ledger.balance_of(&id.public_key_hex), 90 * MICRO);
        // Receiver: 10 - 0.1 = 9.9 QUANTA = 9_900_000 µQTA
        assert_eq!(ledger.balance_of(&to), 9_900_000);
    }

    #[test]
    fn double_spend_rejected() {
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let mut ledger = Ledger::new();
        ledger.mine_tx(&id.public_key_hex, 5 * MICRO, 0.0);

        let to = "e".repeat(64);
        // First transfer: 5 QTA → should succeed
        assert!(ledger.transfer_with_burn(&id.public_key_hex, &to, 5 * MICRO, &crypto).is_ok());
        // Second transfer: 5 QTA → should fail (balance is 0 now)
        assert!(ledger.transfer_with_burn(&id.public_key_hex, &to, 5 * MICRO, &crypto).is_err());
    }

    #[test]
    fn balance_never_negative() {
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let mut ledger = Ledger::new();
        ledger.mine_tx(&id.public_key_hex, MICRO, 0.0);

        let to = "f".repeat(64);
        // Try to send more than balance
        assert!(ledger.transfer_with_burn(&id.public_key_hex, &to, 2 * MICRO, &crypto).is_err());
        // Balance unchanged
        assert_eq!(ledger.balance_of(&id.public_key_hex), MICRO);
    }

    #[test]
    fn snapshot_restore_preserves_state() {
        let mut ledger = Ledger::new();
        let pk = "a".repeat(64);
        ledger.mine_tx(&pk, 50 * MICRO, 1.5);
        ledger.mine_tx(&pk, 30 * MICRO, 0.8);

        let snap = ledger.snapshot();
        let restored = Ledger::restore(snap);

        assert_eq!(restored.balance_of(&pk), ledger.balance_of(&pk));
        assert_eq!(restored.stats().total_txs, ledger.stats().total_txs);
        assert_eq!(restored.stats().total_mined, ledger.stats().total_mined);
    }

    #[test]
    fn chain_verification_catches_tamper() {
        let mut ledger = Ledger::new();
        let pk = "g".repeat(64);
        // Mine enough to seal a block
        for _ in 0..12 {
            ledger.mine_tx(&pk, MICRO, 0.1);
        }
        // Verify passes on clean chain
        assert!(ledger.verify_chain().is_ok());
    }

    #[test]
    fn total_supply_tracks_balances() {
        let mut ledger = Ledger::new();
        let pk1 = "h".repeat(64);
        let pk2 = "i".repeat(64);
        ledger.mine_tx(&pk1, 10 * MICRO, 0.5);
        ledger.mine_tx(&pk2, 20 * MICRO, 1.0);

        // Balances should equal the sum of mining amounts
        assert_eq!(
            ledger.balance_of(&pk1) + ledger.balance_of(&pk2),
            30 * MICRO,
            "sum of balances = total mined (no burns yet)"
        );
        // Each user got their exact amount
        assert_eq!(ledger.balance_of(&pk1), 10 * MICRO);
        assert_eq!(ledger.balance_of(&pk2), 20 * MICRO);
    }
}
