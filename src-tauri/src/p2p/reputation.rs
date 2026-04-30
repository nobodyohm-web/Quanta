// p2p/reputation.rs — V2 Trust Score & Mining Engine
// Distribution proportionnelle à l'énergie consommée (100 SOVA/h fixe, pas de halving)
// Shapley Value prêt en Phase 3 (travail utile + validation + uptime)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::Utc;
use crate::p2p::energy::{EnergyOracle, estimate_watts};
use crate::p2p::sybil::SybilGuard;
use crate::p2p::shapley;

/// User reputation profile — V2 (énergie + mining uniquement)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserReputation {
    pub public_key: String,
    pub trust_score: f64,
    pub status: TrustStatus,
    pub atn_earned: f64,
    pub atn_balance: f64,
    pub atn_staked: f64,
    pub uptime_minutes: u64,
    pub energy_kwh: f64,
    pub energy_atn_mined: f64,
    pub joined_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TrustStatus {
    New,           // 0-50
    Member,        // 50-200
    Contributor,   // 200-500
    Expert,        // 500-1000
    Star,          // 1000+
}

impl TrustStatus {
    pub fn from_score(score: f64) -> Self {
        match score as i64 {
            s if s >= 1000 => Self::Star,
            s if s >= 500  => Self::Expert,
            s if s >= 200  => Self::Contributor,
            s if s >= 50   => Self::Member,
            _              => Self::New,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::New         => "Nouveau",
            Self::Member      => "Membre",
            Self::Contributor => "Contributeur",
            Self::Expert      => "Expert",
            Self::Star        => "Star",
        }
    }
}

/// Watts mesurés si sysinfo indisponible
const WATTS_IDLE_FALLBACK: f64 = 15.0;
/// V2 — Émission fixe : 100 SOVA par heure, réparti proportionnellement aux watts
const NETWORK_EMISSION_PER_HOUR: f64 = 100.0;
const EMISSION_PER_TICK: f64 = NETWORK_EMISSION_PER_HOUR / 60.0; // 1.6667 SOVA/min

/// V2 trust score basé uniquement sur énergie + uptime + stake (pas d'actions sociales)
fn compute_trust_score(user: &UserReputation) -> f64 {
    let uptime_factor = (user.uptime_minutes as f64 / 60.0).min(1000.0);
    let energy_factor = (user.energy_kwh * 100.0).min(500.0);
    let stake_factor  = (user.atn_staked * 2.0).min(500.0);
    (uptime_factor + energy_factor + stake_factor).max(0.0)
}

pub struct ReputationEngine {
    users: HashMap<String, UserReputation>,
    #[allow(dead_code)]
    started_at: std::time::Instant,
}

impl ReputationEngine {
    pub fn new() -> Self {
        Self {
            users: HashMap::new(),
            started_at: std::time::Instant::now(),
        }
    }

    /// V2 — Appelé toutes les 60s — mine du SOVA proportionnellement aux watts réels.
    /// En solo (Phase 1), le nœud reçoit 100% de l'émission.
    /// En multi-nœuds (Phase 3), `total_network_watts` vient du CRDT GCounter.
    ///
    /// Retourne `(sova_minté, kwh_consommé_ce_tick)`
    pub fn uptime_tick(&mut self, pk: &str, _total_mined: f64, total_network_watts: f64) -> (f64, f64) {
        let watts = estimate_watts();
        let kwh_per_min = watts / 1000.0 / 60.0;

        let user = self.get_or_create(pk);
        user.uptime_minutes += 1;
        user.energy_kwh += kwh_per_min;

        // V2 : émission via Shapley — solo si aucun pair connu
        let my_share = if total_network_watts <= 0.0 {
            EMISSION_PER_TICK
        } else {
            let my_contrib = shapley::NodeContribution {
                node_id: pk.to_string(),
                watts,
                tasks_completed: 0, // Phase 3: from marketplace
                blocks_verified: 0, // TODO: from DAG
                uptime_minutes: user.uptime_minutes,
            };
            let mut contribs = HashMap::new();
            contribs.insert(pk.to_string(), my_contrib);
            // TODO Phase 3: ajouter les contributions des peers depuis gossip
            let shares = shapley::compute_all_shares(&contribs);
            shares.get(pk).copied().unwrap_or(1.0) * EMISSION_PER_TICK
        };

        let poc = SybilGuard::poc_score(user);
        let sova = my_share * SybilGuard::mining_multiplier(poc);

        user.atn_balance += sova;
        user.atn_earned += sova;
        user.energy_atn_mined += sova;

        let score = compute_trust_score(user);
        user.trust_score = score;
        user.status = TrustStatus::from_score(score);

        (sova, kwh_per_min)
    }

