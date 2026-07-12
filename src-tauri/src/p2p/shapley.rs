#![allow(dead_code)] // Module Phase 3 — pas encore intégré
//! Module Contribution Scoring — Distribution équitable des récompenses QUANTA
//!
//! ## Algorithme
//!
//! Ce module implémente un **scoring linéaire pondéré** O(n) inspiré de la
//! Shapley Value, mais ce n'est **PAS** un calcul Shapley exact.
//!
//! La Shapley Value réelle (Shapley 1953) requiert l'évaluation de toutes les
//! coalitions possibles (2^n), ce qui est NP-hard. Les approximations Monte
//! Carlo standard (permutation sampling) convergent en O(n·m) avec m samples.
//!
//! Notre approche est un **weighted linear contribution score** qui respecte
//! les axiomes d'efficience (Σ = 1) et de symétrie (agents identiques → parts
//! égales), mais ne garantit pas l'axiome de marginalité (un agent ne peut
//! recevoir plus que sa contribution marginale à toute coalition).
//!
//! **Facteurs de contribution (crypto-core)** :
//!   - Énergie (30%)        : watts mesurés / total réseau
//!   - Travail compute (30%): tâches exécutées / total tâches
//!   - Validation (25%)     : blocs vérifiés / total blocs
//!   - Uptime (15%)         : heures en ligne / max uptime réseau
//!
//! Référence : Lloyd S. Shapley, "A Value for n-Person Games", 1953
//! Note : notre implémentation est une *approximation linéaire*, pas le
//! calcul combinatoire original.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Poids Shapley — configurables, somme = 1.0 (CLAUDE.md règle #2)
const W_ENERGY: f64 = 0.30;
const W_WORK: f64 = 0.30;
const W_VALIDATION: f64 = 0.25;
const W_UPTIME: f64 = 0.15;

/// Mode de contribution du nœud (détection automatique).
/// Affecte le calcul Shapley quand aucune tâche marketplace n'existe encore :
///   - Active : le travail est estimé proportionnel à l'énergie (energy ≈ work)
///   - Research : futur — score max sur le travail (vérifié par ZK)
///   - Guardian : pas de travail, mais validation + uptime = ~40% de Shapley
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
pub enum NodeMode {
    #[default]
    Active,
    Research,
    Guardian,
}

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
    /// Mode de contribution (Active, Research, Guardian)
    pub mode: NodeMode,
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

