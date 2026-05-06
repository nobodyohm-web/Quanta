//! QUANTA Protocol v1.0 — Energy-indexed Sovereign P2P Protocol.
//!
//! Architecture:
//! - `security/` — Ed25519 identity, PQ Vault, hybrid crypto
//! - `p2p/` — Gossip, ledger, reputation, consensus, mining, marketplace
//! - `storage/` — SQLite persistence layer
//!
//! Background tasks (all respect `CancellationToken` for graceful shutdown):
//! - Mining loop (60s interval)
//! - Gossip outgoing drain (mpsc → iroh)
//! - Gossip incoming dispatch (iroh → dispatcher)
//! - Peer cleanup (30s dead-peer TTL)
//! - State persistence (30s snapshot to SQLite)

mod security;
mod p2p;
mod storage;
mod commands_v3;

use security::{CryptoEngine, pq_vault::PQVault};
use p2p::willow_node::WillowNode;
use storage::db::Database;
use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::Manager;

pub struct AppState {
    pub crypto: Mutex<CryptoEngine>,
    pub db: Mutex<Option<Database>>,
    pub node: WillowNode,
}

// ─── P2P Web Publishing ─────────────────────────────────────────

#[tauri::command]
async fn publish_page(state: tauri::State<'_, Arc<AppState>>, title: String, content: String) -> Result<(), String> {
    // Get our public key
    let pk = {
        let crypto = state.crypto.lock().await;
        crypto.get_identity()
            .map(|id| id.public_key_hex)
            .map_err(|e| e.to_string())?
    };

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default().as_secs();

    let version = {
        let store = state.node.page_store.read().await;
        store.get_page(&pk).map(|p| p.version + 1).unwrap_or(1)
    };

    // Sign the content
    let signable_content = format!("{}:{}:{}", pk, version, content);
    let sig_bytes = {
        let crypto = state.crypto.lock().await;
        crypto.sign(signable_content.as_bytes())?
    };
    let signature = hex::encode(&sig_bytes);

    let page = p2p::page_store::PublishedPage {
        author_pk: pk.clone(),
        content,
        title,
        updated_at: timestamp,
        signature,
        version,
    };

    // Store locally
    {
        let mut store = state.node.page_store.write().await;
        store.publish(page.clone())?;
    }

    // Broadcast to network via gossip channel
    let page_json = serde_json::to_string(&page).map_err(|e| e.to_string())?;
    let msg = p2p::gossip::GossipMessage::PublishPage { page_json };
    let ts = chrono::Utc::now().to_rfc3339();
    let nonce = state.node.gossip.read().await.next_outgoing_nonce();
    let signable = p2p::gossip::GossipRouter::signable_envelope_bytes(&pk, nonce, &ts, &msg);
    let env_sig = state.crypto.lock().await.sign(&signable).unwrap_or_default();
    let env = p2p::gossip::GossipRouter::build_signed_envelope(pk, msg, nonce, ts, &env_sig)
        .map_err(|e| e.to_string())?;
    state.node.gossip.write().await.mark_seen(&env.id);
    let _ = state.node.gossip_tx.send(env);
    state.node.gossip.write().await.stats.pages_published += 1;

    Ok(())
}

#[tauri::command]
async fn get_page(state: tauri::State<'_, Arc<AppState>>, pk: String) -> Result<serde_json::Value, String> {
    let store = state.node.page_store.read().await;
    match store.get_page(&pk) {
        Some(page) => serde_json::to_value(page).map_err(|e| e.to_string()),
        None => Err("Page non trouvée".into()),
    }
}

#[tauri::command]
async fn list_pages(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let store = state.node.page_store.read().await;
    let pages: Vec<_> = store.list_pages().iter().map(|p| {
        serde_json::json!({
            "author_pk": p.author_pk,
            "title": p.title,
            "updated_at": p.updated_at,
            "version": p.version,
            "size": p.content.len(),
        })
    }).collect();
    Ok(serde_json::Value::Array(pages))
}

