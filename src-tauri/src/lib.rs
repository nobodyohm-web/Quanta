// lib.rs — Sovereign Web Engine v4 "Titan" Core
// Defense-grade Actor system: security, p2p, search, storage

mod security;
mod p2p;
mod storage;

use security::{CryptoEngine, pq_vault::PQVault};
use p2p::willow_node::WillowNode;
use storage::db::Database;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct AppState {
    pub crypto: Mutex<CryptoEngine>,
    pub db: Mutex<Option<Database>>,
    pub node: WillowNode,
}

// ─── Identity (PQ Vault) ────────────────────────────────────────

#[tauri::command]
async fn check_identity(state: tauri::State<'_, Arc<AppState>>) -> Result<bool, String> {
    let db = state.db.lock().await;
    Ok(db.as_ref().ok_or("DB not ready")?.get_active_keypair().await?.is_some())
}

#[tauri::command]
async fn create_identity(
    state: tauri::State<'_, Arc<AppState>>, display_name: String, password: String,
) -> Result<security::pq_vault::TitanIdentity, String> {
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
) -> Result<security::pq_vault::TitanIdentity, String> {
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
/// Le peer doit aussi être abonné au topic SOVA pour que le sync démarre.
#[tauri::command]
async fn connect_peer(state: tauri::State<'_, Arc<AppState>>, peer_id: String) -> Result<(), String> {
    let peer_id = peer_id.trim().to_string();
    if peer_id.is_empty() { return Err("EndpointId vide".into()); }
    state.node.connect_peer(&peer_id).await
}

// ─── Reputation & Social (REMOVED — V2 pure crypto) ─────────────

#[tauri::command]
async fn get_my_reputation(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let pk = state.crypto.lock().await.get_identity().map(|i| i.public_key_hex).unwrap_or_default();
    let rep = state.node.reputation.read().await;
    Ok(serde_json::json!(rep.get_user(&pk)))
}

#[tauri::command]
async fn transfer_atn(state: tauri::State<'_, Arc<AppState>>, to_pk: String, amount: f64) -> Result<(), String> {
    let my_pk = state.crypto.lock().await.get_identity().map(|i| i.public_key_hex).unwrap_or_default();
    state.node.reputation.write().await.transfer(&my_pk, &to_pk, amount)
}

#[tauri::command]
async fn stake_atn(state: tauri::State<'_, Arc<AppState>>, amount: f64) -> Result<f64, String> {
    let my_pk = state.crypto.lock().await.get_identity().map(|i| i.public_key_hex).unwrap_or_default();
    state.node.reputation.write().await.stake(&my_pk, amount)
}

#[tauri::command]
async fn get_trust_leaderboard(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!(state.node.reputation.read().await.get_leaderboard(20)))
}

#[tauri::command]
async fn get_energy_stats(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let rep = state.node.reputation.read().await;
    let (kwh, mined, uptime) = rep.network_energy_stats();
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
        "atn_mined": mined,
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
    let total_sova_supply = stats.total_mined;

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
        "total_sova_supply": total_sova_supply,
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
    Ok(serde_json::json!(ledger.recent_txs(50)))
}

