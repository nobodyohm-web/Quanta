//! Chain / finality / economy reads for the frontend visualizations. All
//! read-only, cheap, and safe to poll at high frequency. Amounts µQTA → QUANTA
//! for display.

use crate::p2p;
use crate::sm;
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

    let max_supply = p2p::reputation::MAX_SUPPLY_MICRO;
    let next_emission = p2p::reputation::emission_for_tick(stats.total_mined);
    Ok(serde_json::json!({
        "height": height,
        "total_supply_qta": ledger.total_supply() as f64 / micro,
        "total_mined_qta": stats.total_mined as f64 / micro,
        "total_burned_qta": ledger.total_burned() as f64 / micro,
        "max_supply_qta": max_supply as f64 / micro,
        "remaining_qta": max_supply.saturating_sub(stats.total_mined) as f64 / micro,
        "pct_to_cap": (stats.total_mined as f64 / max_supply as f64) * 100.0,
        "emission_next_tick_qta": next_emission as f64 / micro,
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
    use sm::finality::EPOCH_LENGTH_BLOCKS;
    let addr = state.crypto.lock().await.pq_address_hex().unwrap_or_default();
    let ledger = state.node.ledger.read().await;
    let micro = p2p::ledger::MICRO as f64;
    let height = ledger.chain_height();
    let floor = ledger.finalized_floor_index();
    let stakes = ledger.validator_stakes();
    let total_staked: u64 = stakes.values().sum();
    let eligible = stakes
        .values()
        .filter(|&&s| s >= p2p::pos_consensus::MIN_VALIDATOR_STAKE)
        .count();
    let my_stake = stakes.get(&addr).copied().unwrap_or(0);
    let blocks_into_epoch = height % EPOCH_LENGTH_BLOCKS;
    Ok(serde_json::json!({
        "height": height,
        "finalized_floor": floor,
        "epoch": height / EPOCH_LENGTH_BLOCKS,
        "epoch_length": EPOCH_LENGTH_BLOCKS,
        "blocks_into_epoch": blocks_into_epoch,
        "next_checkpoint": height - blocks_into_epoch + EPOCH_LENGTH_BLOCKS,
        "validators": eligible,
        "total_staked": total_staked as f64 / micro,
        "my_stake": my_stake as f64 / micro,
        "i_am_validator": my_stake >= p2p::pos_consensus::MIN_VALIDATOR_STAKE,
        "quorum_num": 2,
        "quorum_den": 3,
    }))
}

/// V2 — Snapshot of the QUANTA economy: supply, emission rate, burn total (in QUANTA, f64 for UI).
#[tauri::command]
pub async fn get_economy_stats(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let stats = state.node.ledger.read().await.stats();
    let burned_uqta = state.node.ledger.read().await.total_burned();
    let total_mined_uqta = stats.total_mined;
    let circulating_uqta = total_mined_uqta.saturating_sub(burned_uqta);
    let micro = p2p::ledger::MICRO as f64;
    // Taux d'émission RÉEL au point actuel de la chaîne : décroît à mesure que
    // `total_mined` s'approche du plafond. 1 tick = 60 s ⇒ /min ; ×60 ⇒ /h.
    // Aucune valeur fixe codée en dur : on interroge la même fonction que le minage.
    // Aucune conversion en euros : QUANTA n'a pas de prix de marché.
    let emission_per_min_uqta = p2p::reputation::emission_for_tick(total_mined_uqta);
    let emission_per_minute = emission_per_min_uqta as f64 / micro;
    let emission_per_hour = emission_per_minute * 60.0;
    Ok(serde_json::json!({
        "total_mined": total_mined_uqta as f64 / micro,
        "total_burned": burned_uqta as f64 / micro,
        "circulating": circulating_uqta as f64 / micro,
        "emission_model": "decaying_capped",
        "emission_per_hour": emission_per_hour,
        "emission_per_minute": emission_per_minute,
        "max_supply": p2p::reputation::MAX_SUPPLY_MICRO as f64 / micro,
    }))
}