#[tauri::command]
async fn get_gossip_stats(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let g = state.node.gossip.read().await;
    Ok(serde_json::json!({
        "messages_sent": g.stats.messages_sent,
        "messages_received": g.stats.messages_received,
        "bytes_sent": g.stats.bytes_sent,
        "bytes_received": g.stats.bytes_received,
        "nodes_synced": g.stats.nodes_synced,
        "peers_reported": g.stats.peers_reported,
        "dropped_signature": g.stats.dropped_signature,
        "dropped_rate_limit": g.stats.dropped_rate_limit,
        "dropped_nonce": g.stats.dropped_nonce,
        "pages_received": g.stats.pages_received,
        "pages_published": g.stats.pages_published,
    }))
}

// ─── Identity (PQ Vault) ─────────────────────────────────────────

#[tauri::command]
async fn check_identity(state: tauri::State<'_, Arc<AppState>>) -> Result<bool, String> {
    let db = state.db.lock().await;
    Ok(db.as_ref().ok_or("DB not ready")?.get_active_keypair().await?.is_some())
}

#[tauri::command]
async fn create_identity(
    state: tauri::State<'_, Arc<AppState>>, display_name: String, password: String,
) -> Result<security::pq_vault::QuantaIdentity, String> {
    let display_name = display_name.trim().to_string();
    if display_name.is_empty() { return Err("Le nom d'affichage est requis".into()); }
    if password.len() < 8 { return Err("Mot de passe trop court (min. 8)".into()); }

    let mut engine = state.crypto.lock().await;
    let (id, pk_bytes, enc_sk, nonce) = PQVault::create_identity(&mut engine, &display_name, &password)?;
    let db = state.db.lock().await;
    db.as_ref().ok_or("DB not ready")?
        .store_keypair(&pk_bytes, &enc_sk, &nonce, &display_name)
        .await?;
    Ok(id)
}

#[tauri::command]
async fn unlock_identity(
    state: tauri::State<'_, Arc<AppState>>, password: String,
) -> Result<security::pq_vault::QuantaIdentity, String> {
    let db = state.db.lock().await;
    let kp = db.as_ref().ok_or("DB not ready")?.get_active_keypair().await?.ok_or("No identity")?;
    let mut engine = state.crypto.lock().await;
    PQVault::unlock_identity(
        &mut engine,
        &kp.public_key, &kp.encrypted_secret_key, &kp.nonce,
        &password,
        &kp.display_name,
        &kp.created_at,
    )
}

#[tauri::command]
async fn get_public_key(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
    state.crypto.lock().await.get_identity().map(|i| i.public_key_hex)
}

#[tauri::command]
async fn get_recovery_key(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
    let engine = state.crypto.lock().await;
    let secret = engine.get_secret_bytes()?;
    // Format: 8 groups of 8 hex chars (64 chars total = 32 bytes Ed25519 secret)
    let hex = hex::encode(&secret);
    let formatted: Vec<&str> = (0..8).map(|i| &hex[i*8..(i+1)*8]).collect();
    Ok(formatted.join("-"))
}

// ─── P2P (Willow Node) ─────────────────────────────────────────

#[tauri::command]
async fn get_node_status(state: tauri::State<'_, Arc<AppState>>) -> Result<p2p::NodeStatus, String> {
    Ok(state.node.get_status().await)
}

/// Returns the current node contribution mode based on real-time CPU watts.
#[tauri::command]
async fn get_node_mode() -> Result<serde_json::Value, String> {
    let watts = p2p::energy::estimate_watts();
    let mode = if watts < 5.0 {
        p2p::shapley::NodeMode::Guardian
    } else {
        p2p::shapley::NodeMode::Active
    };
    Ok(serde_json::json!({
        "mode": mode,
        "watts": watts,
        "label": match mode {
            p2p::shapley::NodeMode::Guardian => "Guardian 🛡️",
            p2p::shapley::NodeMode::Active => "Active ⚡",
            p2p::shapley::NodeMode::Research => "Research 🔬",
        },
    }))
}