#[tauri::command]
async fn get_balance(state: tauri::State<'_, Arc<AppState>>, pk: String) -> Result<f64, String> {
    let ledger = state.node.ledger.read().await;
    Ok(ledger.balance_of(&pk))
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
    let crypto = state.crypto.lock().await;
    let from = crypto.get_identity()?.public_key_hex;
    let mut ledger = state.node.ledger.write().await;
    let (tx, burn_amount) = ledger.transfer_with_burn(&from, &to, amount, &crypto)?;
    let net_amount = amount - burn_amount;
    // Mirror the transfer on the in-memory reputation balances so the UI stays in sync
    drop(ledger);
    let _ = state.node.reputation.write().await.transfer(&from, &to, net_amount);
    // Phase 3 — double-ledger : refléter le transfert net dans le PN-Counter CRDT
    {
        let milli = (net_amount * 1000.0) as u64;
        let mut cons = state.node.consensus.write().await;
        cons.ledger.debit(&from, &from, milli);
        cons.ledger.credit(&from, &to, milli);
    }
    Ok(serde_json::json!({
        "tx": tx,
        "burn_amount": burn_amount,
        "net_amount": net_amount,
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

/// V2 — Snapshot of the SOVA economy: supply, emission rate, burn total.
#[tauri::command]
async fn get_economy_stats(state: tauri::State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let stats = state.node.ledger.read().await.stats();
    let burned = state.node.ledger.read().await.total_burned();
    let total_mined = stats.total_mined;
    let floor = p2p::reputation::ReputationEngine::atn_floor_eur();
    Ok(serde_json::json!({
        "total_mined": total_mined,
        "total_burned": burned,
        "circulating": (total_mined - burned).max(0.0),
        "emission_model": "fixed",
        "emission_per_hour": 100.0,
        "emission_per_minute": 100.0 / 60.0,
        "sova_floor_eur": floor,
    }))
}

// ─── Marketplace Compute (Phase 3) ──────────────────────────────

/// Soumet une tâche compute. `task_type` ∈ {"scientific","ml_training","render_3d","wasm"}.
/// Le soumetteur paye `reward` SOVA (2% brûlé via BME, 98% au worker).
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

    let mut mp = state.node.marketplace.write().await;
    let task = mp.submit_task(&submitter, kind, reward, &deadline, 0.0)?;
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
        .manage(app_state.clone())
        .setup(move |_app| {
            let state = app_state.clone();
            tauri::async_runtime::spawn(async move {
                // Init DB
                let data_dir = dirs::data_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("sovereign-web-engine");
                let db_path = data_dir.join("swe_titan.db");
                match Database::new(&db_path).await {
                    Ok(database) => {
                        // ── Phase 1.1: Restore persisted state ───────────
                        if let Ok(Some(json)) = database.load_state("ledger").await {
                            if let Ok(snap) = serde_json::from_str::<p2p::ledger::LedgerSnapshot>(&json) {
                                let restored = p2p::ledger::Ledger::restore(snap);
                                // Phase 3.1: Verify chain integrity on load
                                match restored.verify_chain() {
                                    Ok((vb, vt)) => {
                                        log::info!("◈ [Titan] Ledger restored & verified ({} blocks, {} txs)", vb, vt);
                                        *state.node.ledger.write().await = restored;
                                    }
                                    Err(e) => {
                                        log::error!("◈ [Titan] Ledger CORRUPTED — starting fresh: {}", e);
                                        // Corrupted chain: do NOT restore, keep fresh genesis
                                    }
                                }
                            }
                        }
                        if let Ok(Some(json)) = database.load_state("reputation").await {
                            if let Ok(snap) = serde_json::from_str::<p2p::reputation::ReputationSnapshot>(&json) {
                                let user_count = snap.users.len();
                                *state.node.reputation.write().await = p2p::reputation::ReputationEngine::restore(snap);
                                log::info!("◈ [Titan] Reputation restored ({} users)", user_count);
                            }
                        }
                        // attention + notifications: removed in V2 pure crypto
                        // Phase 4 — restauration DAG / consensus CRDT / gossip router.
                        if let Ok(Some(json)) = database.load_state("dag").await {
                            if let Ok(snap) = serde_json::from_str::<p2p::merkle_dag::DagSnapshot>(&json) {
                                let count = snap.nodes.len();
                                *state.node.dag.write().await = p2p::merkle_dag::MerkleDAG::restore(snap);
                                log::info!("◈ [Titan] DAG restored ({} nodes)", count);
                            }
                        }
                        if let Ok(Some(json)) = database.load_state("consensus").await {
                            if let Ok(snap) = serde_json::from_str::<p2p::consensus::ConsensusSnapshot>(&json) {
                                let n_accounts = snap.ledger.balances.len();
                                *state.node.consensus.write().await = p2p::consensus::ConsensusEngine::restore(snap);
                                log::info!("◈ [Titan] CRDT consensus restored ({} accounts)", n_accounts);
                            }
                        }
                        if let Ok(Some(json)) = database.load_state("gossip").await {
                            if let Ok(snap) = serde_json::from_str::<p2p::gossip::GossipRouterSnapshot>(&json) {
                                let seen = snap.seen_messages.len();
                                *state.node.gossip.write().await = p2p::gossip::GossipRouter::restore(snap);
                                log::info!("◈ [Titan] Gossip router restored ({} seen msgs)", seen);
                            }
                        }

                        *state.db.lock().await = Some(database);
                        log::info!("◈ [Titan] libSQL initialized at {:?}", db_path);
                    }
                    Err(e) => log::error!("◈ [Titan] DB init failed: {}", e),
                }
                // Init Iroh P2P endpoint
                match state.node.init_endpoint().await {
                    Ok(()) => log::info!("◈ [Titan] Iroh QUIC endpoint active"),
                    Err(e) => log::warn!("◈ [Titan] P2P offline: {} (local mode)", e),
                }

                // Phase 4 — drain des enveloppes gossip sortantes : broadcast réel via iroh-gossip.
                // Si le topic n'a pas pu être souscrit, on accumule juste les stats.
                let gs = state.clone();
                tokio::spawn(async move {
                    if let Some(mut rx) = gs.node.take_gossip_receiver().await {
                        while let Some(env) = rx.recv().await {
                            let bytes = serde_json::to_vec(&env).unwrap_or_default();
                            let len = bytes.len();
                            let topic_sender = gs.node.gossip_topic_sender.read().await.clone();
                            if let Some(sender) = topic_sender {
                                if let Err(e) = sender.broadcast(bytes.into()).await {
                                    log::warn!("◈ [Gossip] broadcast failed: {}", e);
                                    continue;
                                }
                            }
                            let mut g = gs.node.gossip.write().await;
                            g.stats.messages_sent += 1;
                            g.stats.bytes_sent += len as u64;
                            log::debug!("◈ [Gossip] outgoing {} bytes id={}", len, &env.id[..env.id.len().min(12)]);
                        }
                    }
                });

                // Phase 4 — dispatcher des messages gossip entrants (iroh-gossip Stream).
                let ds = state.clone();
                tokio::spawn(async move {
                    use futures_util::StreamExt as _;
                    let Some(mut rx) = ds.node.take_gossip_topic_receiver().await else { return; };
                    while let Some(event) = rx.next().await {
                        match event {
                            Ok(iroh_gossip::api::Event::Received(msg)) => {
                                p2p::dispatcher::dispatch_incoming(&ds, &msg.content).await;
                            }
                            Ok(iroh_gossip::api::Event::NeighborUp(id)) => {
                                log::info!("◈ [Gossip] NeighborUp {}", id);
                            }
                            Ok(iroh_gossip::api::Event::NeighborDown(id)) => {
                                log::info!("◈ [Gossip] NeighborDown {}", id);
                            }
                            Ok(iroh_gossip::api::Event::Lagged) => {
                                log::warn!("◈ [Gossip] receiver lagged — messages dropped");
                            }
                            Err(e) => {
                                log::warn!("◈ [Gossip] event error: {}", e);
                            }
                        }
                    }
                });

                // Phase 3 — annonce initiale : "voici mon code pays" pour la moyenne réseau.
                // S'exécute une fois ; les peers qui reçoivent ce Hello mettent à jour
                // leur table peer_country_reports et donc leur prix énergie moyen.
                let hs = state.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    let pk = hs.crypto.lock().await.get_identity()
                        .map(|i| i.public_key_hex).unwrap_or_default();
                    if pk.is_empty() { return; }
                    let country = p2p::energy::EnergyOracle::detect_country().to_string();
                    let watts = p2p::energy::estimate_watts();
                    // Marque notre propre rapport pour que la moyenne réseau parte juste.
                    hs.node.peer_country_reports.write().await.insert(country.clone(), 1);
                    let heads = hs.node.dag.read().await.heads();
                    let msg = p2p::gossip::GossipRouter::build_hello(heads, pk.clone(), watts, country.clone());
                    let bytes = p2p::gossip::GossipRouter::payload_bytes(&msg);
                    let sig = hs.crypto.lock().await.sign(&bytes).unwrap_or_default();
                    if let Ok(env) = p2p::gossip::GossipRouter::wrap_outgoing(pk, msg, &sig) {
                        hs.node.gossip.write().await.mark_seen(&env.id);
                        let _ = hs.node.gossip_tx.send(env);
                    }
                    log::info!("◈ [Gossip] Hello broadcast (country={})", country);
                });

                // Uptime mining: mint ATN every 60s based on energy cost + halving + quality
                let ms = state.clone();
                tokio::spawn(async move {
                    const MINE_INTERVAL_SECS: u64 = 60;
                    const SEAL_EVERY_N_TICKS: u32 = 5;              // 5 minutes between blocks max
                    let mut tick: u32 = 0;

                    loop {
                        tokio::time::sleep(tokio::time::Duration::from_secs(MINE_INTERVAL_SECS)).await;
                        let pk = ms.crypto.lock().await.get_identity()
                            .map(|i| i.public_key_hex).unwrap_or_default();
                        if pk.is_empty() { continue; }

                        // V2: Read total supply for DAG metadata (no halving)
                        let total_mined = ms.node.ledger.read().await.stats().total_mined;

                        // V2: agréger les watts des pairs pour le mining proportionnel
                        let my_watts = p2p::energy::estimate_watts();
                        let peer_watts = ms.node.peer_watts.read().await;
                        let total_network_watts = peer_watts.values().sum::<f64>() + my_watts;
                        drop(peer_watts);

                        // Phase 3 — uptime_tick retourne (atn, kwh_réel) ; on propage le kwh
                        // mesuré au ledger pour ancrer la valeur ATN à l'énergie effective.
                        let (atn, kwh) = ms.node.reputation.write().await.uptime_tick(&pk, total_mined, total_network_watts);
                        ms.node.ledger.write().await.mine_tx(&pk, atn, kwh);

                        // Phase 3 — chaque mining tick devient un nœud dans le Merkle-DAG,
                        // prêt pour le consensus multi-nœuds via gossip.
                        let payload = serde_json::to_vec(&serde_json::json!({
                            "type": "mining",
                            "pk": &pk,
                            "atn": atn,
                            "kwh": kwh,
                            "model": "fixed_100h",
                        })).unwrap_or_default();
                        let parents = ms.node.dag.read().await.heads();
                        let parents = if parents.is_empty() { vec!["genesis".into()] } else { parents };
                        let dag_node = p2p::merkle_dag::DagNode::new(parents, payload, pk.clone());
                        let _ = ms.node.dag.write().await.insert(dag_node);

                        // Phase 3 — broadcast l'évènement aux pairs (drain Iroh dès qu'un peer arrive).
                        // Sérialisation de la tx minée pour permettre aux pairs de la rejouer.
                        if let Ok(tx_json) = serde_json::to_string(
                            &ms.node.ledger.read().await.recent_txs(1).first().cloned(),
                        ) {
                            let msg = p2p::gossip::GossipMessage::BroadcastTx { tx_json };
                            let bytes = p2p::gossip::GossipRouter::payload_bytes(&msg);
                            let sig = ms.crypto.lock().await.sign(&bytes).unwrap_or_default();
                            if let Ok(env) = p2p::gossip::GossipRouter::wrap_outgoing(pk.clone(), msg, &sig) {
                                ms.node.gossip.write().await.mark_seen(&env.id);
                                let _ = ms.node.gossip_tx.send(env);
                            }
                        }

                        tick = tick.wrapping_add(1);

                        // Force-seal so rewards (likes, views, creates) don't pile up forever
                        // when the user is solo (block fills < 10 tx threshold otherwise).
                        if tick.is_multiple_of(SEAL_EVERY_N_TICKS) {
                            if let Some(b) = ms.node.ledger.write().await.seal_if_pending(&pk, 0.0) {
                                log::info!("◈ [Ledger] Block #{} sealed ({} tx)", b.index, b.transactions.len());
                            }
                        }
                    }
                });
                // ── Phase 1.1: Periodic state persistence (every 30s) ────
                let ps = state.clone();
                tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
                        // Step 1: Snapshot engines (short read locks, released immediately)
                        let ledger_json = {
                            let snap = ps.node.ledger.read().await.snapshot();
                            serde_json::to_string(&snap).unwrap_or_default()
                        };
                        let rep_json = {
                            let snap = ps.node.reputation.read().await.snapshot();
                            serde_json::to_string(&snap).unwrap_or_default()
                        };
                        // Phase 4 — DAG / consensus CRDT / gossip router
                        let dag_json = {
                            let snap = ps.node.dag.read().await.snapshot();
                            serde_json::to_string(&snap).unwrap_or_default()
                        };
                        let cons_json = {
                            let snap = ps.node.consensus.read().await.snapshot();
                            serde_json::to_string(&snap).unwrap_or_default()
                        };
                        let gos_json = {
                            let snap = ps.node.gossip.read().await.snapshot();
                            serde_json::to_string(&snap).unwrap_or_default()
                        };
                        // Step 2: Write to DB (db lock only, no engine locks held)
                        let db_guard = ps.db.lock().await;
                        if let Some(db) = db_guard.as_ref() {
                            let _ = db.save_state("ledger", &ledger_json).await;
                            let _ = db.save_state("reputation", &rep_json).await;
                            let _ = db.save_state("dag", &dag_json).await;
                            let _ = db.save_state("consensus", &cons_json).await;
                            let _ = db.save_state("gossip", &gos_json).await;
                            log::debug!("◈ [Titan] State persisted to disk");
                        }
                    }
                });
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            check_identity, create_identity, unlock_identity, get_public_key, get_recovery_key,
            get_node_status,
            get_security_audit,
            get_node_ticket, connect_peer,
            get_my_reputation, transfer_atn, stake_atn, get_trust_leaderboard,
            get_energy_stats, get_consensus_stats, get_network_health,
            get_ledger_stats, get_recent_txs, get_balance, ledger_transfer,
            verify_ledger,
            get_economy_stats,
            submit_compute_task, get_pending_tasks, get_marketplace_stats,
        ])
        .run(tauri::generate_context!())
        .expect("SWE Titan v5 failed to launch");
}
