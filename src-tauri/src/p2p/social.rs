#![allow(dead_code)] // V3 module — helpers exposés pour tests externes & V3.3
//! Torus V3 — Économie d'attention : likes (quadratic), abonnements, tips, boosts.
//!
//! ▸ **Like quadratique** : le voteur dépense `n` µQTA, l'influence vaut `√n` —
//!   décourage les fermes de bots, encourage la diversité.
//! ▸ **Abonnement** 3 tiers (signal / supporter / mécène). Tier 2/3 redirigent
//!   un % du mining mensuel de l'abonné vers le créateur.
//! ▸ **Tip** : transfert direct (le ledger gère le BME 1%).
//! ▸ **Boost** : payer pour amplifier le ranking d'une page X heures (cap quotidien).
//!
//! Toutes les actions sont signées Ed25519 et propagées en gossip.
//! Le ledger débite/crédite les wallets ; ce module garde l'**état dérivé**
//! (sommes pondérées par cible, listes d'abonnements, boosts actifs).

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// ─── Constantes ─────────────────────────────────────────────────────────────

pub const LIKE_BASE_COST_MICRO_QTA: u64 = 100_000; // 0.1 QTA
pub const FOLLOW_TIER2_MICRO_QTA: u64 = 1_000_000; // 1 QTA / mois
pub const FOLLOW_TIER3_MICRO_QTA: u64 = 10_000_000; // 10 QTA / mois
pub const BOOST_BURN_BPS: u64 = 500; // 5% burn
pub const BOOST_MAX_DAILY_PER_PAGE_MICRO_QTA: u64 = 100_000_000; // 100 QTA
pub const BOOST_DURATION_SECS: u64 = 24 * 3600;
pub const BOOST_MULTIPLIER: f64 = 1.5;

// ─── Tiers d'abonnement ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FollowTier {
    Signal,    // gratuit
    Supporter, // 1 QTA / mois → +5% mining boost créateur
    Patron,    // 10 QTA / mois → +15% mining boost créateur
}

impl FollowTier {
    pub fn monthly_micro_qta(self) -> u64 {
        match self {
            FollowTier::Signal => 0,
            FollowTier::Supporter => FOLLOW_TIER2_MICRO_QTA,
            FollowTier::Patron => FOLLOW_TIER3_MICRO_QTA,
        }
    }
    pub fn boost_pct(self) -> u32 {
        match self {
            FollowTier::Signal => 0,
            FollowTier::Supporter => 5,
            FollowTier::Patron => 15,
        }
    }
}