// ─── Security Audit (CBOM + Monitoring) ─────────────────────────

#[tauri::command]
async fn get_security_audit(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let cbom = security::crypto_agility::CryptoBOM::current();
    let node = state.node.get_status().await;
    let has_id = state.crypto.lock().await.get_identity().is_ok();
    Ok(serde_json::json!({
        "grade": cbom.security_grade(),
        "pq_pending": cbom.pq_migration_count(),
        "signing": cbom.signing,
        "key_exchange": cbom.key_exchange,
        "hashing": cbom.hashing,
        "symmetric": cbom.symmetric,
        "kdf": cbom.kdf,
        "node_online": node.is_online,
        "active_subspaces": node.active_subspaces,
        "peer_count": node.peer_count,
        "identity_active": has_id,
        "protocol": node.protocol,
    }))
}

// ─── Proof of Attention Economy (REMOVED — V2 pure crypto) ──────

#[tauri::command]
async fn get_node_ticket(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
    state.node.get_ticket().await.ok_or_else(|| "No ticket available (offline)".into())
}

/// Phase 4 — connecte ce nœud à un peer via son EndpointId Iroh.
/// Le peer doit aussi être abonné au topic QUANTA pour que le sync démarre.
/// After successful connection, immediately broadcasts a Hello so chain sync begins.
#[tauri::command]
async fn connect_peer(state: tauri::State<'_, Arc<AppState>>, peer_id: String) -> Result<(), String> {
    let peer_id = peer_id.trim().to_string();
    if peer_id.is_empty() { return Err("EndpointId vide".into()); }
    state.node.connect_peer(&peer_id).await?;
    // Trigger immediate Hello so the peer sees us and sync starts now
    let state_clone = state.inner().clone();
    tokio::spawn(async move {
        // Small delay to let gossip topology settle
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        p2p::gossip_tasks::trigger_hello_now(&state_clone).await;
    });
    Ok(())
}

// ─── Reputation & Social (REMOVED — V2 pure crypto) ─────────────

#[tauri::command]
async fn get_my_reputation(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let pk = state.crypto.lock().await.get_identity().map(|i| i.public_key_hex).unwrap_or_default();
    let rep = state.node.reputation.read().await;
    let micro = p2p::ledger::MICRO as f64;
    // Convert µQTA → QUANTA for frontend display
    match rep.get_user(&pk) {
        Some(user) => Ok(serde_json::json!({
            "public_key": user.public_key,
            "trust_score": user.trust_score,
            "status": user.status,
            "atn_earned": user.atn_earned as f64 / micro,
            "atn_balance": user.atn_balance as f64 / micro,
            "atn_staked": user.atn_staked as f64 / micro,
            "uptime_minutes": user.uptime_minutes,
            "energy_kwh": user.energy_kwh,
            "energy_atn_mined": user.energy_atn_mined as f64 / micro,
            "joined_at": user.joined_at,
        })),
        None => Ok(serde_json::json!({
            "public_key": pk,
            "trust_score": 0.0,
            "status": "New",
            "atn_earned": 0.0,
            "atn_balance": 0.0,
            "atn_staked": 0.0,
            "uptime_minutes": 0,
            "energy_kwh": 0.0,
            "energy_atn_mined": 0.0,
            "joined_at": "",
        })),
    }
}

#[tauri::command]
async fn transfer_atn(state: tauri::State<'_, Arc<AppState>>, to_pk: String, amount: f64) -> Result<(), String> {
    let my_pk = state.crypto.lock().await.get_identity().map(|i| i.public_key_hex).unwrap_or_default();
    let uqta = quanta_to_uqta(amount)?;
    state.node.reputation.write().await.transfer(&my_pk, &to_pk, uqta)
}

#[tauri::command]
async fn stake_atn(state: tauri::State<'_, Arc<AppState>>, amount: f64) -> Result<f64, String> {
    let my_pk = state.crypto.lock().await.get_identity().map(|i| i.public_key_hex).unwrap_or_default();
    let uqta = quanta_to_uqta(amount)?;
    let staked_uqta = state.node.reputation.write().await.stake(&my_pk, uqta)?;
    Ok(staked_uqta as f64 / p2p::ledger::MICRO as f64)
}

