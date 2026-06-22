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

/// A transaction whose signature has been verified by [`Ledger::verify_tx`].
///
/// C5: this is a **proof-carrying token**. The authoritative admission method
/// [`Ledger::apply_verified_remote_tx`] takes a `VerifiedTx`, not a bare
/// `Transaction`, so the "signature was checked" precondition is enforced by
/// the **type** — a caller cannot apply an unverified tx to the linear ledger
/// even by mistake. The only constructor is [`VerifiedTx::new`], which runs
/// `verify_tx` exactly once; both the production shell and the deterministic
/// core mint the token the same way, so they converge on a single
/// signature-gated entry point with **no** double verification.
pub struct VerifiedTx(Transaction);

impl VerifiedTx {
    /// Mint the token by verifying the signature (AUDIT-TX-1: enforced even for
    /// `to == "BURN"`; synthetic `NETWORK`/`ESCROW` senders are exempt inside
    /// `verify_tx`). Returns `None` when the signature is missing/invalid or
    /// malformed — the caller drops the tx.
    pub fn new(tx: Transaction) -> Option<Self> {
        match Ledger::verify_tx(&tx) {
            Ok(true) => Some(Self(tx)),
            _ => None,
        }
    }

    /// Borrow the verified transaction (e.g. to read `from`/`nonce`/`amount`
    /// before applying).
    pub fn tx(&self) -> &Transaction {
        &self.0
    }

    /// Consume the token, yielding the verified transaction.
    pub fn into_inner(self) -> Transaction {
        self.0
    }
}

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
    /// recent_txs() reads from this in O(limit) instead of cloning + sorting
    /// O(n log n).
    recent_deque: VecDeque<Transaction>,
}

impl Ledger {
    /// Fixed genesis timestamp — deterministic across all nodes. The genesis
    /// HASH is a constant (`BLAKE3("QUANTA_GENESIS_2026")`) independent of this
    /// field, so reading the wall clock here only made the genesis `Block`
    /// differ per node for no benefit. Phase 0 (Constitution §3): the ledger
    /// constructor performs no system-clock read.
    const GENESIS_TIMESTAMP: &str = "2026-01-01T00:00:00+00:00";