    /// V2 — Taux de mining avec proportionnalité réseau (Phase 3+).
    pub fn mining_rate_proportional(my_watts: f64, total_network_watts: f64, poc_score: f64) -> f64 {
        if total_network_watts <= 0.0 { return EMISSION_PER_TICK; }
        let share = (my_watts / total_network_watts).min(1.0);
        share * EMISSION_PER_TICK * SybilGuard::mining_multiplier(poc_score)
    }

    /// Plancher ATN en EUR basé sur le prix réel de l'électricité du pays détecté.
    pub fn atn_floor_eur() -> f64 {
        let oracle  = EnergyOracle::new();
        let country = EnergyOracle::detect_country();
        oracle.atn_floor_eur(country, WATTS_IDLE_FALLBACK)
    }

    /// Stats énergie agrégées sur tous les nœuds connus
    pub fn network_energy_stats(&self) -> (f64, f64, u64) {
        let total_kwh: f64    = self.users.values().map(|u| u.energy_kwh).sum();
        let total_mined: f64  = self.users.values().map(|u| u.energy_atn_mined).sum();
        let total_uptime: u64 = self.users.values().map(|u| u.uptime_minutes).sum();
        (total_kwh, total_mined, total_uptime)
    }

    pub fn stake(&mut self, pk: &str, amount: f64) -> Result<f64, String> {
        let user = self.get_or_create(pk);
        if user.atn_balance < amount { return Err("Insufficient ATN".into()); }
        user.atn_balance -= amount;
        user.atn_staked += amount;
        Ok(user.atn_staked)
    }

    pub fn transfer(&mut self, from_pk: &str, to_pk: &str, amount: f64) -> Result<(), String> {
        let from = self.get_or_create(from_pk);
        if from.atn_balance < amount { return Err("Insufficient ATN".into()); }
        from.atn_balance -= amount;
        let to = self.get_or_create(to_pk);
        to.atn_balance += amount;
        Ok(())
    }

    pub fn burn(&mut self, pk: &str, amount: f64) -> Result<(), String> {
        let user = self.get_or_create(pk);
        if user.atn_balance < amount { return Err("Insufficient ATN".into()); }
        user.atn_balance -= amount;
        Ok(())
    }

    pub fn get_user(&self, pk: &str) -> Option<&UserReputation> {
        self.users.get(pk)
    }

    pub fn get_leaderboard(&self, top_n: usize) -> Vec<UserReputation> {
        let mut all: Vec<UserReputation> = self.users.values().cloned().collect();
        all.sort_by(|a, b| b.trust_score.partial_cmp(&a.trust_score).unwrap_or(std::cmp::Ordering::Equal));
        all.truncate(top_n);
        all
    }

    fn get_or_create(&mut self, pk: &str) -> &mut UserReputation {
        self.users.entry(pk.to_string()).or_insert_with(|| UserReputation {
            public_key:        pk.to_string(),
            trust_score:       10.0,
            status:            TrustStatus::New,
            atn_earned:        0.0,
            atn_balance:       10.0,
            atn_staked:        0.0,
            uptime_minutes:    0,
            energy_kwh:        0.0,
            energy_atn_mined:  0.0,
            joined_at:         Utc::now().to_rfc3339(),
        })
    }
}