/// STRUCT-2: Convert a frontend-supplied QUANTA value (f64) to µQTA (u64) safely.
/// Rejects negatives, NaN, infinities, and values that would overflow u64.
fn quanta_to_uqta(amount: f64) -> Result<u64, String> {
    if !amount.is_finite() || amount < 0.0 {
        return Err("Montant invalide".into());
    }
    let uqta_f = amount * p2p::ledger::MICRO as f64;
    if uqta_f >= u64::MAX as f64 {
        return Err("Montant trop grand".into());
    }
    Ok(uqta_f.round() as u64)
}

#[tauri::command]
async fn get_trust_leaderboard(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!(state.node.reputation.read().await.get_leaderboard(20)))
}

#[tauri::command]
async fn get_energy_stats(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let rep = state.node.reputation.read().await;
    let (kwh, mined_uqta, uptime) = rep.network_energy_stats();
    drop(rep);
    // Phase 3 — moyenne réseau : si des peers ont rapporté leur pays, utiliser
    // la moyenne pondérée ; sinon le prix local du pays détecté.
    let reports = state.node.peer_country_reports.read().await.clone();
    let oracle = state.node.energy_oracle.read().await;
    let floor = if reports.is_empty() {
        p2p::reputation::ReputationEngine::atn_floor_eur()
    } else {
        let pairs: Vec<(String, u64)> = reports.into_iter().collect();
        let avg_price = oracle.network_weighted_average(&pairs);
        avg_price * (15.0 / 1000.0) // WATTS_IDLE_FALLBACK / 1000
    };
    drop(oracle);
    Ok(serde_json::json!({
        "kwh_consumed": kwh,
        "atn_mined": mined_uqta as f64 / p2p::ledger::MICRO as f64,
        "uptime_minutes": uptime,
        "atn_floor_eur": floor,
    }))
}

/// Phase 3 — état du consensus CRDT + DAG pour le frontend.
#[tauri::command]
async fn get_consensus_stats(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let dag = state.node.dag.read().await;
    let dag_nodes = dag.node_count();
    let dag_heads = dag.head_count();
    drop(dag);
    let cons = state.node.consensus.read().await;
    let crdt_accounts = cons.ledger.account_count();
    drop(cons);
    let gossip = state.node.gossip.read().await;
    let stats = gossip.stats.clone();
    drop(gossip);
    Ok(serde_json::json!({
        "dag_nodes": dag_nodes,
        "dag_heads": dag_heads,
        "crdt_accounts": crdt_accounts,
        "gossip_messages_sent": stats.messages_sent,
        "gossip_messages_received": stats.messages_received,
        "gossip_nodes_synced": stats.nodes_synced,
    }))
}

/// Phase 3 — santé globale du réseau (économie + énergie + sécurité).
#[tauri::command]
async fn get_network_health(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let stats = state.node.ledger.read().await.stats();
    let total_quanta_supply = stats.total_mined as f64 / p2p::ledger::MICRO as f64;

    // PoC score de l'identité courante (si présente)
    let pk = state.crypto.lock().await.get_identity().map(|i| i.public_key_hex).unwrap_or_default();
    let (poc_score, mining_multiplier) = if pk.is_empty() {
        (0.0_f64, 0.1_f64)
    } else {
        let rep = state.node.reputation.read().await;
        match rep.get_user(&pk) {
            Some(u) => {
                let s = p2p::sybil::SybilGuard::poc_score(u);
                (s, p2p::sybil::SybilGuard::mining_multiplier(s))
            }
            None => (0.0, 0.1),
        }
    };

    // Prix énergie : local si pas de peers, moyenne réseau sinon
    let reports = state.node.peer_country_reports.read().await.clone();
    let oracle = state.node.energy_oracle.read().await;
    let local_country = p2p::energy::EnergyOracle::detect_country();
    let energy_price_local_eur = if reports.is_empty() {
        oracle.price_for(local_country)
    } else {
        let pairs: Vec<(String, u64)> = reports.into_iter().collect();
        oracle.network_weighted_average(&pairs)
    };
    drop(oracle);

    let node = state.node.get_status().await;

    Ok(serde_json::json!({
        "total_quanta_supply": total_quanta_supply,
        "emission_model": "fixed",
        "emission_per_hour": 100.0,
        "energy_price_local_eur": energy_price_local_eur,
        "country": local_country,
        "peer_country_reports": state.node.peer_country_reports.read().await.clone(),
        "poc_score": poc_score,
        "mining_multiplier": mining_multiplier,
        "connected_peers": node.peer_count,
        "is_online": node.is_online,
    }))
}

