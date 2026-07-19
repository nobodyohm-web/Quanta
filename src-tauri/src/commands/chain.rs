//! Chain / finality / economy reads for the frontend visualizations. All
//! read-only, cheap, and safe to poll at high frequency. Amounts µQTA → QUANTA
//! for display.

use crate::p2p;
use crate::AppState;
use std::sync::Arc;

/// Vue d'ensemble de la chaîne pour la visualisation live : offre/émission
/// globales + les N derniers blocs (du plus récent au plus ancien) avec les
/// QUANTA créés par chacun. Lecture seule — sûr à interroger à haute fréquence.
#[tauri::command]
pub async fn get_chain_overview(
    state: tauri::State<'_, Arc<AppState>>,
    limit: Option<u64>,
) -> Result<serde_json::Value, String> {
    use p2p::ledger_types::TxType;
    let ledger = state.node.ledger.read().await;
    let micro = p2p::ledger::MICRO as f64;
    let stats = ledger.stats();
    let height = ledger.chain_height();
    let lim = limit.unwrap_or(14).min(60);
    let start = height.saturating_sub(lim);

    let mut blocks: Vec<serde_json::Value> = Vec::new();
    for i in (start..height).rev() {
        if let Some(b) = ledger.block_at(i) {
            let minted: u64 = b
                .transactions
                .iter()
                .filter(|t| matches!(t.tx_type, TxType::Mining))
                .map(|t| t.amount)
                .sum();
            blocks.push(serde_json::json!({
                "index": b.index,
                "timestamp": b.timestamp,
                "hash": b.hash,
                "miner": b.miner,
                "tx_count": b.transactions.len(),
                "minted_qta": minted as f64 / micro,
                "energy_kwh": b.energy_kwh,
            }));
        }
    }

    // Shared provable-supply math (see `crate::views`), rendered in QUANTA.
    // `total_supply_qta` uses `circulating_uqta` (== `Ledger::total_supply()` ==
    // minted − burned), avoiding a second full-chain scan. `pending`/`holders`/
    // `total_energy_kwh`/`blocks` stay surface-specific (chain-viz projection).
    let v = crate::views::supply_view(&ledger, &stats);
    Ok(serde_json::json!({
        "height": height,
        "total_supply_qta": v.circulating_uqta as f64 / micro,
        "total_mined_qta": v.minted_uqta as f64 / micro,
        "total_burned_qta": v.burned_uqta as f64 / micro,
        "max_supply_qta": v.max_supply_uqta as f64 / micro,
        "remaining_qta": v.remaining_uqta as f64 / micro,
        "pct_to_cap": (v.minted_uqta as f64 / v.max_supply_uqta as f64) * 100.0,
        "emission_next_tick_qta": v.emission_next_tick_uqta as f64 / micro,
        "pending": stats.pending,
        "holders": stats.holders,
        "total_energy_kwh": stats.total_energy,
        "blocks": blocks,
    }))
}

