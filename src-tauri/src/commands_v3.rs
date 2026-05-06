//! Torus V3.2 — Tauri commands for the Social Web stack.
//!
//! Domaines / Recherche / Social / Modération / Forums / Web of Trust.
//!
//! Patron commun :
//!   1. récupérer la pubkey identité courante
//!   2. construire le payload V3, signer via `CryptoEngine` (zéro accès direct au SK)
//!   3. mettre à jour l'état local (registre / index / engine)
//!   4. broadcaster en gossip via `wrap_broadcast()`
//!
//! Toutes les conversions QUANTA (f64) ↔ µQTA (u64) passent par `to_uqta()`
//! pour rejeter NaN, négatifs et overflows à la frontière.

use crate::p2p;
use crate::p2p::gossip::{GossipMessage, GossipRouter};
use crate::AppState;
use serde::Serialize;
use std::sync::Arc;

// ─── Helpers internes ───────────────────────────────────────────────────────

fn to_uqta(amount: f64) -> Result<u64, String> {
    if !amount.is_finite() || amount < 0.0 {
        return Err("Montant invalide".into());
    }
    let v = amount * p2p::ledger::MICRO as f64;
    if v >= u64::MAX as f64 {
        return Err("Montant trop grand".into());
    }
    Ok(v.round() as u64)
}

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

// ─── DOMAINES ───────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ClaimedDomain {
    pub name: String,
    pub owner_pk: String,
    pub target_pk: String,
    pub value_qta: f64,
    pub last_paid_ts: u64,
}

/// Réserve un domaine `name.torus`. Coût : INITIAL_CLAIM (1 QTA) + value Harberger.
/// `value_qta` est la valeur déclarée (utilisée pour calculer le loyer mensuel à 1%).
#[tauri::command]
pub async fn claim_domain(
    state: tauri::State<'_, Arc<AppState>>,
    name: String,
    target_pk: Option<String>,
    value_qta: f64,
) -> Result<ClaimedDomain, String> {
    p2p::domains::validate_name(&name).map_err(|e| format!("{e:?}"))?;
    let value = to_uqta(value_qta)?;
    let pk = my_pk(&state).await?;
    let target = target_pk.unwrap_or_else(|| pk.clone());
    if target.len() != 64 || hex::decode(&target).is_err() {
        return Err("target_pk invalide".into());
    }

    let now = now_secs();
    let mut rec = p2p::domains::DomainRecord {
        name: name.clone(),
        owner_pk: pk.clone(),
        target_pk: target.clone(),
        value_micro_qta: value,
        last_paid_ts: now,
        updated_at: now,
        version: 1,
        signature: String::new(),
    };
    let signable = p2p::domains::signable_bytes(&rec);
    rec.signature = sign_hex(&state, &signable).await?;

    // Débit sur le ledger : INITIAL_CLAIM brûlé (paiement au "réseau", pas à un bénéficiaire).
    {
        let mut ledger = state.node.ledger.write().await;
        let crypto = state.crypto.lock().await;
        let _ = ledger.burn_tx(&pk, p2p::domains::INITIAL_CLAIM_MICRO_QTA, &crypto);
    }

    {
        let mut reg = state.node.domains.write().await;
        reg.claim(rec.clone(), p2p::domains::INITIAL_CLAIM_MICRO_QTA)
            .map_err(|e| format!("{e:?}"))?;
    }

    let record_json = serde_json::to_string(&rec).map_err(|e| e.to_string())?;
    wrap_broadcast(&state, GossipMessage::PublishDomain { record_json }).await?;

    Ok(ClaimedDomain {
        name,
        owner_pk: pk,
        target_pk: target,
        value_qta,
        last_paid_ts: now,
    })
}

/// Paie le loyer Harberger dû sur un domaine. Renvoie le montant payé (QTA).
#[tauri::command]
pub async fn pay_domain_rent(
    state: tauri::State<'_, Arc<AppState>>,
    name: String,
) -> Result<f64, String> {
    let pk = my_pk(&state).await?;
    let now = now_secs();

    // Récupère le record + calcule le dû.
    let (existing_rec, due) = {
        let reg = state.node.domains.read().await;
        let rec = reg
            .get(&name)
            .cloned()
            .ok_or_else(|| "Domaine inconnu".to_string())?;
        if rec.owner_pk != pk {
            return Err("Vous n'êtes pas propriétaire".into());
        }
        let due = p2p::domains::rent_due(rec.value_micro_qta, rec.last_paid_ts, now);
        (rec, due)
    };
    if due == 0 {
        return Ok(0.0);
    }

    // Construit le nouveau record signé via CryptoEngine.
    let mut new_rec = p2p::domains::DomainRecord {
        last_paid_ts: now,
        updated_at: now,
        version: existing_rec.version + 1,
        signature: String::new(),
        ..existing_rec
    };
    let signable = p2p::domains::signable_bytes(&new_rec);
    new_rec.signature = sign_hex(&state, &signable).await?;

    // Débit ledger (loyer brûlé : pas de bénéficiaire — soutient la rareté).
    {
        let mut ledger = state.node.ledger.write().await;
        let crypto = state.crypto.lock().await;
        ledger.burn_tx(&pk, due, &crypto)?;
    }

    // Update local + gossip.
    {
        let mut reg = state.node.domains.write().await;
        reg.update(new_rec.clone()).map_err(|e| format!("{e:?}"))?;
    }
    let record_json = serde_json::to_string(&new_rec).map_err(|e| e.to_string())?;
    wrap_broadcast(&state, GossipMessage::PublishDomain { record_json }).await?;

    Ok(due as f64 / p2p::ledger::MICRO as f64)
}