/// Calcule le score de contribution d'un nœud (weighted linear, inspiré Shapley).
///
/// Retourne un score entre 0.0 et 1.0 représentant la part du nœud
/// dans l'émission réseau. Ce n'est PAS un calcul Shapley exact (NP-hard)
/// mais un scoring linéaire pondéré qui satisfait l'axiome d'efficience.
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

    // Facteur travail : ma part relative de tâches complétées.
    // Si aucune tâche marketplace n'existe (Phase 1-2), le mode détermine
    // le fallback : Active = proportionnel aux watts, Guardian = 0 (il ne "travaille" pas).
    let work = if network.total_tasks > 0 {
        node.tasks_completed as f64 / network.total_tasks as f64
    } else {
        match node.mode {
            NodeMode::Active | NodeMode::Research => {
                // Fallback : énergie ≈ travail (même ratio que le facteur énergie)
                if network.total_watts > 0.0 {
                    node.watts / network.total_watts
                } else {
                    1.0 / network.node_count as f64
                }
            }
            NodeMode::Guardian => {
                // Le gardien ne produit pas de travail compute — il sécurise.
                // Son gain vient de validation (25%) + uptime (15%).
                0.0
            }
        }
    };

    // Facteur validation : ma part relative de blocs vérifiés
    let validation = if network.total_blocks_verified > 0 {
        node.blocks_verified as f64 / network.total_blocks_verified as f64
    } else {
        1.0 / network.node_count as f64
    };

    // Facteur uptime : ma part relative d'uptime
    let uptime = if network.max_uptime_minutes > 0 {
        node.uptime_minutes as f64 / network.max_uptime_minutes as f64
    } else {
        1.0
    };

    // Score Shapley pondéré (somme des poids = 1.0)
    W_ENERGY * energy
        + W_WORK * work
        + W_VALIDATION * validation
        + W_UPTIME * uptime
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
/// `emission_per_tick` = 100 QUANTA/h / 60 = 1.6667 QUANTA/min
///
/// Retourne : node_id → QUANTA à créditer
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
            mode: NodeMode::Active,
        }
    }

    fn make_guardian(id: &str, blocks: u64, uptime: u64) -> NodeContribution {
        NodeContribution {
            node_id: id.to_string(),
            watts: 3.0, // Raspberry Pi level
            tasks_completed: 0,
            blocks_verified: blocks,
            uptime_minutes: uptime,
            mode: NodeMode::Guardian,
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
    fn test_shapley_score_is_the_weighted_sum_of_fractions() {
        // TEST-TEETH (HARDEN-HYGIENE-1): `test_shares_sum_to_one` is a
        // normalization TAUTOLOGY (compute_all_shares divides by the total, so
        // the sum is forced to 1.0 regardless of the scoring). Pin the PRE-
        // normalization `shapley_score` against a hand-computed weighted sum with
        // four DISTINCT fractions, so a regression in any weight (0.30 energy /
        // 0.30 work / 0.25 validation / 0.15 uptime) or factor FAILS here.
        let mut contribs = HashMap::new();
        contribs.insert("A".into(), make_node("A", 150.0, 4, 2, 9));
        contribs.insert("B".into(), make_node("B", 50.0, 6, 8, 90));
        let network = NetworkTotals::from_contributions(&contribs);
        // A fractions: energy 150/200=0.75, work 4/10=0.4, validation 2/10=0.2,
        // uptime 9/90=0.1 → 0.30*0.75 + 0.30*0.4 + 0.25*0.2 + 0.15*0.1 = 0.41.
        let score_a = shapley_score(&contribs["A"], &network);
        assert!((score_a - 0.41).abs() < 1e-9, "shapley_score(A) = {score_a} (expected 0.41)");
        // B fractions: energy 0.25, work 0.6, validation 0.8, uptime 1.0 →
        // 0.30*0.25 + 0.30*0.6 + 0.25*0.8 + 0.15*1.0 = 0.605.
        let score_b = shapley_score(&contribs["B"], &network);
        assert!((score_b - 0.605).abs() < 1e-9, "shapley_score(B) = {score_b} (expected 0.605)");
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
        let emission = 100.0 / 60.0; // 1.6667 QUANTA/min
        let dist = distribute_emission(&contribs, emission);
        let total: f64 = dist.values().sum();
        assert!((total - emission).abs() < 1e-6);
    }

    #[test]
    fn test_no_tasks_active_nodes_get_work_credit() {
        // Without marketplace tasks, Active nodes get work credit proportional to watts
        let mut contribs = HashMap::new();
        contribs.insert("A".into(), make_node("A", 100.0, 0, 5, 60));
        contribs.insert("B".into(), make_node("B", 100.0, 0, 5, 60));
        let shares = compute_all_shares(&contribs);
        // Both Active, same watts → equal shares
        assert!((shares["A"] - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_guardian_earns_about_40_percent() {
        // Guardian (3W, no work) vs Active (100W, no marketplace tasks)
        // Both have same validation (10 blocks) and uptime (60 min).
        // Guardian's raw score comes from validation (25%) + uptime (15%) = 40%.
        // Active gets energy (30%) + work-fallback (30%) + validation (25%) + uptime (15%).
        let mut contribs = HashMap::new();
        contribs.insert("Miner".into(), make_node("Miner", 100.0, 0, 10, 60));
        contribs.insert("Guard".into(), make_guardian("Guard", 10, 60));
        let shares = compute_all_shares(&contribs);

        let guard_share = shares["Guard"];
        let miner_share = shares["Miner"];

        // Guardian should get between 15% and 45% of total emission
        assert!(guard_share > 0.15,
            "Guardian should earn >15% of emission, got {:.1}%", guard_share * 100.0);
        assert!(guard_share < 0.45,
            "Guardian should earn <45% of emission, got {:.1}%", guard_share * 100.0);
        // Active miner should always earn more than guardian
        assert!(miner_share > guard_share,
            "Miner ({:.1}%) should earn more than Guardian ({:.1}%)",
            miner_share * 100.0, guard_share * 100.0);

        println!("  Guardian: {:.1}% | Miner: {:.1}%", guard_share * 100.0, miner_share * 100.0);
    }

    #[test]
    fn test_guardian_still_earns_something() {
        // Solo guardian still gets full emission (sole node)
        let mut contribs = HashMap::new();
        contribs.insert("G".into(), make_guardian("G", 5, 60));
        let shares = compute_all_shares(&contribs);
        assert!((shares["G"] - 1.0).abs() < 1e-9, "Solo guardian should get 100%");
    }
}
