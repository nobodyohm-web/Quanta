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

/// SLICE-CLASS (HARDEN-HYGIENE-1): a char-safe prefix of `s` of at most `max`
/// **bytes**, truncated DOWN to the nearest UTF-8 char boundary. Byte-indexing a
/// `&str` (`&s[..n]`) panics if `n` lands inside a multi-byte char OR past the
/// end, so every log/error that truncates an attacker-controlled `String` field
/// (block hash, prev_hash, tx id, miner) MUST go through this — a crafted block
/// with a short or multi-byte `hash`/`id` would otherwise panic the single
/// inbound-gossip dispatch task and make the node deaf to all gossip (remote
/// DoS). **Display-only**: never feeds a hash, signature, merkle leaf, or
/// consensus comparison (those use the full field), so the truncation has zero
/// consensus effect.
pub(crate) fn short(s: &str, max: usize) -> &str {
    let mut end = max.min(s.len());
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// TX-AUTH-NONCE-1 §1: maximum a remote tx's nonce may exceed the account's
/// current high-water. A tx further ahead than this is **rejected** at
/// admission, so the high-water advance can never iterate an unbounded range
/// (the old `for _ in current..new_hw { increment_nonce }` with an
/// attacker-supplied nonce near `u64::MAX` looped ~2^64 times **under the
/// ledger write lock** — a global hang / remote DoS).
///
/// **§4 policy note**: the *existence* of a bound is required for soundness; the
/// exact *value* is a policy choice (how much out-of-order / future nonce slack
/// to tolerate before rejecting). 1024 comfortably covers realistic gossip
/// reordering while keeping the advance O(bounded). Flagged for Alexandre.
pub(crate) const MAX_NONCE_GAP: u64 = 1_024;

/// ONCHAIN-STAKE-1 §3 (🛑 — constante à figer avec le temps de bloc et la §12).
///
/// Delay, in **block heights**, between an `Unstake` and the moment the unbonding
/// funds become spendable again. Indexing the unlock by **block height** (not the
/// wall clock) keeps the whole stake state a deterministic function of the chain.
///
/// **Hard constraint (ADR-003, slashing — not implemented here):**
/// `UNBONDING_PERIOD_BLOCKS ≥ slashing window`. If funds could unbond faster than
/// equivocation can be detected and punished, slashing would be trivially
/// bypassable (unstake-and-run). The stake state below is the **place** a future
/// slashing rule will reduce `staked`; this delay is what makes it enforceable.
///
/// **Durée d'unbonding (blocs). Ratifiée *réglable* par ADR-009 = 10 080**
/// (≈ 2 semaines au rythme actuel : un seal ~2 min ⇒ ~720 blocs/jour).
/// ADR-009 tranche la **classe** (ajustable, durée, modifiable par **fork**) et
/// pose **10 080** comme défaut ancré ; la **contrainte gravée** est
/// `SLASH_EVIDENCE_WINDOW_BLOCKS ≤ UNBONDING_PERIOD_BLOCKS` (anti *unstake-and-run*,
/// GADGET-4). Le *mécanisme* est codé ; changer le *nombre* par fork co-détermine
/// le taux de churn du validator-set — un réglage, pas une promesse.
pub const UNBONDING_PERIOD_BLOCKS: u64 = 10_080;

/// ONCHAIN-STAKE-1 §1: one pending unlock created by an `Unstake`. `amount` µQTA
/// leave the bonded `staked` pool and sit here, **locked** (they weigh nothing in
/// consensus and are not yet spendable), until the chain reaches `unlock_height`
/// — at which point [`Ledger::mature_unbonding`] folds them back into the
/// spendable balance cache. `tx_hash` ties the entry to the `Unstake` tx that
/// created it, so a single-block fork reorg can revert exactly that entry.
#[derive(Debug, Clone)]
pub(crate) struct UnbondEntry {
    amount: u64,
    unlock_height: u64,
    tx_hash: String,
}

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

/// MSIG-1 — native post-quantum **M-of-N multisig** authority, carried as JSON in a
/// multisig tx's `pq_signature` field (the tx is flagged by `pq_public_key ==
/// MSIG_TAG`). No new `Transaction` wire field, so every single-key tx is byte-for-
/// byte unchanged (mainnet frozen genesis intact).
///
/// The differentiator: institutional custodians have **no threshold/MPC scheme for
/// ML-DSA**, so quorum custody of a post-quantum account looked impossible. This
/// closes it with **on-chain** multisig — N independent ML-DSA keys, each signing the
/// same pre-image, verified on-chain — needing *no* threshold cryptography on the
/// lattice signature. The account address commits to `{keys, threshold}`
/// ([`crate::security::multisig_address_bytes`]), so the policy is revealed at spend
/// and cannot be swapped (rebind-proof).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MultisigAuth {
    /// The N registered ML-DSA-65 public keys (hex).
    pub pubkeys: Vec<String>,
    /// The quorum M: at least this many DISTINCT registered keys must sign.
    pub threshold: u32,
    /// ML-DSA signatures over the tx pre-image (≥ threshold valid, distinct keys).
    pub signatures: Vec<String>,
}

/// MSIG-1 — the `pq_public_key` value that discriminates a multisig tx (also the
/// value bound into its signing pre-image).
const MSIG_TAG: &str = "msig1";

/// The ATN distributed ledger
///
/// `Clone` backs GADGET-5B's **validate-before-commit** fork reconciliation: a
/// competing multi-block fork is trial-applied on a clone and committed only if
/// the whole fork integrates cleanly, so a malformed fork can never truncate the
/// live chain (AUDIT-BLK-2 generalized). The clone happens only on a rare
/// partition-heal reorg, never on the steady-state path.
#[derive(Clone)]
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
    /// ONCHAIN-STAKE-1 §1: per-account **bonded** stake in µQTA — the on-chain
    /// consensus weight (ADR-002). Maintained at **block-application** time
    /// (block-index-anchored), so it is a pure function of the chain and identical
    /// on every node. This is what [`Ledger::validator_stakes`] feeds to the
    /// validator set, closing the second half of the fork vector (the first half
    /// — reputation in the weight — was closed by STAKE-WEIGHT-1).
    staked: HashMap<String, u64>,
    /// ONCHAIN-STAKE-1 §1: per-account list of **unbonding** entries — stake that
    /// an `Unstake` moved out of `staked` but that is not yet spendable. Each entry
    /// matures (folds back into `balance_cache`) once the chain reaches its
    /// `unlock_height`. Also chain-derived, so it survives restart/sync identically.
    unbonding: HashMap<String, Vec<UnbondEntry>>,
    /// LIVE-2 — the chain index of the **last finalized** block (the finality
    /// gadget's floor). Blocks at index `≤ finalized_floor_index` are **irreversible**:
    /// no fork may replace them (GADGET-3 finalization + GADGET-4 accountable
    /// safety). `0` = only genesis finalized (a fresh node). Pushed **down the
    /// chain** by the live gadget as certificates finalize checkpoints
    /// ([`Self::set_finalized_floor`]), and consulted by the fork-resolution paths
    /// ([`Self::integrate_remote_block`]) as an **absolute veto** — the free
    /// lexicographic tie-break still applies *above* the floor (Gasper: fork-choice
    /// is free above finality, frozen at/below it). A **pure** safety guard:
    /// rejecting a reorg never mutates a balance, so conservation is untouched.
    finalized_floor_index: u64,
}

impl Ledger {
    /// Fixed genesis timestamp — deterministic across all nodes (no wall-clock
    /// read; Phase 0, Constitution §3). PQ-MIG-5: the genesis HASH is no longer a
    /// standalone literal — it is the **canonical block hash**
    /// ([`Self::block_hash_hex`]) over the genesis allocation, so it commits to the
    /// initial state and two nodes building the same genesis derive the same hash
    /// byte-for-byte (C1).
    // GENESIS-V4 (2026-07-18): fresh launch genesis. The v3 chain was throwaway
    // test data (early blocks carried no value); v4 restarts on a clean, zero-
    // premine genesis. Changing this timestamp changes the content-bound frozen
    // genesis hash → the v4 chain is deterministically distinct and incompatible
    // with any earlier chain (paired with the TORUS_PROTOCOL_VERSION 4→5 bump and
    // the snapshot genesis-guard in state_persistence). Still a PURE function of
    // its content (C1): two nodes build the identical v4 genesis byte-for-byte.
    const GENESIS_TIMESTAMP: &str = "2026-07-18T00:00:00+00:00";

    /// Default ledger: an **EMPTY** genesis — `genesis_with_allocation(&[])`.
    /// PQ-MIG-5: the default chain carries **zéro premine** (offre 0 au bloc 0,
    /// bloc de genèse sans transaction), preserving the mission invariant. A
    /// non-empty initial distribution is a §12 decision (indécise, pré-genèse);
    /// the genesis machinery that would encode it is
    /// [`Self::genesis_with_allocation`], built and exercised by the PQ-MIG-5 teeth.
    pub fn new() -> Self {
        Self::genesis_with_allocation(&[])
    }

    /// PQ-MIG-5 §1 — build a ledger whose **genesis block** encodes an initial
    /// allocation: the mapping `(adresse ML-DSA, solde dépensable µQTA, enjeu
    /// µQTA)`. The genesis is a **pure, deterministic function** of `allocation`
    /// (same mapping ⇒ byte-identical genesis block + hash, C1) and **conserves
    /// exactly** at block 0: `miné == Σ(solde + enjeu)` — the genesis stake comes
    /// out of the allocated value, never created on top (§3).
    ///
    /// Determinism/IO: pure — no wall clock, no entropy, no `HashMap` iteration in
    /// the genesis bytes (the allocation is consumed in slice order; the derived
    /// state goes through [`Self::rebuild_cache`], whose moves are additive). The
    /// `addresses` are PQ-MIG-2 ML-DSA addresses `BLAKE3(ADDR_DOMAIN ‖ clé)`; this
    /// builder does not need the keys (an address only needs its key to be *spent*,
    /// via `verify_tx`, in a later block).
    pub fn genesis_with_allocation(allocation: &[(&str, u64, u64)]) -> Self {
        let txs = Self::genesis_transactions(allocation);
        let prev_hash = "0".repeat(64);
        let hash = Self::block_hash_hex(0, &prev_hash, Self::GENESIS_TIMESTAMP, "GENESIS", &txs);
        let genesis_hashes: Vec<String> = txs.iter().map(|t| t.hash.clone()).collect();
        let genesis = Block {
            index: 0,
            timestamp: Self::GENESIS_TIMESTAMP.to_string(),
            transactions: txs,
            prev_hash,
            hash,
            miner: "GENESIS".into(),
            energy_kwh: 0.0,
        };
        let mut ledger = Self {
            chain: vec![genesis],
            pending: vec![],
            tx_counter: 0,
            seen_tx_hashes: HashSet::new(),
            account_nonces: HashMap::new(),
            balance_cache: HashMap::new(),
            recent_deque: VecDeque::new(),
            staked: HashMap::new(),
            unbonding: HashMap::new(),
            finalized_floor_index: 0, // LIVE-2: only genesis finalized on a fresh chain
        };
        // PQ-MIG-5 §1+§3: derive balance_cache + staked/unbonding from the genesis
        // block — the SINGLE source of truth, exactly as a restore would — so the
        // initial state is a pure function of the allocation and conservation holds
        // at block 0. Record the genesis tx hashes for anti-replay.
        ledger.rebuild_cache();
        ledger.seen_tx_hashes.extend(genesis_hashes);
        ledger
    }

    /// PQ-MIG-5 §1 — assemble the deterministic genesis transaction list from an
    /// allocation mapping. Empty mapping ⇒ **no transactions** (zéro premine, the
    /// default). Otherwise: a SINGLE `Mining` mint of the whole allocation
    /// `Σ(solde + enjeu)` to the first account (one reward per block, ≤ the
    /// per-block emission bound at supply 0), then **balance-neutral** `Transfer`s
    /// distributing every other account's full share, then a `Stake` per validator
    /// (enjeu > 0). Conservation is exact by construction (`Σ Mining == Σ(solde +
    /// enjeu)`; every later tx only MOVES value). Order is slice order — no map.
    fn genesis_transactions(allocation: &[(&str, u64, u64)]) -> Vec<Transaction> {
        if allocation.is_empty() {
            return Vec::new();
        }
        let total_minted: u64 = allocation
            .iter()
            .map(|(_, bal, stake)| bal.saturating_add(*stake))
            .sum();
        let treasury = allocation[0].0;
        let mut txs = Vec::new();
        // ① one mint of the entire allocation to the first account (the distributor).
        txs.push(Self::genesis_tx(
            "genesis_mint",
            "NETWORK",
            treasury,
            total_minted,
            TxType::Mining,
        ));
        // ② distribute every OTHER account's full share (solde + enjeu) from it.
        for (addr, bal, stake) in allocation.iter().skip(1) {
            let share = bal.saturating_add(*stake);
            if share > 0 {
                txs.push(Self::genesis_tx(
                    &format!("genesis_alloc_{addr}"),
                    treasury,
                    addr,
                    share,
                    TxType::Transfer,
                ));
            }
        }
        // ③ bond each validator's stake (enjeu > 0), in declaration order.
        for (addr, _, stake) in allocation.iter() {
            if *stake > 0 {
                txs.push(Self::genesis_tx(
                    &format!("genesis_stake_{addr}"),
                    addr,
                    Self::STAKE_SINK,
                    *stake,
                    TxType::Stake,
                ));
            }
        }
        txs
    }

    /// PQ-MIG-5 §1 — build ONE deterministic genesis transaction. Genesis txs are
    /// **system-constructed**: never gossiped, never `verify_tx`'d (genesis has no
    /// `prev`, so `validate_block_against_prev` never runs on it). The `Mining`
    /// mint is synthetic (`from = NETWORK`); the distributing `Transfer`/`Stake`
    /// carry no signature. Each id is a stable string and the hash binds the
    /// content via the canonical pre-image, so the genesis Merkle root — hence the
    /// genesis block hash — is byte-identical on every node (C1).
    fn genesis_tx(id: &str, from: &str, to: &str, amount: u64, tx_type: TxType) -> Transaction {
        let ts = Self::GENESIS_TIMESTAMP;
        let payload = Self::tx_signing_preimage(id, from, to, amount, ts, &tx_type, 0, "");
        let hash = hex::encode(blake3::hash(payload.as_bytes()).as_bytes());
        Transaction {
            id: id.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            amount,
            tx_type,
            timestamp: ts.to_string(),
            signature: String::new(),
            hash,
            nonce: 0,
            pq_signature: None,
            pq_public_key: None,
            fault_proof: None,
            slash_unbonding: None,
        }
    }

    /// BLK-HASH-1 / PQ-MIG-5 §1 — the canonical block hash: BLAKE3 over
    /// `index:prev_hash:ts:miner:len:merkle_root`, where the Merkle root commits to
    /// each tx's content + signature + ML-DSA authority layer. Shared verbatim by
    /// [`Self::seal_block_at`] (sealed blocks), [`Self::validate_block_against_prev`]
    /// (received blocks), and [`Self::genesis_with_allocation`] (the genesis block)
    /// so the three can never drift — the genesis hash is computed by the SAME block
    /// hashing, domain-separated by `index = 0`, `prev_hash = 0×64`,
    /// `miner = "GENESIS"`.
    fn block_hash_hex(
        index: u64,
        prev_hash: &str,
        ts: &str,
        miner: &str,
        txs: &[Transaction],
    ) -> String {
        let tx_root = Self::compute_merkle_root(txs);
        let payload = format!(
            "{}:{}:{}:{}:{}:{}",
            index,
            prev_hash,
            ts,
            miner,
            txs.len(),
            tx_root
        );
        hex::encode(blake3::hash(payload.as_bytes()).as_bytes())
    }

    /// 🛑 PQ-MIG-5 §2 — **PLACEHOLDER DEV** genesis ML-DSA addresses (§12 réglable,
    /// **jamais** définitif). Chacune est l'adresse PQ-MIG-2
    /// `BLAKE3(ADDR_DOMAIN ‖ clé_publique_ML-DSA)` d'une clé de test dérivée d'une
    /// graine figée (`seeded_identity(5_000_00N)`) ; le test
    /// `pqmig5_genesis_addresses_bind_their_seeds` épingle ce lien. Test-only — le
    /// défaut `new()` n'alloue rien (zéro premine).
    #[cfg(test)]
    pub(crate) const GENESIS_ADDR_0: &str =
        "f9d618b390aa1596ec3ccfdaffd32c35b7ac3eadd3e08cb741c7973f0ec2e1d2";
    #[cfg(test)]
    pub(crate) const GENESIS_ADDR_1: &str =
        "07ac1346445df462d00d4cee9f00a39b438c3d87163093e31e7137c3cce01c6e";
    #[cfg(test)]
    pub(crate) const GENESIS_ADDR_2: &str =
        "76e709edc6e3b0b223f91a1d4dd74ce2d63623e50c06146761a25a301886aedd";