/// Rachat Harberger : paye `value` au propriétaire actuel et redéclare une nouvelle valeur.
#[tauri::command]
pub async fn overbid_domain(
    state: tauri::State<'_, Arc<AppState>>,
    name: String,
    new_target_pk: Option<String>,
    new_value_qta: f64,
) -> Result<f64, String> {
    let pk = my_pk(&state).await?;
    let new_value = to_uqta(new_value_qta)?;
    let now = now_secs();

    let (current_owner, payment) = {
        let reg = state.node.domains.read().await;
        let rec = reg
            .get(&name)
            .cloned()
            .ok_or_else(|| "Domaine inconnu".to_string())?;
        (rec.owner_pk.clone(), rec.value_micro_qta)
    };

    let target = new_target_pk.unwrap_or_else(|| pk.clone());
    if target.len() != 64 || hex::decode(&target).is_err() {
        return Err("new_target_pk invalide".into());
    }

    // Construit + signe le nouveau record (challenger).
    let mut new_rec = p2p::domains::DomainRecord {
        name: name.clone(),
        owner_pk: pk.clone(),
        target_pk: target,
        value_micro_qta: new_value,
        last_paid_ts: now,
        updated_at: now,
        version: 0, // overwritten below
        signature: String::new(),
    };
    {
        let reg = state.node.domains.read().await;
        let existing = reg.get(&name).ok_or_else(|| "Domaine inconnu".to_string())?;
        new_rec.version = existing.version + 1;
    }
    let signable = p2p::domains::signable_bytes(&new_rec);
    new_rec.signature = sign_hex(&state, &signable).await?;

    // Paiement : challenger → propriétaire actuel (transfer 1% burn auto via ledger).
    {
        let mut ledger = state.node.ledger.write().await;
        let crypto = state.crypto.lock().await;
        ledger.transfer_with_burn(&pk, &current_owner, payment, &crypto)?;
    }

    // V3.3 — Insertion via la méthode dédiée `apply_overbid_record` qui accepte
    // un changement de owner_pk si la signature du challenger est valide.
    {
        let mut reg = state.node.domains.write().await;
        reg.apply_overbid_record(new_rec.clone())
            .map_err(|e| format!("{e:?}"))?;
    }

    let record_json = serde_json::to_string(&new_rec).map_err(|e| e.to_string())?;
    wrap_broadcast(&state, GossipMessage::PublishDomain { record_json }).await?;

    Ok(payment as f64 / p2p::ledger::MICRO as f64)
}

#[derive(Serialize)]
pub struct DomainResolution {
    pub name: String,
    pub target_pk: Option<String>,
    pub state: String, // "current" | "grace" | "expired" | "unknown"
    pub due_qta: f64,
}

#[tauri::command]
pub async fn resolve_domain(
    state: tauri::State<'_, Arc<AppState>>,
    name: String,
) -> Result<DomainResolution, String> {
    let now = now_secs();
    let reg = state.node.domains.read().await;
    let target_pk = reg.resolve(&name, now);
    let (state_str, due_qta) = if let Some(rec) = reg.get(&name) {
        match p2p::domains::rent_state(rec, now) {
            p2p::domains::RentState::Current => ("current".to_string(), 0.0),
            p2p::domains::RentState::Grace { due_micro_qta, .. } => (
                "grace".to_string(),
                due_micro_qta as f64 / p2p::ledger::MICRO as f64,
            ),
            p2p::domains::RentState::Expired { due_micro_qta } => (
                "expired".to_string(),
                due_micro_qta as f64 / p2p::ledger::MICRO as f64,
            ),
        }
    } else {
        ("unknown".to_string(), 0.0)
    };
    Ok(DomainResolution {
        name,
        target_pk,
        state: state_str,
        due_qta,
    })
}

