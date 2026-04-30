#![allow(dead_code)] // Module Phase 3 — pas encore intégré
//! Module Shapley Value — Distribution équitable des récompenses SOVA
//!
//! Implémente une approximation linéaire O(n) de la Shapley Value
//! pour distribuer les 100 SOVA/h entre les nœuds du réseau.
//!
//! Facteurs de contribution :
//!   - Énergie (30%) : watts mesurés / total réseau
//!   - Travail (35%) : tâches exécutées / total tâches
//!   - Validation (20%) : blocs vérifiés / total blocs
//!   - Uptime (15%) : heures en ligne / max uptime réseau
//!
//! Référence : Lloyd S. Shapley, "A Value for n-Person Games", 1953

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Poids Shapley — configurables, somme = 1.0
const W_ENERGY: f64 = 0.30;
const W_WORK: f64 = 0.35;
const W_VALIDATION: f64 = 0.20;
const W_UPTIME: f64 = 0.15;

/// Statistiques d'un nœud pour le calcul Shapley
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodeContribution {
    pub node_id: String,
    /// Watts CPU mesurés en temps réel
    pub watts: f64,
    /// Nombre de tâches compute complétées (Phase 3: Marketplace)
    pub tasks_completed: u64,
    /// Nombre de blocs/transactions vérifiés
    pub blocks_verified: u64,
    /// Minutes en ligne depuis le dernier reset
    pub uptime_minutes: u64,
}

/// Agrégat réseau (totaux de tous les nœuds)
#[derive(Debug, Clone, Default)]
pub struct NetworkTotals {
    pub total_watts: f64,
    pub total_tasks: u64,
    pub total_blocks_verified: u64,
    pub max_uptime_minutes: u64,
    pub node_count: usize,
}

impl NetworkTotals {
    /// Calcule les totaux depuis une map de contributions
    pub fn from_contributions(contribs: &HashMap<String, NodeContribution>) -> Self {
        let mut total_watts = 0.0_f64;
        let mut total_tasks = 0u64;
        let mut total_blocks_verified = 0u64;
        let mut max_uptime_minutes = 0u64;
        for c in contribs.values() {
            total_watts += c.watts;
            total_tasks += c.tasks_completed;
            total_blocks_verified += c.blocks_verified;
            if c.uptime_minutes > max_uptime_minutes {
                max_uptime_minutes = c.uptime_minutes;
            }
        }
        Self {
            total_watts,
            total_tasks,
            total_blocks_verified,
            max_uptime_minutes,
            node_count: contribs.len(),
        }
    }
}

/// Calcule le score Shapley d'un nœud (approximation linéaire).
///
/// Retourne un score entre 0.0 et 1.0 représentant la part du nœud
/// dans l'émission réseau.
///
/// # Invariants
/// - Si un seul nœud → score = 1.0 (reçoit tout)
/// - Si aucune contribution → score = 1/n (distribution uniforme)
/// - Somme des scores de tous les nœuds ≈ 1.0 (à epsilon float près)
pub fn shapley_score(node: &NodeContribution, network: &NetworkTotals) -> f64 {
    if network.node_count == 0 {
        return 0.0;
    }
    if network.node_count == 1 {
        return 1.0;
    }

    // Facteur énergie : ma part relative de watts
    let energy = if network.total_watts > 0.0 {
        node.watts / network.total_watts
    } else {
        1.0 / network.node_count as f64
    };

    // Facteur travail : ma part relative de tâches complétées
    let work = if network.total_tasks > 0 {
        node.tasks_completed as f64 / network.total_tasks as f64
    } else {
        1.0 / network.node_count as f64 // pas de tâches = distribution uniforme
    };

    // Facteur validation : ma part relative de blocs vérifiés
    let validation = if network.total_blocks_verified > 0 {
        node.blocks_verified as f64 / network.total_blocks_verified as f64
    } else {
        1.0 / network.node_count as f64
    };

    // Facteur uptime : mon uptime relatif au meilleur
    let uptime = if network.max_uptime_minutes > 0 {
        node.uptime_minutes as f64 / network.max_uptime_minutes as f64
    } else {
        1.0
    };

    // Score Shapley pondéré
    W_ENERGY * energy + W_WORK * work + W_VALIDATION * validation + W_UPTIME * uptime
}

