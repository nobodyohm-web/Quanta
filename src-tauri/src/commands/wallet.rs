//! Wallet commands — balance, transfers, on-chain staking, and the wallet
//! overview (chain-truth money split). The cross-cutting `broadcast_signed_tx`
//! helper lives in `crate` (also used by `crate::rpc`) and is reused here so
//! every user-authored tx leg propagates identically (AUDIT-TX-2).

use crate::broadcast_signed_tx;
use crate::commands::error::CmdError;
use crate::p2p;
use crate::AppState;
use std::sync::Arc;

#[tauri::command]
pub async fn get_my_reputation(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    // REPUT-ID-1: reputation is keyed by the ML-DSA **address** (economic actor),
    // matching the mining loop's `uptime_tick(&addr, …)` — not the transport key.
    let pk = state.crypto.lock().await.pq_address_hex().unwrap_or_default();
    // Le total forgé vient de la chaîne, pas du miroir : `atn_earned` cumule une
    // part de Shapley sur des watts estimés localement, hors du chemin monétaire
    // depuis MINT-EXACT-1, et n'est jamais remis à zéro quand la genèse est
    // rejouée.
    //
    // L'ordre de verrous canonique (`mining_loop.rs`) est `reputation → ledger`.
    // Ce chemin prend le ledger en premier, donc le guard est **explicitement**
    // clos par ce bloc : les deux verrous ne sont jamais tenus ensemble, et il n'y
    // a pas d'inversion possible.
    let mined_uqta = {
        let ledger = state.node.ledger.read().await;
        ledger.mined_by(&pk)
    };
    let rep = state.node.reputation.read().await;
    let micro = p2p::ledger::MICRO as f64;
    // Convert µQTA → QUANTA for frontend display. The frontend reads only
    // public_key, joined_at (Profile) and atn_earned, trust_score, uptime_minutes
    // (Dashboard); the wallet money split comes from `get_wallet_overview`
    // (chain truth), so the reputation-mirror `atn_balance`/`atn_staked` are no
    // longer surfaced here — and `atn_earned` now carries the chain's coinbase
    // total for this address, keeping the Mining screen and the Wallet screen on
    // one number instead of two that drifted apart.
    match rep.get_user(&pk) {
        Some(user) => Ok(serde_json::json!({
            "public_key": user.public_key,
            "trust_score": user.trust_score,
            "status": user.status,
            "atn_earned": mined_uqta as f64 / micro,
            "uptime_minutes": user.uptime_minutes,
            "energy_kwh": user.energy_kwh,
            "energy_atn_mined": user.energy_atn_mined as f64 / micro,
            "joined_at": user.joined_at,
        })),
        None => Ok(serde_json::json!({
            "public_key": pk,
            "trust_score": 0.0,
            "status": "New",
            "atn_earned": mined_uqta as f64 / micro,
            "uptime_minutes": 0,
            "energy_kwh": 0.0,
            "energy_atn_mined": 0.0,
            "joined_at": "",
        })),
    }
}

/// STRUCT-2: Convert a frontend-supplied QUANTA value (f64) to µQTA (u64) safely.
/// Rejects negatives, NaN, infinities, and values that would overflow u64.
fn quanta_to_uqta(amount: f64) -> Result<u64, String> {
    if !amount.is_finite() || amount < 0.0 {
        return Err(CmdError::InvalidAmount.into());
    }
    let uqta_f = amount * p2p::ledger::MICRO as f64;
    if uqta_f >= u64::MAX as f64 {
        return Err(CmdError::AmountTooLarge.into());
    }
    Ok(uqta_f.round() as u64)
}