#[tauri::command]
pub async fn list_my_domains(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<serde_json::Value, String> {
    let pk = my_pk(&state).await?;
    let now = now_secs();
    let reg = state.node.domains.read().await;
    let domains: Vec<_> = reg
        .list()
        .filter(|r| r.owner_pk == pk)
        .map(|r| {
            let due = p2p::domains::rent_due(r.value_micro_qta, r.last_paid_ts, now);
            serde_json::json!({
                "name": r.name,
                "target_pk": r.target_pk,
                "value_qta": r.value_micro_qta as f64 / p2p::ledger::MICRO as f64,
                "due_qta": due as f64 / p2p::ledger::MICRO as f64,
                "last_paid_ts": r.last_paid_ts,
                "version": r.version,
            })
        })
        .collect();
    Ok(serde_json::Value::Array(domains))
}

/// V3.3 — Délègue un sous-domaine `child.parent.torus`. Le caller doit posséder le parent.
/// `target_pk` = wallet vers lequel pointe le sous-domaine (par défaut = soi-même).
#[tauri::command]
pub async fn grant_subdomain(
    state: tauri::State<'_, Arc<AppState>>,
    name: String,
    target_pk: Option<String>,
) -> Result<serde_json::Value, String> {
    // Validation forme : `child.parent.torus`
    let (_child, parent) =
        p2p::domains::validate_subdomain(&name).map_err(|e| format!("{e:?}"))?;

    let pk = my_pk(&state).await?;
    let target = target_pk.unwrap_or_else(|| pk.clone());
    if target.len() != 64 || hex::decode(&target).is_err() {
        return Err("target_pk invalide".into());
    }

    // Vérifie ownership du parent + version next.
    let next_version = {
        let reg = state.node.domains.read().await;
        let parent_rec = reg
            .get(&parent)
            .ok_or_else(|| format!("Parent {parent} inconnu"))?;
        if parent_rec.owner_pk != pk {
            return Err(format!("Vous n'êtes pas propriétaire de {parent}"));
        }
        reg.get_subdomain(&name)
            .map(|g| g.version + 1)
            .unwrap_or(1)
    };

    let now = now_secs();
    let mut g = p2p::domains::SubdomainGrant {
        name: name.clone(),
        parent: parent.clone(),
        target_pk: target.clone(),
        created_at: now,
        version: next_version,
        signature: String::new(),
    };
    let signable = p2p::domains::signable_bytes_subdomain(&g);
    g.signature = sign_hex(&state, &signable).await?;

    {
        let mut reg = state.node.domains.write().await;
        reg.grant_subdomain(g.clone()).map_err(|e| format!("{e:?}"))?;
    }

    let grant_json = serde_json::to_string(&g).map_err(|e| e.to_string())?;
    wrap_broadcast(&state, GossipMessage::PublishSubdomain { grant_json }).await?;

    Ok(serde_json::json!({
        "name": g.name,
        "parent": g.parent,
        "target_pk": g.target_pk,
        "version": g.version,
    }))
}

// ─── RECHERCHE ──────────────────────────────────────────────────────────────

/// Indexe une page dans le moteur local + broadcast pour partage shard.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn index_my_page(
    state: tauri::State<'_, Arc<AppState>>,
    cid: String,
    title: String,
    snippet: String,
    body: String,
    lang: String,
    kind: String,
    torus_domain: Option<String>,
) -> Result<u32, String> {
    let pk = my_pk(&state).await?;
    let kind_enum = match kind.as_str() {
        "site" => p2p::search::DocKind::Site,
        "blog" => p2p::search::DocKind::Blog,
        "forum" => p2p::search::DocKind::Forum,
        "comment" => p2p::search::DocKind::Comment,
        "shop" => p2p::search::DocKind::Shop,
        _ => return Err("kind invalide".into()),
    };
    let tokens = p2p::search::tokenize(&format!("{title} {snippet} {body}"));
    let tf = p2p::search::term_freq(&tokens);
    let tf_count = tf.len() as u32;
    let doc = p2p::search::IndexedDoc {
        cid: cid.clone(),
        title,
        snippet,
        author_pk: pk,
        kind: kind_enum,
        lang,
        updated_at: now_secs(),
        term_freq: tf,
        torus_domain,
    };
    state.node.search.write().await.upsert(doc.clone());
    let doc_json = serde_json::to_string(&doc).map_err(|e| e.to_string())?;
    wrap_broadcast(&state, GossipMessage::PublishSite { doc_json }).await?;
    Ok(tf_count)
}

#[tauri::command]
pub async fn search_pages(
    state: tauri::State<'_, Arc<AppState>>,
    query: String,
    lang: Option<String>,
    kind: Option<String>,
    since_ts: Option<u64>,
    creator_pk: Option<String>,
    limit: Option<usize>,
) -> Result<serde_json::Value, String> {
    let kind_enum = kind.as_deref().and_then(|k| match k {
        "site" => Some(p2p::search::DocKind::Site),
        "blog" => Some(p2p::search::DocKind::Blog),
        "forum" => Some(p2p::search::DocKind::Forum),
        "comment" => Some(p2p::search::DocKind::Comment),
        "shop" => Some(p2p::search::DocKind::Shop),
        _ => None,
    });
    let filters = p2p::search::SearchFilters {
        lang,
        since_ts,
        kind: kind_enum,
        creator_pk,
        min_likes: None,
    };
    let now = now_secs();
    let limit = limit.unwrap_or(20).min(100);

    // Snapshot léger de l'état social pour le scoring.
    let social = state.node.social.read().await.snapshot();
    let rep_snapshot: std::collections::HashMap<String, f64> = state
        .node
        .reputation
        .read()
        .await
        .get_leaderboard(2000)
        .into_iter()
        .map(|u| (u.public_key, u.trust_score))
        .collect();

    let index = state.node.search.read().await;
    let hits = index.search(&query, &filters, now, limit, |cid| {
        let stats = social.pages.get(cid);
        let weighted_likes = stats.map(|s| s.weighted_likes).unwrap_or(0.0);
        let (followers, rep) = if let Some(doc) = index_doc_author(&index, cid) {
            let f = social
                .creators
                .get(&doc)
                .map(|c| c.follower_count)
                .unwrap_or(0);
            let r = rep_snapshot.get(&doc).copied().unwrap_or(0.5);
            (f, r)
        } else {
            (0, 0.5)
        };
        p2p::search::SocialSignals {
            weighted_likes,
            follower_count: followers,
            creator_reputation: rep,
            moderation_malus: 0.0,
        }
    });

    Ok(serde_json::to_value(hits).unwrap_or(serde_json::Value::Null))
}