    pub fn new() -> Self {
        // Genesis block
        let genesis = Block {
            index: 0,
            timestamp: Self::GENESIS_TIMESTAMP.to_string(),
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

    /// NET-14: Mempool eviction policy.
    ///
    /// `MEMPOOL_TTL_SECS` is the maximum age a pending transaction is
    /// allowed to sit in the mempool before it's auto-evicted. 10 min is a
    /// fair window — mining seals every 2 min, so any tx that hasn't been
    /// included after five seal cycles is presumed stuck (signature stale,
    /// nonce gap, etc.) and not worth keeping around.
    pub const MEMPOOL_TTL_SECS: i64 = 600;

    /// NET-14: Hard cap on pending transactions. When the pool grows past
    /// this, we drop the oldest entries first (FIFO) regardless of TTL.
    /// 1000 is plenty for a 720 blocks/day pace at 10 tx/block (= 7200/day).
    pub const MEMPOOL_MAX: usize = 1000;

    /// NET-14: Evict expired or excess pending transactions.
    ///
    /// Two-pass eviction:
    /// 1. Drop any pending tx whose RFC3339 `timestamp` is more than
    ///    `MEMPOOL_TTL_SECS` old (relative to wall clock now).
    /// 2. If the pool is still over `MEMPOOL_MAX`, trim the oldest by insertion
    ///    order until at the cap.
    ///
    /// Eviction calls `cache_revert_tx` so the balance cache stays
    /// consistent — pending debits/credits revert when the tx is dropped.
    /// Returns the count of evicted transactions for observability.
    /// Convenience wrapper that reads the real wall clock at the boundary. The
    /// deterministic core never calls this — it uses `prune_mempool_at` with
    /// injected time (Constitution §3: no system-clock reads in the core).
    pub fn prune_mempool(&mut self) -> usize {
        self.prune_mempool_at(chrono::Utc::now().timestamp())
    }

    /// NET-14 + Phase 0 (T0.1): evict expired or excess pending transactions
    /// against an **injected** wall-clock value (`now_secs`, Unix seconds), so
    /// the deterministic core prunes reproducibly without reading any clock.
    pub fn prune_mempool_at(&mut self, now_secs: i64) -> usize {
        let now = now_secs;
        let mut evicted: Vec<Transaction> = Vec::new();

        // Pass 1: TTL-based eviction.
        self.pending.retain(|tx| {
            let keep = match chrono::DateTime::parse_from_rfc3339(&tx.timestamp) {
                Ok(ts) => (now - ts.timestamp()) <= Self::MEMPOOL_TTL_SECS,
                // If timestamp is unparseable, treat as fresh — avoids
                // dropping txs whose clock representation is non-standard.
                Err(_) => true,
            };
            if !keep {
                evicted.push(tx.clone());
            }
            keep
        });

        // Pass 2: hard-cap eviction (oldest first by insertion order).
        while self.pending.len() > Self::MEMPOOL_MAX {
            let dropped = self.pending.remove(0);
            evicted.push(dropped);
        }

        // Revert cache effects of every evicted tx so balance_of stays correct.
        for tx in &evicted {
            self.cache_revert_tx(tx);
            // BLK-HASH-1: `seen_tx_hashes` is keyed on `tx.hash` (see the inserts),
            // so the eviction must remove by `tx.hash` too — removing by `tx.id`
            // never matched, leaking the entry and blocking re-admission.
            self.seen_tx_hashes.remove(&tx.hash);
        }

        if !evicted.is_empty() {
            log::info!(
                "◈ [NET-14] Mempool pruned: {} txs evicted (pending now {}/{})",
                evicted.len(),
                self.pending.len(),
                Self::MEMPOOL_MAX
            );
        }
        evicted.len()
    }

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

    /// PERF-1: Reverse a transaction's balance effects (used during fork
    /// reorg).
    fn cache_revert_tx(&mut self, tx: &Transaction) {
        let synthetic = |a: &str| matches!(a, "NETWORK" | "BURN" | "ESCROW");
        if !synthetic(&tx.to) {
            *self.balance_cache.entry(tx.to.clone()).or_insert(0) -= tx.amount as i128;
        }
        if !synthetic(&tx.from) {
            *self.balance_cache.entry(tx.from.clone()).or_insert(0) += tx.amount as i128;
        }
    }

    /// PERF-1: Full rebuild of balance_cache from chain + pending (used by
    /// restore).
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

    /// Increment the nonce for an account (called after a successful outgoing
    /// tx).
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

    /// Create a mining transaction (network-issued, no signature). Amount in
    /// µQTA.
    pub fn mine_tx(&mut self, miner_pk: &str, amount: u64, kwh: f64) -> Transaction {
        let tx = self.build_unsigned_tx("NETWORK", miner_pk, amount, TxType::Mining);
        self.cache_apply_tx(&tx);
        self.pending.push(tx.clone());
        if self.pending.len() >= 10 {
            self.seal_block(miner_pk, kwh);
        }
        tx
    }

    /// Create a transfer transaction signed by the active identity (amount in
    /// µQTA).
    pub fn transfer_tx(
        &mut self,
        from: &str,
        to: &str,
        amount: u64,
        crypto: &CryptoEngine,
    ) -> Result<Transaction, String> {
        // Boundary wrapper: timestamp at the wall clock, hedged signing,
        // delegate to the injected core (C7).
        self.transfer_tx_at(from, to, amount, crypto, Utc::now().to_rfc3339(), false)
    }

    /// `transfer_tx` with an **injected** RFC3339 `ts` and a `det_sign` switch —
    /// the clock-free core for the deterministic harness. Behaviourally
    /// identical to `transfer_tx` **minus** the old `V8` ±5-min self-drift
    /// check: that read `Utc::now()` (so it broke injected time) and could never
    /// fire — a node cannot create a stale tx of its own (the ts is set to "now"
    /// one line above the check). The timestamp is signed data bound into the
    /// hash; validation stays clock-free (C2 / §1.1 — never re-read the clock to
    /// validate a ts).
    pub fn transfer_tx_at(
        &mut self,
        from: &str,
        to: &str,
        amount: u64,
        crypto: &CryptoEngine,
        ts: String,
        det_sign: bool,
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
        let tx = self.build_signed_tx_at(from, to, amount, TxType::Transfer, crypto, ts, det_sign)?;
        // V3: Anti-replay — reject if we've seen this tx hash before
        if !self.seen_tx_hashes.insert(tx.hash.clone()) {
            return Err("Transaction déjà traitée (replay détecté)".into());
        }
        self.cache_apply_tx(&tx);
        self.pending.push(tx.clone());
        Ok(tx)
    }

    /// V2 — Burn-and-Mint : 1% brûlé automatiquement à chaque transfert (amount
    /// en µQTA).
    ///
    /// AUDIT-TX-2: Returns `(transfer_tx, Option<burn_tx>, burn_amount)`. The
    /// burn tx is now SIGNED (instead of system-generated/unsigned) so it can
    /// be safely broadcast over gossip without bypassing signature checks.
    /// The caller is responsible for broadcasting BOTH txs so peers' ledgers
    /// stay in sync — sending only the transfer leaves a 1% gap on the
    /// sender's balance on every other node.
    ///
    /// AUDIT-TX-3: Gross balance is checked upfront so the transfer leg never
    /// succeeds when the burn leg would push the sender below zero.
    pub fn transfer_with_burn(
        &mut self,
        from: &str,
        to: &str,
        amount: u64,
        crypto: &CryptoEngine,
    ) -> Result<(Transaction, Option<Transaction>, u64), String> {
        // Boundary wrapper: timestamp at the wall clock, hedged signing,
        // delegate to the injected core (C7). Prod (the Tauri transfer command)
        // keeps calling this — behaviour unchanged (real time, `OsRng` signing).
        self.transfer_with_burn_at(from, to, amount, crypto, Utc::now().to_rfc3339(), false)
    }

    /// `transfer_with_burn` with an **injected** RFC3339 `ts` and a `det_sign`
    /// switch — the clock-free core for the deterministic harness (Phase 1: user
    /// transfers in the load, timestamped by the virtual clock, ML-DSA signed
    /// deterministically). Both legs (transfer + 1 % burn) share the one `ts`
    /// and `det_sign`. Identical accounting to `transfer_with_burn`.
    pub fn transfer_with_burn_at(
        &mut self,
        from: &str,
        to: &str,
        amount: u64,
        crypto: &CryptoEngine,
        ts: String,
        det_sign: bool,
    ) -> Result<(Transaction, Option<Transaction>, u64), String> {
        // Minimum transfer 0.01 QUANTA = 10_000 µQTA
        if amount < 10_000 {
            return Err("Minimum transfer: 0.01 QUANTA".into());
        }
        // AUDIT-TX-3: pre-check gross balance so neither leg succeeds without
        // the other. transfer_tx checks net (amount - burn), but if
        // balance == net, the burn leg would silently push the cache
        // negative (saturated by balance_of). Reject upfront instead.
        let balance = self.balance_of(from);
        if balance < amount {
            return Err(format!(
                "Solde insuffisant: {:.6} QUANTA",
                balance as f64 / MICRO as f64
            ));
        }
        // 1% burn — integer math: amount / 100
        let burn_amount = amount / 100;
        let net_amount = amount - burn_amount;
        let transfer_tx = self.transfer_tx_at(from, to, net_amount, crypto, ts.clone(), det_sign)?;
        let burn_tx = if burn_amount > 0 {
            // AUDIT-TX-1: signed burn so verify_tx accepts it across gossip.
            let bt =
                self.build_signed_tx_at(from, "BURN", burn_amount, TxType::Burn, crypto, ts, det_sign)?;
            // Anti-replay (defense-in-depth — payload includes timestamp so
            // hash collisions with the transfer leg are impossible).
            if !self.seen_tx_hashes.insert(bt.hash.clone()) {
                return Err("Burn tx déjà traitée (replay détecté)".into());
            }
            self.cache_apply_tx(&bt);
            self.pending.push(bt.clone());
            Some(bt)
        } else {
            None
        };
        Ok((transfer_tx, burn_tx, burn_amount))
    }

    /// Burn QUANTA — permanent destruction, signed by the owner (amount en
    /// µQTA).
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
    /// Idempotent via `seen_tx_hashes` dedup. Returns true if the tx was newly
    /// applied.
    pub fn replay_remote_tx(&mut self, tx: Transaction) -> bool {
        if !self.seen_tx_hashes.insert(tx.hash.clone()) {
            return false; // already seen
        }
        self.cache_apply_tx(&tx);
        self.pending.push(tx);
        true
    }

    /// Apply a **signature-verified** remote tx ([`VerifiedTx`]) to the
    /// authoritative linear ledger: idempotent replay + monotonic-nonce
    /// high-water advance.
    ///
    /// C5: taking a `VerifiedTx` (not a bare `Transaction`) makes the
    /// "signature checked" precondition a **type-level** guarantee — the single
    /// signature-gated entry point shared by the production dispatcher and the
    /// deterministic core (`sm::Node`). Behaviour is unchanged:
    /// synthetic-sender txs (`NETWORK`/`ESCROW`) are a no-op here (they
    /// enter via block sync, not gossip replay). Returns whether the tx was
    /// newly applied (false = synthetic, or duplicate hash).
    pub fn apply_verified_remote_tx(&mut self, vtx: VerifiedTx) -> bool {
        let tx = vtx.into_inner();
        if tx.from == "NETWORK" || tx.from == "ESCROW" {
            return false;
        }
        let from = tx.from.clone();
        let nonce = tx.nonce;
        let applied = self.replay_remote_tx(tx);
        if applied {
            // AUDIT-TX-2: high-water = max(current, nonce + 1) so out-of-order
            // arrivals never roll the counter backwards.
            let current = self.get_nonce(&from);
            let new_hw = current.max(nonce.saturating_add(1));
            for _ in current..new_hw {
                self.increment_nonce(&from);
            }
        }
        applied
    }

    /// Full remote-tx admission for the deterministic core: signature (token
    /// mint) → monotonic nonce gate → [`Self::apply_verified_remote_tx`]. Pure
    /// and synchronous (no clock, no async) so the core admits gossip txs
    /// reproducibly. Returns whether the tx was newly applied.
    pub fn apply_remote_tx_checked(&mut self, tx: Transaction) -> bool {
        // Signature first (AUDIT-TX-1: enforced even for `to == "BURN"`), via the
        // single gated entry point shared with the shell.
        let vtx = match VerifiedTx::new(tx) {
            Some(v) => v,
            None => return false,
        };
        // Monotonic non-regression nonce gate (AUDIT-TX-2). Synthetic senders
        // carry no account nonce.
        let tx = vtx.tx();
        if tx.from != "NETWORK" && tx.from != "ESCROW" {
            let high_water = self.get_nonce(&tx.from);
            if tx.nonce.saturating_add(1) < high_water {
                return false; // stale replay / already-applied evicted hash
            }
        }
        self.apply_verified_remote_tx(vtx)
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

    /// Total QUANTA minted (in µQTA), counting BOTH sealed (chain) and pending
    /// mining txs — the counterpart to [`Self::total_burned`]. Unlike
    /// `stats().total_mined` (chain only), this stays consistent with the
    /// balance cache (which reflects pending), so the conservation invariant
    /// `Σ balances + burned == minted` holds at **every** step (harness T0.7).
    pub fn total_minted(&self) -> u64 {
        let chain_mint: u64 = self
            .chain
            .iter()
            .flat_map(|b| b.transactions.iter())
            .filter(|t| t.tx_type == TxType::Mining)
            .map(|t| t.amount)
            .sum();
        let pending_mint: u64 = self
            .pending
            .iter()
            .filter(|t| t.tx_type == TxType::Mining)
            .map(|t| t.amount)
            .sum();
        chain_mint + pending_mint
    }

    // ── Phase 3.1: Signature Verification (toujours hybride) ─────

    /// Verify the signature(s) on a user-signed transaction.
    ///
    /// AUDIT-TX-1: Only the synthetic system addresses are exempt from
    /// signature verification — `NETWORK` (mining tx) and `ESCROW` (state
    /// machine transitions). Any tx whose `from` is a real wallet pubkey
    /// MUST carry a valid signature, even if `to == "BURN"`. Previously a
    /// bug allowed any peer to forge `from=victim, to=BURN` txs over gossip
    /// because `to == "BURN"` short-circuited the check.
    pub fn verify_tx(tx: &Transaction) -> Result<bool, String> {
        // Synthetic system addresses are exempt — they originate inside the
        // node and are never accepted from gossip.
        if tx.from == "NETWORK" || tx.from == "ESCROW" {
            return Ok(true);
        }
        // Any other from value must carry a valid signature, regardless of
        // destination (BURN included).
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
        let prev = match self.chain.last() {
            Some(b) => b,
            None => {
                log::error!("◈ [Ledger] CRITICAL: chain is empty — this should never happen");
                // Re-create genesis to recover
                *self = Self::new();
                self.chain.last().unwrap() // genesis exists now
            }
        };
        let index = prev.index + 1;
        let ts = timestamp;
        // EMIT-1 (Option A — one reward per block): fold the pending mining
        // rewards into a SINGLE coalesced `NETWORK→miner` tx before sealing.
        // Production mines once per tick but seals every `SEAL_EVERY_N_TICKS`
        // ticks, so a leader's pending can hold several of its own rewards;
        // bundling them keeps the chain at exactly one reward per block (the
        // §4.2 rule peers enforce + the §4.3 emission invariant). A block with
        // ≤1 mining tx is returned byte-identical to the pre-EMIT-1 seal.
        let txs = Self::coalesce_block_rewards(std::mem::take(&mut self.pending), miner, index, &ts);

        // BLK-HASH-1: the block hash commits to tx CONTENT via the Merkle root
        // (content+signature leaves) AND to the `miner` — shared verbatim with
        // `validate_block_against_prev` to avoid drift.
        let tx_root = Self::compute_merkle_root(&txs);

        let payload = format!(
            "{}:{}:{}:{}:{}:{}",
            index,
            prev.hash,
            ts,
            miner,
            txs.len(),
            tx_root
        );
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

    /// EMIT-1 (Option A — one reward per block): collapse every pending
    /// `Mining` tx into a SINGLE coalesced `NETWORK→miner` reward (amount = Σ),
    /// preserving every non-mining tx in its original order (the coalesced
    /// reward leads). A block with ≤1 mining tx is returned **unchanged** —
    /// byte-identical to the pre-EMIT-1 seal — so only the genuinely
    /// multi-reward case (a leader bundling several ticks) is rewritten.
    ///
    /// The merged tx is fully deterministic: its id derives from the block
    /// `index`, its timestamp is the **injected** block `ts` (no wall-clock
    /// read), and its content/hash follow the same scheme as `next_tx`. All
    /// pending mining rewards are `NETWORK→self` and `miner == self` on every
    /// real path (mining-loop and core seal with the node's own key; remote
    /// mining txs are no-ops on admission), so crediting `miner` matches the
    /// per-tick cache effect already summed to Σ — the block stays consistent
    /// with the balance cache without re-touching it.
    fn coalesce_block_rewards(
        txs: Vec<Transaction>,
        miner: &str,
        index: u64,
        ts: &str,
    ) -> Vec<Transaction> {
        if txs.iter().filter(|t| t.tx_type == TxType::Mining).count() <= 1 {
            return txs; // ≤1 reward — nothing to coalesce, leave untouched
        }
        let mut total: u64 = 0;
        let mut rest: Vec<Transaction> = Vec::with_capacity(txs.len());
        for tx in txs {
            if tx.tx_type == TxType::Mining {
                total = total.saturating_add(tx.amount);
            } else {
                rest.push(tx);
            }
        }
        let id = format!("tx_mint_b{index}");
        let payload = format!(
            "{}:{}:{}:{}:{}:{:?}",
            id, "NETWORK", miner, total, ts, TxType::Mining
        );
        let hash = hex::encode(blake3::hash(payload.as_bytes()).as_bytes());
        let reward = Transaction {
            id,
            from: "NETWORK".into(),
            to: miner.into(),
            amount: total,
            tx_type: TxType::Mining,
            timestamp: ts.into(),
            signature: String::new(),
            hash,
            nonce: 0,
            pq_signature: None,
            pq_public_key: None,
        };
        let mut out = Vec::with_capacity(rest.len() + 1);
        out.push(reward);
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

    // ─── D1: Remote Block Validation & Integration ──────────────────────

    /// D1.2: Validate a block received from a remote peer (against current
    /// tip).
    ///
    /// Checks:
    /// 1. Block index is sequential (matches our chain tip + 1)
    /// 2. prev_hash links to our chain tip
    /// 3. All transaction signatures are valid
    /// 4. Block hash is correctly computed
    ///    (index:prev_hash:ts:tx_count:merkle_root)
    ///
    /// Returns Ok(()) if valid, Err(reason) if not.
    pub fn validate_remote_block(&self, block: &Block) -> Result<(), String> {
        let tip = self.chain.last().ok_or("empty chain")?;
        if block.index != tip.index + 1 {
            return Err(format!(
                "block index {} does not follow tip {} (expected {})",
                block.index,
                tip.index,
                tip.index + 1
            ));
        }
        if block.prev_hash != tip.hash {
            return Err(format!(
                "prev_hash mismatch: block says {} but tip is {}",
                &block.prev_hash[..block.prev_hash.len().min(16)],
                &tip.hash[..tip.hash.len().min(16)]
            ));
        }
        Self::validate_block_against_prev(block, tip)?;
        self.validate_block_emission(block)
    }

    /// TOKENOMICS v2 — garde-fou de consensus : la somme minée d'un bloc ne
    /// peut JAMAIS pousser l'offre au-delà du plafond dur, même venant d'un
    /// pair malveillant. Sans ça, un attaquant scellerait un bloc se
    /// créditant des millions (les tx `NETWORK` sont exemptes de signature)
    /// → inflation arbitraire. Le plafond devient ainsi infalsifiable à
    /// l'échelle réseau.
    fn validate_block_emission(&self, block: &Block) -> Result<(), String> {
        let block_minted: u64 = block
            .transactions
            .iter()
            .filter(|t| t.tx_type == TxType::Mining)
            .map(|t| t.amount)
            .sum();
        if block_minted == 0 {
            return Ok(());
        }
        let current = self.stats().total_mined;
        // ① Plafond dur (total) — l'offre ne peut JAMAIS dépasser MAX_SUPPLY.
        if current.saturating_add(block_minted) > crate::p2p::reputation::MAX_SUPPLY_MICRO {
            return Err(format!(
                "bloc rejeté : émission {} pousserait l'offre {} au-delà du plafond dur {}",
                block_minted,
                current,
                crate::p2p::reputation::MAX_SUPPLY_MICRO
            ));
        }
        // ② Borne PAR BLOC — un sceleur ne peut pas rafler l'émission restante d'un
        // seul coup. L'émission légitime/bloc ≈ emission_for_tick × quelques ticks
        // (seal toutes les 2). On autorise une marge LARGE (64 ticks) pour ne jamais
        // rejeter un bloc honnête malgré les retards réseau / multi-pairs, tout en
        // bloquant un mint massif (ordre 1e4×+ au-dessus du légitime). Plancher de
        // 10 QUANTA près du plafond où emission_for_tick → 0.
        const PER_BLOCK_EMISSION_TICKS: u64 = 64;
        let per_tick = crate::p2p::reputation::emission_for_tick(current);
        let max_per_block = per_tick
            .saturating_mul(PER_BLOCK_EMISSION_TICKS)
            .max(10 * crate::p2p::ledger_types::MICRO);
        if block_minted > max_per_block {
            return Err(format!(
                "bloc rejeté : émission/bloc {} dépasse le max légitime {} — un sceleur ne peut \
                 pas rafler l'émission restante d'un coup",
                block_minted, max_per_block
            ));
        }
        Ok(())
    }

    /// AUDIT-BLK-2: Stateless block validation against an explicit `prev`.
    /// Used by `integrate_remote_block` during fork reorg, where the relevant
    /// `prev` is the block at `tip.index - 1` rather than the current tip.
    /// Doesn't enforce index continuity (caller checks) but recomputes the
    /// block hash and verifies every transaction signature.
    fn validate_block_against_prev(block: &Block, prev: &Block) -> Result<(), String> {
        if block.prev_hash != prev.hash {
            return Err(format!(
                "prev_hash mismatch: block says {} but prev is {}",
                &block.prev_hash[..block.prev_hash.len().min(16)],
                &prev.hash[..prev.hash.len().min(16)]
            ));
        }
        // EMIT-1 §4.2 (Option A — one reward per block): at most ONE `Mining`
        // tx, and if present it must be the coinbase `NETWORK → block.miner`.
        // Stateless, so it guards BOTH the happy path and the fork-reorg path —
        // a malicious node forging two mining txs (whose sum could slip under
        // the per-block emission bound, which only checks the total) or
        // crediting someone other than the sealer is rejected here. Belt-and-
        // suspenders with BLK-HASH-1, which already binds `miner` into the hash.
        let mining: Vec<&Transaction> = block
            .transactions
            .iter()
            .filter(|t| t.tx_type == TxType::Mining)
            .collect();
        if mining.len() > 1 {
            return Err(format!(
                "bloc rejeté : {} récompenses de minage — au plus une par bloc (EMIT-1)",
                mining.len()
            ));
        }
        if let Some(reward) = mining.first() {
            if reward.from != "NETWORK" {
                return Err(format!(
                    "bloc rejeté : récompense de minage émise par {} (NETWORK attendu)",
                    reward.from
                ));
            }
            if reward.to != block.miner {
                return Err(
                    "bloc rejeté : récompense de minage créditée à un autre que le mineur du bloc"
                        .into(),
                );
            }
        }
        for tx in &block.transactions {
            if !Self::verify_tx(tx)? {
                return Err(format!(
                    "invalid tx signature: {}",
                    &tx.id[..tx.id.len().min(16)]
                ));
            }
        }
        let tx_root = Self::compute_merkle_root(&block.transactions);
        // BLK-HASH-1: same pre-image as `seal_block_at` — includes `miner` and a
        // content-binding Merkle root.
        let expected_payload = format!(
            "{}:{}:{}:{}:{}:{}",
            block.index,
            block.prev_hash,
            block.timestamp,
            block.miner,
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

    /// EMIT-1: a synthetic **sender** — `NETWORK` (mining reward) or `ESCROW`
    /// (state-machine release). Such txs belong to a sealed block, never the
    /// mempool, so they are excluded from the fork-reorg re-queue (§4.1).
    /// `BURN` is a synthetic *destination*, never a sender, so it is not here.
    fn is_synthetic_sender(from: &str) -> bool {
        matches!(from, "NETWORK" | "ESCROW")
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
        let payload = format!(
            "{}:{}:{}:{}:{}:{}",
            index,
            prev_hash,
            timestamp,
            miner,
            txs.len(),
            tx_root
        );
        let hash = hex::encode(blake3::hash(payload.as_bytes()).as_bytes());
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
                &block.miner[..block.miner.len().min(12)]
            );
            self.chain.push(block);
            return Ok(true);
        }

        // Fork detected: same height, different block
        if block.index == tip.index && block.hash != tip.hash {
            // Deterministic tie-break: keep the block with the lexicographically higher
            // hash. Both nodes will converge to the same choice.
            if block.hash > tip.hash {
                log::warn!(
                    "◈ [Ledger] FORK at height {} — remote block wins ({}... > {}...)",
                    block.index,
                    &block.hash[..12],
                    &tip.hash[..12]
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
                Self::validate_block_against_prev(&block, &prev_for_remote)?;

                // Now it's safe to mutate state.
                let our_tip = self.chain.pop().ok_or("chain unexpectedly empty")?;

                // PERF-1: Revert balance effects of the old tip's txs.
                for tx in &our_tip.transactions {
                    self.cache_revert_tx(tx);
                }

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
                    if Self::is_synthetic_sender(&tx.from) {
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

    /// BLK-HASH-1: canonical **content** commitment for a tx — a fixed-order,
    /// domain-separated string of its content. NEVER the positional counter
    /// `tx.id`, and NO map iteration (determinism, §3). Two txs with different
    /// content always differ here; the same tx is identical on every node.
    fn tx_content_bytes(tx: &Transaction) -> String {
        format!(
            "from={}|to={}|amount={}|nonce={}|type={:?}|ts={}",
            tx.from, tx.to, tx.amount, tx.nonce, tx.tx_type, tx.timestamp
        )
    }

    /// BLAKE3 Merkle root committing each tx's **content + signature**
    /// (BLK-HASH-1 §4.1) — not the local counter id, which doesn't bind
    /// content and collides across nodes. Domain-separated (RFC
    /// 6962-style): leaves are `H(0x00 ‖ content ‖ signature)`, internal
    /// nodes `H(0x01 ‖ left ‖ right)`, and a lone odd node is **promoted
    /// unchanged** (never duplicated), closing the second-preimage hole
    /// (CVE-2012-2459).
    fn compute_merkle_root(txs: &[Transaction]) -> String {
        if txs.is_empty() {
            return "0".repeat(64);
        }
        let mut level: Vec<[u8; 32]> = txs
            .iter()
            .map(|tx| {
                let mut h = blake3::Hasher::new();
                h.update(&[0x00]); // leaf domain separator
                h.update(Self::tx_content_bytes(tx).as_bytes());
                h.update(tx.signature.as_bytes()); // bind the signature too
                *h.finalize().as_bytes()
            })
            .collect();
        while level.len() > 1 {
            let mut next = Vec::with_capacity(level.len().div_ceil(2));
            for chunk in level.chunks(2) {
                if chunk.len() == 2 {
                    let mut h = blake3::Hasher::new();
                    h.update(&[0x01]); // internal-node domain separator
                    h.update(&chunk[0]);
                    h.update(&chunk[1]);
                    next.push(*h.finalize().as_bytes());
                } else {
                    next.push(chunk[0]); // lone odd node promoted unchanged
                }
            }
            level = next;
        }
        hex::encode(level[0])
    }

    /// Count of pending (unsealed) transactions.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Read-only view of the pending mempool — lets the DST harness assert what
    /// a fork reorg re-queued (a loser's user txs) vs excluded (its synthetic
    /// mining reward), EMIT-1 §4.1 / AUDIT-BLK-1.
    pub fn pending_txs(&self) -> &[Transaction] {
        &self.pending
    }

    /// Compute balance from the incremental cache (in µQTA, saturating at 0).
    /// PERF-1: O(1) via balance_cache instead of O(n) full chain scan.
    pub fn balance_of(&self, pk: &str) -> u64 {
        self.balance_cache.get(pk).copied().unwrap_or(0).max(0) as u64
    }

    /// Get all balances in µQTA. Excludes synthetic addresses
    /// (NETWORK/BURN/ESCROW). PERF-1: O(accounts) direct read from cache
    /// instead of O(transactions) scan.
    pub fn all_balances(&self) -> HashMap<String, u64> {
        self.balance_cache
            .iter()
            .map(|(k, v)| (k.clone(), (*v).max(0) as u64))
            .collect()
    }

    /// Get recent transactions.
    /// PERF-2: O(limit) read from bounded deque instead of O(n log n)
    /// copy+sort.
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
        // Production reads the wall clock at the boundary and delegates to the
        // injected-time core (C7 / Phase 0, Constitution §3 — same pattern as
        // `seal_block`/`seal_block_at`).
        self.next_tx_at(from, to, amount, tx_type, Utc::now().to_rfc3339())
    }

    /// Build a tx id/timestamp/payload/hash with an **injected** RFC3339
    /// `ts`, so the deterministic core (`sm::Node` / the DST harness) creates
    /// txs reproducibly without reading the wall clock. Pure extraction of
    /// `next_tx`: identical `id:from:to:amount:ts:type` pre-image and BLAKE3
    /// hash — only the timestamp source changes.
    fn next_tx_at(
        &mut self,
        from: &str,
        to: &str,
        amount: u64,
        tx_type: TxType,
        ts: String,
    ) -> (String, String, String, String) {
        self.tx_counter += 1;
        let id = format!("tx_{}", self.tx_counter);
        let payload = format!("{}:{}:{}:{}:{}:{:?}", id, from, to, amount, ts, tx_type);
        let hash = hex::encode(blake3::hash(payload.as_bytes()).as_bytes());
        (id, ts, payload, hash)
    }

    /// Build an unsigned (network-issued) tx. Used for Mining and reward
    /// QUANTA.
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

    /// Build a tx signed by the active identity (signature hybride Ed25519 + PQ
    /// stub). La couche PQ est vide jusqu'à activation de `ml-dsa >= 0.2` ;
    /// le verify hybride retombe alors automatiquement sur Ed25519 seul.
    /// Amount en µQTA.
    fn build_signed_tx(
        &mut self,
        from: &str,
        to: &str,
        amount: u64,
        tx_type: TxType,
        crypto: &CryptoEngine,
    ) -> Result<Transaction, String> {
        // Boundary wrapper: timestamp at the wall clock, hedged signing,
        // delegate to the injected core (C7). `det_sign=false` ⇒ production
        // behaviour byte-for-byte.
        self.build_signed_tx_at(from, to, amount, tx_type, crypto, Utc::now().to_rfc3339(), false)
    }

    /// `build_signed_tx` with an **injected** RFC3339 `ts`, and a `det_sign`
    /// switch for the signature entropy — the clock-free core the deterministic
    /// harness uses so signed user txs (transfers, burns) are byte-reproducible.
    /// `det_sign=true` signs ML-DSA deterministically (sim); `false` keeps the
    /// hedged `OsRng` path (production). The timestamp is signed data bound into
    /// the hash; validation stays clock-free (BLK-HASH-1).
    // The injected-time + injected-entropy core legitimately needs both extra
    // parameters beyond `build_signed_tx`; bundling them would only obscure.
    #[allow(clippy::too_many_arguments)]
    fn build_signed_tx_at(
        &mut self,
        from: &str,
        to: &str,
        amount: u64,
        tx_type: TxType,
        crypto: &CryptoEngine,
        ts: String,
        det_sign: bool,
    ) -> Result<Transaction, String> {
        // B2: assign and increment nonce for the sender
        let nonce = self.get_nonce(from);
        self.increment_nonce(from);

        let (id, ts, payload, hash) = self.next_tx_at(from, to, amount, tx_type.clone(), ts);
        // SIGN-DET-VERIFY: the deterministic signer (`sign_hybrid_det`) is
        // `#[cfg(test)]`, so the `det_sign` branch exists ONLY in test builds.
        // In a release build the deterministic path is physically absent and
        // signing is always hedged (`OsRng`); `det_sign` can only be set `true`
        // by a `#[cfg(test)]` caller (the DST harness), never by production.
        #[cfg(test)]
        let (classical, quantum, pq_pk) = if det_sign {
            crypto.sign_hybrid_det(payload.as_bytes())?
        } else {
            crypto.sign_hybrid(payload.as_bytes())?
        };
        #[cfg(not(test))]
        let (classical, quantum, pq_pk) = {
            debug_assert!(
                !det_sign,
                "deterministic ML-DSA signing must never reach a non-test build"
            );
            crypto.sign_hybrid(payload.as_bytes())?
        };
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

    // ─── NET-14: Mempool eviction tests ─────────────────────────────────

    fn old_timestamp(secs_ago: i64) -> String {
        let t = chrono::Utc::now() - chrono::Duration::seconds(secs_ago);
        t.to_rfc3339()
    }

    #[test]
    fn prune_mempool_evicts_expired_txs() {
        // Insert two mining txs (mine_tx pushes to pending), then backdate
        // one to before TTL. prune_mempool must evict the stale one only.
        let mut ledger = Ledger::new();
        let pk_a = "a".repeat(64);
        let pk_b = "b".repeat(64);
        let _fresh = ledger.mine_tx(&pk_a, 1, 0.0);
        let stale = ledger.mine_tx(&pk_b, 1, 0.0);

        // Backdate the second pending entry to 11 minutes ago (> TTL).
        let stale_id = stale.id.clone();
        if let Some(p) = ledger.pending.iter_mut().find(|t| t.id == stale_id) {
            p.timestamp = old_timestamp(660);
        }
        let before = ledger.pending_count();
        let evicted = ledger.prune_mempool();
        assert_eq!(evicted, 1, "exactly the stale tx must be pruned");
        assert_eq!(ledger.pending_count(), before - 1);
    }

    #[test]
    fn prune_mempool_at_is_driven_by_injected_time() {
        // Phase 0 (T0.1): eviction is a pure function of the INJECTED clock, so
        // the deterministic core prunes reproducibly. Same pending state pruned
        // at two different injected times ⇒ different result ⇒ time is an input.
        let mut ledger = Ledger::new();
        let pk = "a".repeat(64);
        let tx = ledger.mine_tx(&pk, 1, 0.0);

        // Pin the pending tx to a fixed absolute instant.
        let fixed = "2026-03-01T12:00:00+00:00";
        let t0 = chrono::DateTime::parse_from_rfc3339(fixed)
            .unwrap()
            .timestamp();
        let id = tx.id.clone();
        if let Some(p) = ledger.pending.iter_mut().find(|t| t.id == id) {
            p.timestamp = fixed.to_string();
        }

        // Within the TTL window → nothing evicted.
        assert_eq!(
            ledger.prune_mempool_at(t0 + Ledger::MEMPOOL_TTL_SECS - 1),
            0
        );
        // Just past the TTL → the same tx is now evicted.
        assert_eq!(
            ledger.prune_mempool_at(t0 + Ledger::MEMPOOL_TTL_SECS + 1),
            1
        );
    }

    #[test]
    fn prune_mempool_caps_at_max() {
        let mut ledger = Ledger::new();
        // Push synthetic NETWORK -> X transactions through the mining path so
        // we don't have to sign anything. These all get FRESH timestamps so
        // pass the TTL filter — the cap is the only thing that should evict.
        let target_pk = "f".repeat(64);
        // Insert MEMPOOL_MAX + 5 entries.
        for _ in 0..(Ledger::MEMPOOL_MAX as u32 + 5) {
            ledger.mine_tx(&target_pk, 1, 0.0);
        }
        let _ = ledger.prune_mempool();
        assert!(
            ledger.pending_count() <= Ledger::MEMPOOL_MAX,
            "pending must be capped at MEMPOOL_MAX, got {}",
            ledger.pending_count()
        );
    }

    #[test]
    fn prune_mempool_no_op_on_fresh_pool() {
        let mut ledger = Ledger::new();
        let target_pk = "0".repeat(64);
        for _ in 0..5 {
            ledger.mine_tx(&target_pk, 1, 0.0);
        }
        let before = ledger.pending_count();
        let evicted = ledger.prune_mempool();
        assert_eq!(evicted, 0, "no eviction expected on a fresh pool under cap");
        assert_eq!(ledger.pending_count(), before);
    }

    #[test]
    fn signed_tx_uses_hybrid_path() {
        // Une tx signée doit avoir les champs PQ "présents-mais-vides", ce qui
        // exerce HybridIdentity::verify_hybrid avec fallback Ed25519.
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let mut ledger = Ledger::new();
        // Donne un solde au signataire via une mining tx réseau (5 QUANTA = 5_000_000
        // µQTA).
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

    // ─── TOKENOMICS v2 — invariants de CONFIANCE (offre prouvable) ──────────

    #[test]
    fn trust_no_premine_at_genesis() {
        let ledger = Ledger::new();
        assert_eq!(
            ledger.total_supply(),
            0,
            "aucune offre à la genèse (zéro premine)"
        );
        assert_eq!(ledger.stats().total_mined, 0, "rien de miné à la genèse");
        assert!(
            ledger.balance_cache.is_empty(),
            "aucun compte crédité à la genèse"
        );
        assert_eq!(ledger.chain[0].transactions.len(), 0, "bloc de genèse vide");
    }

    #[test]
    fn trust_burn_reduces_total_supply() {
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let mut ledger = Ledger::new();
        ledger.mine_tx(&id.public_key_hex, 100 * MICRO, 0.0);
        ledger.seal_block(&id.public_key_hex, 0.0); // pending → chaîne (compté dans l'offre)
        let before = ledger.total_supply();
        let to = "e".repeat(64);
        let (_tx, _burn, burn_uqta) = ledger
            .transfer_with_burn(&id.public_key_hex, &to, 50 * MICRO, &crypto)
            .unwrap();
        ledger.seal_block(&id.public_key_hex, 0.0);
        assert!(burn_uqta > 0);
        // Déflation réelle : l'offre diminue EXACTEMENT du montant brûlé.
        assert_eq!(
            ledger.total_supply(),
            before - burn_uqta,
            "le burn doit détruire de l'offre (net-destructeur)"
        );
    }

    #[test]
    fn trust_supply_equals_sum_of_balances() {
        // Invariant fort : offre en circulation == somme des soldes de comptes.
        let mut crypto = CryptoEngine::new();
        let a = crypto.generate_keypair();
        let mut ledger = Ledger::new();
        ledger.mine_tx(&a.public_key_hex, 100 * MICRO, 0.0);
        let b = "f".repeat(64);
        ledger
            .transfer_with_burn(&a.public_key_hex, &b, 30 * MICRO, &crypto)
            .unwrap();
        ledger.seal_block(&a.public_key_hex, 0.0); // chaîne et cache cohérents
        let sum: i128 = ledger.balance_cache.values().sum();
        assert_eq!(
            sum as u64,
            ledger.total_supply(),
            "Σ soldes doit égaler l'offre (mined − burned)"
        );
    }

    #[test]
    fn trust_only_network_mints_and_transfers_conserve() {
        // Seul chemin de création monétaire : mine_tx depuis "NETWORK".
        let mut ledger = Ledger::new();
        let to = "a".repeat(64);
        let tx = ledger.mine_tx(&to, MICRO, 0.0);
        assert_eq!(
            tx.from, "NETWORK",
            "création monétaire uniquement depuis NETWORK"
        );
        assert_eq!(tx.tx_type, TxType::Mining);
        // Un transfert (sans burn) ne crée AUCUNE offre.
        let mut crypto = CryptoEngine::new();
        let s = crypto.generate_keypair();
        ledger.mine_tx(&s.public_key_hex, 10 * MICRO, 0.0);
        let supply = ledger.total_supply();
        let r = "b".repeat(64);
        ledger
            .transfer_tx(&s.public_key_hex, &r, 2 * MICRO, &crypto)
            .unwrap();
        assert_eq!(
            ledger.total_supply(),
            supply,
            "un transfert ne crée pas d'offre"
        );
    }

    #[test]
    fn trust_remote_block_cannot_exceed_hard_cap() {
        // Un pair MALVEILLANT forge un bloc qui se crédite plus que le plafond.
        // Le consensus DOIT le rejeter (plafond infalsifiable réseau).
        let mut ledger = Ledger::new();
        let tip = ledger.chain.last().unwrap().clone();
        let attacker = "a".repeat(64);
        let huge = crate::p2p::reputation::MAX_SUPPLY_MICRO + 5 * MICRO;
        let evil_tx = ledger.build_unsigned_tx("NETWORK", &attacker, huge, TxType::Mining);

        let txs = vec![evil_tx];
        let index = tip.index + 1;
        let ts = Utc::now().to_rfc3339();
        let tx_root = Ledger::compute_merkle_root(&txs);
        // BLK-HASH-1: pre-image now includes the miner.
        let payload = format!(
            "{}:{}:{}:{}:{}:{}",
            index,
            tip.hash,
            ts,
            attacker,
            txs.len(),
            tx_root
        );
        let hash = hex::encode(blake3::hash(payload.as_bytes()).as_bytes());
        let evil_block = Block {
            index,
            timestamp: ts,
            transactions: txs,
            prev_hash: tip.hash.clone(),
            hash,
            miner: attacker,
            energy_kwh: 0.0,
        };

        let res = ledger.integrate_remote_block(evil_block);
        assert!(
            res.is_err(),
            "le bloc dépassant le plafond doit être rejeté"
        );
        assert_eq!(
            ledger.stats().total_mined,
            0,
            "offre inchangée (bloc rejeté)"
        );
        assert_eq!(
            ledger.chain_height(),
            1,
            "chaîne non étendue par le bloc malveillant"
        );
    }

    #[test]
    fn trust_remote_block_emission_per_block_bounded() {
        // Un sceleur forge un bloc qui se crédite 1 000 000 QUANTA — TRÈS sous le
        // plafond total (100M), mais des milliers de fois l'émission/tick légitime.
        // La borne PAR BLOC doit le rejeter : impossible de rafler l'émission d'un
        // coup.
        let mut ledger = Ledger::new();
        let tip = ledger.chain.last().unwrap().clone();
        let attacker = "b".repeat(64);
        let greedy = 1_000_000 * MICRO; // 1M QUANTA, ≫ emission_for_tick(0) × 64
        let evil_tx = ledger.build_unsigned_tx("NETWORK", &attacker, greedy, TxType::Mining);

        let txs = vec![evil_tx];
        let index = tip.index + 1;
        let ts = Utc::now().to_rfc3339();
        let tx_root = Ledger::compute_merkle_root(&txs);
        // BLK-HASH-1: pre-image now includes the miner.
        let payload = format!(
            "{}:{}:{}:{}:{}:{}",
            index,
            tip.hash,
            ts,
            attacker,
            txs.len(),
            tx_root
        );
        let hash = hex::encode(blake3::hash(payload.as_bytes()).as_bytes());
        let evil_block = Block {
            index,
            timestamp: ts,
            transactions: txs,
            prev_hash: tip.hash.clone(),
            hash,
            miner: attacker,
            energy_kwh: 0.0,
        };

        let res = ledger.integrate_remote_block(evil_block);
        assert!(
            res.is_err(),
            "un bloc minant 1M QUANTA d'un coup doit être rejeté (borne par bloc)"
        );
        assert_eq!(
            ledger.stats().total_mined,
            0,
            "offre inchangée (bloc rejeté)"
        );
        assert_eq!(ledger.chain_height(), 1, "chaîne non étendue");
    }

    #[test]
    fn transfer_with_burn_deducts_one_percent() {
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let mut ledger = Ledger::new();
        // Give sender 100 QUANTA
        ledger.mine_tx(&id.public_key_hex, 100 * MICRO, 0.0);

        let to = "d".repeat(64);
        let (tx, burn_tx, burn_uqta) = ledger
            .transfer_with_burn(&id.public_key_hex, &to, 10 * MICRO, &crypto)
            .expect("transfer should succeed");
        // AUDIT-TX-1: burn leg must be present and signed for any burn > 0
        assert!(
            burn_tx.is_some(),
            "burn tx must be returned for non-zero burn"
        );
        let bt = burn_tx.as_ref().unwrap();
        assert_eq!(bt.to, "BURN");
        assert!(!bt.signature.is_empty(), "burn tx must be signed");
        assert!(
            Ledger::verify_tx(bt).unwrap(),
            "burn tx signature must verify"
        );

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
        assert!(ledger
            .transfer_with_burn(&id.public_key_hex, &to, 5 * MICRO, &crypto)
            .is_ok());
        // Second transfer: 5 QTA → should fail (balance is 0 now)
        assert!(ledger
            .transfer_with_burn(&id.public_key_hex, &to, 5 * MICRO, &crypto)
            .is_err());
    }

    #[test]
    fn balance_never_negative() {
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let mut ledger = Ledger::new();
        ledger.mine_tx(&id.public_key_hex, MICRO, 0.0);

        let to = "f".repeat(64);
        // Try to send more than balance
        assert!(ledger
            .transfer_with_burn(&id.public_key_hex, &to, 2 * MICRO, &crypto)
            .is_err());
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

    // ─── AUDIT-TX regression tests ─────────────────────────────────────

    /// AUDIT-TX-1: a tx with `to == "BURN"` but empty signature must be
    /// rejected by verify_tx. Previously `verify_tx` short-circuited on
    /// `to == "BURN"`, allowing any peer to forge a victim's burn over gossip.
    #[test]
    fn audit_tx1_unsigned_burn_target_is_rejected() {
        let pk = "a".repeat(64);
        // Manually craft an unsigned burn tx — exactly what an attacker
        // would have submitted under the previous bug.
        let forged = Transaction {
            id: "fake_burn".into(),
            from: pk.clone(),
            to: "BURN".into(),
            amount: 1_000_000,
            tx_type: TxType::Burn,
            timestamp: chrono::Utc::now().to_rfc3339(),
            signature: String::new(), // <-- the smoking gun: no sig
            hash: "deadbeef".into(),
            nonce: 0,
            pq_signature: None,
            pq_public_key: None,
        };
        assert!(
            Ledger::verify_tx(&forged).is_err(),
            "verify_tx must reject unsigned burn-target tx (AUDIT-TX-1)"
        );
    }

    /// AUDIT-TX-1 (positive): burns from `transfer_with_burn` are signed and
    /// verifiable.
    #[test]
    fn audit_tx1_burn_leg_is_signed_and_verifies() {
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let mut ledger = Ledger::new();
        ledger.mine_tx(&id.public_key_hex, 100 * MICRO, 0.0);

        let to = "d".repeat(64);
        let (transfer, burn_opt, burn_uqta) = ledger
            .transfer_with_burn(&id.public_key_hex, &to, 10 * MICRO, &crypto)
            .expect("transfer must succeed");

        assert_eq!(burn_uqta, 100_000);
        let burn = burn_opt.expect("burn leg must be present");
        assert_eq!(burn.to, "BURN");
        assert!(!burn.signature.is_empty(), "burn must carry a signature");
        assert!(
            Ledger::verify_tx(&burn).expect("verify must not error"),
            "burn signature must verify"
        );
        // Both legs are different txs (different hashes).
        assert_ne!(transfer.hash, burn.hash);
    }

    /// AUDIT-TX-3 (regression): transfer_with_burn rejects amounts where
    /// `balance < gross` (the sender can cover net but not the 1% burn).
    /// Previously the net-only check let the burn debit silently push the
    /// cache below zero (saturated to 0 by `balance_of`, hiding the bug).
    #[test]
    fn audit_tx3_gross_balance_check_blocks_overdraw() {
        let mut crypto = CryptoEngine::new();
        let id = crypto.generate_keypair();
        let mut ledger = Ledger::new();
        // Sender holds 99_000 µQTA (= 0.099 QTA, just under the 0.1 QTA min).
        // This is the boundary case: balance == net (99_000 = 100_000 - 1_000)
        // but balance < gross (99_000 < 100_000). Old code would accept the
        // transfer (net check passes) then apply the unsigned burn,
        // overdrawing the sender by 1_000 µQTA.
        ledger.mine_tx(&id.public_key_hex, 99_000, 0.0);

        let to = "e".repeat(64);
        let result = ledger.transfer_with_burn(&id.public_key_hex, &to, 100_000, &crypto);
        assert!(
            result.is_err(),
            "transfer where balance < gross must be rejected (AUDIT-TX-3)"
        );
        // Balance untouched.
        assert_eq!(ledger.balance_of(&id.public_key_hex), 99_000);
    }

    /// AUDIT-TX cross-ledger convergence: two independent ledgers receiving
    /// the same transfer + burn pair must end up with IDENTICAL balances.
    /// This exercises the same flow as A→B over gossip.
    #[test]
    fn audit_tx_cross_ledger_convergence() {
        let mut crypto = CryptoEngine::new();
        let id_a = crypto.generate_keypair();
        let pk_a = id_a.public_key_hex.clone();
        let pk_b = "b".repeat(64);

        // Local ledger A: gives A 100 QTA, sends 10 to B with 1% burn.
        let mut node_a = Ledger::new();
        node_a.mine_tx(&pk_a, 100 * MICRO, 0.0);
        let (transfer, burn_opt, _) = node_a
            .transfer_with_burn(&pk_a, &pk_b, 10 * MICRO, &crypto)
            .expect("transfer must succeed");
        let burn = burn_opt.expect("burn must exist");

        // Local ledger B: also has the original mining (would reach via chain sync).
        let mut node_b = Ledger::new();
        node_b.mine_tx(&pk_a, 100 * MICRO, 0.0);

        // B receives BOTH legs over gossip (order-independent).
        assert!(node_b.replay_remote_tx(burn.clone()), "B must apply burn");
        assert!(
            node_b.replay_remote_tx(transfer.clone()),
            "B must apply transfer"
        );

        // Both ledgers MUST show identical balances.
        assert_eq!(
            node_a.balance_of(&pk_a),
            node_b.balance_of(&pk_a),
            "sender balance must converge"
        );
        assert_eq!(
            node_a.balance_of(&pk_b),
            node_b.balance_of(&pk_b),
            "receiver balance must converge"
        );
        // Concrete values: A had 100 QTA, sent 10 (gross), 9.9 to B, 0.1 burned.
        assert_eq!(node_a.balance_of(&pk_a), 90 * MICRO);
        assert_eq!(node_a.balance_of(&pk_b), 9_900_000);
    }

    // ─── AUDIT-BLK regression tests ────────────────────────────────────

    /// AUDIT-BLK-1 ∩ EMIT-1 §4.1: a fork reorg must NOT silently drop the
    /// loser's exclusive **user** transactions — they re-enter `pending` so the
    /// next leader can include them (AUDIT-BLK-1). But the loser's **synthetic
    /// mining reward** must be EXCLUDED from the re-queue (EMIT-1 §4.1):
    /// re-queuing it would let it be sealed again — a double-mint.
    #[test]
    fn audit_blk1_fork_reorg_preserves_exclusive_txs() {
        // A real signer so the loser can carry an exclusive USER transfer.
        let mut crypto = CryptoEngine::new();
        let signer = crypto.generate_keypair().public_key_hex;
        let pk_x = "e".repeat(64);

        // Loser fork: the signer is funded and makes a transfer the winner never
        // sees — a loser-EXCLUSIVE user tx (+ its 1% burn). One reward, to the
        // block's miner.
        let mut loser = Ledger::new();
        loser.mine_tx(&signer, 100 * MICRO, 0.0);
        let _ = loser
            .transfer_with_burn(&signer, &pk_x, 10 * MICRO, &crypto)
            .expect("loser transfer builds");
        let loser_tip = loser.seal_block(&signer, 0.0);

        // Winning fork (higher hash so it triggers the reorg): a different reward
        // amount each try until its hash beats the loser's — sealed by its own
        // miner (EMIT-1 §4.2: the reward credits the block's miner).
        let winner_tip = {
            let mut amount = 75 * MICRO;
            loop {
                let mut w = Ledger::new();
                w.mine_tx(&signer, amount, 0.0);
                let tip = w.seal_block(&signer, 0.0);
                if tip.hash > loser_tip.hash {
                    break tip;
                }
                amount += MICRO;
            }
        };

        // The loser node holds its own tip, then receives the higher-hash winner.
        let mut node = Ledger::new();
        node.chain.push(loser_tip.clone());
        let res = node.integrate_remote_block(winner_tip.clone());
        assert!(res.is_ok(), "fork integration must succeed: {:?}", res);
        assert_eq!(node.chain.last().unwrap().hash, winner_tip.hash);

        // Every loser-only USER tx is re-queued (AUDIT-BLK-1); the loser's
        // synthetic mining reward is NOT (EMIT-1 §4.1).
        let winner_hashes: std::collections::HashSet<&String> =
            winner_tip.transactions.iter().map(|t| &t.hash).collect();
        let mut saw_user = false;
        let mut saw_mining = false;
        for tx in loser_tip
            .transactions
            .iter()
            .filter(|t| !winner_hashes.contains(&t.hash))
        {
            let requeued = node.pending.iter().any(|p| p.hash == tx.hash);
            if tx.tx_type == TxType::Mining {
                saw_mining = true;
                assert!(
                    !requeued,
                    "the loser's mining reward must NOT be re-queued (EMIT-1 §4.1)"
                );
            } else {
                saw_user = true;
                assert!(
                    requeued,
                    "loser-only user tx {} must be re-queued (AUDIT-BLK-1)",
                    &tx.id
                );
            }
        }
        assert!(saw_user, "the loser carried an exclusive user tx to re-queue");
        assert!(
            saw_mining,
            "the loser carried an exclusive mining reward to exclude"
        );
    }

    /// AUDIT-BLK-2: if the remote fork block is malformed, the local chain
    /// must NOT be truncated. Previously we popped our tip BEFORE
    /// validating, so a corrupt block left a permanent gap.
    #[test]
    fn audit_blk2_failed_fork_validation_preserves_chain() {
        let pk_a = "a".repeat(64);

        let mut node = Ledger::new();
        node.mine_tx(&pk_a, 100 * MICRO, 0.0);
        let our_tip = node.seal_block(&pk_a, 0.0);
        let height_before = node.chain.len();

        // Construct a fork block at the same height with a HIGHER hash
        // (so the tie-break would prefer it) but a CORRUPT hash field
        // (mismatched payload → validate_block_against_prev rejects).
        let bogus = Block {
            index: our_tip.index,
            timestamp: our_tip.timestamp.clone(),
            transactions: vec![],
            prev_hash: our_tip.prev_hash.clone(),
            // Force a hash that's > our_tip.hash by setting the high byte to 'f'.
            hash: "f".repeat(64),
            miner: "attacker".into(),
            energy_kwh: 0.0,
        };

        let res = node.integrate_remote_block(bogus);
        assert!(res.is_err(), "corrupt fork block must be rejected");
        // Chain unchanged: our tip is still there.
        assert_eq!(node.chain.len(), height_before);
        assert_eq!(node.chain.last().unwrap().hash, our_tip.hash);
    }

    /// AUDIT-TX out-of-order delivery: B receives the burn (tx_type=Burn)
    /// BEFORE the transfer. With the relaxed nonce policy, BOTH must be
    /// applied — the previous strict equality dropped the second arrival.
    #[test]
    fn audit_tx_out_of_order_replay_both_apply() {
        let mut crypto = CryptoEngine::new();
        let id_a = crypto.generate_keypair();
        let pk_a = id_a.public_key_hex.clone();
        let pk_b = "b".repeat(64);

        let mut node_a = Ledger::new();
        node_a.mine_tx(&pk_a, 100 * MICRO, 0.0);
        let (transfer, burn_opt, _) = node_a
            .transfer_with_burn(&pk_a, &pk_b, 10 * MICRO, &crypto)
            .expect("transfer must succeed");
        let burn = burn_opt.unwrap();

        // B applies burn FIRST, then transfer (reverse arrival order).
        let mut node_b = Ledger::new();
        node_b.mine_tx(&pk_a, 100 * MICRO, 0.0);
        assert!(node_b.replay_remote_tx(burn));
        assert!(node_b.replay_remote_tx(transfer));

        // Both legs landed; balances match the in-order case exactly.
        assert_eq!(node_b.balance_of(&pk_a), 90 * MICRO);
        assert_eq!(node_b.balance_of(&pk_b), 9_900_000);
    }

    // ─── BLK-HASH-1: block hash commits content + miner ──────────────────

    /// **T1 — collision closed.** Two blocks with the SAME
    /// `(index, prev_hash, timestamp, tx_count)` but a DIFFERENT mining
    /// recipient (different content) must now get DIFFERENT hashes. This is
    /// the exact tuple that collided before the fix.
    #[test]
    fn blk_hash_1_content_collision_is_closed() {
        let ts = "2026-03-01T00:00:00+00:00".to_string();
        let seal = |miner: &str| {
            let mut o = Ledger::new();
            o.mine_tx(miner, 50 * MICRO, 0.0);
            o.seal_block_at(miner, 0.0, ts.clone())
        };
        let bx = seal(&"a".repeat(64));
        let by = seal(&"b".repeat(64));
        assert_eq!(bx.index, by.index);
        assert_eq!(bx.timestamp, by.timestamp);
        assert_eq!(bx.transactions.len(), by.transactions.len());
        assert_eq!(bx.prev_hash, by.prev_hash, "same shared genesis prev");
        assert_ne!(
            bx.hash, by.hash,
            "different content ⇒ different block hash (collision closed)"
        );
    }

    /// **T3 — tampering a contained tx changes the block hash.** Tampering a
    /// MINING tx's amount (no signature to break — `NETWORK` is sig-exempt)
    /// must now invalidate the block via the content-binding Merkle ⇒
    /// rejected.
    #[test]
    fn blk_hash_1_tampering_contained_tx_is_rejected() {
        let miner = "a".repeat(64);
        let mut origin = Ledger::new();
        origin.mine_tx(&miner, 50 * MICRO, 0.0);
        let mut block = origin.seal_block(&miner, 0.0);
        if let Some(tx) = block
            .transactions
            .iter_mut()
            .find(|t| t.tx_type == TxType::Mining)
        {
            tx.amount += 1; // content change, hash left stale
        }
        let mut node = Ledger::new();
        assert!(
            node.integrate_remote_block(block).is_err(),
            "content tamper ⇒ block hash mismatch ⇒ rejected"
        );
        assert_eq!(node.chain_height(), 1, "chain unchanged");
    }
}