// ─── Actions signées (propagées via gossip) ─────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SocialAction {
    /// `voter_pk` like `target_cid` en dépensant `amount_micro_qta` (≥ base).
    /// `weight` est ±1 (like / dislike).
    Vote {
        target_cid: String,
        target_author_pk: String,
        amount_micro_qta: u64,
        weight: i8, // +1 like, -1 dislike
    },
    /// `voter_pk` s'abonne (ou se désabonne si `tier=Signal & active=false`)
    Follow {
        followee_pk: String,
        tier: FollowTier,
        active: bool,
    },
    /// Tip (le ledger fait le transfer ; ce SocialAction est l'annonce).
    Tip {
        target_cid: String,
        target_author_pk: String,
        amount_micro_qta: u64,
        memo: String,
    },
    /// Boost d'une page (`amount` est dépensé, 5% brûlé).
    Boost {
        target_cid: String,
        target_author_pk: String,
        amount_micro_qta: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedAction {
    pub action: SocialAction,
    pub author_pk: String,
    pub timestamp: u64,
    pub nonce: u64,
    pub signature: String, // hex 128
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SocialError {
    InvalidSignature,
    InvalidPayload,
    InsufficientAmount { needed: u64, given: u64 },
    BoostCapExceeded { cap: u64, attempted: u64 },
    DuplicateAction,
    Replay,
}

pub fn signable_bytes(a: &SignedAction) -> Vec<u8> {
    // canonical JSON-like, sans la signature
    let payload = serde_json::to_string(&a.action).unwrap_or_default();
    format!(
        "SOC|{}|{}|{}|{}",
        a.author_pk, a.timestamp, a.nonce, payload
    )
    .into_bytes()
}

pub fn sign_action(sk: &SigningKey, a: &mut SignedAction) {
    let pk = hex::encode(sk.verifying_key().as_bytes());
    a.author_pk = pk;
    let sig = sk.sign(&signable_bytes(a));
    a.signature = hex::encode(sig.to_bytes());
}

pub fn verify(a: &SignedAction) -> Result<(), SocialError> {
    let pk_bytes = hex::decode(&a.author_pk).map_err(|_| SocialError::InvalidSignature)?;
    let sig_bytes = hex::decode(&a.signature).map_err(|_| SocialError::InvalidSignature)?;
    let pk_arr: [u8; 32] = pk_bytes.try_into().map_err(|_| SocialError::InvalidSignature)?;
    let sig_arr: [u8; 64] = sig_bytes.try_into().map_err(|_| SocialError::InvalidSignature)?;
    let vk = VerifyingKey::from_bytes(&pk_arr).map_err(|_| SocialError::InvalidSignature)?;
    let sig = Signature::from_bytes(&sig_arr);
    vk.verify(&signable_bytes(a), &sig)
        .map_err(|_| SocialError::InvalidSignature)
}

// ─── État social (dérivé) ───────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PageStats {
    /// Σ √(amount) sur les votes positifs.
    pub weighted_likes: f64,
    /// Σ √(amount) sur les votes négatifs.
    pub weighted_dislikes: f64,
    pub like_count: u64,
    pub dislike_count: u64,
    pub tip_total_micro_qta: u64,
    /// Boost actif jusqu'à ce timestamp (epoch secs).
    pub boost_until_ts: u64,
    /// Σ µQTA boostés aujourd'hui (anti-spam, reset implicite par jour).
    pub boost_today_micro_qta: u64,
    pub boost_today_day: u64, // jour epoch (now / 86400)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreatorStats {
    pub follower_count: u64,
    /// nb d'abonnés par tier
    pub by_tier: HashMap<String, u64>,
    /// somme du % de mining redirigé vers ce créateur (en bps)
    pub boost_bps: u64,
    /// V3.3 — Σ √(amount) sur tous les votes positifs reçus (toutes pages confondues).
    /// Alimente directement Shapley v2 (15% utilité sociale) sans avoir à joindre
    /// SearchIndex (cid → author) avec SocialState (cid → likes).
    #[serde(default)]
    pub weighted_likes_received: f64,
    /// V3.3 — Σ √(amount) sur les votes négatifs reçus.
    #[serde(default)]
    pub weighted_dislikes_received: f64,
    /// V3.3 — Σ µQTA reçus en tips (toutes pages confondues).
    #[serde(default)]
    pub tip_total_received_micro_qta: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SocialSnapshot {
    pub pages: HashMap<String, PageStats>,
    pub creators: HashMap<String, CreatorStats>,
    pub follows: HashMap<String, HashMap<String, FollowTier>>, // follower → (followee → tier)
    pub seen_action_ids: Vec<String>,
}

#[derive(Debug, Default, Clone)]
pub struct SocialState {
    pages: HashMap<String, PageStats>,         // cid → stats
    creators: HashMap<String, CreatorStats>,   // pk → stats
    follows: HashMap<String, HashMap<String, FollowTier>>, // follower → (followee → tier)
    seen_action_ids: HashSet<String>,
}

impl SocialState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> SocialSnapshot {
        SocialSnapshot {
            pages: self.pages.clone(),
            creators: self.creators.clone(),
            follows: self.follows.clone(),
            seen_action_ids: self.seen_action_ids.iter().cloned().collect(),
        }
    }

    pub fn restore(snap: SocialSnapshot) -> Self {
        Self {
            pages: snap.pages,
            creators: snap.creators,
            follows: snap.follows,
            seen_action_ids: snap.seen_action_ids.into_iter().collect(),
        }
    }

    pub fn page_stats(&self, cid: &str) -> Option<&PageStats> {
        self.pages.get(cid)
    }

    pub fn creator_stats(&self, pk: &str) -> Option<&CreatorStats> {
        self.creators.get(pk)
    }

    pub fn followers_of(&self, pk: &str) -> u64 {
        self.creators.get(pk).map(|c| c.follower_count).unwrap_or(0)
    }

    pub fn boost_factor(&self, cid: &str, now: u64) -> f64 {
        match self.pages.get(cid) {
            Some(p) if p.boost_until_ts > now => BOOST_MULTIPLIER,
            _ => 1.0,
        }
    }

    /// Action ID = BLAKE3(canonical signable bytes + signature). Stable.
    fn action_id(a: &SignedAction) -> String {
        let mut buf = signable_bytes(a);
        buf.extend(a.signature.as_bytes());
        hex::encode(blake3::hash(&buf).as_bytes())
    }

    /// Applique une action signée. Vérifie la signature, déduplique, met à jour.
    pub fn apply(&mut self, a: &SignedAction, now: u64) -> Result<(), SocialError> {
        verify(a)?;
        let id = Self::action_id(a);
        if !self.seen_action_ids.insert(id) {
            return Err(SocialError::DuplicateAction);
        }
        match &a.action {
            SocialAction::Vote {
                target_cid,
                target_author_pk,
                amount_micro_qta,
                weight,
            } => {
                if *amount_micro_qta < LIKE_BASE_COST_MICRO_QTA {
                    return Err(SocialError::InsufficientAmount {
                        needed: LIKE_BASE_COST_MICRO_QTA,
                        given: *amount_micro_qta,
                    });
                }
                if *weight != 1 && *weight != -1 {
                    return Err(SocialError::InvalidPayload);
                }
                let stats = self.pages.entry(target_cid.clone()).or_default();
                let influence = (*amount_micro_qta as f64).sqrt();
                if *weight > 0 {
                    stats.weighted_likes += influence;
                    stats.like_count += 1;
                } else {
                    stats.weighted_dislikes += influence;
                    stats.dislike_count += 1;
                }
                // V3.3 — Crédite l'auteur cible : Shapley v2 lit weighted_likes_received.
                let creator = self.creators.entry(target_author_pk.clone()).or_default();
                if *weight > 0 {
                    creator.weighted_likes_received += influence;
                } else {
                    creator.weighted_dislikes_received += influence;
                }
            }
            SocialAction::Follow {
                followee_pk,
                tier,
                active,
            } => {
                let map = self.follows.entry(a.author_pk.clone()).or_default();
                let creator = self.creators.entry(followee_pk.clone()).or_default();
                let prev = map.get(followee_pk).copied();
                if *active {
                    if prev.is_none() {
                        creator.follower_count += 1;
                    }
                    if let Some(p) = prev {
                        // retire l'ancien tier
                        let key = format!("{:?}", p);
                        let v = creator.by_tier.entry(key).or_insert(0);
                        *v = v.saturating_sub(1);
                        creator.boost_bps =
                            creator.boost_bps.saturating_sub(p.boost_pct() as u64 * 100);
                    }
                    map.insert(followee_pk.clone(), *tier);
                    let key = format!("{:?}", tier);
                    *creator.by_tier.entry(key).or_insert(0) += 1;
                    creator.boost_bps += tier.boost_pct() as u64 * 100;
                } else if let Some(p) = prev {
                    map.remove(followee_pk);
                    creator.follower_count = creator.follower_count.saturating_sub(1);
                    let key = format!("{:?}", p);
                    let v = creator.by_tier.entry(key).or_insert(0);
                    *v = v.saturating_sub(1);
                    creator.boost_bps =
                        creator.boost_bps.saturating_sub(p.boost_pct() as u64 * 100);
                }
            }
            SocialAction::Tip {
                target_cid,
                target_author_pk,
                amount_micro_qta,
                ..
            } => {
                if *amount_micro_qta == 0 {
                    return Err(SocialError::InvalidPayload);
                }
                let stats = self.pages.entry(target_cid.clone()).or_default();
                stats.tip_total_micro_qta = stats
                    .tip_total_micro_qta
                    .saturating_add(*amount_micro_qta);
                // V3.3 — Crédite le créateur cible (vue agrégée).
                let creator = self.creators.entry(target_author_pk.clone()).or_default();
                creator.tip_total_received_micro_qta = creator
                    .tip_total_received_micro_qta
                    .saturating_add(*amount_micro_qta);
            }
            SocialAction::Boost {
                target_cid,
                amount_micro_qta,
                ..
            } => {
                if *amount_micro_qta < LIKE_BASE_COST_MICRO_QTA {
                    return Err(SocialError::InsufficientAmount {
                        needed: LIKE_BASE_COST_MICRO_QTA,
                        given: *amount_micro_qta,
                    });
                }
                let day = now / 86_400;
                let stats = self.pages.entry(target_cid.clone()).or_default();
                if stats.boost_today_day != day {
                    stats.boost_today_day = day;
                    stats.boost_today_micro_qta = 0;
                }
                let new_total = stats.boost_today_micro_qta.saturating_add(*amount_micro_qta);
                if new_total > BOOST_MAX_DAILY_PER_PAGE_MICRO_QTA {
                    return Err(SocialError::BoostCapExceeded {
                        cap: BOOST_MAX_DAILY_PER_PAGE_MICRO_QTA,
                        attempted: new_total,
                    });
                }
                stats.boost_today_micro_qta = new_total;
                stats.boost_until_ts = now.saturating_add(BOOST_DURATION_SECS);
            }
        }
        Ok(())
    }

    /// Helpers pour `search.rs`.
    pub fn signals_for(&self, cid: &str, author_pk: &str) -> (f64, u64) {
        let likes = self.pages.get(cid).map(|p| p.weighted_likes).unwrap_or(0.0);
        let followers = self
            .creators
            .get(author_pk)
            .map(|c| c.follower_count)
            .unwrap_or(0);
        (likes, followers)
    }
}

/// Brûle 5% d'un montant boost. Helper pur, le caller fait le ledger move.
pub fn boost_burn_share(amount_micro_qta: u64) -> u64 {
    (amount_micro_qta as u128 * BOOST_BURN_BPS as u128 / 10_000) as u64
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    fn sk(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn pk_of(s: &SigningKey) -> String {
        hex::encode(s.verifying_key().as_bytes())
    }

    fn vote(
        voter: &SigningKey,
        target_cid: &str,
        author_pk: &str,
        amount: u64,
        weight: i8,
        ts: u64,
        nonce: u64,
    ) -> SignedAction {
        let mut a = SignedAction {
            action: SocialAction::Vote {
                target_cid: target_cid.into(),
                target_author_pk: author_pk.into(),
                amount_micro_qta: amount,
                weight,
            },
            author_pk: String::new(),
            timestamp: ts,
            nonce,
            signature: String::new(),
        };
        sign_action(voter, &mut a);
        a
    }

    #[test]
    fn quadratic_like_influence() {
        let mut s = SocialState::new();
        let v = sk(1);
        let author = pk_of(&sk(9));
        // 1 like à 100 QTA (100_000_000 µQTA)
        s.apply(&vote(&v, "cidA", &author, 100_000_000, 1, 1, 1), 1).unwrap();
        let influence_big = s.page_stats("cidA").unwrap().weighted_likes;
        // 1 like à 1 QTA (1_000_000 µQTA) sur autre cid
        s.apply(&vote(&v, "cidB", &author, 1_000_000, 1, 1, 2), 1).unwrap();
        let influence_small = s.page_stats("cidB").unwrap().weighted_likes;
        // ratio attendu = √100 = 10
        let ratio = influence_big / influence_small;
        assert!((ratio - 10.0).abs() < 0.01);
    }

    #[test]
    fn dislike_separately_tracked() {
        let mut s = SocialState::new();
        let v = sk(1);
        let author = pk_of(&sk(9));
        s.apply(&vote(&v, "cidA", &author, LIKE_BASE_COST_MICRO_QTA, -1, 1, 1), 1).unwrap();
        let p = s.page_stats("cidA").unwrap();
        assert_eq!(p.dislike_count, 1);
        assert_eq!(p.like_count, 0);
        assert!(p.weighted_dislikes > 0.0);
    }

    #[test]
    fn under_min_cost_rejected() {
        let mut s = SocialState::new();
        let v = sk(1);
        let author = pk_of(&sk(9));
        let res = s.apply(&vote(&v, "cidA", &author, 1, 1, 1, 1), 1);
        assert!(matches!(res, Err(SocialError::InsufficientAmount { .. })));
    }

    #[test]
    fn duplicate_action_rejected() {
        let mut s = SocialState::new();
        let v = sk(1);
        let author = pk_of(&sk(9));
        let a = vote(&v, "cidA", &author, LIKE_BASE_COST_MICRO_QTA, 1, 1, 1);
        s.apply(&a, 1).unwrap();
        assert_eq!(s.apply(&a, 1), Err(SocialError::DuplicateAction));
    }

    #[test]
    fn forged_sig_rejected() {
        let mut s = SocialState::new();
        let v = sk(1);
        let mut a = vote(&v, "cidA", &pk_of(&sk(9)), LIKE_BASE_COST_MICRO_QTA, 1, 1, 1);
        a.signature = "00".repeat(64);
        assert_eq!(s.apply(&a, 1), Err(SocialError::InvalidSignature));
    }

    #[test]
    fn follow_tier3_increments_boost() {
        let mut s = SocialState::new();
        let v = sk(1);
        let creator = pk_of(&sk(9));
        let mut a = SignedAction {
            action: SocialAction::Follow {
                followee_pk: creator.clone(),
                tier: FollowTier::Patron,
                active: true,
            },
            author_pk: String::new(),
            timestamp: 1,
            nonce: 1,
            signature: String::new(),
        };
        sign_action(&v, &mut a);
        s.apply(&a, 1).unwrap();
        let c = s.creator_stats(&creator).unwrap();
        assert_eq!(c.follower_count, 1);
        assert_eq!(c.boost_bps, 1500); // 15%
    }

    #[test]
    fn unfollow_decrements() {
        let mut s = SocialState::new();
        let v = sk(1);
        let creator = pk_of(&sk(9));
        let mk = |active, nonce| {
            let mut a = SignedAction {
                action: SocialAction::Follow {
                    followee_pk: creator.clone(),
                    tier: FollowTier::Supporter,
                    active,
                },
                author_pk: String::new(),
                timestamp: nonce,
                nonce,
                signature: String::new(),
            };
            sign_action(&v, &mut a);
            a
        };
        s.apply(&mk(true, 1), 1).unwrap();
        s.apply(&mk(false, 2), 2).unwrap();
        let c = s.creator_stats(&creator).unwrap();
        assert_eq!(c.follower_count, 0);
        assert_eq!(c.boost_bps, 0);
    }

    #[test]
    fn boost_extends_until_and_burns() {
        let mut s = SocialState::new();
        let v = sk(1);
        let mut a = SignedAction {
            action: SocialAction::Boost {
                target_cid: "cidA".into(),
                target_author_pk: pk_of(&sk(9)),
                amount_micro_qta: 50_000_000,
            },
            author_pk: String::new(),
            timestamp: 100,
            nonce: 1,
            signature: String::new(),
        };
        sign_action(&v, &mut a);
        s.apply(&a, 100).unwrap();
        let p = s.page_stats("cidA").unwrap();
        assert_eq!(p.boost_until_ts, 100 + BOOST_DURATION_SECS);
        assert_eq!(boost_burn_share(50_000_000), 2_500_000); // 5%
    }

    #[test]
    fn boost_cap_enforced() {
        let mut s = SocialState::new();
        let v = sk(1);
        let mk = |amount, nonce| {
            let mut a = SignedAction {
                action: SocialAction::Boost {
                    target_cid: "cidA".into(),
                    target_author_pk: pk_of(&sk(9)),
                    amount_micro_qta: amount,
                },
                author_pk: String::new(),
                timestamp: 100,
                nonce,
                signature: String::new(),
            };
            sign_action(&v, &mut a);
            a
        };
        s.apply(&mk(BOOST_MAX_DAILY_PER_PAGE_MICRO_QTA, 1), 100).unwrap();
        let res = s.apply(&mk(LIKE_BASE_COST_MICRO_QTA, 2), 100);
        assert!(matches!(res, Err(SocialError::BoostCapExceeded { .. })));
    }

    #[test]
    fn snapshot_round_trip() {
        let mut s = SocialState::new();
        let v = sk(1);
        s.apply(&vote(&v, "cidA", &pk_of(&sk(9)), LIKE_BASE_COST_MICRO_QTA, 1, 1, 1), 1).unwrap();
        let snap = s.snapshot();
        let s2 = SocialState::restore(snap);
        assert!(s2.page_stats("cidA").is_some());
    }

    /// AUDIT-SOC-1 (regression): verify is invoked BEFORE apply and a tampered
    /// timestamp invalidates the signature — the action must be rejected and
    /// no derived state should be mutated.
    #[test]
    fn audit_soc_verify_runs_before_apply() {
        let mut s = SocialState::new();
        let v = sk(7);
        let mut a = vote(&v, "cidZ", &pk_of(&sk(8)), LIKE_BASE_COST_MICRO_QTA, 1, 1, 1);
        // Tamper the timestamp after signing — sig no longer matches payload.
        a.timestamp = 99_999;
        let res = s.apply(&a, 1);
        assert_eq!(res, Err(SocialError::InvalidSignature));
        // No page stat should have been created.
        assert!(s.page_stats("cidZ").is_none());
    }

    /// AUDIT-SOC-1 (regression): a tip's underlying QUANTA movement is
    /// captured in CreatorStats.tip_total_received_micro_qta — provides a
    /// direct check that the social-state mirror tracks the ledger movement.
    #[test]
    fn audit_soc_tip_credits_creator_stats() {
        let mut s = SocialState::new();
        let v = sk(3);
        let author = pk_of(&sk(4));

        let mut a = SignedAction {
            action: SocialAction::Tip {
                target_cid: "cidT".into(),
                target_author_pk: author.clone(),
                amount_micro_qta: 5_000_000, // 5 QTA
                memo: "thanks".into(),
            },
            author_pk: String::new(),
            timestamp: 1,
            nonce: 1,
            signature: String::new(),
        };
        sign_action(&v, &mut a);
        s.apply(&a, 1).unwrap();

        let creator = s.creator_stats(&author).expect("creator must be tracked");
        assert_eq!(creator.tip_total_received_micro_qta, 5_000_000);
        let page = s.page_stats("cidT").expect("page must be tracked");
        assert_eq!(page.tip_total_micro_qta, 5_000_000);
    }
}