/// V3.3 — Retrouve l'auteur d'un doc indexé via le getter `doc_by_cid` exposé
/// par `SearchIndex`. Le scoring de `search_pages` peut désormais utiliser les
/// vrais signaux follower/reputation au lieu des valeurs par défaut.
fn index_doc_author(index: &p2p::search::SearchIndex, cid: &str) -> Option<String> {
    index.doc_by_cid(cid).map(|d| d.author_pk.clone())
}

#[tauri::command]
pub async fn search_stats(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<serde_json::Value, String> {
    let n = state.node.search.read().await.doc_count();
    Ok(serde_json::json!({ "doc_count": n }))
}

// ─── SOCIAL ─────────────────────────────────────────────────────────────────

/// Like (`weight=+1`) ou Dislike (`weight=-1`) avec montant quadratique.
#[tauri::command]
pub async fn social_vote(
    state: tauri::State<'_, Arc<AppState>>,
    target_cid: String,
    target_author_pk: String,
    amount_qta: f64,
    weight: i8,
) -> Result<(), String> {
    if weight != 1 && weight != -1 {
        return Err("weight doit être ±1".into());
    }
    let amount = to_uqta(amount_qta)?;
    if amount < p2p::social::LIKE_BASE_COST_MICRO_QTA {
        return Err(format!(
            "Min {} µQTA",
            p2p::social::LIKE_BASE_COST_MICRO_QTA
        ));
    }
    let pk = my_pk(&state).await?;
    let action = p2p::social::SocialAction::Vote {
        target_cid,
        target_author_pk: target_author_pk.clone(),
        amount_micro_qta: amount,
        weight,
    };
    let mut signed = p2p::social::SignedAction {
        action,
        author_pk: pk.clone(),
        timestamp: now_secs(),
        nonce: now_secs(),
        signature: String::new(),
    };
    let signable = p2p::social::signable_bytes(&signed);
    signed.signature = sign_hex(&state, &signable).await?;
    // Débit ledger : tip à l'auteur (1% burn auto), influence quadratique côté state.
    {
        let mut ledger = state.node.ledger.write().await;
        let crypto = state.crypto.lock().await;
        let _ = ledger.transfer_with_burn(&pk, &target_author_pk, amount, &crypto);
    }
    state
        .node
        .social
        .write()
        .await
        .apply(&signed, now_secs())
        .map_err(|e| format!("{e:?}"))?;
    let action_json = serde_json::to_string(&signed).map_err(|e| e.to_string())?;
    wrap_broadcast(&state, GossipMessage::BroadcastSocialAction { action_json }).await?;
    Ok(())
}

/// Suivre/dé-suivre un créateur. `tier`: "signal" | "supporter" | "patron".
#[tauri::command]
pub async fn social_follow(
    state: tauri::State<'_, Arc<AppState>>,
    followee_pk: String,
    tier: String,
    active: bool,
) -> Result<(), String> {
    let tier_enum = match tier.as_str() {
        "signal" => p2p::social::FollowTier::Signal,
        "supporter" => p2p::social::FollowTier::Supporter,
        "patron" => p2p::social::FollowTier::Patron,
        _ => return Err("tier invalide".into()),
    };
    let pk = my_pk(&state).await?;
    let action = p2p::social::SocialAction::Follow {
        followee_pk: followee_pk.clone(),
        tier: tier_enum,
        active,
    };
    let mut signed = p2p::social::SignedAction {
        action,
        author_pk: pk.clone(),
        timestamp: now_secs(),
        nonce: now_secs(),
        signature: String::new(),
    };
    let signable = p2p::social::signable_bytes(&signed);
    signed.signature = sign_hex(&state, &signable).await?;
    state
        .node
        .social
        .write()
        .await
        .apply(&signed, now_secs())
        .map_err(|e| format!("{e:?}"))?;
    // Web of Trust : maj du graphe local.
    {
        let mut g = state.node.follow_graph.write().await;
        let entry = g.entry(pk.clone()).or_default();
        if active {
            if !entry.contains(&followee_pk) {
                entry.push(followee_pk);
            }
        } else {
            entry.retain(|p| p != &followee_pk);
        }
    }
    let action_json = serde_json::to_string(&signed).map_err(|e| e.to_string())?;
    wrap_broadcast(&state, GossipMessage::BroadcastSocialAction { action_json }).await?;
    Ok(())
}

#[tauri::command]
pub async fn social_tip(
    state: tauri::State<'_, Arc<AppState>>,
    target_cid: String,
    target_author_pk: String,
    amount_qta: f64,
    memo: Option<String>,
) -> Result<f64, String> {
    let amount = to_uqta(amount_qta)?;
    if amount == 0 {
        return Err("Montant requis".into());
    }
    let pk = my_pk(&state).await?;
    let (_tx, burn) = {
        let mut ledger = state.node.ledger.write().await;
        let crypto = state.crypto.lock().await;
        ledger.transfer_with_burn(&pk, &target_author_pk, amount, &crypto)?
    };
    let action = p2p::social::SocialAction::Tip {
        target_cid,
        target_author_pk,
        amount_micro_qta: amount,
        memo: memo.unwrap_or_default(),
    };
    let mut signed = p2p::social::SignedAction {
        action,
        author_pk: pk,
        timestamp: now_secs(),
        nonce: now_secs(),
        signature: String::new(),
    };
    let signable = p2p::social::signable_bytes(&signed);
    signed.signature = sign_hex(&state, &signable).await?;
    let _ = state.node.social.write().await.apply(&signed, now_secs());
    let action_json = serde_json::to_string(&signed).map_err(|e| e.to_string())?;
    wrap_broadcast(&state, GossipMessage::BroadcastSocialAction { action_json }).await?;
    Ok(burn as f64 / p2p::ledger::MICRO as f64)
}

#[tauri::command]
pub async fn social_boost(
    state: tauri::State<'_, Arc<AppState>>,
    target_cid: String,
    target_author_pk: String,
    amount_qta: f64,
) -> Result<f64, String> {
    let amount = to_uqta(amount_qta)?;
    let burn = p2p::social::boost_burn_share(amount);
    let pk = my_pk(&state).await?;
    // Brûle 5% + transfère le reste à l'auteur.
    {
        let mut ledger = state.node.ledger.write().await;
        let crypto = state.crypto.lock().await;
        let _ = ledger.burn_tx(&pk, burn, &crypto);
        ledger.transfer_with_burn(&pk, &target_author_pk, amount - burn, &crypto)?;
    }
    let action = p2p::social::SocialAction::Boost {
        target_cid,
        target_author_pk,
        amount_micro_qta: amount,
    };
    let mut signed = p2p::social::SignedAction {
        action,
        author_pk: pk,
        timestamp: now_secs(),
        nonce: now_secs(),
        signature: String::new(),
    };
    let signable = p2p::social::signable_bytes(&signed);
    signed.signature = sign_hex(&state, &signable).await?;
    state
        .node
        .social
        .write()
        .await
        .apply(&signed, now_secs())
        .map_err(|e| format!("{e:?}"))?;
    let action_json = serde_json::to_string(&signed).map_err(|e| e.to_string())?;
    wrap_broadcast(&state, GossipMessage::BroadcastSocialAction { action_json }).await?;
    Ok(burn as f64 / p2p::ledger::MICRO as f64)
}

#[tauri::command]
pub async fn get_page_social_stats(
    state: tauri::State<'_, Arc<AppState>>,
    cid: String,
) -> Result<serde_json::Value, String> {
    let s = state.node.social.read().await;
    Ok(serde_json::to_value(s.page_stats(&cid)).unwrap_or(serde_json::Value::Null))
}

#[tauri::command]
pub async fn get_creator_social_stats(
    state: tauri::State<'_, Arc<AppState>>,
    pk: String,
) -> Result<serde_json::Value, String> {
    let s = state.node.social.read().await;
    Ok(serde_json::to_value(s.creator_stats(&pk)).unwrap_or(serde_json::Value::Null))
}

// ─── MODÉRATION ─────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn submit_moderation_report(
    state: tauri::State<'_, Arc<AppState>>,
    target_cid: String,
    target_author_pk: String,
    category: String,
    evidence_cid: Option<String>,
) -> Result<Option<String>, String> {
    let cat = match category.as_str() {
        "spam" => p2p::moderation::ReportCategory::Spam,
        "scam" => p2p::moderation::ReportCategory::Scam,
        "illegal" => p2p::moderation::ReportCategory::IllegalContent,
        "harassment" => p2p::moderation::ReportCategory::Harassment,
        "impersonation" => p2p::moderation::ReportCategory::Impersonation,
        _ => p2p::moderation::ReportCategory::Other,
    };
    let pk = my_pk(&state).await?;
    let now = now_secs();
    let mut report = p2p::moderation::Report {
        target_cid,
        target_author_pk,
        category: cat,
        evidence_cid,
        reporter_pk: pk.clone(),
        timestamp: now,
        nonce: now,
        signature: String::new(),
    };
    let signable = p2p::moderation::signable_report(&report);
    report.signature = sign_hex(&state, &signable).await?;

    // Débit anti-spam (0.1 QTA brûlé).
    {
        let mut ledger = state.node.ledger.write().await;
        let crypto = state.crypto.lock().await;
        let _ = ledger.burn_tx(&pk, p2p::moderation::REPORT_COST_MICRO_QTA, &crypto);
    }

    let pool: Vec<String> = state
        .node
        .reputation
        .read()
        .await
        .get_leaderboard(200)
        .iter()
        .map(|u| u.public_key.clone())
        .collect();
    let seed = state
        .node
        .dag
        .read()
        .await
        .heads()
        .into_iter()
        .next()
        .unwrap_or_else(|| report.target_cid.clone());

    let opened = state
        .node
        .moderation
        .write()
        .await
        .submit_report(report.clone(), || pool, &seed, now)
        .map_err(|e| format!("{e:?}"))?;

    let report_json = serde_json::to_string(&report).map_err(|e| e.to_string())?;
    wrap_broadcast(&state, GossipMessage::BroadcastReport { report_json }).await?;

    Ok(opened)
}

