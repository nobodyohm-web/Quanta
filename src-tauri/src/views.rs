//! Shared read-model builders — the single home for the view-model **math** that
//! both the Tauri command surface (`crate::commands`, feeding the desktop UI) and
//! the JSON-RPC surface (`crate::rpc`, the exchange/explorer integration surface)
//! present.
//!
//! # Why this module exists
//!
//! Before this, `rpc.rs` re-derived by hand the very same aggregates the Tauri
//! commands compute (finality epoch math, provable supply, per-account balance,
//! the bonded validator set, the mempool projection). Two copies of one
//! calculation drift silently — and here the drift would be *between the app the
//! user sees and the numbers an exchange integrates against*. This module makes
//! **the calculation live exactly once**, as a set of **pure functions** of the
//! chain state (`&Ledger` + already-fetched refs), each returning a documented,
//! serializable struct.
//!
//! # Units — always base units in the view
//!
//! Every amount a view carries is **integer µQTA** (`1 QUANTA = 1_000_000 µQTA`),
//! never a float. This is deliberate: the RPC surface must hand exchanges exact
//! integers with zero rounding drift, so the *source of truth* is integer. The
//! desktop surface, which shows QUANTA to humans, converts `µQTA / MICRO → f64`
//! **in its own thin adapter**, not here.
//!
//! # Wire compatibility (ABSOLUTE) — how the adapters relate to the views
//!
//! Both external JSON shapes are frozen: the desktop frontend reads specific
//! field names in QUANTA, and the RPC surface is the exchange integration
//! contract in µQTA. Where a view's shape already **equals** what a surface
//! emitted, that surface serializes the view **directly** (byte-identical). Where
//! the two surfaces differ (unit, field naming, or extra per-surface fields),
//! each keeps a **thin adapter** that maps this shared view onto its own frozen
//! JSON. The per-view docs below record exactly which adapter maps what.

use serde::{Deserialize, Serialize};

use crate::p2p::ledger::{Ledger, LedgerStats};
use crate::p2p::ledger_types::Transaction;

/// Finality-quorum numerator (⅔ of stake). Literal on both surfaces today; kept
/// here so the single quorum constant feeds every view.
pub const QUORUM_NUM: u64 = 2;
/// Finality-quorum denominator (⅔ of stake).
pub const QUORUM_DEN: u64 = 3;

// ───────────────────────────── Finality ─────────────────────────────

/// Casper-FFG finality snapshot — the shared math behind `getfinalityinfo` (RPC)
/// and `get_finality_status` (Tauri). Pure function of the ledger; all stake
/// amounts in **µQTA**.
///
/// ## Consumers & adapters
/// - **RPC `getfinalityinfo`** — serializes this struct **directly**
///   (`serde_json::to_value`); the field set and order below are exactly its
///   frozen JSON, so the wire is byte-identical to the former hand-rolled map.
/// - **Tauri `get_finality_status`** — **thin adapter**: emits `total_staked` in
///   **QUANTA** (`total_staked_uqta / MICRO`), drops `min_validator_stake_uqta`,
///   and adds two node-local fields it computes separately from `addr`
///   (`my_stake` in QUANTA via `Ledger::staked_of`, and `i_am_validator`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinalityView {
    /// Current chain height (block count including genesis).
    pub height: u64,
    /// Last finalized (irreversible) block index — the finality floor.
    pub finalized_floor: u64,
    /// Current epoch index (`height / epoch_length`).
    pub epoch: u64,
    /// Blocks per epoch (`EPOCH_LENGTH_BLOCKS`).
    pub epoch_length: u64,
    /// Blocks elapsed since the current epoch's boundary (`height % epoch_length`).
    pub blocks_into_epoch: u64,
    /// Height of the next epoch checkpoint.
    pub next_checkpoint: u64,
    /// Count of bonded validators with stake ≥ `min_validator_stake_uqta`.
    pub validators: usize,
    /// Sum of **all** bonded stake (µQTA), including sub-minimum stakes — matches
    /// the historical `stakes.values().sum()` on both surfaces.
    pub total_staked_uqta: u64,
    /// Minimum stake to be counted a validator (µQTA).
    pub min_validator_stake_uqta: u64,
    /// Finality quorum numerator (⅔).
    pub quorum_num: u64,
    /// Finality quorum denominator (⅔).
    pub quorum_den: u64,
}