/// Calcule les scores Shapley de tous les nœuds et normalise
/// pour que la somme = 1.0 exactement.
pub fn compute_all_shares(
    contribs: &HashMap<String, NodeContribution>,
) -> HashMap<String, f64> {
    let network = NetworkTotals::from_contributions(contribs);
    let mut raw_scores: HashMap<String, f64> = HashMap::new();
    let mut total_score = 0.0;

    for (id, contrib) in contribs {
        let score = shapley_score(contrib, &network);
        total_score += score;
        raw_scores.insert(id.clone(), score);
    }

    // Normalisation : somme des parts = 1.0
    if total_score > 0.0 {
        for score in raw_scores.values_mut() {
            *score /= total_score;
        }
    }

    raw_scores
}

/// Distribue l'émission d'un tick entre les nœuds selon leur Shapley score.
///
/// `emission_per_tick` = 100 SOVA/h / 60 = 1.6667 SOVA/min
///
/// Retourne : node_id → SOVA à créditer
pub fn distribute_emission(
    contribs: &HashMap<String, NodeContribution>,
    emission_per_tick: f64,
) -> HashMap<String, f64> {
    let shares = compute_all_shares(contribs);
    shares.into_iter()
        .map(|(id, share)| (id, share * emission_per_tick))
        .collect()
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(id: &str, watts: f64, tasks: u64, blocks: u64, uptime: u64) -> NodeContribution {
        NodeContribution {
            node_id: id.to_string(),
            watts, tasks_completed: tasks, blocks_verified: blocks, uptime_minutes: uptime,
        }
    }

    #[test]
    fn test_solo_node_gets_everything() {
        let mut contribs = HashMap::new();
        contribs.insert("A".into(), make_node("A", 100.0, 10, 5, 60));
        let shares = compute_all_shares(&contribs);
        assert!((shares["A"] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_equal_nodes_equal_shares() {
        let mut contribs = HashMap::new();
        contribs.insert("A".into(), make_node("A", 100.0, 10, 5, 60));
        contribs.insert("B".into(), make_node("B", 100.0, 10, 5, 60));
        let shares = compute_all_shares(&contribs);
        assert!((shares["A"] - 0.5).abs() < 1e-9);
        assert!((shares["B"] - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_shares_sum_to_one() {
        let mut contribs = HashMap::new();
        contribs.insert("A".into(), make_node("A", 200.0, 20, 10, 120));
        contribs.insert("B".into(), make_node("B", 50.0, 5, 2, 60));
        contribs.insert("C".into(), make_node("C", 100.0, 10, 5, 90));
        let shares = compute_all_shares(&contribs);
        let sum: f64 = shares.values().sum();
        assert!((sum - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_higher_contribution_higher_share() {
        let mut contribs = HashMap::new();
        contribs.insert("A".into(), make_node("A", 200.0, 20, 10, 120));
        contribs.insert("B".into(), make_node("B", 50.0, 5, 2, 30));
        let shares = compute_all_shares(&contribs);
        assert!(shares["A"] > shares["B"]);
    }

    #[test]
    fn test_emission_distribution() {
        let mut contribs = HashMap::new();
        contribs.insert("A".into(), make_node("A", 100.0, 10, 5, 60));
        contribs.insert("B".into(), make_node("B", 100.0, 10, 5, 60));
        let emission = 100.0 / 60.0; // 1.6667 SOVA/min
        let dist = distribute_emission(&contribs, emission);
        let total: f64 = dist.values().sum();
        assert!((total - emission).abs() < 1e-6);
    }

    #[test]
    fn test_no_tasks_uniform_work_factor() {
        // Quand aucune tâche n'a été soumise, le facteur travail est uniforme
        let mut contribs = HashMap::new();
        contribs.insert("A".into(), make_node("A", 100.0, 0, 0, 60));
        contribs.insert("B".into(), make_node("B", 100.0, 0, 0, 60));
        let shares = compute_all_shares(&contribs);
        assert!((shares["A"] - 0.5).abs() < 1e-9);
    }
}
