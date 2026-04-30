//! Dispatcher des messages gossip entrants — Phase 4.
//!
//! Désérialise une `GossipEnvelope` venant d'iroh-gossip, vérifie la fenêtre
//! temporelle ±5 min et l'anti-replay, puis applique l'action sur l'état local
//! (DAG, CRDT consensus, ledger, peer reports).
//!
//! Réponses sortantes (WantNodes, HaveNodes, Pong) sont rebroadcastées sur le même
//! channel `gossip_tx` que les messages locaux — l'iroh-gossip drain les enverra.

use crate::p2p::gossip::{GossipEnvelope, GossipMessage, GossipRouter, ReportReason};
use crate::p2p::merkle_dag::DagNode;
use crate::AppState;
use std::sync::Arc;

/// Désérialise + vérifie + dispatche une enveloppe entrante.
pub async fn dispatch_incoming(state: &Arc<AppState>, raw: &[u8]) {
    let env: GossipEnvelope = match serde_json::from_slice(raw) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("◈ [Dispatch] envelope JSON invalide: {}", e);
            return;
        }
    };

    // Anti-replay : si on a déjà vu cet ID, ignorer.
    {
        let mut g = state.node.gossip.write().await;
        if !g.mark_seen(&env.id) {
            return;
        }
        g.stats.messages_received += 1;
        g.stats.bytes_received += raw.len() as u64;
    }

    // Fenêtre temporelle ±5 min.
    if !GossipRouter::is_fresh(&env.timestamp) {
        log::debug!("◈ [Dispatch] enveloppe trop ancienne, drop");
        return;
    }

    match env.payload {
        GossipMessage::Hello { heads, node_id, watts, country, version: _ } => {
            handle_hello(state, &env.sender, &node_id, heads, watts, &country).await;
        }
        GossipMessage::WantNodes { ids } => {
            handle_want_nodes(state, &env.sender, ids).await;
        }
        GossipMessage::HaveNodes { nodes } => {
            handle_have_nodes(state, nodes).await;
        }
        GossipMessage::BroadcastTx { tx_json } => {
            handle_broadcast_tx(state, &tx_json).await;
        }
        GossipMessage::Ping { nonce } => {
            handle_ping(state, &env.sender, nonce).await;
        }
        GossipMessage::Pong { nonce } => {
            log::debug!("◈ [Dispatch] Pong from {} nonce={}", &env.sender[..env.sender.len().min(12)], nonce);
        }
        GossipMessage::ReportPeer { peer_id, reason } => {
            handle_report_peer(state, &env.sender, &peer_id, reason).await;
        }
    }
}

/// Hello → enregistre les watts + pays du peer, demande les nœuds DAG manquants.
async fn handle_hello(
    state: &Arc<AppState>,
    sender_pk: &str,
    _node_id: &str,
    their_heads: Vec<String>,
    watts: f64,
    country: &str,
) {
    log::info!("◈ [Dispatch] Hello from {} ({} heads, {:.1}W, {})",
        &sender_pk[..sender_pk.len().min(12)], their_heads.len(), watts, country);

    // V2: enregistrer les watts du pair pour le mining proportionnel
    state.node.peer_watts.write().await.insert(sender_pk.to_string(), watts);

    // Enregistrer le pays du pair pour l'oracle énergie
    *state.node.peer_country_reports.write().await
        .entry(country.to_string()).or_insert(0) += 1;

    // Calculer les heads qu'on ne connaît pas et demander leur contenu.
    let our_known = state.node.dag.read().await.known_ids();
    let want = GossipRouter::compute_want(&their_heads, &our_known);
    if !want.is_empty() {
        let msg = GossipMessage::WantNodes { ids: want };
        broadcast(state, msg).await;
    }
}

/// WantNodes → on envoie les DagNode demandés (HaveNodes).
async fn handle_want_nodes(state: &Arc<AppState>, _sender_pk: &str, ids: Vec<String>) {
    let dag = state.node.dag.read().await;
    let mut nodes = Vec::with_capacity(ids.len());
    for id in &ids {
        if let Some(n) = dag.get(id) {
            nodes.push(n.clone());
        }
    }
    drop(dag);
    if nodes.is_empty() { return; }
    log::info!("◈ [Dispatch] HaveNodes → {} nodes", nodes.len());
    broadcast(state, GossipMessage::HaveNodes { nodes }).await;
}