/// Build the [`FinalityView`] purely from chain state. One `validator_stakes()`
/// read feeds both the total and the eligible count (no double scan).
pub fn finality_view(ledger: &Ledger) -> FinalityView {
    use crate::sm::finality::EPOCH_LENGTH_BLOCKS;
    let min = crate::p2p::pos_consensus::MIN_VALIDATOR_STAKE;
    let height = ledger.chain_height();
    let stakes = ledger.validator_stakes();
    let total_staked_uqta: u64 = stakes.values().sum();
    let validators = stakes.values().filter(|&&s| s >= min).count();
    let blocks_into_epoch = height % EPOCH_LENGTH_BLOCKS;
    FinalityView {
        height,
        finalized_floor: ledger.finalized_floor_index(),
        epoch: height / EPOCH_LENGTH_BLOCKS,
        epoch_length: EPOCH_LENGTH_BLOCKS,
        blocks_into_epoch,
        next_checkpoint: height - blocks_into_epoch + EPOCH_LENGTH_BLOCKS,
        validators,
        total_staked_uqta,
        min_validator_stake_uqta: min,
        quorum_num: QUORUM_NUM,
        quorum_den: QUORUM_DEN,
    }
}

// ───────────────────────────── Supply ─────────────────────────────

/// Provable-supply snapshot (µQTA) — the shared money-supply math behind
/// `get_economy_stats`, `get_chain_overview` (Tauri) and `getinfo` (RPC).
///
/// `circulating_uqta == minted − burned` is exactly what `Ledger::total_supply()`
/// returns, so it also backs `get_chain_overview`'s `total_supply_qta` — one field
/// serves both the "circulating" and "total supply" names the surfaces used.
///
/// ## Consumers & adapters (all **thin adapters**, none serialize this directly)
/// - **Tauri `get_economy_stats`** — QUANTA floats: `total_mined`, `total_burned`,
///   `circulating`, `max_supply` from the matching `_uqta` fields; `emission_per_minute`
///   = `emission_next_tick_uqta / MICRO`, `emission_per_hour` = ×60; plus the constant
///   `emission_model: "decaying_capped"`.
/// - **Tauri `get_chain_overview`** — QUANTA floats: `total_supply_qta` ← `circulating_uqta`,
///   `total_mined_qta`, `total_burned_qta`, `max_supply_qta`, `remaining_qta`,
///   `emission_next_tick_qta`, and `pct_to_cap` = `minted/max_supply × 100`.
/// - **RPC `getinfo`** — integers, only two fields: `minted_uqta`, `max_supply_uqta`
///   (the provable-supply transparency pair); the rest of `getinfo` is node status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SupplyView {
    /// Total ever minted (µQTA) — `stats.total_mined`.
    pub minted_uqta: u64,
    /// Total burned (µQTA).
    pub burned_uqta: u64,
    /// Circulating supply = `minted − burned` (µQTA) — equals `Ledger::total_supply()`.
    pub circulating_uqta: u64,
    /// Hard cap (µQTA) — `MAX_SUPPLY_MICRO`.
    pub max_supply_uqta: u64,
    /// Headroom to the cap = `max_supply − minted` (µQTA).
    pub remaining_uqta: u64,
    /// Emission the next mining tick would mint at the current supply (µQTA),
    /// from the exact same `emission_for_tick` the mining loop uses.
    pub emission_next_tick_uqta: u64,
}

/// Build the [`SupplyView`]. Takes the caller's already-computed [`LedgerStats`]
/// (each consumer already calls `ledger.stats()`), avoiding a second O(chain)
/// scan; `minted` comes from there, `burned` from `total_burned()`.
pub fn supply_view(ledger: &Ledger, stats: &LedgerStats) -> SupplyView {
    let minted_uqta = stats.total_mined;
    let burned_uqta = ledger.total_burned();
    let max_supply_uqta = crate::p2p::reputation::MAX_SUPPLY_MICRO;
    SupplyView {
        minted_uqta,
        burned_uqta,
        circulating_uqta: minted_uqta.saturating_sub(burned_uqta),
        max_supply_uqta,
        remaining_uqta: max_supply_uqta.saturating_sub(minted_uqta),
        emission_next_tick_uqta: crate::p2p::reputation::emission_for_tick(minted_uqta),
    }
}

// ───────────────────────────── Balance ─────────────────────────────

