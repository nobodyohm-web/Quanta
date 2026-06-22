//! Quanta — Tauri commands for sovereign @pseudo identity.
//!
//! Patron commun :
//!   1. récupérer la pubkey identité courante
//!   2. construire le payload, signer via `CryptoEngine` (zéro accès direct au SK)
//!   3. mettre à jour le registre local
//!   4. broadcaster en gossip via `wrap_broadcast()`

use crate::p2p;
use crate::p2p::gossip::{GossipMessage, GossipRouter};
use crate::AppState;
use serde::Serialize;
use std::sync::Arc;

// ─── Helpers internes ───────────────────────────────────────────────────────

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Signe un blob via `CryptoEngine` puis renvoie l'hex de la signature (128 chars).
async fn sign_hex(state: &Arc<AppState>, msg: &[u8]) -> Result<String, String> {
    let crypto = state.crypto.lock().await;
    let sig = crypto.sign(msg)?;
    Ok(hex::encode(sig))
}

async fn my_pk(state: &Arc<AppState>) -> Result<String, String> {
    state
        .crypto
        .lock()
        .await
        .get_identity()
        .map(|i| i.public_key_hex)
}

/// Wrap + signe + envoie une enveloppe gossip (pipeline B/C/D).
async fn wrap_broadcast(state: &Arc<AppState>, msg: GossipMessage) -> Result<(), String> {
    let pk = my_pk(state).await?;
    let ts = chrono::Utc::now().to_rfc3339();
    let nonce = state.node.gossip.read().await.next_outgoing_nonce();
    let signable = GossipRouter::signable_envelope_bytes(&pk, nonce, &ts, &msg);
    let sig = state.crypto.lock().await.sign(&signable)?;
    let env = GossipRouter::build_signed_envelope(pk, msg, nonce, ts, &sig)?;
    state.node.gossip.write().await.mark_seen(&env.id);
    let _ = state.node.gossip_tx.send(env);
    Ok(())
}

// ─── Identité — pseudos uniques @handle (adresse de wallet lisible) ──────────

/// Réserve un pseudo unique pour l'identité courante.
/// Gratuit (aucun débit ledger). Signé par la clé → anti-usurpation, puis
/// diffusé en gossip. L'unicité réseau est garantie par la résolution de
/// conflit déterministe de `UsernameRegistry`.
#[tauri::command]
pub async fn claim_username(
    state: tauri::State<'_, Arc<AppState>>,
    username: String,
) -> Result<serde_json::Value, String> {
    let name = p2p::username::normalize_username(&username);
    p2p::username::validate_username(&name).map_err(|_| {
        "Pseudo invalide : 3 à 20 caractères, minuscules / chiffres / _ , doit commencer par une lettre.".to_string()
    })?;
    let pk = my_pk(&state).await?;

    // Disponibilité (vue locale) : déjà pris par quelqu'un d'autre → refus.
    if let Some(existing) = state.node.usernames.read().await.get(&name) {
        if existing.owner_pk != pk {
            return Err(format!("@{name} est déjà pris."));
        }
    }

    let mut rec = p2p::username::UsernameRecord {
        username: name.clone(),
        owner_pk: pk.clone(),
        claimed_at: now_secs(),
        signature: String::new(),
    };
    rec.signature = sign_hex(&state, &p2p::username::signable_bytes(&rec)).await?;

    state
        .node
        .usernames
        .write()
        .await
        .apply(rec.clone())
        .map_err(|e| format!("{e:?}"))?;

    let record_json = serde_json::to_string(&rec).map_err(|e| e.to_string())?;
    wrap_broadcast(&state, GossipMessage::PublishUsername { record_json }).await?;

    Ok(serde_json::json!({ "username": name, "owner_pk": pk }))
}

/// Résout `@pseudo → clé publique` (l'adresse de wallet). `None` si inconnu.
#[tauri::command]
pub async fn resolve_username(
    state: tauri::State<'_, Arc<AppState>>,
    username: String,
) -> Result<Option<String>, String> {
    Ok(state.node.usernames.read().await.resolve(&username))
}

/// Le pseudo est-il valide ET libre ?
#[tauri::command]
pub async fn is_username_available(
    state: tauri::State<'_, Arc<AppState>>,
    username: String,
) -> Result<bool, String> {
    Ok(state.node.usernames.read().await.is_available(&username))
}

/// Pseudo de l'identité courante (pour l'afficher au lieu de la clé). `None` si
/// l'utilisateur n'en a pas encore réservé.
#[tauri::command]
pub async fn get_my_username(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Option<String>, String> {
    let pk = my_pk(&state).await?;
    Ok(state.node.usernames.read().await.username_of(&pk))
}

/// Pseudo détenu par une clé publique donnée (résolution inverse pour l'UI).
#[tauri::command]
pub async fn username_of_pk(
    state: tauri::State<'_, Arc<AppState>>,
    pk: String,
) -> Result<Option<String>, String> {
    Ok(state.node.usernames.read().await.username_of(&pk))
}

/// Code de connexion de l'identité courante (à dicter à un proche pour qu'il
/// vous ajoute en toute sécurité).
#[tauri::command]
pub async fn get_my_connection_code(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<String, String> {
    let pk = my_pk(&state).await?;
    p2p::username::connection_code(&pk).ok_or_else(|| "Clé invalide".to_string())
}

#[derive(Serialize)]
pub struct VerifiedContact {
    pub username: String,
    pub pk: String,
    pub connection_code: String,
}

/// Vérifie qu'un `@pseudo` + un code de connexion désignent bien la même
/// personne (« trouver ma mère en toute sécurité »). Résout le pseudo → clé,
/// recalcule le code, compare. Renvoie la clé + le pseudo si la vérification
/// réussit ; sinon une erreur claire.
#[tauri::command]
pub async fn verify_connection(
    state: tauri::State<'_, Arc<AppState>>,
    username: String,
    code: String,
) -> Result<VerifiedContact, String> {
    let name = p2p::username::normalize_username(&username);
    let pk = state
        .node
        .usernames
        .read()
        .await
        .resolve(&name)
        .ok_or_else(|| format!("Pseudo introuvable : @{name}"))?;
    let expected = p2p::username::connection_code(&pk).ok_or("Clé invalide")?;
    if p2p::username::normalize_code(&code) != p2p::username::normalize_code(&expected) {
        return Err("Le code ne correspond pas à ce pseudo. Revérifiez auprès de la personne.".into());
    }
    Ok(VerifiedContact {
        username: name,
        pk,
        connection_code: expected,
    })
}
