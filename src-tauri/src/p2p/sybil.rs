//! Anti-Sybil : Proof of Contribution (PoC) V2 — facteurs énergie uniquement
//!
//! Empêche les attaques Sybil en pondérant le taux de mining selon
//! la contribution réelle mesurable de chaque identité (énergie, uptime, stake, ancienneté).
//!
//! Score PoC = uptime×0.35 + énergie×0.30 + stake×0.25 + ancienneté×0.10
//!
//! Interprétation :
//!   - 0.0 → nouveau nœud ou suspect  (mining ×0.10)
//!   - 0.5 → contributeur modéré      (mining ×0.55)
//!   - 1.0 → nœud établi & prouvé     (mining ×1.00)

use crate::p2p::reputation::UserReputation;
use crate::p2p::ledger::MICRO;

// ─── Poids PoC V2 ───────────────────────────────────────────────────────────

const W_UPTIME: f64 = 0.35;
const W_ENERGY: f64 = 0.30;
const W_STAKE:  f64 = 0.25;
const W_AGE:    f64 = 0.10;

// ─── SybilGuard ─────────────────────────────────────────────────────────────

pub struct SybilGuard;

impl SybilGuard {
    /// Calcule le score PoC global d'un utilisateur [0.0, 1.0].
    pub fn poc_score(rep: &UserReputation) -> f64 {
        // STRUCT-2: atn_staked is u64 µQTA — convert to QUANTA for the exp curve.
        let staked_quanta = rep.atn_staked as f64 / MICRO as f64;
        let score = Self::uptime_factor(rep.uptime_minutes) * W_UPTIME
                  + Self::energy_factor(rep.energy_kwh)     * W_ENERGY
                  + Self::stake_factor(staked_quanta)         * W_STAKE
                  + Self::age_factor(&rep.joined_at)        * W_AGE;
        score.clamp(0.0, 1.0)
    }

    /// Multiplicateur de mining [0.1, 1.0] dérivé du score PoC.
    ///
    /// Courbe linéaire : nouveau nœud mine à 10 %, nœud prouvé à 100 %.
    pub fn mining_multiplier(poc_score: f64) -> f64 {
        0.1 + 0.9 * poc_score.clamp(0.0, 1.0)
    }

    // ── Facteurs individuels [0.0, 1.0] ──────────────────────────────────────

    /// Uptime continu — coûte de l'énergie réelle, difficile à simuler.
    /// 0 min → 0.0, 60 min → 0.14, 24 h → 0.59, 7 j → 1.0
    fn uptime_factor(minutes: u64) -> f64 {
        let hours = minutes as f64 / 60.0;
        (hours / (24.0 * 7.0)).min(1.0)
    }

    /// Énergie consommée — preuve physique non falsifiable.
    /// 0 kWh → 0.0, 0.1 kWh → 0.18, 1 kWh → 0.86, 5 kWh → 1.0
    fn energy_factor(kwh: f64) -> f64 {
        if kwh <= 0.0 { return 0.0; }
        1.0 - (-2.0 * kwh).exp()
    }

    /// Stake — skin in the game, progressive jusqu'à 100 QUANTA.
    /// 0 → 0.0, 10 → 0.32, 25 → 0.50, 50 → 0.71, 100 → 1.00
    /// Courbe √(x/100) : chaque QUANTA staké apporte un gain visible.
    fn stake_factor(staked: f64) -> f64 {
        if staked <= 0.0 { return 0.0; }
        (staked / 100.0).sqrt().min(1.0)
    }

    /// Ancienneté — le temps ne se falsifie pas.
    /// 0 j → 0.0, 1 j → 0.11, 7 j → 0.55, 30 j → 0.96, 90 j → 1.0
    fn age_factor(joined_at: &str) -> f64 {
        let Ok(joined) = chrono::DateTime::parse_from_rfc3339(joined_at) else {
            return 0.0;
        };
        let days = (chrono::Utc::now().timestamp() - joined.timestamp()) as f64 / 86_400.0;
        1.0 - (-0.05 * days.max(0.0)).exp()
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: `staked_quanta` est en QUANTA (entiers ou décimaux), converti en µQTA en interne.
    fn mock_rep(uptime_min: u64, kwh: f64, staked_quanta: f64, days_ago: i64) -> UserReputation {
        let joined = (chrono::Utc::now() - chrono::Duration::days(days_ago)).to_rfc3339();
        let staked_uqta = (staked_quanta * MICRO as f64) as u64;
        UserReputation {
            public_key:       "test".into(),
            trust_score:      10.0,
            status:           crate::p2p::reputation::TrustStatus::New,
            atn_earned:       0,
            atn_balance:      10 * MICRO,
            atn_staked:       staked_uqta,
            uptime_minutes:   uptime_min,
            energy_kwh:       kwh,
            energy_atn_mined: 0,
            joined_at:        joined,
        }
    }

    #[test]
    fn new_node_has_low_score() {
        let rep = mock_rep(0, 0.0, 0.0, 0);
        let score = SybilGuard::poc_score(&rep);
        assert!(score < 0.05, "Nouveau nœud : score doit être quasi nul, got {}", score);
        assert!((SybilGuard::mining_multiplier(score) - 0.1).abs() < 0.05);
    }

    #[test]
    fn established_node_has_high_score() {
        // Nœud actif depuis 30 jours, 7j uptime, 1 kWh consommé, 5 QUANTA stakés
        let rep = mock_rep(7 * 24 * 60, 1.0, 5.0, 30);
        let score = SybilGuard::poc_score(&rep);
        assert!(score > 0.7, "Nœud établi : score doit être élevé, got {}", score);
    }

    #[test]
    fn multiplier_range() {
        assert!((SybilGuard::mining_multiplier(0.0) - 0.1).abs() < 1e-9);
        assert!((SybilGuard::mining_multiplier(1.0) - 1.0).abs() < 1e-9);
    }
}