/// Per-account money split (µQTA) — shared by `getbalance` / `getwalletinfo`
/// (RPC) and `get_balance` (Tauri). Keyed by the canonical on-chain **hex**
/// address.
///
/// ## Consumers & adapters
/// - **RPC `getbalance`** — emits both fields as-is plus the echoed `address` (bech32).
/// - **RPC `getwalletinfo`** — emits both fields as-is plus `has_wallet`/`address`.
/// - **Tauri `get_balance`** — returns only `spendable_uqta / MICRO` as a bare `f64`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BalanceView {
    /// Spendable (liquid) balance (µQTA).
    pub spendable_uqta: u64,
    /// Bonded stake (µQTA).
    pub staked_uqta: u64,
}

/// Build the [`BalanceView`] for `addr_hex` (canonical 64-hex on-chain address).
pub fn balance_view(ledger: &Ledger, addr_hex: &str) -> BalanceView {
    BalanceView {
        spendable_uqta: ledger.balance_of(addr_hex),
        staked_uqta: ledger.staked_of(addr_hex),
    }
}

// ───────────────────────────── Validators ─────────────────────────────

/// One entry of the bonded validator set. `address_hex` is the canonical on-chain
/// address; presentation (bech32 encoding) is left to the consuming surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidatorEntry {
    /// Canonical on-chain hex address of the validator.
    pub address_hex: String,
    /// Bonded stake (µQTA).
    pub stake_uqta: u64,
}

/// The canonical on-chain bonded validator set — every account with stake ≥
/// `MIN_VALIDATOR_STAKE`, ordered stake-descending then address-ascending. This
/// is the *same eligibility definition* [`FinalityView::validators`] counts
/// (`≥ MIN`), surfaced here as the full ordered list.
///
/// ## Consumers & adapters
/// - **RPC `getvalidators`** — maps each entry to `{ address (bech32), address_hex,
///   stake_uqta }` and wraps in `{ count, validators }`. Single live consumer today;
///   this is the canonical set-builder, kept pure and testable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidatorsView {
    /// Eligible validators, stake-descending then address-ascending.
    pub validators: Vec<ValidatorEntry>,
}

/// Build the [`ValidatorsView`] purely from chain state.
pub fn validators_view(ledger: &Ledger) -> ValidatorsView {
    let min = crate::p2p::pos_consensus::MIN_VALIDATOR_STAKE;
    let mut vs: Vec<ValidatorEntry> = ledger
        .validator_stakes()
        .into_iter()
        .filter(|(_, s)| *s >= min)
        .map(|(address_hex, stake_uqta)| ValidatorEntry { address_hex, stake_uqta })
        .collect();
    vs.sort_by(|a, b| {
        b.stake_uqta
            .cmp(&a.stake_uqta)
            .then_with(|| a.address_hex.cmp(&b.address_hex))
    });
    ValidatorsView { validators: vs }
}

// ───────────────────────────── Mempool ─────────────────────────────

/// One pending (mempool) transaction projected for display (µQTA). `tx_type`
/// serializes under the JSON key `type` (the frozen RPC field name), rendered
/// with the enum's `Debug` form as the historical code did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MempoolEntry {
    /// Transaction hash.
    pub hash: String,
    /// Sender address (hex or synthetic).
    pub from: String,
    /// Recipient address (hex or synthetic).
    pub to: String,
    /// Amount (µQTA).
    pub amount_uqta: u64,
    /// Transaction type, `Debug`-rendered (e.g. `"Transfer"`, `"Stake"`).
    #[serde(rename = "type")]
    pub tx_type: String,
}

/// Projected mempool — pending txs not yet sealed.
///
/// ## Consumers & adapters
/// - **RPC `getmempool`** — wraps `{ count, transactions }`. Single-surface shape
///   (no Tauri twin; `get_wallet_overview` scans the mempool for a *different*,
///   per-account aggregate). Centralized here for a testable, consistent tx
///   projection, not for cross-surface dedup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MempoolView {
    /// Pending transactions, in mempool order.
    pub transactions: Vec<MempoolEntry>,
}

/// Build the [`MempoolView`] from the ledger's pending set.
pub fn mempool_view(ledger: &Ledger) -> MempoolView {
    let transactions = ledger
        .pending_txs()
        .iter()
        .map(entry_from_tx)
        .collect();
    MempoolView { transactions }
}

