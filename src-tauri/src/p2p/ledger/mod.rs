// p2p/ledger/mod.rs — QUANTA Native Protocol Distributed Ledger
// (ledger split into sibling submodules: validation / stake / slash / reorg / tests
//  — organizational only, pure move; this file keeps the struct, all types, free
//  items, consts, re-exports, and the canonical `p2p::ledger::` public surface)
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

// ── Ledger impl split across sibling submodules (organizational only).
//    `p2p::ledger::` stays the canonical path: every public type / free
//    item / const is defined or re-exported HERE, so no external `use`
//    changes. Submodules add `impl Ledger` blocks and reach shared items
//    via `use super::*;` (child access to this module's private items).
mod reorg;
mod slash;
mod stake;
mod validation;
#[cfg(test)]
mod tests;

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
