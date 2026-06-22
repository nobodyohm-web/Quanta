// p2p/reputation.rs — V2 Trust Score & Mining Engine
// Distribution proportionnelle à l'énergie consommée (100 QUANTA/h fixe, pas de halving)
// Shapley Value prêt en Phase 3 (travail utile + validation + uptime)
//
// STRUCT-2: All monetary balances in µQTA (u64). Energy in kWh stays f64 (physical).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::Utc;
use crate::p2p::energy::{EnergyOracle, estimate_watts};
use crate::p2p::sybil::SybilGuard;
use crate::p2p::shapley;
use crate::p2p::ledger::MICRO;

/// User reputation profile — V2 (énergie + mining uniquement). Balances in µQTA.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserReputation {
    pub public_key: String,
    pub trust_score: f64,
    pub status: TrustStatus,
    /// µQTA earned cumulatively
    pub atn_earned: u64,
    /// µQTA balance available
    pub atn_balance: u64,
    /// µQTA staked
    pub atn_staked: u64,
    pub uptime_minutes: u64,
    pub energy_kwh: f64,
    /// µQTA mined via energy (subset of atn_earned)
    pub energy_atn_mined: u64,
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
// ─── TOKENOMICS — rareté : émission décroissante vers un plafond dur ──────────

/// Plafond DUR prouvable de l'offre : **100 000 000 QUANTA** (en µQTA).
/// L'émission ne peut JAMAIS porter l'offre minée au-dessus de ce plafond.
pub const MAX_SUPPLY_MICRO: u64 = 100_000_000 * MICRO;

/// Chaque tick émet `1/EMISSION_DIVISOR` de l'offre **restante** (plafond −
/// déjà miné). Décroissance géométrique front-loaded convergeant vers le plafond.
/// RYTHME RÉALISTE (pas un « flood ») : à 1 tick/minute, émission de genèse
/// ≈ 2 QUANTA/tick ⇒ ≈ 4 QUANTA/bloc (seal toutes les 2 min) ⇒ ≈ 120 QUANTA/h,
/// puis décroît. Longue traîne crédible (rareté réelle), façon Bitcoin — chaque
/// QUANTA se mérite. (Avant : 1_000_000 ⇒ ~6000 QUANTA/h, irréaliste.)
pub const EMISSION_DIVISOR: u64 = 50_000_000;

/// Émission TOTALE du réseau pour ce tick, fonction de l'offre déjà minée :
/// `floor((MAX_SUPPLY − total_mined) / EMISSION_DIVISOR)`.
/// On n'émet qu'une fraction du restant ⇒ `total_mined` reste **strictement
/// borné par MAX_SUPPLY** (rareté prouvable, dépassement impossible).
pub fn emission_for_tick(total_mined_micro: u64) -> u64 {
    MAX_SUPPLY_MICRO.saturating_sub(total_mined_micro) / EMISSION_DIVISOR
}