/// Histoire COMPLÈTE de la chaîne, depuis la genèse, pour la visualiser d'un
/// coup d'œil : les blocs récents restent individuels (petits), les plus
/// anciens sont **agrégés** en gros blocs (level-of-detail). Le nombre de gros
/// blocs est borné pour rester lisible même sur une longue chaîne.
#[tauri::command]
pub async fn get_chain_history(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<serde_json::Value, String> {
    use p2p::ledger_types::TxType;
    let ledger = state.node.ledger.read().await;
    let micro = p2p::ledger::MICRO as f64;
    let height = ledger.chain_height();

    const RECENT: u64 = 24; // derniers blocs montrés individuellement
    const MAX_BUCKETS: u64 = 48; // borne le nombre de gros blocs
    let split = height.saturating_sub(RECENT);
    // Taille de bucket dynamique : ≥10, et assez grande pour ne pas dépasser MAX_BUCKETS.
    let bucket_size = if split == 0 { 10 } else { (split.div_ceil(MAX_BUCKETS)).max(10) };

    let summarize = |from: u64, to: u64| -> serde_json::Value {
        let mut minted = 0u64;
        let mut tx = 0usize;
        let mut energy = 0f64;
        let mut first_ts = String::new();
        let mut last_ts = String::new();
        for i in from..to {
            if let Some(b) = ledger.block_at(i) {
                if first_ts.is_empty() {
                    first_ts = b.timestamp.clone();
                }
                last_ts = b.timestamp.clone();
                tx += b.transactions.len();
                energy += b.energy_kwh;
                minted += b
                    .transactions
                    .iter()
                    .filter(|t| matches!(t.tx_type, TxType::Mining))
                    .map(|t| t.amount)
                    .sum::<u64>();
            }
        }
        serde_json::json!({
            "from": from, "to": to.saturating_sub(1), "count": to - from,
            "minted_qta": minted as f64 / micro, "tx_count": tx,
            "energy_kwh": energy, "first_ts": first_ts, "last_ts": last_ts,
        })
    };

    let mut buckets: Vec<serde_json::Value> = Vec::new();
    let mut i = 0u64;
    while i < split {
        let end = (i + bucket_size).min(split);
        buckets.push(summarize(i, end));
        i = end;
    }

    let mut recent: Vec<serde_json::Value> = Vec::new();
    for j in split..height {
        if let Some(b) = ledger.block_at(j) {
            let minted: u64 = b
                .transactions
                .iter()
                .filter(|t| matches!(t.tx_type, TxType::Mining))
                .map(|t| t.amount)
                .sum();
            recent.push(serde_json::json!({
                "index": b.index, "minted_qta": minted as f64 / micro,
                "tx_count": b.transactions.len(), "energy_kwh": b.energy_kwh,
                "timestamp": b.timestamp, "hash": b.hash,
            }));
        }
    }

    Ok(serde_json::json!({
        "height": height,
        "bucket_size": bucket_size,
        "recent_individual": RECENT,
        "buckets": buckets,
        "recent": recent,
    }))
}

#[tauri::command]
pub async fn get_recent_txs(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let ledger = state.node.ledger.read().await;
    let micro = p2p::ledger::MICRO as f64;
    // Convert µQTA amounts to QUANTA for frontend display
    let txs: Vec<serde_json::Value> = ledger.recent_txs(50).iter().map(|tx| {
        serde_json::json!({
            "id": tx.id,
            "from": tx.from,
            "to": tx.to,
            "amount": tx.amount as f64 / micro,
            "tx_type": tx.tx_type,
            "timestamp": tx.timestamp,
        })
    }).collect();
    Ok(serde_json::json!(txs))
}

/// Finality-gadget status for the mining screen: how far the chain is into the
/// current epoch, where the irreversibility floor sits, and the live validator
/// set (count + total bonded weight + this node's bonded weight). Read-only,
/// cheap, safe to poll.
#[tauri::command]
pub async fn get_finality_status(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let addr = state.crypto.lock().await.pq_address_hex().unwrap_or_default();
    let ledger = state.node.ledger.read().await;
    let micro = p2p::ledger::MICRO as f64;
    // Shared finality math (see `crate::views`). This surface adapts µQTA → QUANTA
    // for `total_staked` and adds the node-local `my_stake`/`i_am_validator` fields
    // (`staked_of(&addr)` == the former `stakes.get(&addr)`); it omits the RPC-only
    // `min_validator_stake_uqta`.
    let v = crate::views::finality_view(&ledger);
    let my_stake = ledger.staked_of(&addr);
    Ok(serde_json::json!({
        "height": v.height,
        "finalized_floor": v.finalized_floor,
        "epoch": v.epoch,
        "epoch_length": v.epoch_length,
        "blocks_into_epoch": v.blocks_into_epoch,
        "next_checkpoint": v.next_checkpoint,
        "validators": v.validators,
        "total_staked": v.total_staked_uqta as f64 / micro,
        "my_stake": my_stake as f64 / micro,
        "i_am_validator": my_stake >= p2p::pos_consensus::MIN_VALIDATOR_STAKE,
        "quorum_num": v.quorum_num,
        "quorum_den": v.quorum_den,
    }))
}

/// V2 — Snapshot of the QUANTA economy: supply, emission rate, burn total (in QUANTA, f64 for UI).
#[tauri::command]
pub async fn get_economy_stats(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let ledger = state.node.ledger.read().await;
    let stats = ledger.stats();
    let micro = p2p::ledger::MICRO as f64;
    // Shared provable-supply math (see `crate::views`). This surface renders the
    // µQTA view in QUANTA and derives per-minute/hour from `emission_next_tick`.
    // 1 tick = 60 s ⇒ /min ; ×60 ⇒ /h. No hard-coded rate (same `emission_for_tick`
    // the mining loop calls), no euro conversion (QUANTA has no market price).
    let v = crate::views::supply_view(&ledger, &stats);
    let emission_per_minute = v.emission_next_tick_uqta as f64 / micro;
    Ok(serde_json::json!({
        "total_mined": v.minted_uqta as f64 / micro,
        "total_burned": v.burned_uqta as f64 / micro,
        "circulating": v.circulating_uqta as f64 / micro,
        "emission_model": "decaying_capped",
        "emission_per_hour": emission_per_minute * 60.0,
        "emission_per_minute": emission_per_minute,
        "max_supply": v.max_supply_uqta as f64 / micro,
    }))
}