    /// 🛑 PQ-MIG-5 §1–§2 — **PLACEHOLDER DEV** genesis allocation (§12 réglable,
    /// **jamais** figé définitif). Mapping `adresse ML-DSA → (solde dépensable
    /// µQTA, enjeu µQTA)`. La distribution réelle des tokens est **indécise**
    /// (pré-genèse) : ces montants sont **nominaux** et **ne promettent rien** —
    /// ils existent pour exercer la machinerie de genèse (les dents §5). Le défaut
    /// `new()` est l'allocation **vide** (zéro premine) ; câbler une vraie
    /// distribution ici est une décision §12. Conservation (§3) :
    /// `miné == Σ(solde + enjeu)`. Deux validateurs initiaux (enjeu > 0), un
    /// porteur simple (enjeu 0) — total miné 100 QTA (≤ borne d'émission/bloc).
    #[cfg(test)]
    pub(crate) const DEV_GENESIS_ALLOCATION: &[(&str, u64, u64)] = &[
        (Self::GENESIS_ADDR_0, 50 * MICRO, 10 * MICRO),
        (Self::GENESIS_ADDR_1, 25 * MICRO, 5 * MICRO),
        (Self::GENESIS_ADDR_2, 10 * MICRO, 0),
    ];

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
            // LIVE-3 (audit HIGH): a `Slash` is **network-authored**, not user
            // mempool traffic. It carries the fixed `GENESIS_TIMESTAMP` **by design**
            // (deterministic hash / C1 — a wall-clock stamp would break cross-node
            // agreement), which is always "stale" against real time. TTL-evicting it
            // would drop every slash ~30 s after it is queued — before the ~120 s
            // seal could include it — making accountable-safety slashing inoperative
            // in production. Exempt it: a slash is instead cleaned up at seal when it
            // becomes redundant (`invalid_slash_indices` excludes an offender already
            // slashed on-chain), and it can't accumulate (one pending slash per
            // offender, `queue_slash`).
            if matches!(tx.tx_type, TxType::Slash) {
                return true;
            }
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
        // ONCHAIN-STAKE-1: a `Stake` flows through the generic balance move below
        // (`from → STAKE` sink, `STAKE` is NOT synthetic so the coins are credited
        // to the sink, not destroyed) — this LOCKS the staked funds the instant the
        // tx is admitted (HARDEN-STAKE-1: a pending Stake's coins can no longer be
        // double-spent by a concurrent transfer, and conservation stays exact). The
        // per-account *bonded weight* consensus reads is committed at block time.
        // An `Unstake`, by contrast, only reclassifies already-locked sink coins
        // (bonded → unbonding) and has no spendable/sink effect here — it is applied
        // at block time, anchored to the sealing block's height.
        if matches!(tx.tx_type, TxType::Unstake) {
            self.recent_deque.push_back(tx.clone());
            if self.recent_deque.len() > Self::MAX_RECENT {
                self.recent_deque.pop_front();
            }
            return;
        }
        // LIVE-3: a Slash destroys bonded stake. The coins sit in the STAKE sink
        // (not the offender's spendable), so debit the SINK — this drops
        // `locked_stake_total` by `amount`, and `total_burned` counts the Slash
        // (below), so the pair is conservation-neutral (locked_stake ↓, burned ↑).
        // The offender's per-account bonded weight (`staked`) is debited at block
        // time by `apply_block_stake_effects`, keeping sink == Σstaked+Σunbonding.
        if matches!(tx.tx_type, TxType::Slash) {
            *self
                .balance_cache
                .entry(Self::STAKE_SINK.to_string())
                .or_insert(0) -= tx.amount as i128;
            self.recent_deque.push_back(tx.clone());
            if self.recent_deque.len() > Self::MAX_RECENT {
                self.recent_deque.pop_front();
            }
            return;
        }
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
        // ONCHAIN-STAKE-1: a `Stake`'s `from → STAKE` move IS reverted here (the
        // generic path below: it credits `from` back and debits the sink), so an
        // evicted/reorged pending Stake releases its lock correctly. An `Unstake`
        // had no spendable/sink effect at this layer; its block-time bonded-weight
        // effect is reverted by `revert_block_stake_effects` on a fork-reorg pop.
        if matches!(tx.tx_type, TxType::Unstake) {
            return;
        }
        // LIVE-3: reverse a Slash — credit the STAKE sink back (mirror of apply).
        if matches!(tx.tx_type, TxType::Slash) {
            *self
                .balance_cache
                .entry(Self::STAKE_SINK.to_string())
                .or_insert(0) += tx.amount as i128;
            return;
        }
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
        // ONCHAIN-STAKE-1: the stake state is also chain-derived, so a restore
        // rebuilds it from scratch (block-index-anchored) — identical to a node
        // that was never restarted.
        self.staked.clear();
        self.unbonding.clear();
        let synthetic = |a: &str| matches!(a, "NETWORK" | "BURN" | "ESCROW");
        // ONCHAIN-STAKE-1: a `Stake`'s `from → STAKE` spendable move replays through
        // the generic loop (same as live `cache_apply_tx`). Only `Unstake` is
        // excluded here — it has no spendable effect; its bonded-weight reclassify +
        // the unbonding/maturation are replayed per chain block below, anchored to
        // `block.index`, exactly as live block application does.
        // Apply ONE tx's balance-cache effect, EXACTLY as live `cache_apply_tx`:
        //  - Unstake: no spendable effect (reclassified at block time below);
        //  - Slash (LIVE-3): debit the STAKE **sink** (the locked coins are burned),
        //    NEVER the offender's spendable — the CRITICAL restore-vs-live parity bug;
        //  - otherwise: generic `from → to` (a Stake's `pk → STAKE` flows here).
        // Kept as an explicit helper so the chain and pending loops can never drift.
        fn replay_cache_effect(
            cache: &mut HashMap<String, i128>,
            tx: &Transaction,
            synthetic: &impl Fn(&str) -> bool,
        ) {
            match tx.tx_type {
                TxType::Unstake => {}
                TxType::Slash => {
                    *cache.entry(Ledger::STAKE_SINK.to_string()).or_insert(0) -= tx.amount as i128;
                }
                _ => {
                    if !synthetic(&tx.to) {
                        *cache.entry(tx.to.clone()).or_insert(0) += tx.amount as i128;
                    }
                    if !synthetic(&tx.from) {
                        *cache.entry(tx.from.clone()).or_insert(0) -= tx.amount as i128;
                    }
                }
            }
        }
        for block in &self.chain {
            for tx in &block.transactions {
                replay_cache_effect(&mut self.balance_cache, tx, &synthetic);
                self.recent_deque.push_back(tx.clone());
            }
        }
        for tx in &self.pending {
            replay_cache_effect(&mut self.balance_cache, tx, &synthetic);
            self.recent_deque.push_back(tx.clone());
        }
        // Replay the on-chain stake state block by block (one clone at a time, not
        // the whole chain) so `staked`/`unbonding` and the matured-fund credits are
        // reconstructed deterministically from the chain.
        for i in 0..self.chain.len() {
            let block = self.chain[i].clone();
            self.apply_block_stake_effects(&block);
        }
        // Trim deque to MAX_RECENT
        while self.recent_deque.len() > Self::MAX_RECENT {
            self.recent_deque.pop_front();
        }
    }

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
    fn apply_block_stake_effects(&mut self, block: &Block) {
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
    fn revert_block_stake_effects(&mut self, block: &Block) {
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
    fn invalid_slash_indices(&self, txs: &[Transaction]) -> Vec<usize> {
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
    fn evict_stale_pending_slashes(&mut self) {
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

    /// LIVE-1 — the genesis block hash (the finality gadget's epoch-0 checkpoint,
    /// justified and finalized by definition). Always present: the chain ships
    /// with genesis (PQ-MIG-5).
    pub fn genesis_hash(&self) -> String {
        self.chain
            .first()
            .map(|b| b.hash.clone())
            .unwrap_or_default()
    }

    /// LIVE-2 — the chain index of the last finalized block (the finality floor).
    /// Blocks at index `≤` this are irreversible. `0` (genesis) on a fresh node.
    pub fn finalized_floor_index(&self) -> u64 {
        self.finalized_floor_index
    }

    /// LIVE-2 — push the finality floor **down the chain** as the live gadget
    /// finalizes checkpoints. **Monotonic**: it never moves backward (finality is
    /// irreversible — a lower value would re-open finalized history to reorg).
    ///
    /// **Hash-checked (the HIGH-4 fix).** The caller passes the finalized
    /// checkpoint's `(height, hash)`. The floor advances **only if the block we
    /// actually hold at that height IS the finalized one** (`block_at(height).hash
    /// == finalized_hash`). Freezing by index alone was unsafe: a node sitting on a
    /// different block `Y@H` (a lexicographic-tie-break loser, or a minority branch
    /// during the convergence window) while the gadget finalized `X@H` would freeze
    /// `Y` and reject the finalized `X` **forever**. When the local block ≠ the
    /// finalized hash, the node is on a finality-overruled fork: we do **not**
    /// freeze (so it can still integrate `X` once it syncs the finalized branch) and
    /// warn. Returns the effective floor.
    pub fn set_finalized_floor(&mut self, finalized_index: u64, finalized_hash: &str) -> u64 {
        // Only a height we actually hold, and only if it advances the floor.
        if finalized_index <= self.finalized_floor_index {
            return self.finalized_floor_index;
        }
        match self.block_at(finalized_index) {
            Some(b) if b.hash == finalized_hash => {
                // The block we hold at this height IS the finalized one → freeze it.
                self.finalized_floor_index = finalized_index;
            }
            Some(_) => {
                // We hold a DIFFERENT block at this height — finality overruled our
                // branch. Don't freeze the wrong block; we must adopt the finalized
                // branch via sync/fork-resolution before the floor can advance here.
                log::warn!(
                    "◈ [Finality] certificate finalized height {} but our block there differs — \
                     NOT freezing (need to sync the finalized branch)",
                    finalized_index
                );
            }
            None => {
                // We don't hold this height yet (vote references a not-yet-synced
                // block). Leave the floor; it advances once we integrate that height
                // and re-observe the certificate.
            }
        }
        self.finalized_floor_index
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

    /// TX-AUTH-NONCE-1 §1: set an account's nonce high-water directly in **O(1)**
    /// (monotonic — never lowers it). Replaces a `for _ in current..new_hw`
    /// increment loop that, with an attacker-supplied nonce near `u64::MAX`,
    /// iterated ~2^64 times under the ledger write lock (global hang / DoS). The
    /// end state is identical to the loop's (high-water == `new_hw`).
    pub(crate) fn raise_nonce_high_water(&mut self, pk: &str, high_water: u64) {
        let entry = self.account_nonces.entry(pk.to_string()).or_insert(0);
        if high_water > *entry {
            *entry = high_water;
        }
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
        // TX-AUTH-NONCE-1 §1: reject a nonce too far AHEAD of the high-water
        // BEFORE applying — this bounds the advance so it can never loop ~2^64
        // under the ledger lock (DoS), independent of authentication. This is
        // the single high-water advance shared by every caller (the dispatcher's
        // direct `apply_verified_remote_tx`, the core, and `apply_remote_tx_checked`),
        // so the bound covers them all.
        let current = self.get_nonce(&from);
        if nonce > current.saturating_add(MAX_NONCE_GAP) {
            // Too far ahead → reject, mutate nothing. Logged (debug, low-spam) so
            // a genuinely large-but-honest reorder near the bound is diagnosable.
            log::debug!(
                "◈ [Ledger] tx from {} dropped: nonce {} too far ahead of high-water {} (max gap {})",
                short(&from, 12),
                nonce,
                current,
                MAX_NONCE_GAP
            );
            return false;
        }
        let applied = self.replay_remote_tx(tx);
        if applied {
            // AUDIT-TX-2: high-water = max(current, nonce + 1) so out-of-order
            // arrivals never roll the counter backwards. Direct O(1) set (no
            // loop) — see `raise_nonce_high_water`.
            let new_hw = current.max(nonce.saturating_add(1));
            self.raise_nonce_high_water(&from, new_hw);
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

    /// Total QUANTA permanently burned (in µQTA). LIVE-3: a `Slash` is also a
    /// permanent destruction (STAKE sink → burned), so it counts here — the
    /// counterpart of the `STAKE`-sink debit `cache_apply_tx` performs, keeping
    /// `Σ spendable + locked_stake + burned == minted` exact across a slash.
    pub fn total_burned(&self) -> u64 {
        let is_burn = |t: &&Transaction| matches!(t.tx_type, TxType::Burn | TxType::Slash);
        let chain_burn: u64 = self
            .chain
            .iter()
            .flat_map(|b| b.transactions.iter())
            .filter(is_burn)
            .map(|t| t.amount)
            .sum();
        let pending_burn: u64 = self
            .pending
            .iter()
            .filter(is_burn)
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
    /// PQ-MIG-3B §1 — **stateless** post-quantum gate of a transaction's authority
    /// (ADR-007 b, accounts fully ML-DSA). Returns `Ok(true)` only if, over the
    /// key-binding pre-image:
    /// - `tx.from` IS the ML-DSA **address** of the revealed `pq_public_key`, i.e.
    ///   `lie(from, key)` holds (`from == BLAKE3(ADDR_DOMAIN ‖ pk)`, PQ-MIG-2), and
    /// - a valid **ML-DSA-65** signature from that revealed key (post-quantum
    ///   authority — **mandatory**, no Ed25519 fallback).
    ///
    /// This closes CRYPTO-ID-1 **intrinsically and statelessly**: the account
    /// identifier (`from`) cryptographically commits to the key, so an attacker
    /// cannot attach its own ML-DSA key to a foreign account (a different key ⇒ a
    /// different `from`), and breaking Ed25519 grants no authority (the address no
    /// longer derives from an Ed25519 key). The model-(a) Ed25519 co-factor is
    /// removed from this path; the on-chain binding registry
    /// ([`Self::binding_violations`]) is retained at validation/seal as a redundant
    /// backstop but is now subsumed by this intrinsic check. A tx without an ML-DSA
    /// layer, or whose key does not hash to `from`, is rejected here.
    pub fn verify_tx(tx: &Transaction) -> Result<bool, String> {
        // Synthetic system addresses are exempt — they originate inside the
        // node and are never accepted from gossip.
        if tx.from == "NETWORK" || tx.from == "ESCROW" {
            return Ok(true);
        }
        // LIVE-3: a Slash is NOT signed by the offender — its authority is the
        // embedded fault proof, re-verified against the on-chain stake by
        // `verify_block_slashes` (a stateful check `verify_tx` can't do). It is
        // block-only (never accepted from a `BroadcastTx`, see `handle_broadcast_tx`),
        // so exempting it from the signature gate here opens no gossip-injection hole.
        if tx.tx_type == TxType::Slash {
            return Ok(true);
        }
        // MINT-GUARD-1: `Mining` is a SYSTEM tx type — legitimately issued ONLY by the
        // synthetic `NETWORK` sender (exempted above). A tx reaching this point has a
        // real user `from`, so a user-authorized `Mining` tx (single-key OR multisig)
        // is a forgery: left unchecked it would be swept into the block reward by
        // `coalesce_block_rewards`, minting unbacked QUANTA. Reject it at THE gate
        // (verify_tx is the single portal before admission AND in block validation).
        if tx.tx_type == TxType::Mining {
            return Ok(false);
        }
        // MSIG-1: a multisig tx is flagged by `pq_public_key == MSIG_TAG`. Its
        // authority is a QUORUM of independent ML-DSA signatures, not a single one,
        // so it takes a distinct path *before* the single-key requirements below
        // (it carries no Ed25519 `signature` and no single `pq_signature`).
        if tx.pq_public_key.as_deref() == Some(MSIG_TAG) {
            return Self::verify_multisig(tx);
        }
        // Any other from value must carry a valid signature, regardless of
        // destination (BURN included).
        if tx.signature.is_empty() {
            return Err("Transaction non signée".into());
        }

        // PQ-MIG-3 §3: the ML-DSA authority layer is MANDATORY — a tx without a
        // non-empty `pq_public_key` + `pq_signature` is rejected (the Ed25519-only
        // fallback is gone). Empty here ⇒ an Ed25519-only tx ⇒ rejected.
        let pq_pk_hex = tx.pq_public_key.as_deref().unwrap_or("");
        let pq_sig_hex = tx.pq_signature.as_deref().unwrap_or("");
        if pq_pk_hex.is_empty() || pq_sig_hex.is_empty() {
            return Ok(false);
        }

        // PQ-MIG-3B §1 — INTRINSIC key↔account binding (the closure of CRYPTO-ID-1).
        // `tx.from` IS the ML-DSA address `BLAKE3(ADDR_DOMAIN ‖ pk)` (PQ-MIG-2), so
        // the revealed key MUST hash to it (`lie(from, key)`). A tx revealing any
        // key other than the one whose address is `from` is REJECTED here — an
        // attacker can no longer attach its OWN ML-DSA key to someone else's
        // account, because the account identifier itself cryptographically commits
        // to the key. This supersedes the model-(a) Ed25519 co-factor + on-chain
        // binding registry: the binding is now stateless and unforgeable by
        // construction (a different key ⇒ a different `from`).
        if !CryptoEngine::address_hex_binds_key_hex(&tx.from, pq_pk_hex) {
            return Ok(false);
        }

        // TX-AUTH-NONCE-1 §2 + PQ-MIG-3 §2: the pre-image binds `tx.nonce` AND the
        // revealed ML-DSA key — the signature covers them, so a third party can
        // alter neither the nonce nor the revealed key of a signed tx.
        let payload = Self::tx_signing_preimage(
            &tx.id, &tx.from, &tx.to, tx.amount, &tx.timestamp, &tx.tx_type, tx.nonce, pq_pk_hex,
        );

        // TX-AUTH-NONCE-1 §3: recompute the content hash and REJECT a wire hash
        // that disagrees — no hash malleability (dedup `seen_tx_hashes` is keyed
        // on `tx.hash`; a forged hash must not slip past as "unseen").
        let recomputed = hex::encode(blake3::hash(payload.as_bytes()).as_bytes());
        if tx.hash != recomputed {
            return Ok(false);
        }

        // PQ-MIG-3B §1 — authority is PURE ML-DSA: a valid ML-DSA-65 signature from
        // the revealed (address-bound) key over the pre-image, via the project's
        // single ML-DSA verifier ([`CryptoEngine::verify_pq`]). The Ed25519
        // co-factor is GONE from the authority path (ADR-007 b, "sans astérisque"):
        // breaking Ed25519 grants no power over an account, since `from` no longer
        // derives from an Ed25519 key. (`tx.signature` may still carry a vestigial
        // Ed25519 signature for wire-compat; it is NOT consulted for authority.)
        let pq_sig = hex::decode(pq_sig_hex).map_err(|_| "PQ signature invalide")?;
        Ok(CryptoEngine::verify_pq(pq_pk_hex, payload.as_bytes(), &pq_sig))
    }

    /// MSIG-1 — verify an M-of-N multisig authority. `Ok(true)` iff ALL hold:
    /// 1. `from` == the address derived from the revealed policy `{keys, threshold}`
    ///    ([`crate::security::multisig_address_hex`]) — the rebind-proof binding: a
    ///    different key set or threshold yields a different `from`, so an attacker
    ///    cannot substitute their own keys onto someone else's multisig account.
    /// 2. The wire `hash` equals the recomputed pre-image hash (no malleability).
    /// 3. At least `threshold` **distinct** registered keys each carry a valid
    ///    ML-DSA-65 signature over that pre-image (duplicate signatures from one key
    ///    cannot inflate the count — verification is per-distinct-key).
    ///
    /// Pure (no ledger state) and deterministic — same inputs ⇒ same verdict on
    /// every node (C1), like the single-key `verify_tx`.
    fn verify_multisig(tx: &Transaction) -> Result<bool, String> {
        let auth: MultisigAuth = match tx.pq_signature.as_deref() {
            Some(json) => match serde_json::from_str(json) {
                Ok(a) => a,
                Err(_) => return Ok(false),
            },
            None => return Ok(false),
        };
        // Canonicalize exactly as the address derivation does (sorted, de-duplicated)
        // so counting is per-DISTINCT-key and independent of order/duplicates.
        // MSIG-SEC-1: canonicalize by decoded key BYTES (not raw hex strings), so a
        // single key spelled in two hex cases cannot fill two quorum slots; also
        // rejects any malformed / wrong-length key. This is the SAME canonicalization
        // the address derivation uses, so counting and binding can never disagree.
        let keys = match crate::security::canonicalize_msig_keys(&auth.pubkeys) {
            Some(k) => k,
            None => return Ok(false),
        };
        if auth.threshold == 0 || auth.threshold as usize > keys.len() {
            return Ok(false);
        }
        // (1) Binding — `from` must be the address of exactly this (canonical) policy.
        match crate::security::multisig_address_hex(&keys, auth.threshold) {
            Some(addr) if addr == tx.from => {}
            _ => return Ok(false),
        }
        // (2) Pre-image + hash integrity. The pq_pk slot signed is the MSIG tag.
        let payload = Self::tx_signing_preimage(
            &tx.id, &tx.from, &tx.to, tx.amount, &tx.timestamp, &tx.tx_type, tx.nonce, MSIG_TAG,
        );
        if tx.hash != hex::encode(blake3::hash(payload.as_bytes()).as_bytes()) {
            return Ok(false);
        }
        // (3) Count distinct registered keys carrying ≥1 valid signature.
        let mut sigs: Vec<Vec<u8>> = Vec::with_capacity(auth.signatures.len());
        for s in &auth.signatures {
            match hex::decode(s) {
                Ok(b) => sigs.push(b),
                Err(_) => return Ok(false),
            }
        }
        let valid_signers = keys
            .iter()
            .filter(|pk| {
                sigs.iter()
                    .any(|sig| CryptoEngine::verify_pq(pk, payload.as_bytes(), sig))
            })
            .count();
        Ok(valid_signers as u32 >= auth.threshold)
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
        // Production mines once per tick but seals every `SEAL_EVERY_N_TICKS`
        // ticks, so a leader's pending can hold several of its own rewards;
        // bundling them keeps the chain at exactly one reward per block (the
        // §4.2 rule peers enforce + the §4.3 emission invariant). A block with
        // ≤1 mining tx is returned byte-identical to the pre-EMIT-1 seal.
        let candidate = Self::coalesce_block_rewards(std::mem::take(&mut self.pending), miner, index, &ts);

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
        let txs = if uncovered.is_empty() && unbound.is_empty() && bad_slashes.is_empty() {
            candidate
        } else {
            let drop_idx: HashSet<usize> =
                uncovered.into_iter().chain(unbound).chain(bad_slashes).collect();
            let mut kept = Vec::with_capacity(candidate.len());
            for (i, tx) in candidate.into_iter().enumerate() {
                if drop_idx.contains(&i) {
                    log::warn!(
                        "◈ [Ledger] COVER-2/PQ-MIG-3: excluding uncovered-or-unbound tx {} (from {}) from sealed block #{}",
                        short(&tx.hash, 12),
                        short(&tx.from, 12),
                        index
                    );
                    self.cache_revert_tx(&tx); // undo its admission-time cache effect
                } else {
                    kept.push(tx);
                }
            }
            kept
        };

        // BLK-HASH-1: the block hash commits to tx CONTENT via the Merkle root
        // (content+signature leaves) AND to the `miner` — via the shared
        // `block_hash_hex` (PQ-MIG-5), used verbatim by genesis and validation so
        // the hashings can never drift.
        let hash = Self::block_hash_hex(index, &prev_hash, &ts, miner, &txs);
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
        // MINT-GUARD-2 (defense in depth for the critical mint vector): `Mining` is
        // NETWORK-only. ONLY genuine `NETWORK` rewards are coalesced; a `Mining` tx
        // from any other sender is a forgery (MINT-GUARD-1 already rejects it at
        // admission — this ensures a corrupted mempool could never mint either) and
        // is DROPPED here, never summed into the reward.
        let network_rewards = txs
            .iter()
            .filter(|t| t.tx_type == TxType::Mining && t.from == "NETWORK")
            .count();
        let has_forged = txs
            .iter()
            .any(|t| t.tx_type == TxType::Mining && t.from != "NETWORK");
        // ≤1 genuine reward and no forgery → byte-identical to the pre-EMIT-1 seal.
        if network_rewards <= 1 && !has_forged {
            return txs;
        }
        let mut total: u64 = 0;
        let mut rest: Vec<Transaction> = Vec::with_capacity(txs.len());
        for tx in txs {
            if tx.tx_type == TxType::Mining {
                if tx.from == "NETWORK" {
                    total = total.saturating_add(tx.amount);
                }
                // else: forged Mining tx — dropped, never minted.
            } else {
                rest.push(tx);
            }
        }
        if total == 0 {
            return rest; // no genuine reward to emit; any forgeries were dropped
        }
        let id = format!("tx_mint_b{index}");
        // TX-AUTH-NONCE-1: one canonical pre-image everywhere. Synthetic NETWORK
        // reward → nonce 0 (it is unsigned and `verify_tx`-exempt; block-bound
        // via the Merkle root over `tx_content_bytes`).
        let payload = Self::tx_signing_preimage(&id, "NETWORK", miner, total, ts, &TxType::Mining, 0, "");
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
            fault_proof: None,
            slash_unbonding: None,
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
                short(&block.prev_hash, 16),
                short(&tip.hash, 16)
            ));
        }
        // COVER-1: seed the shared validator's coverage check with the on-chain
        // balances up to the tip (this block extends it). Mempool-free → identical
        // verdict on every node.
        let onchain_before = self.onchain_spendable_before(tip);
        let bindings_before = self.pq_bindings_before(tip);
        // PROPOSER-1: `self` sits at the parent (tip) on the linear path (and on
        // the reorg trial clone), so the live `validator_stakes()` IS the bonded
        // set as of the parent — O(1), no replay needed.
        let bonded_before = self.validator_stakes();
        Self::validate_block_against_prev(block, tip, &onchain_before, &bindings_before, &bonded_before)?;
        self.validate_block_emission(block)
    }

    /// TOKENOMICS v2 — garde-fou de consensus : la somme minée d'un bloc ne
    /// peut JAMAIS pousser l'offre au-delà du plafond dur, même venant d'un
    /// pair malveillant. Sans ça, un attaquant scellerait un bloc se
    /// créditant des millions (les tx `NETWORK` sont exemptes de signature)
    /// → inflation arbitraire. Le plafond devient ainsi infalsifiable à
    /// l'échelle réseau.
    fn validate_block_emission(&self, block: &Block) -> Result<(), String> {
        // Linear (happy-path) emission: the block extends the tip, so the
        // supply *before* it is the whole current chain's minted total.
        Self::validate_block_emission_against(block, self.stats().total_mined)
    }

    /// FORK-CAP-1: emission validation parameterised by `prior_mined` — the
    /// minted supply that exists **before** this block — so the happy path and
    /// the fork-reorg path enforce the SAME hard cap + per-block bound from one
    /// source of truth and can never diverge on emission again. The happy path
    /// passes the whole chain's `total_mined`; the fork-reorg path passes
    /// `total_mined` MINUS the tip it replaces (else the popped tip's reward is
    /// double-counted, over-stating supply and falsely rejecting honest reorgs
    /// near the cap).
    pub(crate) fn validate_block_emission_against(
        block: &Block,
        prior_mined: u64,
    ) -> Result<(), String> {
        let block_minted: u64 = block
            .transactions
            .iter()
            .filter(|t| t.tx_type == TxType::Mining)
            .map(|t| t.amount)
            .sum();
        if block_minted == 0 {
            return Ok(());
        }
        let current = prior_mined;
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

    /// COVER-1 §2: the **on-chain** spendable balance of every account just
    /// before `prev`'s child block — a pure function of the chain up to and
    /// including `prev`, **never** the pending mempool (a mempool-derived figure
    /// differs node-to-node and would make the coverage verdict diverge). It
    /// mirrors the ledger's own balance semantics exactly, so it can never drift:
    ///
    /// - generic `from → to` moves, identical to [`Self::cache_apply_tx`] (a
    ///   `Stake`'s `pk → STAKE` lock flows through here — `STAKE` is not synthetic
    ///   — so staked coins are correctly debited from `from`);
    /// - `Unstake` has no spendable effect (it reclassifies already-locked sink
    ///   coins), exactly as `cache_apply_tx` skips it;
    /// - height-triggered unbonding **maturation** (`STAKE → pk`), identical to
    ///   [`Self::mature_unbonding`], so coins whose unlock height is `≤ prev.index`
    ///   are counted as spendable again.
    ///
    /// Both validation paths seed [`Self::validate_block_against_prev`]'s coverage
    /// loop with this map, so neither the linear nor the fork-reorg path can admit
    /// an uncovered spend (the FORK-CAP lesson: one check, on the shared
    /// validator). A `#[cfg(test)]` no-drift guard asserts this equals the live
    /// cache for a pending-free chain.
    fn onchain_spendable_before(&self, prev: &Block) -> HashMap<String, i128> {
        let synthetic = |a: &str| matches!(a, "NETWORK" | "BURN" | "ESCROW");
        let mut bal: HashMap<String, i128> = HashMap::new();
        // pk → list of (amount, unlock_height) still locked in the unbonding pool.
        let mut unbonding: HashMap<String, Vec<(u64, u64)>> = HashMap::new();
        for block in self.chain.iter().filter(|b| b.index <= prev.index) {
            for tx in &block.transactions {
                if matches!(tx.tx_type, TxType::Unstake) {
                    // Bonded → unbonding: no spendable move; the coins stay in the
                    // STAKE sink until they mature (recorded for the pass below).
                    unbonding.entry(tx.from.clone()).or_default().push((
                        tx.amount,
                        block.index.saturating_add(UNBONDING_PERIOD_BLOCKS),
                    ));
                    continue;
                }
                // LIVE-3: a Slash destroys locked stake (STAKE sink → burned); it has
                // NO spendable effect, so it must not debit the offender's spendable
                // here (mirrors `cache_apply_tx`, which touches only the sink). Skip.
                if matches!(tx.tx_type, TxType::Slash) {
                    continue;
                }
                if !synthetic(&tx.to) {
                    *bal.entry(tx.to.clone()).or_insert(0) += tx.amount as i128;
                }
                if !synthetic(&tx.from) {
                    *bal.entry(tx.from.clone()).or_insert(0) -= tx.amount as i128;
                }
            }
            // Height-triggered maturation — mirrors `mature_unbonding(block.index)`.
            let height = block.index;
            let mut matured: Vec<(String, u64)> = Vec::new();
            for (pk, entries) in unbonding.iter_mut() {
                entries.retain(|(amount, unlock)| {
                    if *unlock <= height {
                        matured.push((pk.clone(), *amount));
                        false
                    } else {
                        true
                    }
                });
            }
            unbonding.retain(|_, v| !v.is_empty());
            for (pk, amount) in matured {
                *bal.entry(Self::STAKE_SINK.to_string()).or_insert(0) -= amount as i128;
                *bal.entry(pk).or_insert(0) += amount as i128;
            }
        }
        bal
    }

    /// PROPOSER-1 (GENESIS-V4) — the **bonded-stake map as of `prev`**, a pure
    /// function of the chain prefix `chain[.. = prev.index]`. Mirrors
    /// [`Self::apply_block_stake_effects`] for the per-account bonded weight
    /// (`staked`) exactly — `Stake` bonds, `Unstake` unbonds, `Slash` destroys the
    /// bonded portion — so `staked_before(tip) == validator_stakes()` (locked by
    /// the `staked_before_matches_live_cache` test). Maturation touches only the
    /// unbonding pool, never bonded weight, so it is irrelevant here.
    ///
    /// Used to verify a block's proposer against the validator set **as of its
    /// parent**, identically on every node and every admission path (linear /
    /// reorg / sync). The linear + trial-clone paths sit at the parent already, so
    /// they use the O(1) live [`Self::validator_stakes`]; this O(chain) replay is
    /// only needed on the rare 1-block fork tie-break, where `self` is one block
    /// ahead of the parent.
    fn staked_before(&self, prev: &Block) -> HashMap<String, u64> {
        let mut staked: HashMap<String, u64> = HashMap::new();
        for block in self.chain.iter().filter(|b| b.index <= prev.index) {
            for tx in &block.transactions {
                match tx.tx_type {
                    TxType::Stake => {
                        *staked.entry(tx.from.clone()).or_insert(0) += tx.amount;
                    }
                    TxType::Unstake => {
                        let bonded = staked.entry(tx.from.clone()).or_insert(0);
                        *bonded = bonded.saturating_sub(tx.amount);
                        if *bonded == 0 {
                            staked.remove(&tx.from);
                        }
                    }
                    TxType::Slash => {
                        let consumed_unbonding: u64 =
                            tx.slash_unbonding.iter().flatten().map(|e| e.amount).sum();
                        let bonded_take = tx.amount.saturating_sub(consumed_unbonding);
                        if bonded_take > 0 {
                            let bonded = staked.entry(tx.from.clone()).or_insert(0);
                            *bonded = bonded.saturating_sub(bonded_take);
                        }
                        if staked.get(&tx.from).copied().unwrap_or(0) == 0 {
                            staked.remove(&tx.from);
                        }
                    }
                    _ => {}
                }
            }
        }
        staked
    }

    /// PQ-MIG-3 §1 — the chain-derived ML-DSA **binding registry** as of `prev`:
    /// the first-seen `from → pq_public_key` over every sealed, real-sender, signed
    /// tx up to and including `prev`. A **pure function of the chain** (like
    /// [`Self::onchain_spendable_before`]), so every node computes the identical
    /// map and binding validation is deterministic — no extra persisted state.
    fn pq_bindings_before(&self, prev: &Block) -> HashMap<String, String> {
        let synthetic = |a: &str| matches!(a, "NETWORK" | "BURN" | "ESCROW");
        let mut bound: HashMap<String, String> = HashMap::new();
        for block in self.chain.iter().filter(|b| b.index <= prev.index) {
            for tx in &block.transactions {
                if synthetic(&tx.from) {
                    continue;
                }
                if let Some(k) = tx.pq_public_key.as_deref() {
                    if !k.is_empty() {
                        bound.entry(tx.from.clone()).or_insert_with(|| k.to_string());
                    }
                }
            }
        }
        bound
    }

    /// PQ-MIG-3 §1 — the **single source of truth** for the ML-DSA key binding,
    /// mirroring `uncovered_tx_indices` (COVER-1/2). Walks `txs` in order over a
    /// running binding map seeded from `bindings_before`; returns the indices of
    /// txs that **violate** the binding — a real-sender signed tx whose
    /// `pq_public_key` differs from the key already bound to its `from`. The FIRST
    /// signed tx from an as-yet-unbound `from` **establishes** the binding (immutable
    /// thereafter) and is not a violation. Synthetic/unsigned txs bind nothing.
    ///
    /// This is the **stateful** half of closing CRYPTO-ID-1: once an account's
    /// independent ML-DSA key is bound, no later tx may swap in a different key — so
    /// a future Ed25519 break can neither re-bind an attacker's key nor produce the
    /// bound (independent) key's signature. Used by BOTH validation (reject) and
    /// seal (exclude), so the two can never disagree (the COVER-2 lesson).
    fn binding_violations(
        bindings_before: &HashMap<String, String>,
        txs: &[Transaction],
    ) -> Vec<usize> {
        let synthetic = |a: &str| matches!(a, "NETWORK" | "BURN" | "ESCROW");
        let mut bound = bindings_before.clone();
        let mut out = Vec::new();
        for (i, tx) in txs.iter().enumerate() {
            if synthetic(&tx.from) {
                continue;
            }
            // A real sender with no ML-DSA key is already rejected by `verify_tx`;
            // here we only police key *consistency* for keyed txs.
            let key = match tx.pq_public_key.as_deref() {
                Some(k) if !k.is_empty() => k,
                _ => continue,
            };
            match bound.get(&tx.from) {
                Some(existing) if existing != key => out.push(i),
                Some(_) => {}
                None => {
                    bound.insert(tx.from.clone(), key.to_string());
                }
            }
        }
        out
    }

    /// AUDIT-BLK-2: Stateless block validation against an explicit `prev`.
    /// Used by `integrate_remote_block` during fork reorg, where the relevant
    /// `prev` is the block at `tip.index - 1` rather than the current tip.
    /// Doesn't enforce index continuity (caller checks) but recomputes the
    /// block hash and verifies every transaction signature.
    ///
    /// COVER-1: also enforces **sequential coverage** against `onchain_before`
    /// (the on-chain spendable balances up to `prev`, from
    /// [`Self::onchain_spendable_before`]). Because BOTH integration paths route
    /// through here, no node can finalize a spend or stake of coins that don't
    /// exist — on either path (FORK-CAP lesson).
    fn validate_block_against_prev(
        block: &Block,
        prev: &Block,
        onchain_before: &HashMap<String, i128>,
        bindings_before: &HashMap<String, String>,
        bonded_before: &HashMap<String, u64>,
    ) -> Result<(), String> {
        if block.prev_hash != prev.hash {
            return Err(format!(
                "prev_hash mismatch: block says {} but prev is {}",
                short(&block.prev_hash, 16),
                short(&prev.hash, 16)
            ));
        }
        // PROPOSER-1 (GENESIS-V4) — deterministic proposer check. The reported
        // CRITICAL was that the PoS election was checked only on the SEAL side
        // (`mining_loop`), never on receive: a modified node could seal any slot
        // with any address — staked or not — and the network accepted it. Here we
        // reject, on every admission path (this is the SHARED validator: linear
        // integration, 1-block fork tie-break, deep reorg trial clone, and sync),
        // any non-genesis block whose proposer is not a **bonded validator as of
        // the parent** (`bonded_before`, stake ≥ MIN_VALIDATOR_STAKE).
        //
        // Why bonded-membership and not "is the elected leader/fallback": without a
        // trusted clock we cannot time-gate the fallback tiers, so the deterministic
        // rule is the time-ungated UNION of {primary ∪ fallbacks ∪ any-eligible} =
        // "any bonded validator". That is a SUPERSET of every proposer
        // `is_valid_proposer` (the seal side) can ever return, so an honestly-sealed
        // block always passes (seal/receive never disagree), it is clock-free (no
        // drift forks — C1), and it fully closes the hole: an unstaked/arbitrary
        // address is rejected. Out-of-turn proposing by a *bonded* validator is
        // contained by fork-choice (LMD-GHOST + lexicographic tie-break) and, for
        // equivocation, by the finality slashing gadget.
        //
        // Bootstrap: before anyone has staked, `bonded_before` has no eligible
        // entry → sealing is permissionless (mirrors `mining_loop`'s bootstrap
        // branch), so the fresh v4 chain can start from an empty genesis.
        if block.index > 0 {
            let has_eligible = bonded_before
                .values()
                .any(|&s| s >= crate::p2p::pos_consensus::MIN_VALIDATOR_STAKE);
            if has_eligible {
                let proposer_stake =
                    bonded_before.get(&block.miner).copied().unwrap_or(0);
                if proposer_stake < crate::p2p::pos_consensus::MIN_VALIDATOR_STAKE {
                    return Err(format!(
                        "bloc rejeté : proposeur {} non bondé — seul un validateur avec enjeu ≥ {} µQTA peut proposer (PROPOSER-1)",
                        short(&block.miner, 12),
                        crate::p2p::pos_consensus::MIN_VALIDATOR_STAKE
                    ));
                }
            }
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
                    short(&tx.id, 16)
                ));
            }
        }
        // PQ-MIG-3 §1/§4: enforce the ML-DSA key binding (the stateful half of
        // closing CRYPTO-ID-1). A block carrying a real-sender tx whose ML-DSA key
        // ≠ the key already bound to its account is INVALID and rejected whole —
        // this is the "clé non liée rejetée" guarantee (an attacker cannot attach
        // its own key to someone else's account). Same single rule the seal path
        // uses to EXCLUDE (COVER-2 symmetry), so reject/exclude never disagree.
        let binding_viol = Self::binding_violations(bindings_before, &block.transactions);
        if let Some(&i) = binding_viol.first() {
            let tx = &block.transactions[i];
            return Err(format!(
                "bloc rejeté : clé ML-DSA non liée — {} présente une clé ≠ celle liée à son compte (CRYPTO-ID-1)",
                short(&tx.from, 12)
            ));
        }
        // BLK-HASH-1: same canonical hash as production via the shared
        // `block_hash_hex` (PQ-MIG-5) — includes `miner` and a content-binding
        // Merkle root.
        let expected_hash = Self::block_hash_hex(
            block.index,
            &block.prev_hash,
            &block.timestamp,
            &block.miner,
            &block.transactions,
        );
        if block.hash != expected_hash {
            return Err(format!(
                "block hash mismatch: declared {} but computed {}",
                short(&block.hash, 16),
                short(&expected_hash, 16)
            ));
        }
        // COVER-1 §2–§3: sequential coverage against the on-chain balance. A block
        // received from a peer with ANY uncovered real-sender tx is INVALID and
        // rejected whole — on whichever integration path called us. The coverage
        // rule itself lives in the shared `uncovered_tx_indices` (COVER-2: same
        // single source of truth the seal path uses to EXCLUDE, so reject and
        // exclude can never disagree).
        let uncovered = Self::uncovered_tx_indices(onchain_before, &block.transactions);
        if let Some(&i) = uncovered.first() {
            let tx = &block.transactions[i];
            return Err(format!(
                "bloc rejeté : dépense non couverte — {} en {} {} µQTA sans couverture (COVER-1)",
                short(&tx.from, 12),
                if matches!(tx.tx_type, TxType::Stake) { "enjeu" } else { "transfère" },
                tx.amount
            ));
        }
        Ok(())
    }

    /// COVER-1/COVER-2 — the **single source of truth** for sequential coverage.
    /// Walks `txs` in declared order over a running balance seeded from
    /// `onchain_before` (the on-chain spendable state up to the parent, from
    /// [`Self::onchain_spendable_before`]); returns the indices of the txs that are
    /// **uncovered** — a REAL-sender `Transfer`/`Stake` whose running balance is
    /// insufficient at that point. An uncovered tx's effect is **not** applied to
    /// the running balance (it will be rejected/excluded, so it must not fund later
    /// txs), while every covered tx, every synthetic-sender tx (`NETWORK`/`ESCROW`
    /// mint; `BURN` destination-only), and every `Unstake` (no spendable debit)
    /// applies its move — so **intra-block credits fund later txs** (§3).
    ///
    /// Two consumers, one rule: validation ([`Self::validate_block_against_prev`])
    /// REJECTS the whole block iff this is non-empty (COVER-1); seal
    /// ([`Self::seal_block_at`]) EXCLUDES exactly these txs to build a
    /// valid-by-construction block (COVER-2). One rule ⇒ a self-sealed block always
    /// passes validation.
    fn uncovered_tx_indices(
        onchain_before: &HashMap<String, i128>,
        txs: &[Transaction],
    ) -> Vec<usize> {
        let synthetic = |a: &str| matches!(a, "NETWORK" | "BURN" | "ESCROW");
        let mut running: HashMap<String, i128> = onchain_before.clone();
        let mut uncovered = Vec::new();
        for (i, tx) in txs.iter().enumerate() {
            if matches!(tx.tx_type, TxType::Unstake) {
                continue; // no spendable debit to cover
            }
            // LIVE-3: a Slash spends LOCKED stake (STAKE sink), not the offender's
            // spendable balance — its legitimacy is checked by the embedded fault
            // proof + bonded-stake amount in `validate_block_against_prev`, not by
            // spendable coverage. Exempt it here (like Unstake) so it neither
            // requires spendable cover nor perturbs the running balance.
            if matches!(tx.tx_type, TxType::Slash) {
                continue;
            }
            if !synthetic(&tx.from) {
                let have = running.get(&tx.from).copied().unwrap_or(0);
                if have < tx.amount as i128 {
                    uncovered.push(i);
                    continue; // excluded/rejected → do NOT move the running balance
                }
            }
            // Mirror `cache_apply_tx` so intra-block credits fund later txs.
            if !synthetic(&tx.to) {
                *running.entry(tx.to.clone()).or_insert(0) += tx.amount as i128;
            }
            if !synthetic(&tx.from) {
                *running.entry(tx.from.clone()).or_insert(0) -= tx.amount as i128;
            }
        }
        uncovered
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
            // Deterministic tie-break: keep the block with the lexicographically higher
            // hash. Both nodes will converge to the same choice.
            if block.hash > tip.hash {
                log::warn!(
                    "◈ [Ledger] FORK at height {} — remote block wins ({}... > {}...)",
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
                Self::validate_block_against_prev(&block, &prev_for_remote, &onchain_before, &bindings_before, &bonded_before)?;
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
                    "◈ [Ledger] FORK at height {} — our block wins ({}... > {}...)",
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
        popped
    }

    /// BLK-HASH-1: canonical **content** commitment for a tx — a fixed-order,
    /// domain-separated string of its content. NEVER the positional counter
    /// `tx.id`, and NO map iteration (determinism, §3). Two txs with different
    /// content always differ here; the same tx is identical on every node.
    fn tx_content_bytes(tx: &Transaction) -> String {
        let base = format!(
            "from={}|to={}|amount={}|nonce={}|type={:?}|ts={}",
            tx.from, tx.to, tx.amount, tx.nonce, tx.tx_type, tx.timestamp
        );
        // LIVE-3: bind the FULL embedded fault proof into the Merkle leaf for a
        // Slash, so the block hash commits to it — a relay cannot swap the proof for
        // another (even a valid one) without changing the block hash. Appended ONLY
        // when present, so every non-slash tx's content stays byte-identical (no
        // existing block/genesis hash shifts). Deterministic (the proof JSON is
        // built from deterministic votes).
        let base = match &tx.fault_proof {
            Some(proof) => format!("{base}|slash_proof={proof}"),
            None => base,
        };
        // LIVE-3B: bind the consumed-unbonding breakdown too — it drives BOTH the
        // apply (which entries die) and the reorg revert (which entries come back),
        // so a relay must not be able to alter it without changing the block hash.
        // Appended only when present: purely-bonded slashes and all other txs stay
        // byte-identical (genesis/history hashes unshifted).
        match &tx.slash_unbonding {
            Some(consumed) => match serde_json::to_string(consumed) {
                Ok(j) => format!("{base}|slash_unbonding={j}"),
                // Serialization of plain structs cannot fail; if it somehow did,
                // fall back to a marker that still perturbs the leaf (never allow
                // a silent unbound breakdown).
                Err(_) => format!("{base}|slash_unbonding=<unserializable>"),
            },
            None => base,
        }
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
                h.update(tx.signature.as_bytes()); // bind the Ed25519 signature
                // PQ-MIG-3 §2: bind the ML-DSA authority layer (key + signature)
                // into the block hash, so a peer cannot strip or swap it post-seal.
                h.update(tx.pq_public_key.as_deref().unwrap_or("").as_bytes());
                h.update(tx.pq_signature.as_deref().unwrap_or("").as_bytes());
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
    ///
    /// COVER-1 §4 — the `.max(0)` clamp is REVIEWED and **kept** (the spec's
    /// "ne force pas" branch). COVER-1 closes uncovered spends at the **block**
    /// (on-chain) level, so no *sealed* balance can go negative. But this cache is
    /// **pending-inclusive**, and remote-tx admission (`replay_remote_tx`) has no
    /// coverage gate — gating it would risk rejecting an out-of-order arrival
    /// whose funding tx hasn't landed yet, breaking AUDIT-TX-2 convergence (so §5
    /// keeps mempool coverage optional). A transient uncovered *pending* tx can
    /// thus still drive a cache entry negative, and an unclamped `i128 as u64` of a
    /// negative wraps to a colossal fabricated balance — far worse than clamping.
    /// So removal has real other effects → the clamp stays. A true *on-chain*
    /// negative (a genuine bug) is still surfaced loudly: it never reaches the
    /// cache (rejected at validation), and any residual phantom inflates the
    /// conservation sum (`Σ ≠ minted`) rather than hiding.
    pub fn balance_of(&self, pk: &str) -> u64 {
        self.balance_cache.get(pk).copied().unwrap_or(0).max(0) as u64
    }

    /// Get all **spendable** balances in µQTA. Excludes the synthetic
    /// `STAKE` sink (ONCHAIN-STAKE-1: locked stake is not a spendable holder —
    /// it is accounted via [`Self::locked_stake_total`]); `NETWORK`/`BURN`/`ESCROW`
    /// never appear here (they are never credited). PERF-1: O(accounts) direct read
    /// from cache instead of O(transactions) scan.
    pub fn all_balances(&self) -> HashMap<String, u64> {
        self.balance_cache
            .iter()
            .filter(|(k, _)| k.as_str() != Self::STAKE_SINK)
            .map(|(k, v)| (k.clone(), (*v).max(0) as u64))
            .collect()
    }

    /// ONCHAIN-STAKE-1 §5: total µQTA currently **locked in stake** — the balance
    /// of the synthetic `STAKE` sink. This is bonded stake + unbonding stake +
    /// any pending (un-sealed) stake — all **locked, not destroyed**. Conservation
    /// counts it on the balances side: `Σ spendable + locked_stake + burned == minted`.
    /// In a sealed state (no pending stake) it equals `staked_total + unbonding_total`.
    pub fn locked_stake_total(&self) -> u64 {
        self.balance_cache
            .get(Self::STAKE_SINK)
            .copied()
            .unwrap_or(0)
            .max(0) as u64
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

    /// TX-AUTH-NONCE-1 §2: the canonical signed pre-image of a tx — the exact
    /// bytes the signature covers AND that `tx.hash` commits to. Now includes the
    /// **nonce**, so altering a signed tx's nonce breaks BOTH the signature and
    /// the recomputed hash (closes the nonce-griefing / hash-malleability vector).
    /// Used identically at creation (`next_tx_at`), the coalesced reward, and
    /// verification (`verify_tx`) — ONE encoding, no divergence (C1). The first
    /// six fields are unchanged from the pre-TX-AUTH pre-image; only `:nonce` is
    /// appended.
    /// PQ-MIG-3 §2 — the signed pre-image now binds `pq_pk` (the ML-DSA public key
    /// the tx reveals). The Ed25519 **and** ML-DSA signatures both cover these
    /// bytes, so swapping the revealed key changes the pre-image and invalidates
    /// **both** signatures — an attacker cannot keep a valid signature while
    /// substituting another key, and the Ed25519 signature that authorizes the
    /// FIRST on-chain binding commits to exactly which key is being bound.
    /// Synthetic/unsigned txs pass `pq_pk = ""` (they are `verify_tx`-exempt).
    // The canonical pre-image legitimately commits to all tx fields plus the
    // bound key; bundling them into a struct would only obscure the wire format.
    #[allow(clippy::too_many_arguments)]
    fn tx_signing_preimage(
        id: &str,
        from: &str,
        to: &str,
        amount: u64,
        ts: &str,
        tx_type: &TxType,
        nonce: u64,
        pq_pk: &str,
    ) -> String {
        format!("{}:{}:{}:{}:{}:{:?}:{}:{}", id, from, to, amount, ts, tx_type, nonce, pq_pk)
    }

    fn next_tx(
        &mut self,
        from: &str,
        to: &str,
        amount: u64,
        tx_type: TxType,
        nonce: u64,
        pq_pk: &str,
    ) -> (String, String, String, String) {
        // Production reads the wall clock at the boundary and delegates to the
        // injected-time core (C7 / Phase 0, Constitution §3 — same pattern as
        // `seal_block`/`seal_block_at`).
        self.next_tx_at(from, to, amount, tx_type, Utc::now().to_rfc3339(), nonce, pq_pk)
    }

    /// Build a tx id/timestamp/payload/hash with an **injected** RFC3339
    /// `ts`, so the deterministic core (`sm::Node` / the DST harness) creates
    /// txs reproducibly without reading the wall clock. Pure extraction of
    /// `next_tx`: identical `id:from:to:amount:ts:type` pre-image and BLAKE3
    /// hash — only the timestamp source changes.
    #[allow(clippy::too_many_arguments)]
    fn next_tx_at(
        &mut self,
        from: &str,
        to: &str,
        amount: u64,
        tx_type: TxType,
        ts: String,
        nonce: u64,
        pq_pk: &str,
    ) -> (String, String, String, String) {
        self.tx_counter += 1;
        let id = format!("tx_{}", self.tx_counter);
        let payload = Self::tx_signing_preimage(&id, from, to, amount, &ts, &tx_type, nonce, pq_pk);
        let hash = hex::encode(blake3::hash(payload.as_bytes()).as_bytes());
        (id, ts, payload, hash)
    }

    /// Build an unsigned (network-issued) tx. Used for Mining and reward
    /// QUANTA. `pub(crate)` so cross-module tests (e.g. the LIVE-3 reorg teeth in
    /// `finality_live`) can forge competing fork blocks with network-issued rewards.
    pub(crate) fn build_unsigned_tx(
        &mut self,
        from: &str,
        to: &str,
        amount: u64,
        tx_type: TxType,
    ) -> Transaction {
        // Network-issued (synthetic) txs carry no account nonce → nonce 0, and no
        // ML-DSA authority key (they are `verify_tx`-exempt) → empty `pq_pk`.
        let (id, ts, _payload, hash) = self.next_tx(from, to, amount, tx_type.clone(), 0, "");
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
            fault_proof: None,
            slash_unbonding: None,
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

        // PQ-MIG-3 §2: the revealed ML-DSA key (the **independent primary**, NOT the
        // legacy seed-derived layer) must be known BEFORE building the pre-image, so
        // the pre-image — and therefore both signatures and the tx hash — bind it.
        let pq_pk = crypto
            .pq_identity_hex()
            .ok_or("No ML-DSA primary identity for tx authority")?;
        let (id, ts, payload, hash) =
            self.next_tx_at(from, to, amount, tx_type.clone(), ts, nonce, &pq_pk);
        // PQ-MIG-3: authority is signed on BOTH layers (Ed25519 + primary ML-DSA)
        // over the key-binding pre-image. SIGN-DET-VERIFY: the deterministic signer
        // (`sign_tx_authority_det`) is `#[cfg(test)]`, so the `det_sign` branch
        // exists ONLY in test builds — a release build is always hedged (`OsRng`)
        // and `det_sign` can only be set `true` by a `#[cfg(test)]` caller.
        #[cfg(test)]
        let (classical, quantum, signed_pq_pk) = if det_sign {
            crypto.sign_tx_authority_det(payload.as_bytes())?
        } else {
            crypto.sign_tx_authority(payload.as_bytes())?
        };
        #[cfg(not(test))]
        let (classical, quantum, signed_pq_pk) = {
            debug_assert!(
                !det_sign,
                "deterministic ML-DSA signing must never reach a non-test build"
            );
            crypto.sign_tx_authority(payload.as_bytes())?
        };
        // Invariant: the key that signed == the key bound into the pre-image.
        debug_assert_eq!(signed_pq_pk, pq_pk, "authority key must match pre-image key");
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
            fault_proof: None,
            slash_unbonding: None,
        })
    }

    /// MSIG-1 — build a signed M-of-N multisig transaction. `signers` is the subset
    /// (≥ `threshold`) of the registered keys that sign now; each signs the SAME
    /// pre-image **independently** (no threshold cryptography). The result verifies
    /// through [`Self::verify_multisig`]. `pub(crate)` for tests + the future wallet
    /// multisig flow. Fails on an invalid policy (empty keys, threshold out of range).
    ///
    /// `#[cfg(test)]` for now: the only caller is the test suite. The production
    /// multi-party signing flow (each holder signs offline, signatures combined)
    /// lands with the wallet UX; the on-chain verification path is already live.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn build_multisig_tx(
        &mut self,
        to: &str,
        amount: u64,
        tx_type: TxType,
        pubkeys: &[String],
        threshold: u32,
        signers: &[&CryptoEngine],
    ) -> Result<Transaction, String> {
        let keys = crate::security::canonicalize_msig_keys(pubkeys)
            .ok_or("politique multisig invalide (clé malformée)")?;
        if threshold == 0 || threshold as usize > keys.len() {
            return Err("politique multisig invalide (seuil hors bornes)".into());
        }
        let from = crate::security::multisig_address_hex(&keys, threshold)
            .ok_or("politique multisig invalide")?;
        let nonce = self.get_nonce(&from);
        self.increment_nonce(&from);
        let (id, ts, payload, hash) = self.next_tx_at(
            &from,
            to,
            amount,
            tx_type.clone(),
            Utc::now().to_rfc3339(),
            nonce,
            MSIG_TAG,
        );
        let mut signatures = Vec::with_capacity(signers.len());
        for s in signers {
            signatures.push(hex::encode(s.sign_pq(payload.as_bytes())?));
        }
        let auth = MultisigAuth { pubkeys: keys, threshold, signatures };
        let auth_json = serde_json::to_string(&auth).map_err(|_| "sérialisation multisig")?;
        Ok(Transaction {
            id,
            from,
            to: to.into(),
            amount,
            tx_type,
            timestamp: ts,
            signature: String::new(),
            hash,
            nonce,
            pq_signature: Some(auth_json),
            pq_public_key: Some(MSIG_TAG.to_string()),
            fault_proof: None,
            slash_unbonding: None,
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
            finalized_floor_index: self.finalized_floor_index,
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
            staked: HashMap::new(),
            unbonding: HashMap::new(),
            finalized_floor_index: snap.finalized_floor_index, // LIVE-2: irreversible across restart
        };
        // PERF-1 + ONCHAIN-STAKE-1: rebuild the balance cache AND the on-chain
        // stake state from the restored chain. Both are block-index-anchored, so
        // a restored node reconstructs byte-identical `staked`/`unbonding` maps.
        ledger.rebuild_cache();
        ledger
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PQ-MIG-3 test wallet: an engine carrying an **independent ML-DSA primary**
    /// (the tx-authority key the ledger binds). Returned without an Ed25519
    /// keypair yet — callers that need one still call `generate_keypair()`, which
    /// sets the Ed25519 layer **without disturbing** the primary. So a wallet that
    /// signs a tx satisfies `verify_tx`'s mandatory ML-DSA layer + the binding.
    /// Uses `CryptoEngine::default()` (≡ `new()`) so the engine starts empty.
    fn pq_wallet() -> CryptoEngine {
        let mut c = CryptoEngine::default();
        c.generate_pq_identity().expect("ml-dsa primary");
        c
    }

    /// PQ-MIG-3B test helper — a wallet's **account identity** is its ML-DSA
    /// address (`from`/`to`/miner/balance key under ADR-007 b). Installs the
    /// Ed25519 signing layer (so `sign_tx_authority` co-signs the pre-image) and
    /// returns `pq_address_hex()`. Replaces the model-(a) `…public_key_hex`.
    fn gen_addr(c: &mut CryptoEngine) -> String {
        c.generate_keypair();
        c.pq_address_hex().expect("ml-dsa address")
    }

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
        // PQ-MIG-3: une tx signée porte des champs PQ **non vides** (la clé +
        // signature ML-DSA du primaire), et `verify_tx` exige strictement les deux
        // couches (Ed25519 + ML-DSA) — il n'y a plus de repli Ed25519 seul.
        let mut crypto = pq_wallet();
        let id = gen_addr(&mut crypto);
        let mut ledger = Ledger::new();
        // Donne un solde au signataire via une mining tx réseau (5 QUANTA = 5_000_000
        // µQTA).
        ledger.mine_tx(&id, 5 * MICRO, 0.0);

        // Adresse destinataire valide (64 chars hex)
        let to = "b".repeat(64);
        let tx = ledger
            .transfer_tx(&id, &to, MICRO, &crypto)
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
            !tx.pq_signature.as_deref().unwrap_or("").is_empty(),
            "la couche ML-DSA est non vide (autorité PQ obligatoire)"
        );
        assert!(
            Ledger::verify_tx(&tx).unwrap(),
            "verify_tx doit valider la tx (Ed25519 + ML-DSA stricts)"
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

    // ─── PQ-MIG-5 : genèse post-quantique (les dents §5) ────────────────────

    /// **PQ-MIG-5 §5 — genèse déterministe.** Deux constructions de la même
    /// allocation ⇒ hash **identique** (C1), et le hash est **figé** sur un vecteur
    /// connu (comme les vecteurs d'adresses épinglés de PQ-MIG-2). La genèse par
    /// défaut (`new()`, allocation vide, zéro premine) et la genèse d'allocation DEV
    /// ont chacune un hash fixe ; le hash **lie le contenu** — changer l'allocation
    /// change le hash (vide ≠ alloué).
    #[test]
    fn pqmig5_genesis_hash_is_deterministic_and_frozen() {
        assert_eq!(
            Ledger::new().chain[0].hash,
            Ledger::new().chain[0].hash,
            "genèse vide : deux constructions ⇒ même hash (C1)"
        );
        assert_eq!(
            Ledger::genesis_with_allocation(Ledger::DEV_GENESIS_ALLOCATION).chain[0].hash,
            Ledger::genesis_with_allocation(Ledger::DEV_GENESIS_ALLOCATION).chain[0].hash,
            "genèse allouée : deux constructions ⇒ même hash (C1)"
        );
        // Vecteurs figés — GENESIS-V4 (timestamp 2026-07-18), recalculés via le
        // hachage de bloc existant. Le changement délibéré de la genèse met à jour
        // ces vecteurs gelés (c'est leur rôle : verrouiller la genèse contre toute
        // dérive silencieuse ; une refonte volontaire les réinitialise).
        assert_eq!(
            Ledger::new().chain[0].hash,
            "ee58235deda396dbb7c84ac2e86829c990fa5562e89b4edcb80caf19ac2a1dff",
            "hash de genèse vide figé (v4)"
        );
        assert_eq!(
            Ledger::genesis_with_allocation(Ledger::DEV_GENESIS_ALLOCATION).chain[0].hash,
            "875cb2b2a8912b76db46af04eae064b06e5a2ea5183a9ed030b4bee14a925160",
            "hash de genèse DEV figé (v4)"
        );
        assert_ne!(
            Ledger::new().chain[0].hash,
            Ledger::genesis_with_allocation(Ledger::DEV_GENESIS_ALLOCATION).chain[0].hash,
            "le hash lie le contenu : vide ≠ alloué"
        );
    }

    /// **PROPOSER-1 (GENESIS-V4) — the CRITICAL is closed on receive.** Once a
    /// validator is bonded, a block whose proposer is NOT a bonded validator is
    /// rejected on the receive path (`integrate_remote_block`), while a block
    /// proposed by a bonded validator is accepted. This is the fix for
    /// "any address could seal any slot and the network accepted it".
    #[test]
    fn proposer1_rejects_unbonded_proposer_once_staked() {
        use crate::p2p::pos_consensus::MIN_VALIDATOR_STAKE;
        // DEV genesis already bonds G0 (10 QTA) and G1 (5 QTA) — both ≥ MIN.
        let base = Ledger::genesis_with_allocation(Ledger::DEV_GENESIS_ALLOCATION);
        assert!(
            base.validator_stakes().values().any(|&s| s >= MIN_VALIDATOR_STAKE),
            "précondition : au moins un validateur bondé"
        );

        // (a) a block proposed by a NON-validator address is REJECTED.
        let attacker = "z".repeat(64);
        let mut evil_src = base.clone();
        let evil = evil_src.seal_block_at(&attacker, 0.0, "2026-07-19T00:00:00Z".into());
        assert_eq!(evil.miner, attacker, "le bloc malveillant se scelle sous l'attaquant");
        let mut a = base.clone();
        assert!(
            a.integrate_remote_block(evil).is_err(),
            "PROPOSER-1 : un proposeur non bondé est rejeté à la réception"
        );
        assert_eq!(a.chain_height(), 1, "chaîne non étendue par le proposeur non bondé");

        // (b) a block proposed by a bonded genesis validator is ACCEPTED.
        let mut good_src = base.clone();
        let good = good_src.seal_block_at(Ledger::GENESIS_ADDR_0, 0.0, "2026-07-19T00:00:00Z".into());
        let mut b = base.clone();
        assert!(
            b.integrate_remote_block(good).is_ok(),
            "un proposeur validateur bondé est accepté"
        );
        assert_eq!(b.chain_height(), 2, "la chaîne est étendue par le validateur");
    }

    /// **PROPOSER-1 bootstrap — permissionless before anyone stakes.** On the fresh
    /// empty v4 genesis (zero validators) any proposer may seal, so the chain can
    /// start; the check only bites once `has_eligible` becomes true.
    #[test]
    fn proposer1_bootstrap_allows_any_proposer() {
        let base = Ledger::new(); // empty v4 genesis, zero bonded validators
        assert!(base.validator_stakes().is_empty(), "aucun validateur au démarrage");
        let anyone = "b".repeat(64);
        let mut src = base.clone();
        let blk = src.seal_block_at(&anyone, 0.0, "2026-07-19T00:00:00Z".into());
        let mut a = base.clone();
        assert!(
            a.integrate_remote_block(blk).is_ok(),
            "bootstrap : proposeur permissionless accepté tant que personne n'a staké"
        );
        assert_eq!(a.chain_height(), 2);
    }

    /// **PROPOSER-1 determinism lock.** `staked_before(tip)` (the pure chain replay
    /// used on the rare fork tie-break) MUST equal the live `validator_stakes()`
    /// (used on the O(1) linear/clone paths), so every admission path computes the
    /// identical bonded set as of the parent — no node ever disagrees on a
    /// proposer's eligibility.
    #[test]
    fn staked_before_matches_live_cache() {
        let l = Ledger::genesis_with_allocation(Ledger::DEV_GENESIS_ALLOCATION);
        let tip = l.chain.last().unwrap().clone();
        assert_eq!(
            l.staked_before(&tip),
            l.validator_stakes(),
            "staked_before(tip) == validator_stakes() (les deux sources de PROPOSER-1 concordent)"
        );
    }

    /// **PQ-MIG-5 §5 — conservation à la genèse (EMIT-1 / classe conservation).**
    /// Au bloc 0 : `miné == Σ(solde + enjeu)` (l'enjeu vient de l'allocation, pas
    /// créé en plus) et `Σ dépensable + enjeu-verrouillé + brûlé == miné`. Puis la
    /// **dent négative** : planter un enjeu de genèse **non couvert** par une
    /// émission (un `Stake` sans `Mining` derrière) **casse** l'égalité de
    /// conservation — la vérification mord, un déséquilibre de genèse ne passe pas.
    #[test]
    fn pqmig5_dev_genesis_conserves_at_block_zero() {
        let l = Ledger::genesis_with_allocation(Ledger::DEV_GENESIS_ALLOCATION);
        let expected_alloc: u64 = Ledger::DEV_GENESIS_ALLOCATION
            .iter()
            .map(|(_, bal, stake)| bal + stake)
            .sum();
        assert_eq!(l.total_minted(), expected_alloc, "miné == Σ(solde + enjeu)");
        assert_eq!(l.total_minted(), 100 * MICRO, "allocation DEV = 100 QTA");
        assert_eq!(
            conservation_lhs(&l),
            l.total_minted(),
            "Σ dépensable + enjeu-verrouillé + brûlé == miné, dès le bloc 0"
        );
        assert_eq!(l.total_burned(), 0, "rien de brûlé à la genèse");
        assert_eq!(l.locked_stake_total(), 15 * MICRO, "enjeu de genèse = 10 + 5");

        // Dent négative — planter un enjeu non couvert (Stake sans Mining derrière).
        let mut bad = Ledger::genesis_with_allocation(&[]); // miné 0
        let planted = Ledger::genesis_tx(
            "planted_unbacked_stake",
            Ledger::GENESIS_ADDR_0,
            Ledger::STAKE_SINK,
            10 * MICRO,
            TxType::Stake,
        );
        bad.chain[0].transactions.push(planted);
        bad.rebuild_cache();
        assert_eq!(bad.total_minted(), 0, "aucune émission ne couvre l'enjeu planté");
        assert_eq!(bad.locked_stake_total(), 10 * MICRO, "10 verrouillés sans backing");
        assert_ne!(
            conservation_lhs(&bad),
            bad.total_minted(),
            "enjeu de genèse non couvert ⇒ conservation cassée (la vérif mord)"
        );
    }

    /// **PQ-MIG-5 §5 — validateurs initiaux.** `validator_stakes()` au bloc 0
    /// reflète **exactement** le mapping de genèse, indexé par **adresse ML-DSA** :
    /// les deux comptes à enjeu > 0 (G0, G1), pas le porteur simple (G2, enjeu 0).
    #[test]
    fn pqmig5_dev_genesis_validators_reflect_mapping() {
        let l = Ledger::genesis_with_allocation(Ledger::DEV_GENESIS_ALLOCATION);
        let stakes = l.validator_stakes();
        assert_eq!(stakes.len(), 2, "deux validateurs initiaux (enjeu > 0)");
        assert_eq!(stakes.get(Ledger::GENESIS_ADDR_0).copied(), Some(10 * MICRO));
        assert_eq!(stakes.get(Ledger::GENESIS_ADDR_1).copied(), Some(5 * MICRO));
        assert_eq!(
            stakes.get(Ledger::GENESIS_ADDR_2).copied(),
            None,
            "le porteur sans enjeu n'est pas un validateur"
        );
        // Mêmes valeurs via l'accès par compte (chaîne ⇒ état pur).
        assert_eq!(l.staked_of(Ledger::GENESIS_ADDR_0), 10 * MICRO);
        assert_eq!(l.staked_of(Ledger::GENESIS_ADDR_1), 5 * MICRO);
        assert_eq!(l.staked_of(Ledger::GENESIS_ADDR_2), 0);
        // Soldes dépensables = colonne « solde » du mapping (enjeu retiré).
        assert_eq!(l.balance_of(Ledger::GENESIS_ADDR_0), 50 * MICRO);
        assert_eq!(l.balance_of(Ledger::GENESIS_ADDR_1), 25 * MICRO);
        assert_eq!(l.balance_of(Ledger::GENESIS_ADDR_2), 10 * MICRO);
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
        let mut crypto = pq_wallet();
        let id = gen_addr(&mut crypto);
        let mut ledger = Ledger::new();
        ledger.mine_tx(&id, 100 * MICRO, 0.0);
        ledger.seal_block(&id, 0.0); // pending → chaîne (compté dans l'offre)
        let before = ledger.total_supply();
        let to = "e".repeat(64);
        let (_tx, _burn, burn_uqta) = ledger
            .transfer_with_burn(&id, &to, 50 * MICRO, &crypto)
            .unwrap();
        ledger.seal_block(&id, 0.0);
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
        let mut crypto = pq_wallet();
        let a = gen_addr(&mut crypto);
        let mut ledger = Ledger::new();
        ledger.mine_tx(&a, 100 * MICRO, 0.0);
        let b = "f".repeat(64);
        ledger
            .transfer_with_burn(&a, &b, 30 * MICRO, &crypto)
            .unwrap();
        ledger.seal_block(&a, 0.0); // chaîne et cache cohérents
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
        let mut crypto = pq_wallet();
        let s = gen_addr(&mut crypto);
        ledger.mine_tx(&s, 10 * MICRO, 0.0);
        let supply = ledger.total_supply();
        let r = "b".repeat(64);
        ledger
            .transfer_tx(&s, &r, 2 * MICRO, &crypto)
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

    // ─── FORK-CAP-1: le chemin fork-reorg applique la MÊME validation ───────
    // d'émission que le chemin linéaire. L'audit HARDEN-AUDIT-1 a confirmé 2×
    // que la branche de reorg sautait `validate_block_emission`, laissant un
    // adversaire réseau minter au-delà des 100M via un bloc de remplacement.
    // Ces tests pilotent la BRANCHE REORG (même hauteur, hash gagnant), là où
    // les anciens tests de plafond ne pilotaient que le chemin happy (tip+1).

    /// Construit une chaîne genesis + 1 bloc scellé (tip à la hauteur 1), pour
    /// qu'un reorg soit possible (`tip.index >= 1`). Renvoie le ledger, le tip
    /// honnête, et le hash de genesis auquel le fork doit se rattacher.
    fn forkcap_two_block_chain() -> (Ledger, Block, String) {
        let mut ledger = Ledger::new();
        let genesis_hash = ledger.chain.last().unwrap().hash.clone();
        let honest_miner = "f".repeat(64);
        ledger.mine_tx(&honest_miner, 2 * MICRO, 0.0); // émission légitime
        let tip = ledger.seal_block(&honest_miner, 0.0);
        assert_eq!(tip.index, 1, "setup: le tip doit être à la hauteur 1 pour un reorg");
        (ledger, tip, genesis_hash)
    }

    /// Forge un bloc de fork avec un hash CORRECT (même pré-image que la prod)
    /// qui bat lexicographiquement `must_beat`, pour gagner le tie-break et
    /// atteindre la validation. Le timestamp (libre, seulement haché) sert de
    /// nonce de grind — BLAKE3 est ~uniforme, donc quelques essais suffisent.
    fn forge_winning_fork(
        index: u64,
        prev_hash: &str,
        miner: &str,
        txs: Vec<Transaction>,
        must_beat: &str,
    ) -> Block {
        for nonce in 0..1_000_000u64 {
            let ts = format!("forkcap-grind-{nonce}");
            let block = Ledger::forge_block_at(index, prev_hash, &ts, miner, txs.clone());
            if block.hash.as_str() > must_beat {
                return block;
            }
        }
        panic!("impossible de grinder un hash de fork au-dessus du tip honnête");
    }

    /// Après un reorg REJETÉ : la chaîne n'a pas bougé, le tip honnête reste, et
    /// la masse en circulation ne dépasse JAMAIS le plafond dur.
    fn forkcap_assert_rejected(ledger: &Ledger, honest_tip: &Block) {
        assert_eq!(ledger.chain.len(), 2, "chaîne non réorganisée (reorg rejeté)");
        assert_eq!(
            ledger.chain.last().unwrap().hash,
            honest_tip.hash,
            "le tip honnête doit être conservé"
        );
        assert_eq!(
            ledger.stats().total_mined,
            2 * MICRO,
            "offre minée inchangée par le bloc rejeté"
        );
        assert!(
            ledger.total_supply() <= crate::p2p::reputation::MAX_SUPPLY_MICRO,
            "la masse ne dépasse JAMAIS 100M"
        );
    }

    #[test]
    fn forkcap_reorg_rejects_multiple_mining_rewards() {
        // (a) Plus d'une récompense de minage dans le bloc de reorg.
        let (mut ledger, tip, genesis_hash) = forkcap_two_block_chain();
        let attacker = "a".repeat(64);
        let r1 = ledger.build_unsigned_tx("NETWORK", &attacker, MICRO, TxType::Mining);
        let r2 = ledger.build_unsigned_tx("NETWORK", &attacker, MICRO, TxType::Mining);
        let evil = forge_winning_fork(tip.index, &genesis_hash, &attacker, vec![r1, r2], &tip.hash);
        assert!(
            evil.hash.as_str() > tip.hash.as_str(),
            "le fork doit gagner le tie-break pour atteindre la validation"
        );

        let res = ledger.integrate_remote_block(evil);
        assert!(res.is_err(), "(a) reorg à ≥2 récompenses de minage doit être REJETÉ");
        forkcap_assert_rejected(&ledger, &tip);
    }

    #[test]
    fn forkcap_reorg_rejects_emission_past_hard_cap() {
        // (b) Récompense sur-dimensionnée poussant la masse au-delà du plafond.
        let (mut ledger, tip, genesis_hash) = forkcap_two_block_chain();
        let attacker = "a".repeat(64);
        let huge = crate::p2p::reputation::MAX_SUPPLY_MICRO + 5 * MICRO;
        let reward = ledger.build_unsigned_tx("NETWORK", &attacker, huge, TxType::Mining);
        let evil = forge_winning_fork(tip.index, &genesis_hash, &attacker, vec![reward], &tip.hash);

        let res = ledger.integrate_remote_block(evil);
        assert!(
            res.is_err(),
            "(b) reorg dépassant le plafond dur 100M doit être REJETÉ"
        );
        forkcap_assert_rejected(&ledger, &tip);
    }

    #[test]
    fn forkcap_reorg_rejects_emission_per_block_bound() {
        // (b′) 1M QUANTA d'un coup — très sous 100M, mais des milliers de fois
        // l'émission/tick légitime : la borne PAR BLOC doit aussi le rejeter sur
        // le chemin de reorg.
        let (mut ledger, tip, genesis_hash) = forkcap_two_block_chain();
        let attacker = "a".repeat(64);
        let greedy = 1_000_000 * MICRO;
        let reward = ledger.build_unsigned_tx("NETWORK", &attacker, greedy, TxType::Mining);
        let evil = forge_winning_fork(tip.index, &genesis_hash, &attacker, vec![reward], &tip.hash);

        let res = ledger.integrate_remote_block(evil);
        assert!(
            res.is_err(),
            "(b′) reorg minant 1M QUANTA d'un coup doit être REJETÉ (borne par bloc)"
        );
        forkcap_assert_rejected(&ledger, &tip);
    }

    #[test]
    fn forkcap_reorg_rejects_reward_to_non_miner() {
        // (c) Récompense créditée à un autre que le mineur du bloc.
        let (mut ledger, tip, genesis_hash) = forkcap_two_block_chain();
        let miner = "a".repeat(64);
        let someone_else = "c".repeat(64);
        let reward = ledger.build_unsigned_tx("NETWORK", &someone_else, MICRO, TxType::Mining);
        let evil = forge_winning_fork(tip.index, &genesis_hash, &miner, vec![reward], &tip.hash);

        let res = ledger.integrate_remote_block(evil);
        assert!(
            res.is_err(),
            "(c) reorg créditant un non-mineur doit être REJETÉ"
        );
        forkcap_assert_rejected(&ledger, &tip);
    }

    #[test]
    fn forkcap_reorg_accepts_honest_competing_block() {
        // Contrôle positif (anti-masquage) : un fork HONNÊTE de même hauteur,
        // émission légitime + hash gagnant, DOIT être accepté. Fermer le plafond
        // ne doit pas rejeter les reorgs honnêtes ni « filtrer » par excès. Le
        // `prior_mined` exclut le tip remplacé, donc 2 QUANTA valident contre une
        // offre de 0, exactement comme un bloc à la hauteur 1.
        let (mut ledger, tip, genesis_hash) = forkcap_two_block_chain();
        let rival_miner = "e".repeat(64);
        let reward = ledger.build_unsigned_tx("NETWORK", &rival_miner, 2 * MICRO, TxType::Mining);
        let honest_fork =
            forge_winning_fork(tip.index, &genesis_hash, &rival_miner, vec![reward], &tip.hash);

        let res = ledger.integrate_remote_block(honest_fork.clone());
        assert_eq!(
            res,
            Ok(true),
            "un fork honnête de même hauteur (hash gagnant) doit être accepté"
        );
        assert_eq!(ledger.chain.len(), 2, "la chaîne reste à la même hauteur après reorg");
        assert_eq!(
            ledger.chain.last().unwrap().hash,
            honest_fork.hash,
            "le fork honnête devient le tip"
        );
        assert_eq!(
            ledger.stats().total_mined,
            2 * MICRO,
            "offre minée cohérente après un reorg honnête (le nouveau bloc remplace l'ancien)"
        );
        assert!(
            ledger.total_supply() <= crate::p2p::reputation::MAX_SUPPLY_MICRO,
            "masse ≤ 100M"
        );
    }

    // ─── GADGET-5B: multi-block fork reconciliation (reorg_to_fork) ─────────
    // The ledger mechanics the GHOST engine drives at partition heal: a
    // competing fork is adopted with full conservation (loser emission reverted,
    // user txs re-queued, synthetic rewards dropped) and the finalized floor is
    // absolute (a fork that would undo finalized history is refused).

    /// **GADGET-5B §2 tooth — conservation revert is load-bearing.** Reorging to
    /// a competing fork REVERTS the abandoned branch's emission (so it is never
    /// double-minted) and RE-QUEUES its user txs (never lost, AUDIT-BLK-1), while
    /// its synthetic reward is dropped — and `Σ(spendable+staked+unbonding)+burned
    /// == minted` still holds globally afterwards.
    #[test]
    fn gadget5b_reorg_reverts_loser_emission_and_requeues_user_tx() {
        let mut ledger = Ledger::new();
        let mut wallet = pq_wallet();
        let pk = gen_addr(&mut wallet);
        let y = "d".repeat(64);

        // G, b1 (mining → pk 100 QTA, funds the transfer).
        ledger.mine_tx(&pk, 100 * MICRO, 0.0);
        let b1 = ledger.seal_block(&pk, 0.0);
        assert_eq!(b1.index, 1);

        // A signed transfer pk→y (10 QTA) + its 1% burn, deterministic.
        let ts = "2026-06-25T00:00:00+00:00".to_string();
        let (xfer, burn_opt, _) = ledger
            .transfer_with_burn_at(&pk, &y, 10 * MICRO, &wallet, ts.clone(), true)
            .expect("signed transfer");
        let burn = burn_opt.expect("burn leg present");

        // LOSER block (index 2): the user transfer + burn + a 5 QTA mining reward.
        let loser_reward = ledger.build_unsigned_tx("NETWORK", &pk, 5 * MICRO, TxType::Mining);
        let loser = Ledger::forge_block_at(
            2,
            &b1.hash,
            &ts,
            &pk,
            vec![xfer.clone(), burn.clone(), loser_reward],
        );
        assert_eq!(ledger.integrate_remote_block(loser.clone()), Ok(true), "loser sealed at index 2");
        assert_eq!(ledger.pending_count(), 0, "transfer + burn now sealed in the loser block");
        assert_eq!(ledger.total_minted(), 105 * MICRO, "b1(100) + loser mining(5)");

        // WINNER fork (index 2, rooted at b1): mining only (7 QTA) — replaces the loser.
        let winner_reward = ledger.build_unsigned_tx("NETWORK", &pk, 7 * MICRO, TxType::Mining);
        let winner = Ledger::forge_block_at(2, &b1.hash, &ts, &pk, vec![winner_reward]);

        assert_eq!(
            ledger.reorg_to_fork(std::slice::from_ref(&winner), 0),
            Ok(true),
            "the competing fork is adopted (floor 0, valid)"
        );

        // (a) the winner is the new tip.
        assert_eq!(ledger.chain.last().unwrap().hash, winner.hash, "winner is the tip");
        assert_eq!(ledger.chain.len(), 3, "same height after the reorg");

        // (b) the loser's USER txs are re-queued; its mining reward is NOT.
        let pending = ledger.pending_txs();
        assert!(
            pending.iter().any(|t| t.hash == xfer.hash && t.tx_type == TxType::Transfer),
            "the loser's transfer is re-queued (AUDIT-BLK-1)"
        );
        assert!(
            pending.iter().any(|t| t.hash == burn.hash && t.tx_type == TxType::Burn),
            "the loser's burn (a user tx) is re-queued"
        );
        assert!(
            pending.iter().all(|t| t.tx_type != TxType::Mining),
            "the loser's mining reward is NOT re-queued — no double-mint (EMIT-1 §4.1)"
        );

        // (c) the loser's emission was UNDONE: minted counts the winner's 7 QTA,
        //     not the dropped loser's 5 (b1 100 + winner 7 = 107).
        assert_eq!(
            ledger.total_minted(),
            107 * MICRO,
            "loser emission reverted, winner counted — the revert is load-bearing"
        );

        // (d) global conservation holds after reconciliation.
        let spendable: u64 = ledger.all_balances().values().sum();
        assert_eq!(
            spendable + ledger.locked_stake_total() + ledger.total_burned(),
            ledger.total_minted(),
            "Σ(spendable+staked+unbonding)+burned == minted after reconciliation"
        );
    }

    /// **GADGET-5B §4 tooth — finality preserved (the floor is absolute).** A fork
    /// that would replace a **finalized** block can never win the reconciliation:
    /// `reorg_to_fork` refuses to disturb any block at or below `floor_index`. The
    /// positive control proves the floor blocks exactly — and only — what finality
    /// protects (a fork above the floor still reorganizes).
    #[test]
    fn gadget5b_reorg_to_fork_refuses_below_finalized_floor() {
        let mut ledger = Ledger::new();
        let miner = "f".repeat(64);
        ledger.mine_tx(&miner, 2 * MICRO, 0.0);
        let b1 = ledger.seal_block(&miner, 0.0);
        ledger.mine_tx(&miner, 2 * MICRO, 0.0);
        let b2 = ledger.seal_block(&miner, 0.0);
        assert_eq!(ledger.chain.len(), 3, "setup: G, b1, b2");

        // A valid competing block at index 2, rooted at b1 — it would REPLACE b2.
        let rival = "e".repeat(64);
        let reward = ledger.build_unsigned_tx("NETWORK", &rival, 2 * MICRO, TxType::Mining);
        let winner = Ledger::forge_block_at(2, &b1.hash, "fork-ts", &rival, vec![reward]);

        // Floor at index 2 (b2 finalized): the fork roots BELOW the floor (at b1=1),
        // so adopting it would undo finalized b2 → REFUSED, chain untouched.
        assert_eq!(
            ledger.reorg_to_fork(std::slice::from_ref(&winner), 2),
            Ok(false),
            "a fork that would replace a FINALIZED block is refused (5A floor, absolute)"
        );
        assert_eq!(ledger.chain.last().unwrap().hash, b2.hash, "finalized tip preserved");
        assert_eq!(ledger.chain.len(), 3, "chain unchanged by the refused reorg");

        // Positive control: floor at index 1 (only b1 finalized) ⇒ b2 is fair game,
        // so the SAME fork is adopted. The floor protects exactly what is finalized.
        assert_eq!(
            ledger.reorg_to_fork(std::slice::from_ref(&winner), 1),
            Ok(true),
            "a fork above the finalized floor reorganizes normally"
        );
        assert_eq!(ledger.chain.last().unwrap().hash, winner.hash, "the fork is now the tip");
    }

    // ─── LIVE-2: finality floor on the LIVE single-block-fork path ──────────

    #[test]
    fn live2_integrate_refuses_to_reorg_a_finalized_tip() {
        // The finality-safety property on the live path: once the tip is finalized
        // (floor at its index), a higher-hash competitor at the SAME height — which
        // would normally WIN the lexicographic tie-break — is REFUSED. Finalized
        // history is irreversible. This closes on `integrate_remote_block` the same
        // hole `reorg_to_fork` already guards on the multi-block path.
        let (mut ledger, tip, genesis_hash) = forkcap_two_block_chain();
        let rival = "e".repeat(64);
        let reward = ledger.build_unsigned_tx("NETWORK", &rival, 2 * MICRO, TxType::Mining);
        // A competitor engineered to BEAT the tip's hash (would win without finality).
        let winner =
            forge_winning_fork(tip.index, &genesis_hash, &rival, vec![reward], &tip.hash);
        assert!(winner.hash > tip.hash, "setup: the competitor wins the raw tie-break");

        // Finalize the tip (floor = its index + our matching hash). The higher-hash
        // fork must be refused.
        let floor = ledger.set_finalized_floor(tip.index, &tip.hash);
        assert_eq!(floor, tip.index, "floor advanced to the finalized tip");
        assert_eq!(
            ledger.integrate_remote_block(winner.clone()),
            Ok(false),
            "a fork replacing a FINALIZED tip is refused regardless of hash",
        );
        assert_eq!(ledger.chain.last().unwrap().hash, tip.hash, "finalized tip preserved");
        assert_eq!(ledger.chain.len(), 2, "chain unchanged by the refused reorg");
    }

    #[test]
    fn live2_integrate_still_reorgs_above_the_floor() {
        // Positive control (anti-masking): finality freezes ONLY what it finalized.
        // With the floor left at genesis (0), the SAME higher-hash competitor at
        // height 1 wins the tie-break and reorganizes normally — fork-choice stays
        // free above the floor (Gasper).
        let (mut ledger, tip, genesis_hash) = forkcap_two_block_chain();
        let rival = "e".repeat(64);
        let reward = ledger.build_unsigned_tx("NETWORK", &rival, 2 * MICRO, TxType::Mining);
        let winner =
            forge_winning_fork(tip.index, &genesis_hash, &rival, vec![reward], &tip.hash);
        assert_eq!(ledger.finalized_floor_index(), 0, "only genesis finalized");
        assert_eq!(
            ledger.integrate_remote_block(winner.clone()),
            Ok(true),
            "above the floor the free lexicographic tie-break still applies",
        );
        assert_eq!(ledger.chain.last().unwrap().hash, winner.hash, "the heavier fork won");
    }

    #[test]
    fn live2_finalized_floor_is_monotonic_hash_checked_and_bounded() {
        // The setter never lowers the floor (finality is irreversible), never freezes
        // a height the node doesn't hold, and (HIGH-4) only freezes when OUR block at
        // that height matches the finalized hash.
        let (mut ledger, tip, _g) = forkcap_two_block_chain(); // tip at index 1
        // HIGH-4: a WRONG hash at the finalized height must NOT advance the floor —
        // the node is on a finality-overruled fork and must be able to sync `X` later.
        assert_eq!(
            ledger.set_finalized_floor(1, "not-our-block-hash"),
            0,
            "a mismatched finalized hash does NOT freeze our (different) block",
        );
        // Matching hash → advance.
        assert_eq!(ledger.set_finalized_floor(1, &tip.hash), 1, "advances to a held+matching height");
        assert_eq!(ledger.set_finalized_floor(0, "GENESIS"), 1, "never moves backward");
        assert_eq!(
            ledger.set_finalized_floor(999, "whatever"),
            tip.index,
            "a not-yet-held height can't freeze (we don't have that block)",
        );
    }

    #[test]
    fn live2_floor_survives_snapshot_restore() {
        // The floor is persisted (it is NOT chain-derivable — it depends on votes),
        // so a restart keeps finalized history irreversible before votes re-flow.
        let (mut ledger, tip, _g) = forkcap_two_block_chain();
        ledger.set_finalized_floor(tip.index, &tip.hash);
        let restored = Ledger::restore(ledger.snapshot());
        assert_eq!(
            restored.finalized_floor_index(),
            tip.index,
            "the finality floor round-trips through snapshot/restore",
        );
    }

    // ─── SLICE-CLASS (HARDEN-HYGIENE-1): char-safe log truncation ───────────

    #[test]
    fn slice_short_is_char_safe() {
        assert_eq!(short("hello", 3), "hel"); // plain ASCII prefix
        assert_eq!(short("hello", 10), "hello"); // max >= len → whole string
        assert_eq!(short("", 5), "");
        // A multi-byte char straddling the cut index is DROPPED, not split
        // (byte-indexing there panics). "héllo": é occupies bytes 1..3.
        assert_eq!(short("héllo", 2), "h");
        assert_eq!(short("héllo", 1), "h");
        assert_eq!(short("é", 1), ""); // cut inside the only char → empty, no panic
        assert_eq!(short("ÿÿ", 99), "ÿÿ");
        // The exact shape that panicked the hard slice: 13 bytes, cut at 12 lands
        // mid-'é'. Must not panic.
        let s = format!("g{}é", "f".repeat(10));
        assert_eq!(short(&s, 12), &format!("g{}", "f".repeat(10)));
    }

    #[test]
    fn slice_multibyte_block_hash_does_not_panic_in_fork_log() {
        // A malicious peer sends a fork-height block whose `hash` is a short,
        // multi-byte string lexicographically ABOVE the tip — reaching the
        // fork-resolution log that used a HARD `&block.hash[..12]`, which
        // panicked the single gossip-dispatch task (remote DoS). With `short` it
        // logs safely and the block is REJECTED (hash mismatch), never panics.
        let (mut ledger, tip, genesis_hash) = forkcap_two_block_chain();
        // 13 bytes: 'g' + ten 'f' + 'é'; byte 12 lands inside 'é'. 'g' (0x67) >
        // any hex digit, so block.hash > tip.hash → enters the reorg `>` arm.
        let evil_hash = format!("g{}é", "f".repeat(10));
        assert!(
            evil_hash.as_str() > tip.hash.as_str(),
            "must win the tie-break to reach the log slice"
        );
        let evil = Block {
            index: tip.index,
            timestamp: "t".into(),
            transactions: vec![],
            prev_hash: genesis_hash,
            hash: evil_hash,
            miner: "z".repeat(64),
            energy_kwh: 0.0,
        };
        let res = ledger.integrate_remote_block(evil); // pre-fix: PANIC at [..12]
        assert!(res.is_err(), "bogus-hash fork block must be rejected, not panic");
        assert_eq!(ledger.chain.len(), 2, "chain unchanged");
    }

    // ─── TX-AUTH-NONCE-1: nonce/hash authentication + hang bound ────────────

    /// Build a tx signed over the canonical pre-image with an ATTACKER-CHOSEN
    /// nonce (hash recomputed to match), the way a hostile peer crafts one.
    /// Mirrors `build_signed_tx_at`'s encoding so a legit nonce verifies and a
    /// tampered field is caught.
    fn sign_tx_with_nonce(
        crypto: &CryptoEngine,
        from: &str,
        to: &str,
        amount: u64,
        nonce: u64,
    ) -> Transaction {
        let id = format!("tx_evil_{nonce}");
        let ts = "2026-01-01T00:00:00+00:00".to_string();
        let tx_type = TxType::Transfer;
        // PQ-MIG-3: bind the primary ML-DSA key into the pre-image, then sign both
        // layers with it — mirrors `build_signed_tx_at`.
        let pq_pk = crypto.pq_identity_hex().expect("ml-dsa primary");
        let payload =
            Ledger::tx_signing_preimage(&id, from, to, amount, &ts, &tx_type, nonce, &pq_pk);
        let (classical, quantum, pq_pk) = crypto.sign_tx_authority(payload.as_bytes()).unwrap();
        Transaction {
            id,
            from: from.into(),
            to: to.into(),
            amount,
            tx_type,
            timestamp: ts,
            signature: hex::encode(&classical),
            hash: hex::encode(blake3::hash(payload.as_bytes()).as_bytes()),
            nonce,
            pq_signature: Some(hex::encode(&quantum)),
            pq_public_key: Some(pq_pk),
            fault_proof: None,
            slash_unbonding: None,
        }
    }

    #[test]
    fn txauth_far_ahead_nonce_rejected_no_hang() {
        // §5 anti-hang: a validly-SIGNED tx with a nonce near u64::MAX (far beyond
        // MAX_NONCE_GAP) must be REJECTED in O(1) — never the old ~2^64 increment
        // loop under the ledger lock. (The test completing at all = no hang.)
        let mut crypto = pq_wallet();
        let pk = gen_addr(&mut crypto);
        let mut ledger = Ledger::new();
        ledger.mine_tx(&pk, 100 * MICRO, 0.0);
        ledger.seal_block("GENESIS", 0.0);
        let to = "d".repeat(64);

        let evil = sign_tx_with_nonce(&crypto, &pk, &to, 5 * MICRO, u64::MAX);
        assert!(
            Ledger::verify_tx(&evil).unwrap(),
            "the crafted tx is correctly signed — the GAP BOUND rejects it, not the signature"
        );
        let before_hw = ledger.get_nonce(&pk);
        let applied = ledger.apply_remote_tx_checked(evil);
        assert!(!applied, "a nonce far ahead of the high-water must be rejected (gap bound)");
        assert_eq!(
            ledger.get_nonce(&pk),
            before_hw,
            "the rejected tx must not advance/poison the nonce high-water"
        );
    }

    #[test]
    fn txauth_forged_nonce_breaks_signature() {
        // §5: altering a signed tx's nonce (with the hash recomputed to bypass §3)
        // must invalidate the SIGNATURE — the nonce is now in the signed pre-image.
        let mut crypto = pq_wallet();
        let pk = gen_addr(&mut crypto);
        let to = "d".repeat(64);
        let tx = sign_tx_with_nonce(&crypto, &pk, &to, 10 * MICRO, 7);
        assert!(Ledger::verify_tx(&tx).unwrap(), "the untouched signed tx verifies");

        let mut forged = tx.clone();
        forged.nonce = 8;
        let payload = Ledger::tx_signing_preimage(
            &forged.id, &forged.from, &forged.to, forged.amount, &forged.timestamp,
            &forged.tx_type, forged.nonce, forged.pq_public_key.as_deref().unwrap_or(""),
        );
        forged.hash = hex::encode(blake3::hash(payload.as_bytes()).as_bytes());
        assert!(
            !Ledger::verify_tx(&forged).unwrap(),
            "altering the signed nonce invalidates the signature (§2)"
        );
    }

    #[test]
    fn txauth_malleable_hash_rejected() {
        // §5: a wire hash that disagrees with the recomputed content hash is
        // rejected (§3 — no hash malleability past the dedup).
        let mut crypto = pq_wallet();
        let pk = gen_addr(&mut crypto);
        let to = "d".repeat(64);
        let tx = sign_tx_with_nonce(&crypto, &pk, &to, 10 * MICRO, 3);
        let mut forged = tx.clone();
        forged.hash = "0".repeat(64);
        assert!(
            !Ledger::verify_tx(&forged).unwrap(),
            "a wire hash != the recomputed content hash must be rejected (§3)"
        );
    }

    #[test]
    fn txauth_valid_signed_tx_verifies_and_hash_binds_nonce() {
        // §5 happy path: the real production path (transfer_tx) still verifies, and
        // tx.hash now commits to the nonce. C1 byte-identical determinism is
        // covered by determinism_meta_test_128_runs_are_byte_identical (green).
        let mut crypto = pq_wallet();
        let pk = gen_addr(&mut crypto);
        let mut ledger = Ledger::new();
        ledger.mine_tx(&pk, 100 * MICRO, 0.0);
        ledger.seal_block("GENESIS", 0.0);
        let to = "d".repeat(64);
        let tx = ledger.transfer_tx(&pk, &to, 10 * MICRO, &crypto).unwrap();
        assert!(Ledger::verify_tx(&tx).unwrap(), "a freshly-signed tx verifies (nonce in the pre-image)");
        let payload = Ledger::tx_signing_preimage(
            &tx.id, &tx.from, &tx.to, tx.amount, &tx.timestamp, &tx.tx_type, tx.nonce,
            tx.pq_public_key.as_deref().unwrap_or(""),
        );
        assert_eq!(
            tx.hash,
            hex::encode(blake3::hash(payload.as_bytes()).as_bytes()),
            "tx.hash commits to the nonce"
        );
    }

    #[test]
    fn transfer_with_burn_deducts_one_percent() {
        let mut crypto = pq_wallet();
        let id = gen_addr(&mut crypto);
        let mut ledger = Ledger::new();
        // Give sender 100 QUANTA
        ledger.mine_tx(&id, 100 * MICRO, 0.0);

        let to = "d".repeat(64);
        let (tx, burn_tx, burn_uqta) = ledger
            .transfer_with_burn(&id, &to, 10 * MICRO, &crypto)
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
        assert_eq!(ledger.balance_of(&id), 90 * MICRO);
        // Receiver: 10 - 0.1 = 9.9 QUANTA = 9_900_000 µQTA
        assert_eq!(ledger.balance_of(&to), 9_900_000);
    }

    #[test]
    fn double_spend_rejected() {
        let mut crypto = pq_wallet();
        let id = gen_addr(&mut crypto);
        let mut ledger = Ledger::new();
        ledger.mine_tx(&id, 5 * MICRO, 0.0);

        let to = "e".repeat(64);
        // First transfer: 5 QTA → should succeed
        assert!(ledger
            .transfer_with_burn(&id, &to, 5 * MICRO, &crypto)
            .is_ok());
        // Second transfer: 5 QTA → should fail (balance is 0 now)
        assert!(ledger
            .transfer_with_burn(&id, &to, 5 * MICRO, &crypto)
            .is_err());
    }

    #[test]
    fn balance_never_negative() {
        let mut crypto = pq_wallet();
        let id = gen_addr(&mut crypto);
        let mut ledger = Ledger::new();
        ledger.mine_tx(&id, MICRO, 0.0);

        let to = "f".repeat(64);
        // Try to send more than balance
        assert!(ledger
            .transfer_with_burn(&id, &to, 2 * MICRO, &crypto)
            .is_err());
        // Balance unchanged
        assert_eq!(ledger.balance_of(&id), MICRO);
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
            fault_proof: None,
            slash_unbonding: None,
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
        let mut crypto = pq_wallet();
        let id = gen_addr(&mut crypto);
        let mut ledger = Ledger::new();
        ledger.mine_tx(&id, 100 * MICRO, 0.0);

        let to = "d".repeat(64);
        let (transfer, burn_opt, burn_uqta) = ledger
            .transfer_with_burn(&id, &to, 10 * MICRO, &crypto)
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
        let mut crypto = pq_wallet();
        let id = gen_addr(&mut crypto);
        let mut ledger = Ledger::new();
        // Sender holds 99_000 µQTA (= 0.099 QTA, just under the 0.1 QTA min).
        // This is the boundary case: balance == net (99_000 = 100_000 - 1_000)
        // but balance < gross (99_000 < 100_000). Old code would accept the
        // transfer (net check passes) then apply the unsigned burn,
        // overdrawing the sender by 1_000 µQTA.
        ledger.mine_tx(&id, 99_000, 0.0);

        let to = "e".repeat(64);
        let result = ledger.transfer_with_burn(&id, &to, 100_000, &crypto);
        assert!(
            result.is_err(),
            "transfer where balance < gross must be rejected (AUDIT-TX-3)"
        );
        // Balance untouched.
        assert_eq!(ledger.balance_of(&id), 99_000);
    }

    /// AUDIT-TX cross-ledger convergence: two independent ledgers receiving
    /// the same transfer + burn pair must end up with IDENTICAL balances.
    /// This exercises the same flow as A→B over gossip.
    #[test]
    fn audit_tx_cross_ledger_convergence() {
        let mut crypto = pq_wallet();
        let id_a = gen_addr(&mut crypto);
        let pk_a = id_a.clone();
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
        let mut crypto = pq_wallet();
        let signer = gen_addr(&mut crypto);
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
                    tx.id
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
        let mut crypto = pq_wallet();
        let id_a = gen_addr(&mut crypto);
        let pk_a = id_a.clone();
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

    // ─── ONCHAIN-STAKE-1: on-chain stake state ──────────────────────────────

    /// The conservation LHS the harness checks (§5): spendable + locked-stake +
    /// burned. It must equal `total_minted` at **every** step of a stake lifecycle
    /// — staking moves coins into the `STAKE` sink, it never destroys them.
    fn conservation_lhs(l: &Ledger) -> u64 {
        let spendable: u64 = l.all_balances().values().sum();
        spendable + l.locked_stake_total() + l.total_burned()
    }

    /// **ONCHAIN-STAKE-1 §5 — conservation through Stake → Unstake → unlock.**
    /// Each phase moves coins between the spendable, staked, and unbonding pools;
    /// conservation (`Σ = minted`) holds at every step, and the funds become
    /// spendable again only once the unlock height is reached. HARDEN-STAKE-1: a
    /// Stake **locks** spendable funds the instant it is admitted (mempool), while
    /// its consensus **weight** is committed only on seal.
    #[test]
    fn onchain_stake_conservation_through_stake_unstake_unlock() {
        let mut crypto = pq_wallet();
        let id = gen_addr(&mut crypto);
        let pk = id.clone();

        let mut l = Ledger::new();
        // Fund 100 QUANTA on-chain.
        l.mine_tx(&pk, 100 * MICRO, 0.0);
        l.seal_block(&pk, 0.0);
        let minted = l.total_minted();
        assert_eq!(l.balance_of(&pk), 100 * MICRO, "funded spendable");
        assert_eq!(conservation_lhs(&l), minted, "conservation after funding");

        // ── Stake 40: spendable LOCKS immediately (100 → 60) at mempool, but the
        //    consensus bonded weight is only committed on seal.
        l.stake_tx(&pk, 40 * MICRO, &crypto).expect("stake builds");
        assert_eq!(l.balance_of(&pk), 60 * MICRO, "stake LOCKS spendable at mempool");
        assert_eq!(l.staked_of(&pk), 0, "but bonded weight not committed until sealed");
        assert_eq!(l.locked_stake_total(), 40 * MICRO, "40 locked in the STAKE sink");
        assert_eq!(conservation_lhs(&l), minted, "conservation while pending stake");
        l.seal_block(&pk, 0.0);
        assert_eq!(l.balance_of(&pk), 60 * MICRO, "spendable stays locked");
        assert_eq!(l.staked_of(&pk), 40 * MICRO, "40 now bonded (consensus weight)");
        assert_eq!(conservation_lhs(&l), minted, "conservation after stake");

        // ── Unstake 25: bonded 40 → 15, unbonding 0 → 25 (still locked).
        l.unstake_tx(&pk, 25 * MICRO, &crypto).expect("unstake builds");
        l.seal_block(&pk, 0.0);
        let unbond_block = l.chain_height() - 1; // index that sealed the unstake
        let unlock = unbond_block + UNBONDING_PERIOD_BLOCKS;
        assert_eq!(l.staked_of(&pk), 15 * MICRO, "15 still bonded");
        assert_eq!(l.unbonding_of(&pk), 25 * MICRO, "25 unbonding");
        assert_eq!(
            l.balance_of(&pk),
            60 * MICRO,
            "unbonding funds are NOT spendable yet"
        );
        assert_eq!(conservation_lhs(&l), minted, "conservation after unstake");

        // ── Just before the unlock height: still locked (height-indexed, §3).
        l.mature_unbonding(unlock - 1);
        assert_eq!(
            l.unbonding_of(&pk),
            25 * MICRO,
            "must not unlock before the unbonding period elapses"
        );
        assert_eq!(l.balance_of(&pk), 60 * MICRO, "still locked one block early");
        assert_eq!(conservation_lhs(&l), minted, "conservation while locked");

        // ── At the unlock height (production reaches this via the per-seal
        //    `apply_block_stake_effects(block.index)`): funds return to spendable.
        l.mature_unbonding(unlock);
        assert_eq!(l.unbonding_of(&pk), 0, "unbonding fully matured");
        assert_eq!(l.staked_of(&pk), 15 * MICRO, "still-bonded stake untouched");
        assert_eq!(
            l.balance_of(&pk),
            85 * MICRO,
            "60 spendable + 25 matured = 85"
        );
        assert_eq!(conservation_lhs(&l), minted, "conservation after unlock");
    }

    /// **ONCHAIN-STAKE-1 §7 — anti-divergence (the seal), with teeth.** Two nodes
    /// holding the **same chain** derive the **same** validator weights and elect
    /// the **same** leader at every slot — even though their *local* leaderboards
    /// differ. Non-vacuity: the OLD source (a node-local leaderboard) would have
    /// elected different leaders on those very inputs, so the agreement comes from
    /// the change of source (chain, not leaderboard), not from identical inputs.
    #[test]
    fn onchain_stake_weight_identical_across_nodes_despite_local_leaderboards() {
        use crate::p2p::pos_consensus::{build_validator_set, elect_leader};

        // Build a chain where three accounts bond different amounts.
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);
        let mut crypto_c = pq_wallet();
        let carol = gen_addr(&mut crypto_c);

        let mut node1 = Ledger::new();
        for (pk, c, fund, stake) in [
            (&alice, &crypto_a, 10 * MICRO, 5 * MICRO),
            (&bob, &crypto_b, 10 * MICRO, 3 * MICRO),
            (&carol, &crypto_c, 10 * MICRO, 4 * MICRO),
        ] {
            node1.mine_tx(pk, fund, 0.0);
            node1.seal_block(pk, 0.0);
            node1.stake_tx(pk, stake, c).expect("stake builds");
            node1.seal_block(pk, 0.0);
        }

        // node2 holds the SAME chain (restored from node1's snapshot) — its stake
        // state is rebuilt deterministically from the chain.
        let node2 = Ledger::restore(node1.snapshot());

        // On-chain stake snapshots coincide exactly.
        assert_eq!(
            node1.validator_stakes(),
            node2.validator_stakes(),
            "same chain ⇒ identical on-chain stake snapshot"
        );

        // Validator sets built from the chain (reputation OFF the path: empty map).
        let no_rep = std::collections::HashMap::new();
        let vals1 = build_validator_set(&node1.validator_stakes(), &no_rep);
        let vals2 = build_validator_set(&node2.validator_stakes(), &no_rep);

        // The two nodes elect the SAME leader at every slot.
        for slot in 0..2_000u64 {
            assert_eq!(
                elect_leader("beacon", slot, &vals1),
                elect_leader("beacon", slot, &vals2),
                "chain-sourced weight must elect the same leader on both nodes (slot {slot})"
            );
        }

        // TEETH — the OLD source (node-local leaderboards) would have diverged.
        // Simulate two nodes whose *local* views disagree (the exact hazard
        // ONCHAIN-STAKE-1 closes): node A sees alice richest, node B sees bob
        // richest. Build validator sets from those (the pre-fix path) and show
        // they elect different leaders on at least one slot.
        let local_a = std::collections::HashMap::from([
            (alice.clone(), 9 * MICRO),
            (bob.clone(), MICRO),
            (carol.clone(), MICRO),
        ]);
        let local_b = std::collections::HashMap::from([
            (alice.clone(), MICRO),
            (bob.clone(), 9 * MICRO),
            (carol.clone(), MICRO),
        ]);
        let old_a = build_validator_set(&local_a, &no_rep);
        let old_b = build_validator_set(&local_b, &no_rep);
        let old_would_diverge = (0..2_000u64)
            .any(|slot| elect_leader("beacon", slot, &old_a) != elect_leader("beacon", slot, &old_b));
        assert!(
            old_would_diverge,
            "non-vacuity: locally-sourced stake WOULD have forked the leader — \
             the new agreement is due to the on-chain source, not identical inputs"
        );
    }

    /// **ONCHAIN-STAKE-1 — live ≡ restored stake state.** A node that sealed a
    /// full Stake → Unstake → maturation history reaches **byte-identical** stake
    /// state (bonded map, unbonding, spendable cache, validator snapshot) after a
    /// snapshot → restore. This is the determinism property a synced/restarted node
    /// needs: the stake state is reconstructed purely from the chain, with the same
    /// block-index-anchored unlock heights and matured-fund credits — the live seal
    /// path and the `rebuild_cache` replay path agree.
    #[test]
    fn onchain_stake_state_survives_snapshot_restore_byte_identical() {
        let mut crypto = pq_wallet();
        let pk = gen_addr(&mut crypto);

        let mut live = Ledger::new();
        live.mine_tx(&pk, 100 * MICRO, 0.0);
        live.seal_block(&pk, 0.0);
        live.stake_tx(&pk, 60 * MICRO, &crypto).expect("stake");
        live.seal_block(&pk, 0.0);
        live.unstake_tx(&pk, 20 * MICRO, &crypto).expect("unstake");
        live.seal_block(&pk, 0.0);

        // A comparable fingerprint of the full stake-relevant state.
        let snap_of = |l: &Ledger| {
            let mut bals: Vec<(String, i128)> = l.balance_cache.iter().map(|(k, v)| (k.clone(), *v)).collect();
            bals.sort();
            let mut stk: Vec<(String, u64)> = l.staked.iter().map(|(k, v)| (k.clone(), *v)).collect();
            stk.sort();
            (
                bals,
                stk,
                l.staked_total(),
                l.unbonding_total(),
                l.unbonding_of(&pk),
                l.balance_of(&pk),
            )
        };

        let restored = Ledger::restore(live.snapshot());
        assert_eq!(
            snap_of(&live),
            snap_of(&restored),
            "restored node must rebuild byte-identical stake state from the chain"
        );

        // And maturing both to the same height keeps them identical (the credit
        // fold replays deterministically too).
        let target = restored.chain_height() - 1 + UNBONDING_PERIOD_BLOCKS;
        let mut live2 = live;
        let mut restored2 = restored;
        live2.mature_unbonding(target);
        restored2.mature_unbonding(target);
        assert_eq!(
            snap_of(&live2),
            snap_of(&restored2),
            "maturation folds identically on both"
        );
        assert_eq!(live2.balance_of(&pk), 60 * MICRO, "40 bonded + 20 matured returns 60 spendable");
    }

    /// **HARDEN-STAKE-1 (regression) — a pending Stake LOCKS funds; no double-spend,
    /// no conservation break.** The adversarial review found that, before the fix,
    /// a pending Stake did not reduce `balance_of`, so a concurrent transfer could
    /// spend the same coins → on seal the balance cache went negative, the `.max(0)`
    /// clamp hid it, and `Σ spendable + locked_stake + burned` exceeded `minted`
    /// (µQTA fabricated). Now the Stake debits the spendable balance into the
    /// `STAKE` sink at admission, so the racing transfer is rejected and
    /// conservation holds — at every step, including across a seal.
    #[test]
    fn harden_stake_pending_stake_locks_funds_no_double_spend() {
        let mut crypto = pq_wallet();
        let pk = gen_addr(&mut crypto);
        let bob = "b".repeat(64);

        let mut l = Ledger::new();
        l.mine_tx(&pk, 100 * MICRO, 0.0);
        l.seal_block(&pk, 0.0);
        let minted = l.total_minted();

        // Stake the FULL balance — funds lock immediately (mempool).
        l.stake_tx(&pk, 100 * MICRO, &crypto).expect("stake builds");
        assert_eq!(l.balance_of(&pk), 0, "the staked funds are locked NOW");

        // The racing transfer of the same coins must be REJECTED (the exact
        // double-spend the review reproduced as a conservation break).
        let racing = l.transfer_with_burn(&pk, &bob, 50 * MICRO, &crypto);
        assert!(
            racing.is_err(),
            "a transfer of already-staked funds must be rejected (no double-spend)"
        );

        // Seal the Stake; the balance cache never went negative, conservation holds.
        l.seal_block(&pk, 0.0);
        let spendable: u64 = l.all_balances().values().sum();
        assert_eq!(spendable, 0, "all funds locked in stake, none spendable");
        assert_eq!(l.locked_stake_total(), 100 * MICRO, "100 locked in the STAKE sink");
        assert_eq!(l.staked_of(&pk), 100 * MICRO, "100 bonded");
        assert_eq!(
            spendable + l.locked_stake_total() + l.total_burned(),
            minted,
            "conservation holds — no µQTA fabricated by the (now rejected) double-spend"
        );
        // The raw balance cache is non-negative everywhere (the clamp can hide
        // nothing): the sum of raw entries equals minted − burned exactly.
        let raw_sum: i128 = l.balance_cache.values().sum();
        assert_eq!(
            raw_sum,
            (minted - l.total_burned()) as i128,
            "raw cache conserves with no hidden negative deficit"
        );
        assert!(
            l.balance_cache.values().all(|v| *v >= 0),
            "no balance_cache entry is negative (the bug's signature is absent)"
        );
    }

    // ── COVER-1: block-level coverage validation (no spending what you don't have) ──

    /// Forge a block whose hash exceeds `threshold`, so `integrate_remote_block`
    /// takes the fork-reorg branch (remote wins on the lexicographically higher
    /// hash). Deterministic: only the block timestamp (part of the hash pre-image)
    /// is varied, over a fixed sequence, and the first hash that clears the bar is
    /// returned. The txs — hence the Merkle root — are untouched.
    #[cfg(test)]
    fn forge_block_hash_above(
        index: u64,
        prev_hash: &str,
        miner: &str,
        txs: &[Transaction],
        threshold: &str,
    ) -> Block {
        for i in 0..100_000u32 {
            let ts = format!("2026-02-01T00:00:00.{i:06}Z");
            let b = Ledger::forge_block_at(index, prev_hash, &ts, miner, txs.to_vec());
            if b.hash.as_str() > threshold {
                return b;
            }
        }
        panic!("could not forge a block with hash above the threshold");
    }

    /// **COVER-1 §6.1 — an uncovered transfer makes the block INVALID.** Alice
    /// holds 10 on-chain; a block carrying a (validly-signed) transfer of 50 from
    /// her is rejected at validation — the spend of coins that don't exist never
    /// touches the ledger. The signature is genuine (so the rejection is coverage,
    /// not a forged sig): `build_signed_tx` does not itself check coverage — that
    /// is exactly the hole COVER-1 closes at the block.
    #[test]
    fn cover1_uncovered_transfer_block_rejected() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 10 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let tip = l.block_at(l.chain_height() - 1).unwrap().clone();

        let evil = l
            .build_signed_tx_at(&alice, &bob, 50 * MICRO, TxType::Transfer, &crypto_a,
                "2026-01-01T00:00:00Z".into(), false)
            .expect("an over-spend tx still produces a valid signature");
        let block = Ledger::forge_block_at(tip.index + 1, &tip.hash,
            "2026-01-01T00:00:01Z", &alice, vec![evil]);

        let res = l.integrate_remote_block(block);
        let err = res.expect_err("a transfer exceeding the on-chain balance must be rejected");
        assert!(err.contains("non couverte"), "rejected for coverage, got: {err}");
        // The rejection touched nothing.
        assert_eq!(l.chain_height(), 2, "chain unchanged by a rejected block");
        assert_eq!(l.balance_of(&alice), 10 * MICRO, "balance untouched");
    }

    /// **COVER-1 §6.2 — an uncovered Stake makes the block INVALID.** Identical to
    /// the transfer case but with a `Stake`: staking is a spend (it moves coins
    /// into the locked sink), so it is covered the same way — you cannot bond
    /// coins you don't have (which would forge consensus weight).
    #[test]
    fn cover1_uncovered_stake_block_rejected() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 10 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let tip = l.block_at(l.chain_height() - 1).unwrap().clone();

        let evil = l
            .build_signed_tx_at(&alice, Ledger::STAKE_SINK, 50 * MICRO, TxType::Stake, &crypto_a,
                "2026-01-01T00:00:00Z".into(), false)
            .expect("an over-stake tx still signs");
        let block = Ledger::forge_block_at(tip.index + 1, &tip.hash,
            "2026-01-01T00:00:01Z", &alice, vec![evil]);

        let err = l.integrate_remote_block(block)
            .expect_err("a Stake exceeding the on-chain balance must be rejected");
        assert!(err.contains("non couverte"), "rejected for coverage, got: {err}");
        assert_eq!(l.staked_of(&alice), 0, "no consensus weight forged from absent coins");
    }

    /// **COVER-1 §6.3 — coverage is SEQUENTIAL within a block.** Alice holds 100.
    /// A block `[→Bob 50, →Carol 60]` is rejected: the first leg passes (50 left),
    /// the second is then uncovered (50 < 60). The *same* first leg with a second
    /// leg of 40 (total 90 ≤ 100) is accepted — proving it is the running-balance
    /// depletion that rejects, not merely the presence of two txs. The order is
    /// the block's tx order, deterministic on every node.
    #[test]
    fn cover1_sequential_coverage_within_block() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);
        let mut crypto_c = pq_wallet();
        let carol = gen_addr(&mut crypto_c);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 100 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let tip = l.block_at(l.chain_height() - 1).unwrap().clone();

        // Over-budget sequence: 50 then 60 — second leg becomes uncovered.
        let a0 = l.build_signed_tx_at(&alice, &bob, 50 * MICRO, TxType::Transfer, &crypto_a,
            "2026-01-01T00:00:00Z".into(), false).unwrap();
        let a1 = l.build_signed_tx_at(&alice, &carol, 60 * MICRO, TxType::Transfer, &crypto_a,
            "2026-01-01T00:00:01Z".into(), false).unwrap();
        let bad = Ledger::forge_block_at(tip.index + 1, &tip.hash,
            "2026-01-01T00:00:02Z", &alice, vec![a0, a1]);
        let err = l.integrate_remote_block(bad)
            .expect_err("the second leg is uncovered once the first depletes the balance");
        assert!(err.contains("non couverte"), "got: {err}");

        // In-budget sequence on the SAME start state (the rejection mutated nothing):
        // 50 then 40 — total 90 ≤ 100, both covered sequentially.
        let b0 = l.build_signed_tx_at(&alice, &bob, 50 * MICRO, TxType::Transfer, &crypto_a,
            "2026-01-01T00:00:03Z".into(), false).unwrap();
        let b1 = l.build_signed_tx_at(&alice, &carol, 40 * MICRO, TxType::Transfer, &crypto_a,
            "2026-01-01T00:00:04Z".into(), false).unwrap();
        let good = Ledger::forge_block_at(tip.index + 1, &tip.hash,
            "2026-01-01T00:00:05Z", &alice, vec![b0, b1]);
        assert_eq!(l.integrate_remote_block(good), Ok(true),
            "a sequence within the budget is accepted (no liveness regression)");
        assert_eq!(l.balance_of(&alice), 10 * MICRO, "100 − 50 − 40 = 10");
        assert_eq!(l.balance_of(&bob), 50 * MICRO);
        assert_eq!(l.balance_of(&carol), 40 * MICRO);
    }

    /// **COVER-1 §6.4 — BOTH paths reject the uncovered, identically.** The same
    /// uncovered transfer (Alice → Bob 50) is rejected on the **linear** path (a
    /// block extending the tip) AND on the **fork-reorg** path (a higher-hash block
    /// replacing the tip) — proving the coverage check lives on the *shared*
    /// validator, not just one path (the FORK-CAP lesson). The reorg is checked
    /// against the chain WITHOUT the tip it would replace, and the rejected reorg
    /// truncates nothing (AUDIT-BLK-2 preserved).
    #[test]
    fn cover1_both_paths_reject_uncovered() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);

        // Chain: genesis(0), block1(1: +10 alice), block2 = tip T(2: +10 alice).
        let mut l = Ledger::new();
        l.mine_tx(&alice, 10 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let block1 = l.block_at(1).unwrap().clone();
        l.mine_tx(&alice, 10 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let tip = l.block_at(2).unwrap().clone();
        let minted = l.total_minted();

        // One uncovered tx (50 > alice's balance on EITHER prefix: 10 vs 20).
        let evil = l.build_signed_tx_at(&alice, &bob, 50 * MICRO, TxType::Transfer, &crypto_a,
            "2026-01-01T00:00:00Z".into(), false).unwrap();

        // LINEAR path — block at height 3 extending T (alice has 20 ≤ chain through T).
        let linear = Ledger::forge_block_at(tip.index + 1, &tip.hash,
            "2026-01-01T00:00:01Z", &alice, vec![evil.clone()]);
        let lin_err = l.integrate_remote_block(linear)
            .expect_err("linear integration rejects the uncovered spend");
        assert!(lin_err.contains("non couverte"), "linear: {lin_err}");

        // FORK-REORG path — a higher-hash block at height 2 replacing T, checked
        // against the chain WITHOUT T (alice has 10 there).
        let reorg = forge_block_hash_above(tip.index, &block1.hash, &alice, &[evil], &tip.hash);
        assert!(reorg.hash.as_str() > tip.hash.as_str(), "must trigger the reorg branch");
        let reorg_err = l.integrate_remote_block(reorg)
            .expect_err("fork-reorg rejects the uncovered winner exactly like the linear path");
        assert!(reorg_err.contains("non couverte"), "reorg: {reorg_err}");

        // Neither rejection mutated the chain — T is still the tip, nothing truncated.
        assert_eq!(l.chain_height(), 3, "rejected blocks leave the chain intact");
        assert_eq!(l.block_at(2).unwrap().hash, tip.hash, "tip unchanged (no truncation)");
        assert_eq!(conservation_lhs(&l), minted, "conservation intact after both rejections");
    }

    /// **COVER-1 §6.5 — a fully-covered block PASSES (no liveness regression), and
    /// §3 intra-block credits count.** Alice holds 0 on-chain, but a block credits
    /// her a 5-QUANTA mining reward and then spends 3 of it to Bob, in that order.
    /// The reward (synthetic `NETWORK` sender, exempt) funds the same-block spend
    /// sequentially, so the block is covered and integrates.
    #[test]
    fn cover1_valid_block_with_intra_block_credit_passes() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);

        let mut l = Ledger::new();
        let genesis = l.block_at(0).unwrap().clone();

        // A NETWORK→alice mining reward, minted on a throwaway ledger.
        let mining_tx = {
            let mut tmp = Ledger::new();
            tmp.mine_tx(&alice, 5 * MICRO, 0.0)
        };
        // Alice spends 3 of the reward she receives IN THE SAME BLOCK (§3).
        let spend = l.build_signed_tx_at(&alice, &bob, 3 * MICRO, TxType::Transfer, &crypto_a,
            "2026-03-01T00:00:00Z".into(), false).unwrap();
        // miner == alice (EMIT-1: the single reward is credited to block.miner).
        let block = Ledger::forge_block_at(1, &genesis.hash, "2026-03-01T00:00:01Z",
            &alice, vec![mining_tx, spend]);

        assert_eq!(l.integrate_remote_block(block), Ok(true),
            "reward-funds-same-block-spend is covered sequentially (§3)");
        assert_eq!(l.balance_of(&alice), 2 * MICRO, "5 reward − 3 spent = 2");
        assert_eq!(l.balance_of(&bob), 3 * MICRO, "bob received 3");
        assert_eq!(conservation_lhs(&l), l.total_minted(), "conservation holds for the accepted block");
    }

    /// **COVER-1 §6.6 — rejecting the uncovered keeps conservation exact.** With
    /// the spend rejected at validation it never reaches the cache, so the class
    /// "uncovered spend breaks/masks conservation" is closed AT validation: `Σ
    /// spendable + locked + burned == minted` holds, and no balance entry is
    /// negative. (Determinism — C1 — is the separate 128-run meta-test; this change
    /// only reads the chain and a `Vec`, both order-deterministic.)
    #[test]
    fn cover1_rejected_uncovered_preserves_conservation() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 100 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let minted = l.total_minted();
        let tip = l.block_at(l.chain_height() - 1).unwrap().clone();

        let evil = l.build_signed_tx_at(&alice, &bob, 500 * MICRO, TxType::Transfer, &crypto_a,
            "2026-01-01T00:00:00Z".into(), false).unwrap();
        let block = Ledger::forge_block_at(tip.index + 1, &tip.hash,
            "2026-01-01T00:00:01Z", &alice, vec![evil]);
        assert!(l.integrate_remote_block(block).is_err(), "uncovered block rejected");

        assert_eq!(conservation_lhs(&l), minted, "conservation exact after the rejection");
        assert_eq!(l.balance_of(&alice), 100 * MICRO, "balance untouched by the rejected spend");
        assert!(l.balance_cache.values().all(|v| *v >= 0), "no negative cache entry — nothing leaked in");
    }

    /// **COVER-1 — no-drift guard for `onchain_spendable_before`.** The pure
    /// chain-replay the coverage check relies on must equal the ledger's own
    /// (chain-only) balance cache, or the verdict could diverge from reality. After
    /// a mixed history (mine, transfer+burn, stake, unstake) sealed with an empty
    /// mempool, the replay of the chain up to the tip matches `balance_cache`
    /// entry-for-entry — generic moves, the STAKE-sink lock, and Unstake's
    /// no-spendable-effect all mirrored.
    #[test]
    fn cover1_onchain_replay_matches_live_cache_no_drift() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 100 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        l.transfer_with_burn(&alice, &bob, 20 * MICRO, &crypto_a).expect("transfer");
        l.seal_block(&alice, 0.0);
        l.stake_tx(&alice, 30 * MICRO, &crypto_a).expect("stake");
        l.seal_block(&alice, 0.0);
        l.unstake_tx(&alice, 10 * MICRO, &crypto_a).expect("unstake");
        l.seal_block(&alice, 0.0);
        assert_eq!(l.pending_count(), 0, "all txs sealed — cache is chain-only");

        let tip = l.block_at(l.chain_height() - 1).unwrap().clone();
        let replay = l.onchain_spendable_before(&tip);
        let norm = |m: &HashMap<String, i128>| {
            let mut v: Vec<(String, i128)> =
                m.iter().filter(|(_, x)| **x != 0).map(|(k, x)| (k.clone(), *x)).collect();
            v.sort();
            v
        };
        assert_eq!(norm(&replay), norm(&l.balance_cache),
            "onchain replay up to the tip must equal the live chain-only cache (no drift)");
    }

    // ── COVER-2: coverage at SEAL (exclude uncovered, build a valid block) ──

    /// Seal a block AND assert the §3 invariant: a self-produced block always
    /// passes `validate_block_against_prev` (the integration check). Captures the
    /// pre-seal tip + on-chain balances, seals, then validates the result against
    /// them — exactly what a peer integrating this block would compute.
    #[cfg(test)]
    fn seal_and_validate(l: &mut Ledger, miner: &str, ts: &str) -> Block {
        let prev = l.block_at(l.chain_height() - 1).unwrap().clone();
        let onchain = l.onchain_spendable_before(&prev);
        let bindings = l.pq_bindings_before(&prev);
        // PROPOSER-1: same bonded set a real peer would use (l sits at the parent).
        let bonded = l.validator_stakes();
        let block = l.seal_block_at(miner, 0.0, ts.to_string());
        assert!(
            Ledger::validate_block_against_prev(&block, &prev, &onchain, &bindings, &bonded).is_ok(),
            "COVER-2 §3: a self-sealed block must always pass validation (block #{})",
            block.index
        );
        block
    }

    /// **COVER-2 §6.1 — an uncovered transfer is EXCLUDED at seal.** A node admits
    /// an uncovered transfer via `replay_remote_tx` (the no-coverage-gate mempool
    /// path COVER-1 §5 leaves open), then seals: the tx is ABSENT from the block,
    /// the block passes integration validation (§3), and conservation is restored
    /// (the cache revert undoes the transient phantom the uncovered pending tx
    /// created at admission).
    #[test]
    fn cover2_uncovered_transfer_excluded_at_seal() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 100 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let minted = l.total_minted();

        let evil = l.build_signed_tx_at(&alice, &bob, 200 * MICRO, TxType::Transfer, &crypto_a,
            "2026-04-01T00:00:00Z".into(), false).unwrap();
        let evil_hash = evil.hash.clone();
        assert!(l.replay_remote_tx(evil), "admitted to pending (no coverage gate at admission)");

        let block = seal_and_validate(&mut l, &alice, "2026-04-01T00:00:01Z");
        assert!(!block.transactions.iter().any(|t| t.hash == evil_hash),
            "the uncovered tx is EXCLUDED from the sealed block");
        assert_eq!(conservation_lhs(&l), minted, "conservation restored — phantom undone by the revert");
        assert_eq!(l.balance_of(&alice), 100 * MICRO, "alice's coins are intact (the spend never happened)");
        assert_eq!(l.balance_of(&bob), 0, "bob received nothing");
    }

    /// **COVER-2 §6.2 — an uncovered Stake is EXCLUDED at seal.** Same as the
    /// transfer case: no consensus weight is bonded from coins that don't exist.
    #[test]
    fn cover2_uncovered_stake_excluded_at_seal() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 100 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let minted = l.total_minted();

        let evil = l.build_signed_tx_at(&alice, Ledger::STAKE_SINK, 200 * MICRO, TxType::Stake, &crypto_a,
            "2026-04-01T00:00:00Z".into(), false).unwrap();
        let evil_hash = evil.hash.clone();
        assert!(l.replay_remote_tx(evil));

        let block = seal_and_validate(&mut l, &alice, "2026-04-01T00:00:01Z");
        assert!(!block.transactions.iter().any(|t| t.hash == evil_hash), "uncovered stake excluded");
        assert_eq!(l.staked_of(&alice), 0, "no consensus weight bonded from absent coins");
        assert_eq!(conservation_lhs(&l), minted, "conservation restored");
        assert_eq!(l.balance_of(&alice), 100 * MICRO, "coins intact");
    }

    /// **COVER-2 §6.3 — sequential exclusion at seal.** Two admitted txs: the first
    /// (60) is covered, the second (60) becomes uncovered once the first depletes
    /// the balance (40 < 60). The first is sealed, the second excluded — the
    /// running-balance depletion, not the count, drives the exclusion.
    #[test]
    fn cover2_sequential_exclusion_at_seal() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);
        let mut crypto_c = pq_wallet();
        let carol = gen_addr(&mut crypto_c);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 100 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let minted = l.total_minted();

        let t1 = l.build_signed_tx_at(&alice, &bob, 60 * MICRO, TxType::Transfer, &crypto_a,
            "2026-04-01T00:00:00Z".into(), false).unwrap();
        let t1_hash = t1.hash.clone();
        let t2 = l.build_signed_tx_at(&alice, &carol, 60 * MICRO, TxType::Transfer, &crypto_a,
            "2026-04-01T00:00:01Z".into(), false).unwrap();
        let t2_hash = t2.hash.clone();
        assert!(l.replay_remote_tx(t1));
        assert!(l.replay_remote_tx(t2));

        let block = seal_and_validate(&mut l, &alice, "2026-04-01T00:00:02Z");
        assert!(block.transactions.iter().any(|t| t.hash == t1_hash), "the covered leg is sealed");
        assert!(!block.transactions.iter().any(|t| t.hash == t2_hash), "the now-uncovered leg is excluded");
        assert_eq!(l.balance_of(&alice), 40 * MICRO, "100 − 60 (only the covered leg) = 40");
        assert_eq!(l.balance_of(&bob), 60 * MICRO);
        assert_eq!(l.balance_of(&carol), 0, "carol got nothing — her funding leg was excluded");
        assert_eq!(conservation_lhs(&l), minted, "conservation holds");
    }

    /// **COVER-2 §6.4 — the auto-corruption scenario is CLOSED.** node_a admits a
    /// covered tx AND a malicious uncovered tx via gossip replay, then seals.
    /// COVER-2 excludes the uncovered one, so node_a's block is valid — and the
    /// decisive proof is that a PEER (node_b) integrates it cleanly (`Ok(true)`)
    /// and the two nodes converge. A node can no longer corrupt its own chain.
    #[test]
    fn cover2_auto_corruption_scenario_closed() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);
        let mut crypto_c = pq_wallet();
        let carol = gen_addr(&mut crypto_c);

        let mut node_a = Ledger::new();
        node_a.mine_tx(&alice, 100 * MICRO, 0.0);
        node_a.seal_block(&alice, 0.0);
        let mut node_b = Ledger::restore(node_a.snapshot());

        let covered = node_a.build_signed_tx_at(&alice, &bob, 30 * MICRO, TxType::Transfer, &crypto_a,
            "2026-04-01T00:00:00Z".into(), false).unwrap();
        let cov_hash = covered.hash.clone();
        let evil = node_a.build_signed_tx_at(&alice, &carol, 200 * MICRO, TxType::Transfer, &crypto_a,
            "2026-04-01T00:00:01Z".into(), false).unwrap();
        let evil_hash = evil.hash.clone();
        assert!(node_a.replay_remote_tx(covered));
        assert!(node_a.replay_remote_tx(evil));

        let block = node_a.seal_block_at(&alice, 0.0, "2026-04-01T00:00:02Z".into());
        assert!(block.transactions.iter().any(|t| t.hash == cov_hash), "covered tx sealed");
        assert!(!block.transactions.iter().any(|t| t.hash == evil_hash),
            "uncovered tx excluded — chain not corrupted");

        // Decisive: a peer ACCEPTS node_a's self-sealed block (no divergence).
        assert_eq!(node_b.integrate_remote_block(block), Ok(true),
            "a peer accepts the self-sealed block — auto-corruption closed");
        assert_eq!(node_b.balance_of(&alice), 70 * MICRO, "covered transfer applied on the peer");
        assert_eq!(node_a.balance_of(&alice), node_b.balance_of(&alice), "nodes converge (alice)");
        assert_eq!(node_a.balance_of(&bob), node_b.balance_of(&bob), "nodes converge (bob)");
    }

    /// **COVER-2 §3 (closing invariant) — every block `seal_block_at` produces
    /// passes `validate_block_against_prev`.** Exercised across a covered mempool,
    /// a fully-uncovered mempool (→ empty but valid block), and a mix. The
    /// self-produced block is integration-valid in all cases — the property that
    /// proves a node can no longer corrupt its own chain.
    #[test]
    fn cover2_self_sealed_block_always_validates() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 100 * MICRO, 0.0);

        // (a) covered mempool (just the mining reward) — seal_and_validate asserts §3.
        seal_and_validate(&mut l, &alice, "2026-05-01T00:00:01Z");
        assert_eq!(l.balance_of(&alice), 100 * MICRO);

        // (b) fully-uncovered mempool → all excluded → empty but valid block.
        let e1 = l.build_signed_tx_at(&alice, &bob, 500 * MICRO, TxType::Transfer, &crypto_a,
            "2026-05-01T01:00:00Z".into(), false).unwrap();
        l.replay_remote_tx(e1);
        let empty = seal_and_validate(&mut l, &alice, "2026-05-01T01:00:01Z");
        assert!(empty.transactions.is_empty(), "all-uncovered mempool seals an empty (valid) block");
        assert_eq!(l.balance_of(&alice), 100 * MICRO, "uncovered spend never happened");

        // (c) mixed: one covered, one uncovered.
        let c = l.build_signed_tx_at(&alice, &bob, 20 * MICRO, TxType::Transfer, &crypto_a,
            "2026-05-01T02:00:00Z".into(), false).unwrap();
        l.replay_remote_tx(c);
        let u = l.build_signed_tx_at(&alice, &bob, 900 * MICRO, TxType::Transfer, &crypto_a,
            "2026-05-01T02:00:01Z".into(), false).unwrap();
        l.replay_remote_tx(u);
        let block = seal_and_validate(&mut l, &alice, "2026-05-01T02:00:02Z");
        assert_eq!(
            block.transactions.iter().filter(|t| t.tx_type == TxType::Transfer).count(),
            1,
            "exactly the covered transfer is sealed"
        );
        assert_eq!(l.balance_of(&bob), 20 * MICRO);
    }

    /// **COVER-2 §6 — no regression: covered txs seal normally.** An honestly-built
    /// covered transfer (+ its burn leg) is sealed intact — liveness preserved, the
    /// exclusion logic touches only genuinely-uncovered txs.
    #[test]
    fn cover2_covered_txs_sealed_normally_no_regression() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 100 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);

        let (tx, burn, _) = l.transfer_with_burn(&alice, &bob, 50 * MICRO, &crypto_a)
            .expect("covered transfer builds");
        let tx_hash = tx.hash.clone();
        let burn_hash = burn.as_ref().map(|b| b.hash.clone());

        let block = seal_and_validate(&mut l, &alice, "2026-06-01T00:00:00Z");
        assert!(block.transactions.iter().any(|t| t.hash == tx_hash),
            "covered transfer IS sealed (liveness preserved)");
        if let Some(bh) = burn_hash {
            assert!(block.transactions.iter().any(|t| t.hash == bh), "the burn leg is sealed too");
        }
        assert_eq!(l.balance_of(&alice), 50 * MICRO, "alice: 100 − 50 = 50");
        assert_eq!(l.balance_of(&bob), 49_500_000, "bob: 50 − 1% burn = 49.5");
    }

    /// **COVER-2 — exclusion is per-tx and dependency-aware.** A tx funded ONLY by
    /// an excluded tx is ITSELF excluded (the excluded tx's credit never enters the
    /// running balance), while an INDEPENDENTLY-covered tx that comes *after* an
    /// excluded one is still kept. Proves seal doesn't "drop everything after the
    /// first uncovered" (a liveness bug) nor keep a tx funded by phantom coins (a
    /// safety bug). Cache + conservation are restored regardless.
    #[test]
    fn cover2_dependent_exclusion_keeps_independent_covered() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_z = pq_wallet();
        let zoe = gen_addr(&mut crypto_z); // unfunded sender
        let dave = "d".repeat(64);
        let erin = "e".repeat(64);

        let mut l = Ledger::new();
        l.mine_tx(&alice, 100 * MICRO, 0.0);
        l.seal_block(&alice, 0.0);
        let minted = l.total_minted();

        // tx_A: zoe→dave 50 — UNCOVERED (zoe has 0).
        let tx_a = l.build_signed_tx_at(&zoe, &dave, 50 * MICRO, TxType::Transfer, &crypto_z,
            "2026-07-01T00:00:00Z".into(), false).unwrap();
        let a_hash = tx_a.hash.clone();
        // tx_B: dave→erin 40 — would be covered ONLY by tx_A's phantom credit.
        // (dave is signed by a throwaway key; replay_remote_tx doesn't verify sigs.)
        let mut crypto_d = pq_wallet();
        let _ = crypto_d.generate_keypair();
        let tx_b = l.build_signed_tx_at(&dave, &erin, 40 * MICRO, TxType::Transfer, &crypto_d,
            "2026-07-01T00:00:01Z".into(), false).unwrap();
        let b_hash = tx_b.hash.clone();
        // tx_C: alice→erin 30 — INDEPENDENTLY covered (alice has 100), AFTER the excluded ones.
        let tx_c = l.build_signed_tx_at(&alice, &erin, 30 * MICRO, TxType::Transfer, &crypto_a,
            "2026-07-01T00:00:02Z".into(), false).unwrap();
        let c_hash = tx_c.hash.clone();
        assert!(l.replay_remote_tx(tx_a));
        assert!(l.replay_remote_tx(tx_b));
        assert!(l.replay_remote_tx(tx_c));

        let block = seal_and_validate(&mut l, &alice, "2026-07-01T00:00:03Z");
        assert!(!block.transactions.iter().any(|t| t.hash == a_hash), "tx_A (uncovered) excluded");
        assert!(!block.transactions.iter().any(|t| t.hash == b_hash),
            "tx_B excluded — its only funding (tx_A) was excluded, so it is NOT covered by phantom coins");
        assert!(block.transactions.iter().any(|t| t.hash == c_hash),
            "tx_C kept — independently covered despite following excluded txs (no over-pruning)");
        assert_eq!(l.balance_of(&alice), 70 * MICRO, "100 − 30 (only tx_C) = 70");
        assert_eq!(l.balance_of(&erin), 30 * MICRO, "erin got only tx_C's 30");
        assert_eq!(l.balance_of(&dave), 0, "dave: phantom credit reverted");
        assert_eq!(conservation_lhs(&l), minted, "conservation restored after dependency-aware exclusion");
    }

    /// **COVER-2 — an out-of-order COVERED tx survives exclusion-elsewhere + reorg
    /// (no permanent loss).** Rebuts the eviction-liveness concern: even if a peer
    /// excludes a covered-but-out-of-order tx at seal AND its (T1-only) block wins
    /// the fork, the node that HELD the tx covered re-queues it on reorg
    /// (AUDIT-BLK-1) and re-seals it once its funding is on-chain. The tx is never
    /// lost: it always rides into a block via the holder, then propagates.
    #[test]
    fn cover2_out_of_order_covered_tx_survives_exclusion_and_reorg() {
        let mut crypto_a = pq_wallet();
        let alice = gen_addr(&mut crypto_a);
        let mut crypto_b = pq_wallet();
        let bob = gen_addr(&mut crypto_b);
        let mut crypto_c = pq_wallet();
        let carol = gen_addr(&mut crypto_c);

        // node_a holds T1 (Alice→Bob 30) then T2 (Bob→Carol 20) IN ORDER — both
        // covered (T1 funds T2) — and seals a block with both.
        let mut node_a = Ledger::new();
        node_a.mine_tx(&alice, 100 * MICRO, 0.0);
        node_a.seal_block(&alice, 0.0);
        let tip = node_a.block_at(node_a.chain_height() - 1).unwrap().clone();

        let t1 = node_a.transfer_tx_at(&alice, &bob, 30 * MICRO, &crypto_a,
            "2026-08-01T00:00:00Z".into(), false).unwrap();
        let t2 = node_a.transfer_tx_at(&bob, &carol, 20 * MICRO, &crypto_b,
            "2026-08-01T00:00:01Z".into(), false).unwrap();
        let t2_hash = t2.hash.clone();
        let block_a = node_a.seal_block_at(&alice, 0.0, "2026-08-01T00:00:02Z".into());
        assert!(block_a.transactions.iter().any(|t| t.hash == t2_hash), "node_a sealed the covered T2");

        // A competing block at the same height carries ONLY T1 (the shape a peer
        // that excluded the out-of-order T2 would seal). Forge it with a higher
        // hash so it wins the deterministic fork tie-break.
        let block_b = forge_block_hash_above(block_a.index, &tip.hash, &alice, &[t1], &block_a.hash);
        assert!(block_b.hash.as_str() > block_a.hash.as_str(), "block_b wins the fork");

        // node_a reorgs to the winner → AUDIT-BLK-1 re-queues T2 (absent from the
        // winning block), so it is NOT lost despite being excluded elsewhere.
        assert_eq!(node_a.integrate_remote_block(block_b), Ok(true), "reorg to higher-hash block");
        assert!(node_a.pending_txs().iter().any(|t| t.hash == t2_hash),
            "the covered tx is RE-QUEUED on reorg (AUDIT-BLK-1) — not permanently lost");

        // node_a re-seals: T1 is now on-chain, so T2 is covered again and sealed.
        let block_c = node_a.seal_block_at(&alice, 0.0, "2026-08-01T00:00:03Z".into());
        assert!(block_c.transactions.iter().any(|t| t.hash == t2_hash),
            "T2 rides into a block once its funding is on-chain — survives end to end");
        assert_eq!(node_a.balance_of(&carol), 20 * MICRO, "carol ultimately receives her 20");
    }

    // ───────────────────── PQ-MIG-3 §4 : les dents (CRYPTO-ID-1) ─────────────
    //
    // L'autorité d'une transaction = signature ML-DSA valide depuis la clé
    // RÉVÉLÉE **et** cette clé est celle LIÉE au compte (immuable). Ces tests
    // prouvent la fermeture de CRYPTO-ID-1 : attacher sa propre clé ML-DSA à un
    // compte étranger ne passe plus.

    /// A wallet with a CHOSEN Ed25519 seed and a CHOSEN ML-DSA primary seed, so a
    /// test can model "same Ed25519 account, different ML-DSA key" deterministically.
    fn wallet_seeds(ed: u8, pq: u8) -> CryptoEngine {
        let mut c = CryptoEngine::new();
        c.import_keypair(&[ed; 32]).expect("ed seed");
        c.import_pq_identity(&[pq; 32]).expect("pq seed");
        c
    }

    /// §4 chemin nominal : une tx correctement signée (clé révélée = clé liée,
    /// signature ML-DSA valide) est ACCEPTÉE par `verify_tx` et ne viole aucune
    /// liaison.
    #[test]
    fn pqmig3_nominal_signed_tx_accepted() {
        let victim = wallet_seeds(0x11, 0x22);
        let pk = victim.pq_address_hex().expect("ml-dsa address");
        let mut ledger = Ledger::new();
        ledger.mine_tx(&pk, 100 * MICRO, 0.0);
        ledger.seal_block(&pk, 0.0);
        let tx = ledger.transfer_tx(&pk, &"d".repeat(64), 10 * MICRO, &victim).unwrap();
        assert!(Ledger::verify_tx(&tx).unwrap(), "ML-DSA-signed tx verifies");
        let tip = ledger.chain.last().unwrap().clone();
        let bindings = ledger.pq_bindings_before(&tip);
        assert!(
            Ledger::binding_violations(&bindings, std::slice::from_ref(&tx)).is_empty(),
            "first signed tx establishes the binding — not a violation"
        );
        ledger.seal_block(&pk, 0.0);
        assert!(ledger.balance_of(&"d".repeat(64)) > 0, "nominal tx integrated end to end");
    }

    /// §4 plus aucun repli Ed25519 : une tx ne présentant qu'une signature Ed25519
    /// (couche ML-DSA absente) est REJETÉE.
    #[test]
    fn pqmig3_ed25519_only_tx_rejected() {
        let victim = wallet_seeds(0x11, 0x22);
        let pk = victim.pq_address_hex().expect("ml-dsa address");
        let mut ledger = Ledger::new();
        ledger.mine_tx(&pk, 100 * MICRO, 0.0);
        ledger.seal_block(&pk, 0.0);
        let mut tx = ledger.transfer_tx(&pk, &"d".repeat(64), 10 * MICRO, &victim).unwrap();
        tx.pq_signature = None;
        tx.pq_public_key = None;
        assert!(
            !Ledger::verify_tx(&tx).unwrap(),
            "Ed25519-only tx rejected — the fallback is gone"
        );
    }

    /// §4 signature ML-DSA invalide rejetée.
    #[test]
    fn pqmig3_invalid_ml_dsa_signature_rejected() {
        let victim = wallet_seeds(0x11, 0x22);
        let pk = victim.pq_address_hex().expect("ml-dsa address");
        let mut ledger = Ledger::new();
        ledger.mine_tx(&pk, 100 * MICRO, 0.0);
        ledger.seal_block(&pk, 0.0);
        let tx = ledger.transfer_tx(&pk, &"d".repeat(64), 10 * MICRO, &victim).unwrap();
        let mut bad = tx.clone();
        let mut sig = hex::decode(bad.pq_signature.as_ref().unwrap()).unwrap();
        sig[0] ^= 0xFF;
        bad.pq_signature = Some(hex::encode(sig));
        assert!(!Ledger::verify_tx(&bad).unwrap(), "corrupted ML-DSA signature rejected");
    }

    /// §4 substitution de clé rejetée : remplacer la clé révélée (sans re-signer)
    /// change la préimage ⇒ les deux signatures échouent ⇒ rejet.
    #[test]
    fn pqmig3_key_substitution_rejected() {
        let victim = wallet_seeds(0x11, 0x22);
        let pk = victim.pq_address_hex().expect("ml-dsa address");
        let other = wallet_seeds(0x33, 0x44);
        let mut ledger = Ledger::new();
        ledger.mine_tx(&pk, 100 * MICRO, 0.0);
        ledger.seal_block(&pk, 0.0);
        let tx = ledger.transfer_tx(&pk, &"d".repeat(64), 10 * MICRO, &victim).unwrap();
        let mut swapped = tx.clone();
        swapped.pq_public_key = Some(other.pq_identity_hex().unwrap());
        assert!(
            !Ledger::verify_tx(&swapped).unwrap(),
            "swapping the revealed key (pre-image changes) invalidates the signatures"
        );
    }

    /// §4 / PQ-MIG-3B — **LE** test : clé NON LIÉE rejetée. L'attaque CRYPTO-ID-1 :
    /// un adversaire vise le compte `from` de la victime mais ne possède que **sa
    /// propre** clé ML-DSA K_a. Sous le modèle (b), `from` EST l'adresse `addr(K_v)`,
    /// donc la clé révélée doit hasher vers `from` : `verify_tx` **rejette
    /// directement** (la liaison est intrinsèque, `lie(from, K_a)` faux). Casser
    /// Ed25519 ne donne aucun pouvoir — l'adresse ne dérive plus d'une clé Ed25519.
    #[test]
    fn pqmig3_unbound_key_rejected_closes_crypto_id_1() {
        // Victim's ACCOUNT is its ML-DSA address addr(K_v). It funds + seals a tx.
        let victim = wallet_seeds(0x11, 0x22);
        let pk = victim.pq_address_hex().expect("ml-dsa address"); // = addr(K_v)
        let kv = victim.pq_identity_hex().unwrap();
        let mut ledger = Ledger::new();
        ledger.mine_tx(&pk, 100 * MICRO, 0.0);
        ledger.seal_block(&pk, 0.0);
        let legit = ledger.transfer_tx(&pk, &"d".repeat(64), 10 * MICRO, &victim).unwrap();
        assert!(Ledger::verify_tx(&legit).unwrap());
        ledger.seal_block(&pk, 0.0);
        let tip = ledger.chain.last().unwrap().clone();
        let bindings = ledger.pq_bindings_before(&tip);

        // Attacker: even with the victim's Ed25519 seed (a modelled quantum break),
        // it can only ML-DSA-sign with its OWN independent primary K_a ≠ K_v.
        let attacker = wallet_seeds(0x11, 0x99);
        let ka = attacker.pq_identity_hex().unwrap();
        assert_ne!(ka, kv, "attacker's ML-DSA key differs from the victim's");

        // Evil tx: claims `from = addr(K_v)` but reveals/signs with K_a.
        let evil = sign_tx_with_nonce(&attacker, &pk, &"e".repeat(64), 5 * MICRO, ledger.get_nonce(&pk));
        // INTRINSIC closure: the revealed key does NOT hash to `from` ⇒ `verify_tx`
        // REJECTS it outright (no Ed25519 check, no registry needed) — this IS the
        // closure of CRYPTO-ID-1 under model (b).
        assert!(
            !Ledger::verify_tx(&evil).unwrap(),
            "unbound key (≠ the one whose address is `from`) MUST be rejected by verify_tx"
        );
        assert!(
            !CryptoEngine::address_hex_binds_key_hex(&evil.from, evil.pq_public_key.as_deref().unwrap()),
            "lie(from, K_a) is false — the account commits to K_v, not K_a"
        );
        // Defense-in-depth: the retained on-chain binding registry ALSO flags it.
        assert_eq!(
            Ledger::binding_violations(&bindings, std::slice::from_ref(&evil)),
            vec![0],
            "binding registry (backstop) also rejects the unbound key"
        );
        // …and the full consensus path rejects a block carrying it.
        let tx_root = Ledger::compute_merkle_root(std::slice::from_ref(&evil));
        let idx = tip.index + 1;
        let ts = "2026-09-09T00:00:00Z";
        let payload = format!("{}:{}:{}:{}:{}:{}", idx, tip.hash, ts, pk, 1, tx_root);
        let evil_block = Block {
            index: idx,
            timestamp: ts.into(),
            transactions: vec![evil.clone()],
            prev_hash: tip.hash.clone(),
            hash: hex::encode(blake3::hash(payload.as_bytes()).as_bytes()),
            miner: pk.clone(),
            energy_kwh: 0.0,
        };
        let onchain = ledger.onchain_spendable_before(&tip);
        let bonded = ledger.validator_stakes();
        assert!(
            Ledger::validate_block_against_prev(&evil_block, &tip, &onchain, &bindings, &bonded).is_err(),
            "a block carrying the unbound-key tx is rejected by validation"
        );
    }

    // ── PQ-MIG-3B : from = adresse ML-DSA, autorité PURE ML-DSA + liaison ────────
    //
    // ADR-007 (b) : `tx.from` est désormais l'**adresse ML-DSA** (PQ-MIG-2,
    // `BLAKE3(ADDR_DOMAIN ‖ pk)`). L'autorité d'une tx = signature ML-DSA valide
    // de la clé révélée **et** `lie(from, clé)` — la clé révélée hashe vers `from`.
    // Le co-facteur Ed25519 disparaît du chemin d'autorité (l'adresse n'est plus
    // une clé Ed25519). La faille CRYPTO-ID-1 se ferme **intrinsèquement** : le
    // compte (`from`) commet cryptographiquement la clé, donc on ne peut pas
    // attacher une autre clé à un compte étranger.

    /// PQ-MIG-3B — chemin nominal : une tx dont `from` est l'**adresse ML-DSA** du
    /// signataire, révélant la clé qui hashe vers cette adresse, avec une signature
    /// ML-DSA valide, est ACCEPTÉE. (Échoue avant 3B : l'ancien `verify_tx` exigeait
    /// une signature Ed25519 valide *de `from`*, impossible quand `from` est une
    /// adresse et non une clé Ed25519.)
    #[test]
    fn pqmig3b_nominal_address_tx_accepted() {
        let mut wallet = pq_wallet();
        wallet.generate_keypair(); // couche Ed25519 (transport) ; primaire posé par pq_wallet
        let addr = wallet.pq_address_hex().expect("ml-dsa address");
        let mut ledger = Ledger::new();
        ledger.mine_tx(&addr, 100 * MICRO, 0.0);
        ledger.seal_block(&addr, 0.0);
        let tx = ledger.transfer_tx(&addr, &"d".repeat(64), 10 * MICRO, &wallet).unwrap();
        assert!(
            Ledger::verify_tx(&tx).unwrap(),
            "address-from tx with a valid ML-DSA sig and lie(from,key) holding must verify"
        );
        assert!(
            CryptoEngine::address_hex_binds_key_hex(&tx.from, tx.pq_public_key.as_deref().unwrap()),
            "the revealed key intrinsically hashes to `from` (the address IS the binding)"
        );
    }

    // ── MSIG-1 — native post-quantum M-of-N multisig ─────────────────────────

    fn msig_signer() -> CryptoEngine {
        let mut c = CryptoEngine::new();
        c.generate_pq_identity().unwrap();
        c
    }

    #[test]
    fn msig1_address_is_deterministic_order_free_and_policy_bound() {
        let s: Vec<CryptoEngine> = (0..3).map(|_| msig_signer()).collect();
        let keys: Vec<String> = s.iter().map(|c| c.pq_identity_hex().unwrap()).collect();

        let a = crate::security::multisig_address_hex(&keys, 2).unwrap();
        assert_eq!(a, crate::security::multisig_address_hex(&keys, 2).unwrap(), "deterministic");
        let mut reordered = keys.clone();
        reordered.reverse();
        assert_eq!(a, crate::security::multisig_address_hex(&reordered, 2).unwrap(), "order-independent");
        assert_ne!(a, crate::security::multisig_address_hex(&keys, 3).unwrap(), "threshold-bound");
        // A different key set ⇒ a different address.
        let other = msig_signer().pq_identity_hex().unwrap();
        let swapped = vec![keys[0].clone(), keys[1].clone(), other];
        assert_ne!(a, crate::security::multisig_address_hex(&swapped, 2).unwrap(), "key-set-bound");
        // A multisig address never collides with a single-key address (domain sep).
        assert_ne!(a, CryptoEngine::ml_dsa_address_hex(keys[0].as_bytes()));
    }

    #[test]
    fn msig1_quorum_accept_and_reject() {
        let s: Vec<CryptoEngine> = (0..3).map(|_| msig_signer()).collect();
        let keys: Vec<String> = s.iter().map(|c| c.pq_identity_hex().unwrap()).collect();
        let addr = crate::security::multisig_address_hex(&keys, 2).unwrap();

        // 2-of-3 with two DISTINCT signers → accepted. Any valid pair works.
        for pair in [[0usize, 1], [0, 2], [1, 2]] {
            let mut l = Ledger::new();
            let tx = l
                .build_multisig_tx("recipient", 100, TxType::Transfer, &keys, 2, &[&s[pair[0]], &s[pair[1]]])
                .unwrap();
            assert_eq!(tx.from, addr, "from is the policy address");
            assert!(VerifiedTx::new(tx).is_some(), "2-of-3 pair {pair:?} accepted");
        }

        // Below threshold (one signer) → rejected.
        let mut l = Ledger::new();
        let low = l.build_multisig_tx("r", 100, TxType::Transfer, &keys, 2, &[&s[0]]).unwrap();
        assert!(VerifiedTx::new(low).is_none(), "1 signature < 2 threshold rejected");

        // Duplicate signer cannot inflate the quorum (counted per DISTINCT key).
        let mut l = Ledger::new();
        let dup = l.build_multisig_tx("r", 100, TxType::Transfer, &keys, 2, &[&s[0], &s[0]]).unwrap();
        assert!(VerifiedTx::new(dup).is_none(), "same key twice is one signer");

        // An outsider's signature does not count toward the quorum.
        let outsider = msig_signer();
        let mut l = Ledger::new();
        let out = l
            .build_multisig_tx("r", 100, TxType::Transfer, &keys, 2, &[&s[0], &outsider])
            .unwrap();
        assert!(VerifiedTx::new(out).is_none(), "non-registered signer does not count");
    }

    #[test]
    fn msig1_rejects_rebind_and_tamper() {
        let s: Vec<CryptoEngine> = (0..3).map(|_| msig_signer()).collect();
        let keys: Vec<String> = s.iter().map(|c| c.pq_identity_hex().unwrap()).collect();

        // Rebind: replace a registered key in the auth → `from` no longer matches.
        let mut l = Ledger::new();
        let mut tx = l
            .build_multisig_tx("r", 100, TxType::Transfer, &keys, 2, &[&s[0], &s[1]])
            .unwrap();
        let mut auth: MultisigAuth = serde_json::from_str(tx.pq_signature.as_ref().unwrap()).unwrap();
        auth.pubkeys[0] = msig_signer().pq_identity_hex().unwrap();
        tx.pq_signature = Some(serde_json::to_string(&auth).unwrap());
        assert!(VerifiedTx::new(tx).is_none(), "rebinding keys breaks the address binding");

        // Tamper the amount → recomputed hash disagrees → rejected.
        let mut l = Ledger::new();
        let mut tx = l
            .build_multisig_tx("r", 100, TxType::Transfer, &keys, 2, &[&s[0], &s[1]])
            .unwrap();
        tx.amount = 999_999;
        assert!(VerifiedTx::new(tx).is_none(), "amount tamper breaks the pre-image hash");
    }

    // ── Regression tests for the adversarial-review findings ─────────────────

    /// MSIG-SEC-1: a single key spelled in two hex CASES must not fill two quorum
    /// slots (hex decoding is case-insensitive). Cryptographic canonicalization
    /// collapses the aliases to one key, so a "2-of-2" of one key is impossible.
    #[test]
    fn msig1_case_aliased_keys_collapse() {
        let k = msig_signer();
        let k_lo = k.pq_identity_hex().unwrap();
        let k_hi = k_lo.to_uppercase();
        assert_ne!(k_lo, k_hi, "hex spellings differ as strings");
        let canon = crate::security::canonicalize_msig_keys(&[k_lo.clone(), k_hi.clone()]).unwrap();
        assert_eq!(canon.len(), 1, "case aliases are the SAME key");
        // The "2-of-2" address of the aliases equals the 1-key address (unsatisfiable
        // at threshold 2) — it can never be a genuine 2-of-2.
        assert_eq!(
            crate::security::multisig_address_hex(&[k_lo.clone(), k_hi.clone()], 2),
            crate::security::multisig_address_hex(std::slice::from_ref(&k_lo), 2)
        );
        // Building a 2-of-2 from a single aliased key is refused (threshold 2 > 1 key).
        let mut l = Ledger::new();
        assert!(l
            .build_multisig_tx("r", 1, TxType::Transfer, &[k_lo.clone(), k_hi.clone()], 2, &[&k, &k])
            .is_err());
        // A malformed (non-ML-DSA-65) key invalidates the whole policy.
        assert!(crate::security::canonicalize_msig_keys(&["deadbeef".to_string()]).is_none());
    }

    /// MINT-GUARD-1: a `Mining` tx authorized by a real user account (not the
    /// synthetic `NETWORK`) is a mint forgery and must be rejected at the gate.
    #[test]
    fn mint_guard_rejects_user_authorized_mining_tx() {
        let mut c = CryptoEngine::new();
        let _ = c.generate_keypair();
        c.generate_pq_identity().unwrap();
        let from = c.pq_address_hex().unwrap();
        let mut l = Ledger::new();
        let forged = l.build_signed_tx(&from, &from, 1_000_000, TxType::Mining, &c).unwrap();
        assert_eq!(forged.from, from);
        assert!(VerifiedTx::new(forged).is_none(), "user-signed Mining rejected (MINT-GUARD-1)");
        // A normal Transfer from the same account still verifies (single-key unaffected).
        let ok = l.build_signed_tx(&from, "recipient", 1, TxType::Transfer, &c).unwrap();
        assert!(VerifiedTx::new(ok).is_some());
    }

    /// MINT-GUARD-2: `coalesce_block_rewards` coalesces ONLY genuine `NETWORK`
    /// rewards; a forged non-`NETWORK` `Mining` tx is dropped, never minted.
    #[test]
    fn mint_guard_coalesce_drops_forged_mining() {
        fn mining(from: &str, amount: u64, id: &str) -> Transaction {
            Transaction {
                id: id.into(),
                from: from.into(),
                to: "x".into(),
                amount,
                tx_type: TxType::Mining,
                timestamp: "t".into(),
                signature: String::new(),
                hash: String::new(),
                nonce: 0,
                pq_signature: None,
                pq_public_key: None,
                fault_proof: None,
                slash_unbonding: None,
            }
        }
        let txs = vec![
            mining("NETWORK", 10, "a"),
            mining("NETWORK", 20, "b"),
            mining("attacker", 1000, "c"),
        ];
        let out = Ledger::coalesce_block_rewards(txs, "miner", 1, "ts");
        let rewards: Vec<&Transaction> = out.iter().filter(|t| t.tx_type == TxType::Mining).collect();
        assert_eq!(rewards.len(), 1, "exactly one coalesced reward");
        assert_eq!(rewards[0].from, "NETWORK");
        assert_eq!(rewards[0].amount, 30, "forged Mining amount is DROPPED, never minted");
    }
}