#[tauri::command]
pub async fn juror_commit(
    state: tauri::State<'_, Arc<AppState>>,
    case_id: String,
    verdict: String,
    reveal_nonce_hex: String,
) -> Result<(), String> {
    let v = parse_verdict(&verdict)?;
    let pk = my_pk(&state).await?;
    let now = now_secs();
    let mut commit = p2p::moderation::CommitVote {
        case_id,
        juror_pk: pk,
        commit_hash: p2p::moderation::build_commit_hash(v, &reveal_nonce_hex),
        timestamp: now,
        signature: String::new(),
    };
    let signable = p2p::moderation::signable_commit(&commit);
    commit.signature = sign_hex(&state, &signable).await?;
    state
        .node
        .moderation
        .write()
        .await
        .submit_commit(commit.clone(), now)
        .map_err(|e| format!("{e:?}"))?;
    let commit_json = serde_json::to_string(&commit).map_err(|e| e.to_string())?;
    wrap_broadcast(&state, GossipMessage::BroadcastJurorCommit { commit_json }).await?;
    Ok(())
}

#[tauri::command]
pub async fn juror_reveal(
    state: tauri::State<'_, Arc<AppState>>,
    case_id: String,
    verdict: String,
    reveal_nonce_hex: String,
) -> Result<(), String> {
    let v = parse_verdict(&verdict)?;
    let pk = my_pk(&state).await?;
    let now = now_secs();
    let mut reveal = p2p::moderation::RevealVote {
        case_id,
        juror_pk: pk,
        verdict: v,
        reveal_nonce: reveal_nonce_hex,
        timestamp: now,
        signature: String::new(),
    };
    let signable = p2p::moderation::signable_reveal(&reveal);
    reveal.signature = sign_hex(&state, &signable).await?;
    state
        .node
        .moderation
        .write()
        .await
        .submit_reveal(reveal.clone(), now)
        .map_err(|e| format!("{e:?}"))?;
    let reveal_json = serde_json::to_string(&reveal).map_err(|e| e.to_string())?;
    wrap_broadcast(&state, GossipMessage::BroadcastJurorReveal { reveal_json }).await?;
    Ok(())
}