#[tauri::command]
pub async fn ledger_transfer(state: tauri::State<'_, Arc<AppState>>, to: String, amount: f64) -> Result<serde_json::Value, String> {
    // V7: Input validation. Accept EITHER the public `qta1…` (Bech32m, checksummed)
    // form OR the canonical 64-hex form, and normalize to the on-chain hex the
    // ledger keys on. The bech32 path is checksum-validated, so a mistyped receive
    // address is rejected here instead of sending to a valid-looking wrong account.
    // **BAS-1 (AUDIT-2026-08-13)** — `parse` est désormais un décodeur Bech32m
    // STRICT ; la tolérance à l'hexadécimal est explicite et nommée.
    //
    // Elle reste nécessaire ici : le front envoie une adresse **résolue** quand
    // le destinataire est un `@pseudo` (`resolveUsername` renvoie l'hexadécimal
    // de la chaîne), et cette valeur-là n'a jamais transité par un clavier. La
    // protection contre la faute de frappe se joue donc là où la frappe existe —
    // dans `WalletSend.svelte`, qui vérifie la somme de contrôle d'un `qta1…` et
    // avertit explicitement quand on lui colle de l'hexadécimal nu.
    let to = crate::security::address::parse_hex_unchecked(&to)
        .map(hex::encode)
        .map_err(|_| CmdError::InvalidRecipient)?;
    if amount <= 0.0 || amount > 1_000_000.0 {
        return Err(CmdError::AmountOutOfRange.into());
    }
    let uqta = quanta_to_uqta(amount)?;
    let crypto = state.crypto.lock().await;
    // PQ-MIG-3B: the account identity (`from`, balance key, and the CRDT/reputation
    // mirror key) is the ML-DSA **address**. PQ-ENVELOPE-1: the gossip envelope
    // sender is the ML-DSA **primary public key**. `to` is already an address (64-hex).
    let from = crypto
        .pq_address_hex()
        .ok_or(CmdError::IdentityMissing)?;
    let sender_pk = crypto
        .pq_identity_hex()
        .ok_or(CmdError::IdentityMissing)?;
    let mut ledger = state.node.ledger.write().await;
    let (tx, burn_tx, burn_uqta) = ledger
        .transfer_with_burn(&from, &to, uqta, &crypto)
        .map_err(CmdError::from_ledger)?;
    let net_uqta = uqta - burn_uqta;
    drop(ledger);
    // REPUT-ID-1: the reputation mirror is keyed by the ML-DSA **address** (the
    // economic actor) — both legs. The mining loop now fills it under `addr`
    // (`uptime_tick(&addr, …)`), so a received credit lands in the SAME bucket
    // the recipient mines into. Off the security path (ADR-002), but coherent.
    let _ = state.node.reputation.write().await.transfer(&from, &to, net_uqta);
    // Phase 3 — double-ledger : refléter le transfert net dans le PN-Counter CRDT
    // (µQTA), en **espace de valeur** (adresse ML-DSA), miroir du ledger on-chain.
    {
        let mut cons = state.node.consensus.write().await;
        cons.ledger.debit(&from, &from, net_uqta);
        cons.ledger.credit(&from, &to, net_uqta);
    }

    // ── CRITICAL: Broadcast both legs (transfer + burn) so peers' ledgers
    //   stay aligned with ours. AUDIT-TX-2: a previous bug only sent the
    //   transfer leg, leaving every other node with a 1% balance gap.
    for tx_obj in std::iter::once(&tx).chain(burn_tx.as_ref()) {
        broadcast_signed_tx(&state, &crypto, &sender_pk, tx_obj).await;
    }
    log::info!("◈ [Transfer] Broadcast {} QUANTA (+{:.6} burn) → {}",
        amount, burn_uqta as f64 / p2p::ledger::MICRO as f64, &to[..12]);

    let micro = p2p::ledger::MICRO as f64;
    Ok(serde_json::json!({
        "tx": tx,
        "burn_amount": burn_uqta as f64 / micro,
        "net_amount": net_uqta as f64 / micro,
    }))
}

/// ONCHAIN-STAKE-1 — real on-chain staking from the UI: builds the signed
/// `Stake` tx (spendable → bonded **at seal**) and broadcasts it, so every node
/// derives the same stake state from the chain. Consensus weight (leader
/// election + finality votes) comes from exactly these txs — this is what
/// makes the user a validator.
#[tauri::command]
pub async fn ledger_stake(state: tauri::State<'_, Arc<AppState>>, amount: f64) -> Result<serde_json::Value, String> {
    if amount <= 0.0 || amount > 1_000_000.0 {
        return Err(CmdError::AmountOutOfRange.into());
    }
    let uqta = quanta_to_uqta(amount)?;
    let crypto = state.crypto.lock().await;
    let from = crypto
        .pq_address_hex()
        .ok_or(CmdError::IdentityMissing)?;
    let sender_pk = crypto.pq_identity_hex().ok_or("Identité ML-DSA absente")?;
    let mut ledger = state.node.ledger.write().await;
    let tx = ledger.stake_tx(&from, uqta, &crypto).map_err(CmdError::from_ledger)?;
    drop(ledger);
    broadcast_signed_tx(&state, &crypto, &sender_pk, &tx).await;
    log::info!("◈ [Stake] Broadcast {} QUANTA → bonded at next seal", amount);
    Ok(serde_json::json!({ "tx": tx }))
}