/// HaveNodes → insère chaque nœud dans notre DAG local.
/// Les nœuds dont les parents manquent sont retentés au tour suivant via Hello.
async fn handle_have_nodes(state: &Arc<AppState>, nodes: Vec<DagNode>) {
    let mut dag = state.node.dag.write().await;
    let mut inserted = 0u64;
    // Trier par profondeur de parents : insère d'abord les racines, sinon les insertions
    // peuvent échouer pour orphelinage.
    let mut sorted = nodes;
    sorted.sort_by_key(|n| n.parents.len());
    for n in sorted {
        match dag.insert(n) {
            Ok(()) => inserted += 1,
            Err(e) => log::debug!("◈ [Dispatch] DAG insert skipped: {}", e),
        }
    }
    drop(dag);
    if inserted > 0 {
        let mut g = state.node.gossip.write().await;
        g.stats.nodes_synced += inserted;
        log::info!("◈ [Dispatch] DAG synced +{} nodes", inserted);
    }
}

/// BroadcastTx → parse une transaction JSON, la valide et l'ajoute au ledger local.
async fn handle_broadcast_tx(state: &Arc<AppState>, tx_json: &str) {
    let tx: Option<crate::p2p::ledger::Transaction> = serde_json::from_str(tx_json).ok().flatten();
    let Some(tx) = tx else {
        log::warn!("◈ [Dispatch] BroadcastTx JSON invalide");
        return;
    };

    // Vérifier la signature hybride / Ed25519 avant tout.
    match crate::p2p::ledger::Ledger::verify_tx(&tx) {
        Ok(true) => {}
        Ok(false) => { log::warn!("◈ [Dispatch] tx signature invalide — drop"); return; }
        Err(e) => { log::warn!("◈ [Dispatch] verify_tx erreur: {} — drop", e); return; }
    }

    // Pour l'instant on logue ; le merge complet du ledger linéaire entre nœuds passe
    // par le DAG (BroadcastTx sert surtout d'alerte temps-réel). Le CRDT est mis à jour
    // si la tx est un transfer ou un like/view.
    use crate::p2p::ledger::TxType;
    if tx.tx_type == TxType::Transfer {
        let milli = (tx.amount * 1000.0) as u64;
        let mut cons = state.node.consensus.write().await;
        cons.ledger.debit(&tx.from, &tx.from, milli);
        cons.ledger.credit(&tx.from, &tx.to, milli);
    }
    log::debug!("◈ [Dispatch] tx {} ({:?}) appliquée au CRDT", &tx.id, tx.tx_type);
}

/// Ping → répondre Pong.
async fn handle_ping(state: &Arc<AppState>, _sender_pk: &str, nonce: u64) {
    broadcast(state, GossipMessage::Pong { nonce }).await;
}

/// ReportPeer → log + incrémenter compteur. Pas d'action automatique pour l'instant
/// (anti-coordinated abuse : un seul peer peut signaler n'importe qui, pas de bannissement).
async fn handle_report_peer(state: &Arc<AppState>, sender_pk: &str, peer_id: &str, reason: ReportReason) {
    log::info!("◈ [Dispatch] ReportPeer from {} → {} ({:?})",
        &sender_pk[..sender_pk.len().min(12)],
        &peer_id[..peer_id.len().min(12)], reason);
    state.node.gossip.write().await.stats.peers_reported += 1;
}

/// Helper : signe + emballe + push sur le channel gossip_tx (le drain enverra via iroh-gossip).
async fn broadcast(state: &Arc<AppState>, msg: GossipMessage) {
    let pk = state.crypto.lock().await.get_identity()
        .map(|i| i.public_key_hex).unwrap_or_default();
    if pk.is_empty() { return; }
    let bytes = GossipRouter::payload_bytes(&msg);
    let sig = state.crypto.lock().await.sign(&bytes).unwrap_or_default();
    let env = match GossipRouter::wrap_outgoing(pk, msg, &sig) {
        Ok(e) => e,
        Err(e) => { log::warn!("◈ [Dispatch] wrap_outgoing failed: {}", e); return; }
    };
    state.node.gossip.write().await.mark_seen(&env.id);
    let _ = state.node.gossip_tx.send(env);
}