#[tauri::command]
pub async fn finalize_case(
    state: tauri::State<'_, Arc<AppState>>,
    case_id: String,
) -> Result<String, String> {
    let now = now_secs();
    let v = state
        .node
        .moderation
        .write()
        .await
        .finalize(&case_id, now)
        .map_err(|e| format!("{e:?}"))?;
    Ok(format!("{:?}", v))
}

#[tauri::command]
pub async fn get_open_cases(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<serde_json::Value, String> {
    let m = state.node.moderation.read().await;
    let cases: Vec<_> = m
        .open_cases()
        .map(|c| {
            serde_json::json!({
                "id": c.id,
                "target_cid": c.target_cid,
                "target_author_pk": c.target_author_pk,
                "phase": format!("{:?}", c.phase),
                "jurors": c.jurors,
                "commit_deadline": c.commit_deadline,
                "reveal_deadline": c.reveal_deadline,
            })
        })
        .collect();
    Ok(serde_json::Value::Array(cases))
}

fn parse_verdict(s: &str) -> Result<p2p::moderation::Verdict, String> {
    match s {
        "innocent" => Ok(p2p::moderation::Verdict::Innocent),
        "warning" => Ok(p2p::moderation::Verdict::Warning),
        "hide" => Ok(p2p::moderation::Verdict::Hide),
        "ban" => Ok(p2p::moderation::Verdict::Ban),
        _ => Err("verdict invalide".into()),
    }
}

// ─── FORUMS ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn forum_create(
    state: tauri::State<'_, Arc<AppState>>,
    name: String,
    description: String,
) -> Result<String, String> {
    // build_forum signe via ed25519_dalek::SigningKey ; on reconstruit depuis le secret.
    let sk = signing_key_from_state(&state).await?;
    let now = now_secs();
    let f = p2p::forums::build_forum(&sk, &name, &description, now)
        .map_err(|e| format!("{e:?}"))?;
    let id = f.id.clone();
    state
        .node
        .forums
        .write()
        .await
        .add_forum(f.clone())
        .map_err(|e| format!("{e:?}"))?;
    let node_json = serde_json::to_string(&f).map_err(|e| e.to_string())?;
    wrap_broadcast(
        &state,
        GossipMessage::PublishForumNode {
            kind: "forum".into(),
            node_json,
        },
    )
    .await?;
    Ok(id)
}

#[tauri::command]
pub async fn thread_create(
    state: tauri::State<'_, Arc<AppState>>,
    forum_id: String,
    title: String,
    body: String,
    forked_from: Option<String>,
) -> Result<String, String> {
    let sk = signing_key_from_state(&state).await?;
    let t = p2p::forums::build_thread(
        &sk,
        &forum_id,
        &title,
        &body,
        false,
        forked_from,
        now_secs(),
    )
    .map_err(|e| format!("{e:?}"))?;
    let id = t.id.clone();
    state
        .node
        .forums
        .write()
        .await
        .add_thread(t.clone())
        .map_err(|e| format!("{e:?}"))?;
    let node_json = serde_json::to_string(&t).map_err(|e| e.to_string())?;
    wrap_broadcast(
        &state,
        GossipMessage::PublishForumNode {
            kind: "thread".into(),
            node_json,
        },
    )
    .await?;
    Ok(id)
}