impl Default for ReputationEngine { fn default() -> Self { Self::new() } }

/// Snapshot sérialisable — utilisé pour la persistance SQLite
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationSnapshot {
    pub users: HashMap<String, UserReputation>,
}

impl ReputationEngine {
    pub fn snapshot(&self) -> ReputationSnapshot {
        ReputationSnapshot { users: self.users.clone() }
    }

    pub fn restore(snap: ReputationSnapshot) -> Self {
        Self {
            users: snap.users,
            started_at: std::time::Instant::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uptime_tick_returns_real_kwh() {
        let mut rep = ReputationEngine::new();
        let pk = "a".repeat(64);
        let (atn, kwh) = rep.uptime_tick(&pk, 0.0, 0.0);
        assert!(kwh > 0.0, "kwh doit être positif (watts mesurés > 0)");
        assert!(kwh < 0.01, "kwh par minute doit rester < 10Wh, got {}", kwh);
        assert!(atn > 0.0, "atn doit être positif");
    }

    #[test]
    fn uptime_tick_accumulates_kwh() {
        let mut rep = ReputationEngine::new();
        let pk = "b".repeat(64);
        let (_, k1) = rep.uptime_tick(&pk, 0.0, 0.0);
        let (_, k2) = rep.uptime_tick(&pk, 0.0, 0.0);
        let total = rep.get_user(&pk).unwrap().energy_kwh;
        assert!((total - (k1 + k2)).abs() < 1e-9, "energy_kwh accumulé = somme des ticks");
    }

    #[test]
    fn trust_score_v2_based_on_energy_uptime() {
        let user = UserReputation {
            public_key:       "test".into(),
            trust_score:      0.0,
            status:           TrustStatus::New,
            atn_earned:       0.0,
            atn_balance:      0.0,
            atn_staked:       100.0,
            uptime_minutes:   600,   // 10 heures
            energy_kwh:       1.0,
            energy_atn_mined: 0.0,
            joined_at:        "2026-01-01T00:00:00Z".into(),
        };
        let score = compute_trust_score(&user);
        // uptime: 10.0, energy: 100.0, stake: 200.0 → 310.0
        assert!((score - 310.0).abs() < 0.1, "score V2 attendu ~310, got {}", score);
    }

    #[test]
    fn test_emission_solo_full() {
        let mut rep = ReputationEngine::new();
        let pk = "solo".repeat(16);
        // solo (total_network_watts=0) → reçoit 100% de l'émission
        let (sova, _kwh) = rep.uptime_tick(&pk, 0.0, 0.0);
        assert!(sova > 0.0, "solo doit miner > 0");
        assert!(sova <= EMISSION_PER_TICK * 1.1, "solo ne doit pas dépasser ~EMISSION_PER_TICK, got {}", sova);
    }

    #[test]
    fn test_emission_proportional() {
        // 2 nœuds : 100W et 50W — nœud A doit miner 2× plus que nœud B
        let rate_a = ReputationEngine::mining_rate_proportional(100.0, 150.0, 1.0);
        let rate_b = ReputationEngine::mining_rate_proportional(50.0, 150.0, 1.0);
        assert!((rate_a / rate_b - 2.0).abs() < 0.01, "ratio attendu 2.0, got {:.4}", rate_a / rate_b);
    }

    #[test]
    fn test_trust_score_increases_with_uptime() {
        let mut rep = ReputationEngine::new();
        let pk = "uptime_test".repeat(6);
        rep.uptime_tick(&pk, 0.0, 0.0);
        let score1 = rep.get_user(&pk).unwrap().trust_score;
        for _ in 0..60 { rep.uptime_tick(&pk, 0.0, 0.0); }
        let score2 = rep.get_user(&pk).unwrap().trust_score;
        assert!(score2 > score1, "trust_score doit augmenter avec l'uptime: {} → {}", score1, score2);
    }
}