// ─── ATN Protocol (Native Ledger) ───────────────────────────────

#[tauri::command]
async fn get_ledger_stats(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let ledger = state.node.ledger.read().await;
    Ok(serde_json::json!(ledger.stats()))
}

#[tauri::command]
async fn get_recent_txs(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
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

#[tauri::command]
async fn get_balance(state: tauri::State<'_, Arc<AppState>>, pk: String) -> Result<f64, String> {
    let ledger = state.node.ledger.read().await;
    // STRUCT-2: balance is u64 µQTA — convert to QUANTA (f64) for the frontend.
    Ok(ledger.balance_of(&pk) as f64 / p2p::ledger::MICRO as f64)
}

#[tauri::command]
async fn ledger_transfer(state: tauri::State<'_, Arc<AppState>>, to: String, amount: f64) -> Result<serde_json::Value, String> {
    // V7: Input validation
    if to.len() != 64 || hex::decode(&to).is_err() {
        return Err("Adresse destinataire invalide".into());
    }
    if amount <= 0.0 || amount > 1_000_000.0 {
        return Err("Montant invalide (0 < x ≤ 1 000 000)".into());
    }
    let uqta = quanta_to_uqta(amount)?;
    let crypto = state.crypto.lock().await;
    let from = crypto.get_identity()?.public_key_hex;
    let mut ledger = state.node.ledger.write().await;
    let (tx, burn_uqta) = ledger.transfer_with_burn(&from, &to, uqta, &crypto)?;
    let net_uqta = uqta - burn_uqta;
    drop(ledger);
    let _ = state.node.reputation.write().await.transfer(&from, &to, net_uqta);
    // Phase 3 — double-ledger : refléter le transfert net dans le PN-Counter CRDT (µQTA).
    {
        let mut cons = state.node.consensus.write().await;
        cons.ledger.debit(&from, &from, net_uqta);
        cons.ledger.credit(&from, &to, net_uqta);
    }

    // ── CRITICAL: Broadcast transfer TX to peers via gossip ──
    if let Ok(tx_json) = serde_json::to_string(&tx) {
        let msg = p2p::gossip::GossipMessage::BroadcastTx { tx_json };
        let timestamp = chrono::Utc::now().to_rfc3339();
        let nonce = state.node.gossip.read().await.next_outgoing_nonce();
        let signable = p2p::gossip::GossipRouter::signable_envelope_bytes(&from, nonce, &timestamp, &msg);
        let sig = crypto.sign(&signable).unwrap_or_default();
        if let Ok(env) = p2p::gossip::GossipRouter::build_signed_envelope(
            from.clone(), msg, nonce, timestamp, &sig,
        ) {
            state.node.gossip.write().await.mark_seen(&env.id);
            let _ = state.node.gossip_tx.send(env);
            log::info!("◈ [Transfer] Broadcast {} QUANTA → {}", amount, &to[..12]);
        }
    }

    let micro = p2p::ledger::MICRO as f64;
    Ok(serde_json::json!({
        "tx": tx,
        "burn_amount": burn_uqta as f64 / micro,
        "net_amount": net_uqta as f64 / micro,
    }))
}

#[tauri::command]
async fn verify_ledger(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let ledger = state.node.ledger.read().await;
    let (blocks, txs) = ledger.verify_chain()?;
    Ok(serde_json::json!({
        "verified": true,
        "blocks_verified": blocks,
        "txs_verified": txs,
    }))
}

/// V2 — Snapshot of the QUANTA economy: supply, emission rate, burn total (in QUANTA, f64 for UI).
#[tauri::command]
async fn get_economy_stats(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let stats = state.node.ledger.read().await.stats();
    let burned_uqta = state.node.ledger.read().await.total_burned();
    let total_mined_uqta = stats.total_mined;
    let circulating_uqta = total_mined_uqta.saturating_sub(burned_uqta);
    let micro = p2p::ledger::MICRO as f64;
    let floor = p2p::reputation::ReputationEngine::atn_floor_eur();
    Ok(serde_json::json!({
        "total_mined": total_mined_uqta as f64 / micro,
        "total_burned": burned_uqta as f64 / micro,
        "circulating": circulating_uqta as f64 / micro,
        "emission_model": "fixed",
        "emission_per_hour": 100.0,
        "emission_per_minute": 100.0 / 60.0,
        "quanta_floor_eur": floor,
    }))
}

// ─── Marketplace Compute (Phase 3) ──────────────────────────────

/// Soumet une tâche compute. `task_type` ∈ {"scientific","ml_training","render_3d","wasm"}.
/// Le soumetteur paye `reward QUANTA (2% brûlé via BME, 98% au worker).
#[tauri::command]
async fn submit_compute_task(
    state: tauri::State<'_, Arc<AppState>>,
    task_type: String,
    reward: f64,
    deadline: String,
) -> Result<serde_json::Value, String> {
    if reward <= 0.0 || reward > 1_000_000.0 {
        return Err("Reward invalide (0 < x ≤ 1 000 000)".into());
    }
    chrono::DateTime::parse_from_rfc3339(&deadline)
        .map_err(|_| "Deadline doit être un timestamp RFC3339".to_string())?;
    let submitter = state.crypto.lock().await.get_identity()?.public_key_hex;

    use p2p::marketplace::TaskType;
    let now_ts = chrono::Utc::now().timestamp();
    let kind = match task_type.as_str() {
        "ml_training" => TaskType::MLTraining {
            model_hash: format!("model_{}", now_ts),
            dataset_hash: "default".into(),
        },
        "render_3d" => TaskType::Render3D {
            scene_hash: format!("scene_{}", now_ts),
            resolution: (1920, 1080),
        },
        "scientific" => TaskType::Scientific {
            program_hash: format!("program_{}", now_ts),
        },
        _ => TaskType::GenericWasm {
            wasm_hash: format!("wasm_{}", now_ts),
        },
    };

    let reward_uqta = quanta_to_uqta(reward)?;
    let mut mp = state.node.marketplace.write().await;
    // CRIT-3 fix: use escrow path that verifies balance before creating the task
    let mut ledger = state.node.ledger.write().await;
    let task = mp.submit_task_with_escrow(&mut ledger, &submitter, kind, reward_uqta, &deadline, 0.0)?;
    Ok(serde_json::json!(task))
}

#[tauri::command]
async fn get_pending_tasks(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<serde_json::Value, String> {
    let mp = state.node.marketplace.read().await;
    let tasks: Vec<_> = mp.pending_tasks().into_iter().cloned().collect();
    Ok(serde_json::json!(tasks))
}

#[tauri::command]
async fn get_marketplace_stats(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<serde_json::Value, String> {
    let mp = state.node.marketplace.read().await;
    Ok(serde_json::json!(mp.stats))
}

// ─── App Entry ──────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    let app_state = Arc::new(AppState {
        crypto: Mutex::new(CryptoEngine::new()),
        db: Mutex::new(None),
        node: WillowNode::new(),
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(app_state.clone())
        .setup(move |_app| {
            let state = app_state.clone();
            tauri::async_runtime::spawn(async move {
                // ── Init Database ────────────────────────────────────
                let data_dir = dirs::data_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("quanta-protocol");
                // Auto-migrate from legacy DB name
                let legacy_path = data_dir.join("swe_titan.db");
                let db_path = data_dir.join("quanta.db");
                if legacy_path.exists() && !db_path.exists() {
                    let _ = std::fs::rename(&legacy_path, &db_path);
                    log::info!("◈ [Quanta] Migrated DB: swe_titan.db → quanta.db");
                }
                match storage::db::Database::new(&db_path).await {
                    Ok(database) => {
                        // Restore all persisted state
                        p2p::state_persistence::restore_state(&state, &database).await;
                        *state.db.lock().await = Some(database);
                        log::info!("◈ [Quanta] libSQL initialized at {:?}", db_path);
                    }
                    Err(e) => log::error!("◈ [Quanta] DB init failed: {}", e),
                }

                // ── Init Iroh P2P endpoint ───────────────────────────
                match state.node.init_endpoint().await {
                    Ok(()) => log::info!("◈ [Quanta] Iroh QUIC endpoint active"),
                    Err(e) => log::warn!("◈ [Quanta] P2P offline: {} (local mode)", e),
                }

                // ── Spawn background tasks ───────────────────────────
                p2p::gossip_tasks::spawn_outgoing_drain(state.clone());
                p2p::gossip_tasks::spawn_incoming_dispatch(state.clone());
                p2p::gossip_tasks::spawn_hello_broadcast(state.clone());
                p2p::gossip_tasks::spawn_peer_cleanup(state.clone());
                p2p::gossip_tasks::spawn_auto_reconnect(state.clone());
                p2p::mining_loop::spawn(state.clone());
                p2p::state_persistence::spawn_persistence(state.clone());
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            check_identity, create_identity, unlock_identity, get_public_key, get_recovery_key,
            get_node_status, get_node_mode,
            get_security_audit,
            get_node_ticket, connect_peer,
            get_my_reputation, transfer_atn, stake_atn, get_trust_leaderboard,
            get_energy_stats, get_consensus_stats, get_network_health,
            get_ledger_stats, get_recent_txs, get_balance, ledger_transfer,
            verify_ledger,
            get_economy_stats,
            submit_compute_task, get_pending_tasks, get_marketplace_stats,
            publish_page, get_page, list_pages,
            get_gossip_stats,
            // V3.2 — Domains
            commands_v3::claim_domain,
            commands_v3::pay_domain_rent,
            commands_v3::overbid_domain,
            commands_v3::resolve_domain,
            commands_v3::list_my_domains,
            commands_v3::grant_subdomain,
            // V3.2 — Search
            commands_v3::index_my_page,
            commands_v3::search_pages,
            commands_v3::search_stats,
            // V3.2 — Social
            commands_v3::social_vote,
            commands_v3::social_follow,
            commands_v3::social_tip,
            commands_v3::social_boost,
            commands_v3::get_page_social_stats,
            commands_v3::get_creator_social_stats,
            // V3.2 — Modération
            commands_v3::submit_moderation_report,
            commands_v3::juror_commit,
            commands_v3::juror_reveal,
            commands_v3::finalize_case,
            commands_v3::get_open_cases,
            // V3.2 — Forums
            commands_v3::forum_create,
            commands_v3::thread_create,
            commands_v3::comment_create,
            commands_v3::list_forums,
            commands_v3::list_threads,
            commands_v3::list_comments,
            // V3.3 — Sites multi-pages + assets
            commands_v3::publish_site,
            commands_v3::get_site_page,
            commands_v3::get_site_asset,
            commands_v3::list_sites,
            // V3.2 — Trust graph
            commands_v3::trust_score_for,
            // V3.3 — Subscriptions feed
            commands_v3::list_my_subscriptions,
            commands_v3::subscriptions_feed,
        ])
        .build(tauri::generate_context!())
        .expect("QUANTA Protocol v1.0 failed to build")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                log::info!("◈ [Quanta] App exit — signaling graceful shutdown");
                let state: tauri::State<'_, Arc<AppState>> = app_handle.state();
                state.node.shutdown.cancel();
            }
        });
}