#[tauri::command]
pub async fn comment_create(
    state: tauri::State<'_, Arc<AppState>>,
    thread_id: String,
    body: String,
    parent_comment_id: Option<String>,
) -> Result<String, String> {
    let sk = signing_key_from_state(&state).await?;
    let c = p2p::forums::build_comment(&sk, &thread_id, parent_comment_id, &body, now_secs())
        .map_err(|e| format!("{e:?}"))?;
    let id = c.id.clone();
    state
        .node
        .forums
        .write()
        .await
        .add_comment(c.clone())
        .map_err(|e| format!("{e:?}"))?;
    let node_json = serde_json::to_string(&c).map_err(|e| e.to_string())?;
    wrap_broadcast(
        &state,
        GossipMessage::PublishForumNode {
            kind: "comment".into(),
            node_json,
        },
    )
    .await?;
    Ok(id)
}

#[tauri::command]
pub async fn list_forums(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<serde_json::Value, String> {
    let e = state.node.forums.read().await;
    let arr: Vec<_> = e
        .forums()
        .map(|f| {
            serde_json::json!({
                "id": f.id,
                "name": f.name,
                "description": f.description,
                "creator_pk": f.creator_pk,
                "created_at": f.created_at,
            })
        })
        .collect();
    Ok(serde_json::Value::Array(arr))
}

#[tauri::command]
pub async fn list_threads(
    state: tauri::State<'_, Arc<AppState>>,
    forum_id: String,
) -> Result<serde_json::Value, String> {
    let e = state.node.forums.read().await;
    let arr: Vec<_> = e
        .threads_in(&forum_id)
        .into_iter()
        .map(|t| {
            serde_json::json!({
                "id": t.id,
                "title": t.title,
                "author_pk": t.author_pk,
                "created_at": t.created_at,
                "forked_from": t.forked_from,
            })
        })
        .collect();
    Ok(serde_json::Value::Array(arr))
}

#[tauri::command]
pub async fn list_comments(
    state: tauri::State<'_, Arc<AppState>>,
    thread_id: String,
) -> Result<serde_json::Value, String> {
    let e = state.node.forums.read().await;
    let arr: Vec<_> = e
        .comments_of(&thread_id)
        .into_iter()
        .map(|c| {
            serde_json::json!({
                "id": c.id,
                "thread_id": c.thread_id,
                "parent_comment_id": c.parent_comment_id,
                "body": c.body,
                "author_pk": c.author_pk,
                "created_at": c.created_at,
            })
        })
        .collect();
    Ok(serde_json::Value::Array(arr))
}

// ─── SITES MULTI-PAGES (V3.3) ───────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct SitePageInput {
    pub path: String,
    pub title: String,
    pub html: String,
}

#[derive(serde::Deserialize)]
pub struct SiteAssetInput {
    pub path: String,
    pub mime: String,
    /// base64 du contenu (laisser vide si `dag_cid` rempli)
    #[serde(default)]
    pub content_b64: String,
    #[serde(default)]
    pub dag_cid: Option<String>,
    pub size: u64,
}

/// V3.3 — Publie (ou met à jour) un site multi-page de l'utilisateur courant.
/// Le manifest est signé via CryptoEngine (zéro accès direct à la clé privée).
#[tauri::command]
pub async fn publish_site(
    state: tauri::State<'_, Arc<AppState>>,
    root_path: Option<String>,
    pages: Vec<SitePageInput>,
    assets: Vec<SiteAssetInput>,
) -> Result<serde_json::Value, String> {
    use crate::p2p::page_store::{SiteAsset, SiteManifest, SitePage};

    let pk = my_pk(&state).await?;
    let now = now_secs();

    // Version : prochaine après l'existante.
    let next_version = {
        let store = state.node.page_store.read().await;
        store.get_site(&pk).map(|m| m.version + 1).unwrap_or(1)
    };

    let mut manifest = SiteManifest {
        author_pk: pk.clone(),
        root_path: root_path.unwrap_or_else(|| "/".into()),
        pages: pages
            .into_iter()
            .map(|p| SitePage {
                path: p.path,
                title: p.title,
                html: p.html,
            })
            .collect(),
        assets: assets
            .into_iter()
            .map(|a| SiteAsset {
                path: a.path,
                mime: a.mime,
                content_b64: a.content_b64,
                dag_cid: a.dag_cid,
                size: a.size,
            })
            .collect(),
        updated_at: now,
        version: next_version,
        signature: String::new(),
    };
    let signable = p2p::page_store::signable_manifest_bytes(&manifest);
    manifest.signature = sign_hex(&state, &signable).await?;

    {
        let mut store = state.node.page_store.write().await;
        store
            .publish_site(manifest.clone())
            .map_err(|e| format!("{e:?}"))?;
    }

    let manifest_json = serde_json::to_string(&manifest).map_err(|e| e.to_string())?;
    wrap_broadcast(
        &state,
        GossipMessage::PublishSiteManifest { manifest_json },
    )
    .await?;

    Ok(serde_json::json!({
        "author_pk": pk,
        "version": next_version,
        "page_count": manifest.pages.len(),
        "asset_count": manifest.assets.len(),
    }))
}