/// V2 trust score basé uniquement sur énergie + uptime + stake (pas d'actions sociales).
/// `atn_staked` est en µQTA — converti en QUANTA pour le score.
fn compute_trust_score(user: &UserReputation) -> f64 {
    let uptime_factor = (user.uptime_minutes as f64 / 60.0).min(1000.0);
    let energy_factor = (user.energy_kwh * 100.0).min(500.0);
    let staked_quanta = user.atn_staked as f64 / MICRO as f64;
    let stake_factor = (staked_quanta * 2.0).min(500.0);
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

    /// V2 — Appelé toutes les 60s — mine du QUANTA proportionnellement à la Shapley share.
    /// En solo (aucun peer), le nœud reçoit 100% de l'émission.
    /// STRUCT-6: `peer_contribs` collecte les `NodeContribution` des peers vivants
    /// (depuis `peer_info`), permettant à Shapley de répartir l'émission équitablement.
    ///
    /// Retourne `(uqta_minté, kwh_consommé_ce_tick)` — montant en µQTA.
    pub fn uptime_tick(
        &mut self,
        pk: &str,
        blocks_verified: u64,
        emission_this_tick: u64,
        peer_contribs: &HashMap<String, shapley::NodeContribution>,
    ) -> (u64, f64) {
        let watts = estimate_watts();
        let kwh_per_min = watts / 1000.0 / 60.0;

        let user = self.get_or_create(pk);
        user.uptime_minutes += 1;
        user.energy_kwh += kwh_per_min;

        // Auto-detect mode based on power consumption:
        //   < 5W  → Guardian (Raspberry Pi, VPS, idle laptop)
        //   >= 5W → Active (normal usage, mining)
        // Research mode will activate when marketplace tasks are available (Phase 3).
        let mode = if watts < 5.0 {
            shapley::NodeMode::Guardian
        } else {
            shapley::NodeMode::Active
        };

        // STRUCT-6: Build the full Shapley contribution map (self + peers).
        let my_contrib = shapley::NodeContribution {
            node_id: pk.to_string(),
            watts,
            tasks_completed: 0, // wired from marketplace.completed_by(pk) at the call site
            blocks_verified, // CRIT-B: real counter from WillowNode
            uptime_minutes: user.uptime_minutes,
            mode,
        };
        let my_share: u64 = if peer_contribs.is_empty() {
            // Solo: receive the full (decaying) emission of this tick.
            emission_this_tick
        } else {
            let mut all_contribs = peer_contribs.clone();
            all_contribs.insert(pk.to_string(), my_contrib);
            let shares = shapley::compute_all_shares(&all_contribs);
            let share = shares.get(pk).copied().unwrap_or(0.0).clamp(0.0, 1.0);
            (share * emission_this_tick as f64) as u64
        };

        let poc = SybilGuard::poc_score(user);
        let multiplier = SybilGuard::mining_multiplier(poc).clamp(0.0, 1.0);
        let qta: u64 = (my_share as f64 * multiplier) as u64;

        user.atn_balance = user.atn_balance.saturating_add(qta);
        user.atn_earned = user.atn_earned.saturating_add(qta);
        user.energy_atn_mined = user.energy_atn_mined.saturating_add(qta);

        let score = compute_trust_score(user);
        user.trust_score = score;
        user.status = TrustStatus::from_score(score);

        (qta, kwh_per_min)
    }

    /// Plancher ATN en EUR basé sur le prix réel de l'électricité du pays détecté.
    pub fn atn_floor_eur() -> f64 {
        let oracle  = EnergyOracle::new();
        let country = EnergyOracle::detect_country();
        oracle.atn_floor_eur(country, WATTS_IDLE_FALLBACK)
    }

    /// Stats énergie agrégées sur tous les nœuds connus.
    /// Retourne `(total_kwh, total_mined_uqta, total_uptime_min)`.
    pub fn network_energy_stats(&self) -> (f64, u64, u64) {
        let total_kwh: f64    = self.users.values().map(|u| u.energy_kwh).sum();
        let total_mined: u64  = self.users.values().map(|u| u.energy_atn_mined).sum();
        let total_uptime: u64 = self.users.values().map(|u| u.uptime_minutes).sum();
        (total_kwh, total_mined, total_uptime)
    }

    /// Stake µQTA — locks `amount` µQTA from balance into the staked pool.
    pub fn stake(&mut self, pk: &str, amount: u64) -> Result<u64, String> {
        let user = self.get_or_create(pk);
        if user.atn_balance < amount { return Err("Insufficient QUANTA".into()); }
        user.atn_balance -= amount;
        user.atn_staked += amount;
        Ok(user.atn_staked)
    }

    /// Transfer µQTA between users (mirrored from ledger transfers).
    pub fn transfer(&mut self, from_pk: &str, to_pk: &str, amount: u64) -> Result<(), String> {
        let from = self.get_or_create(from_pk);
        if from.atn_balance < amount { return Err("Insufficient QUANTA".into()); }
        from.atn_balance -= amount;
        let to = self.get_or_create(to_pk);
        to.atn_balance += amount;
        Ok(())
    }

    /// Burn µQTA permanently from a user's balance.
    pub fn burn(&mut self, pk: &str, amount: u64) -> Result<(), String> {
        let user = self.get_or_create(pk);
        if user.atn_balance < amount { return Err("Insufficient QUANTA".into()); }
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
            atn_earned:        0,
            // STRUCT-2: 10 QUANTA welcome credit = 10 * MICRO µQTA
            atn_balance:       10 * MICRO,
            atn_staked:        0,
            uptime_minutes:    0,
            energy_kwh:        0.0,
            energy_atn_mined:  0,
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
        let peers = HashMap::new();
        let (atn, kwh) = rep.uptime_tick(&pk, 0, emission_for_tick(0), &peers);
        assert!(kwh > 0.0, "kwh doit être positif (watts mesurés > 0)");
        assert!(kwh < 0.01, "kwh par minute doit rester < 10Wh, got {}", kwh);
        assert!(atn > 0, "µQUANTA minés doit être positif");
    }

    #[test]
    fn uptime_tick_accumulates_kwh() {
        let mut rep = ReputationEngine::new();
        let pk = "b".repeat(64);
        let peers = HashMap::new();
        let (_, k1) = rep.uptime_tick(&pk, 0, emission_for_tick(0), &peers);
        let (_, k2) = rep.uptime_tick(&pk, 0, emission_for_tick(0), &peers);
        let total = rep.get_user(&pk).unwrap().energy_kwh;
        assert!((total - (k1 + k2)).abs() < 1e-9, "energy_kwh accumulé = somme des ticks");
    }

    #[test]
    fn trust_score_v2_based_on_energy_uptime() {
        let user = UserReputation {
            public_key:       "test".into(),
            trust_score:      0.0,
            status:           TrustStatus::New,
            atn_earned:       0,
            atn_balance:      0,
            // 100 QUANTA staked = 100 * MICRO µQTA
            atn_staked:       100 * MICRO,
            uptime_minutes:   600,   // 10 heures
            energy_kwh:       1.0,
            energy_atn_mined: 0,
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
        // solo (peer_contribs vide) → reçoit 100% de l'émission
        let peers = HashMap::new();
        let (qta, _kwh) = rep.uptime_tick(&pk, 0, emission_for_tick(0), &peers);
        assert!(qta > 0, "solo doit miner > 0");
        // v2 : ne doit pas dépasser l'émission décroissante de ce tick.
        let cap = emission_for_tick(0);
        assert!(qta <= cap,
            "solo ne doit pas dépasser emission_for_tick(0), got {} > {}", qta, cap);
    }

    #[test]
    fn test_trust_score_increases_with_uptime() {
        let mut rep = ReputationEngine::new();
        let pk = "uptime_test".repeat(6);
        let peers = HashMap::new();
        rep.uptime_tick(&pk, 0, emission_for_tick(0), &peers);
        let score1 = rep.get_user(&pk).unwrap().trust_score;
        for _ in 0..60 { rep.uptime_tick(&pk, 0, emission_for_tick(0), &peers); }
        let score2 = rep.get_user(&pk).unwrap().trust_score;
        assert!(score2 > score1, "trust_score doit augmenter avec l'uptime: {} → {}", score1, score2);
    }

    /// TOKENOMICS v2 — invariant de RARETÉ : l'émission décroît avec l'offre et
    /// `total_mined` ne peut JAMAIS franchir le plafond dur (dépassement
    /// mathématiquement impossible), tout en restant fortement front-loaded.
    #[test]
    fn v2_emission_decays_and_respects_hard_cap() {
        assert!(
            emission_for_tick(0) > emission_for_tick(MAX_SUPPLY_MICRO / 2),
            "l'émission doit décroître quand l'offre augmente"
        );
        assert_eq!(emission_for_tick(MAX_SUPPLY_MICRO), 0, "émission nulle au plafond");
        assert_eq!(emission_for_tick(MAX_SUPPLY_MICRO + 1), 0, "jamais sous le plafond");

        // En accumulant tick après tick, total_mined reste STRICTEMENT borné.
        let mut total: u64 = 0;
        for _ in 0..200_000 {
            let e = emission_for_tick(total);
            total = total.saturating_add(e);
            assert!(total <= MAX_SUPPLY_MICRO, "total_mined a dépassé le plafond !");
            if e == 0 {
                break;
            }
        }
        assert!(total <= MAX_SUPPLY_MICRO);
        // Front-loaded mais RÉALISTE : sur 200 000 ticks (~139 j à 1/min) on émet
        // une part non négligeable (~0.4 %) — du vrai, pas un flood. (Avant le
        // ralentissement réaliste : >1 %.) L'aspect front-loaded est déjà prouvé
        // par emission_for_tick(0) > emission_for_tick(MAX/2) plus haut.
        assert!(total > MAX_SUPPLY_MICRO / 1000, "émission front-loaded mais réaliste");
    }
}