/// ONCHAIN-STAKE-1 §3 — begin unlocking bonded stake. On seal the amount moves
/// into an unbonding entry that matures at `height + UNBONDING_PERIOD_BLOCKS`
/// (it stays slashable until then — Casper semantics, LIVE-3B). Broadcast like
/// any user tx.
#[tauri::command]
pub async fn ledger_unstake(state: tauri::State<'_, Arc<AppState>>, amount: f64) -> Result<serde_json::Value, String> {
    if amount <= 0.0 || amount > 1_000_000.0 {
        return Err(CmdError::AmountOutOfRange.into());
    }
    let uqta = quanta_to_uqta(amount)?;
    let crypto = state.crypto.lock().await;
    let from = crypto
        .pq_address_hex()
        .ok_or(CmdError::IdentityMissing)?;
    let sender_pk = crypto.pq_identity_hex().ok_or("Identité ML-DSA absente")?;
    let mut ledger = state.node.ledger.write().await;
    let tx = ledger.unstake_tx(&from, uqta, &crypto).map_err(CmdError::from_ledger)?;
    drop(ledger);
    broadcast_signed_tx(&state, &crypto, &sender_pk, &tx).await;
    log::info!("◈ [Unstake] Broadcast {} QUANTA → unbonding at next seal", amount);
    Ok(serde_json::json!({ "tx": tx }))
}

/// The wallet's single source of truth — everything the UI needs to show the
/// three-way money split, straight from the **ledger** (chain state), not the
/// reputation mirror: spendable, bonded stake, unbonding entries with their
/// unlock heights, plus pending (unsealed) stake movements for instant feedback.
/// All amounts converted µQTA → QUANTA for display.
#[tauri::command]
pub async fn get_wallet_overview(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    // `address` = canonical on-chain hex (keys the ledger, tx `from`/`to`).
    // `address_bech32` = the public checksummed `qta1…` form to show/share/QR.
    let (addr, addr_bech32) = {
        let c = state.crypto.lock().await;
        (
            c.pq_address_hex().unwrap_or_default(),
            c.pq_address_bech32().unwrap_or_default(),
        )
    };
    let ledger = state.node.ledger.read().await;
    // Le « total forgé » est lu **sur la chaîne** — la somme des coinbases qui
    // créditent cette adresse — et non plus dans `reputation.atn_earned`. Ce
    // dernier est un compteur d'affichage alimenté par une part de Shapley sur des
    // watts estimés localement : hors du chemin monétaire depuis MINT-EXACT-1, et
    // persisté dans un CRDT qui a survécu aux dix ruptures de protocole. Il
    // affichait donc « +422 forgés » à côté d'un solde de 8, sur une chaîne dont la
    // genèse avait été rejouée. Les quatre montants de cet écran viennent
    // maintenant de la même source, ce que le commentaire ci-dessus promettait déjà.
    let earned_uqta = ledger.mined_by(&addr);
    let micro = p2p::ledger::MICRO as f64;
    let height = ledger.chain_height();
    let spendable = ledger.balance_of(&addr);
    let staked = ledger.staked_of(&addr);
    let unbonding = ledger.unbonding_of(&addr);
    let entries: Vec<serde_json::Value> = ledger
        .unbonding_entries_of(&addr)
        .into_iter()
        .map(|(amount, unlock_height)| {
            serde_json::json!({
                "amount": amount as f64 / micro,
                "unlock_height": unlock_height,
                "blocks_remaining": unlock_height.saturating_sub(height),
            })
        })
        .collect();
    // Pending (mempool) movements from this account: visible instantly in the
    // UI as "in preparation" even though they only take effect at seal.
    use p2p::ledger_types::TxType;
    let (mut pending_stake, mut pending_unstake) = (0u64, 0u64);
    for tx in ledger.pending_txs() {
        if tx.from == addr {
            match tx.tx_type {
                TxType::Stake => pending_stake += tx.amount,
                TxType::Unstake => pending_unstake += tx.amount,
                _ => {}
            }
        }
    }
    Ok(serde_json::json!({
        "address": addr,
        "address_bech32": addr_bech32,
        "height": height,
        "spendable": spendable as f64 / micro,
        "staked": staked as f64 / micro,
        "unbonding": unbonding as f64 / micro,
        "unbonding_entries": entries,
        "pending_stake": pending_stake as f64 / micro,
        "pending_unstake": pending_unstake as f64 / micro,
        "earned": earned_uqta as f64 / micro,
        "min_validator_stake": p2p::pos_consensus::MIN_VALIDATOR_STAKE as f64 / micro,
        "unbonding_period_blocks": p2p::ledger::UNBONDING_PERIOD_BLOCKS,
    }))
}