/// V3.3 — Récupère une page d'un site (résout `/` vers le `root_path`).
#[tauri::command]
pub async fn get_site_page(
    state: tauri::State<'_, Arc<AppState>>,
    author_pk: String,
    path: String,
) -> Result<Option<serde_json::Value>, String> {
    let store = state.node.page_store.read().await;
    Ok(store.get_site_page(&author_pk, &path).map(|p| {
        serde_json::json!({
            "path": p.path,
            "title": p.title,
            "html": p.html,
        })
    }))
}

/// V3.3 — Récupère un asset (CSS, image…) d'un site.
#[tauri::command]
pub async fn get_site_asset(
    state: tauri::State<'_, Arc<AppState>>,
    author_pk: String,
    path: String,
) -> Result<Option<serde_json::Value>, String> {
    let store = state.node.page_store.read().await;
    Ok(store.get_site_asset(&author_pk, &path).map(|a| {
        serde_json::json!({
            "path": a.path,
            "mime": a.mime,
            "content_b64": a.content_b64,
            "dag_cid": a.dag_cid,
            "size": a.size,
        })
    }))
}

/// V3.3 — Liste les sites multi-page connus (tous wallets).
#[tauri::command]
pub async fn list_sites(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<serde_json::Value, String> {
    let store = state.node.page_store.read().await;
    let v: Vec<_> = store
        .list_sites()
        .iter()
        .map(|m| {
            serde_json::json!({
                "author_pk": m.author_pk,
                "root_path": m.root_path,
                "page_count": m.pages.len(),
                "asset_count": m.assets.len(),
                "updated_at": m.updated_at,
                "version": m.version,
            })
        })
        .collect();
    Ok(serde_json::Value::Array(v))
}

// ─── WEB OF TRUST ───────────────────────────────────────────────────────────

/// V3.3 — Liste les créateurs suivis par l'utilisateur courant + leurs stats agrégées.
/// Utilisé par la vue Subscriptions (feed personnalisé).
#[tauri::command]
pub async fn list_my_subscriptions(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<serde_json::Value, String> {
    let me = my_pk(&state).await?;
    let g = state.node.follow_graph.read().await;
    let followed: Vec<String> = g.get(&me).cloned().unwrap_or_default();
    drop(g);

    let social = state.node.social.read().await;
    let mut out = Vec::with_capacity(followed.len());
    for pk in followed {
        let stats = social.creator_stats(&pk).cloned().unwrap_or_default();
        out.push(serde_json::json!({
            "pk": pk,
            "follower_count": stats.follower_count,
            "weighted_likes_received": stats.weighted_likes_received,
            "tip_total_received_qta": stats.tip_total_received_micro_qta as f64
                / p2p::ledger::MICRO as f64,
            "boost_bps": stats.boost_bps,
        }));
    }
    Ok(serde_json::Value::Array(out))
}

/// V3.3 — Liste les sites publiés par les créateurs suivis (combo social ⇆ search).
/// Renvoie les `IndexedDoc` des followed récemment publiés.
#[tauri::command]
pub async fn subscriptions_feed(
    state: tauri::State<'_, Arc<AppState>>,
    limit: Option<usize>,
) -> Result<serde_json::Value, String> {
    let me = my_pk(&state).await?;
    let limit = limit.unwrap_or(40).min(200);

    let g = state.node.follow_graph.read().await;
    let followed: std::collections::HashSet<String> =
        g.get(&me).cloned().unwrap_or_default().into_iter().collect();
    drop(g);
    if followed.is_empty() {
        return Ok(serde_json::Value::Array(vec![]));
    }

    let index = state.node.search.read().await;
    let mut hits = index.list_by_authors(&followed, limit);
    // Tri : les plus récents d'abord
    hits.sort_by_key(|b| std::cmp::Reverse(b.updated_at));
    Ok(serde_json::to_value(hits).unwrap_or(serde_json::Value::Null))
}

#[tauri::command]
pub async fn trust_score_for(
    state: tauri::State<'_, Arc<AppState>>,
    target_pk: String,
) -> Result<f64, String> {
    let viewer = my_pk(&state).await?;
    let g = state.node.follow_graph.read().await;
    Ok(p2p::trust_graph::trust_score(&g, &viewer, &target_pk))
}

// ─── Helpers internes (SigningKey reconstruit) ──────────────────────────────

/// Reconstruit un `SigningKey` ed25519_dalek depuis le secret stocké en RAM
/// (utilisé uniquement pour les build_* helpers qui requièrent la struct).
/// Le secret est zéroïsé implicitement à la fin du scope (le `Vec` retourné par
/// `get_secret_bytes` est consommé puis dropé).
async fn signing_key_from_state(
    state: &Arc<AppState>,
) -> Result<ed25519_dalek::SigningKey, String> {
    let secret = state.crypto.lock().await.get_secret_bytes()?;
    if secret.len() != 32 {
        return Err("Secret invalide".into());
    }
    let arr: [u8; 32] = secret
        .as_slice()
        .try_into()
        .map_err(|_| "Secret invalide".to_string())?;
    Ok(ed25519_dalek::SigningKey::from_bytes(&arr))
}