/// Project a pending [`Transaction`] into a [`MempoolEntry`] (shared field mapping).
fn entry_from_tx(t: &Transaction) -> MempoolEntry {
    MempoolEntry {
        hash: t.hash.clone(),
        from: t.from.clone(),
        to: t.to.clone(),
        amount_uqta: t.amount,
        tx_type: format!("{:?}", t.tx_type),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::p2p::ledger::{Ledger, MICRO};

    #[test]
    fn finality_view_serializes_byte_compatibly_with_old_getfinalityinfo() {
        // A fresh chain: nobody staked → zero validators, zero total stake, and
        // the frozen quorum/epoch constants.
        let ledger = Ledger::new();
        let v = finality_view(&ledger);
        assert_eq!(v.validators, 0);
        assert_eq!(v.total_staked_uqta, 0);
        assert_eq!(v.quorum_num, 2);
        assert_eq!(v.quorum_den, 3);

        // WIRE-COMPAT PROOF: RPC `getfinalityinfo` now serializes this view
        // directly. Rebuild the JSON the *old* hand-rolled handler produced, from
        // the same ledger, and assert equality — the exact frozen key set + values.
        // (serde_json has no `preserve_order`, so both the old `json!` map and the
        // struct serialize through a sorted `BTreeMap` → identical bytes too.)
        use crate::sm::finality::EPOCH_LENGTH_BLOCKS;
        let min = crate::p2p::pos_consensus::MIN_VALIDATOR_STAKE;
        let height = ledger.chain_height();
        let stakes = ledger.validator_stakes();
        let total_staked: u64 = stakes.values().sum();
        let validators = stakes.values().filter(|&&s| s >= min).count();
        let blocks_into_epoch = height % EPOCH_LENGTH_BLOCKS;
        let expected = serde_json::json!({
            "height": height,
            "finalized_floor": ledger.finalized_floor_index(),
            "epoch": height / EPOCH_LENGTH_BLOCKS,
            "epoch_length": EPOCH_LENGTH_BLOCKS,
            "blocks_into_epoch": blocks_into_epoch,
            "next_checkpoint": height - blocks_into_epoch + EPOCH_LENGTH_BLOCKS,
            "validators": validators,
            "total_staked_uqta": total_staked,
            "min_validator_stake_uqta": min,
            "quorum_num": 2,
            "quorum_den": 3,
        });
        assert_eq!(serde_json::to_value(&v).unwrap(), expected);
        // Byte-level, on the REAL RPC path: the handler returns `to_value(view)`
        // (a `Value`), then it's stringified — so both sides go through serde_json's
        // sorted `BTreeMap`, yielding byte-identical output.
        assert_eq!(serde_json::to_value(&v).unwrap().to_string(), expected.to_string());
    }

    #[test]
    fn supply_view_is_provable_and_capped() {
        let mut ledger = Ledger::new();
        let addr = "aa".repeat(32);
        ledger.mine_tx(&addr, 5 * MICRO, 0.0);
        let stats = ledger.stats();
        let v = supply_view(&ledger, &stats);
        assert_eq!(v.minted_uqta, stats.total_mined);
        assert_eq!(v.max_supply_uqta, crate::p2p::reputation::MAX_SUPPLY_MICRO);
        // circulating == minted − burned == Ledger::total_supply().
        assert_eq!(v.circulating_uqta, ledger.total_supply());
        assert_eq!(v.remaining_uqta, v.max_supply_uqta - v.minted_uqta);
        // Same emission the mining loop would use next tick.
        assert_eq!(
            v.emission_next_tick_uqta,
            crate::p2p::reputation::emission_for_tick(v.minted_uqta)
        );
    }

    #[test]
    fn balance_view_reads_both_compartments() {
        let mut ledger = Ledger::new();
        let addr = "bb".repeat(32);
        ledger.mine_tx(&addr, 42 * MICRO, 0.0);
        let b = balance_view(&ledger, &addr);
        assert_eq!(b.spendable_uqta, ledger.balance_of(&addr));
        assert_eq!(b.staked_uqta, ledger.staked_of(&addr));
        // Unknown address → all zero (matches RPC/desktop behavior).
        let z = balance_view(&ledger, &"00".repeat(32));
        assert_eq!(z.spendable_uqta, 0);
        assert_eq!(z.staked_uqta, 0);
    }

    #[test]
    fn validators_and_mempool_views_are_empty_on_fresh_chain() {
        let ledger = Ledger::new();
        assert!(validators_view(&ledger).validators.is_empty());
        assert!(mempool_view(&ledger).transactions.is_empty());
    }
}
