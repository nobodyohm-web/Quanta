//! Torus V3 — Web of Trust : PageRank personnalisé.
//!
//! Chaque `Follow` actif (`social.rs`) constitue une arête signée.
//! Le score de confiance vu *depuis l'utilisateur courant* est calculé via
//! un PageRank personnalisé (damping 0,85, max 30 itérations, ε = 1e-6).
//!
//! Avantage anti-Sybil : une ferme de likes externe n'affecte pas le score
//! tant qu'elle n'est pas atteignable depuis le graphe de l'utilisateur.

use std::collections::HashMap;

pub const DEFAULT_DAMPING: f64 = 0.85;
pub const MAX_ITER: usize = 30;
pub const EPSILON: f64 = 1e-6;

/// Représentation simple : `pk → Vec<pk_followed>`.
pub type FollowGraph = HashMap<String, Vec<String>>;

/// PageRank personnalisé partant de `seed_pk`.
/// Renvoie `pk → score` normalisé (la somme = 1.0).
pub fn personalized_pagerank(
    graph: &FollowGraph,
    seed_pk: &str,
    damping: f64,
    max_iter: usize,
    epsilon: f64,
) -> HashMap<String, f64> {
    // Nodes = union(out-edges) ∪ {seed} ∪ {targets ∈ adj}
    let mut nodes: std::collections::HashSet<String> = std::collections::HashSet::new();
    nodes.insert(seed_pk.to_string());
    for (k, vs) in graph {
        nodes.insert(k.clone());
        for v in vs {
            nodes.insert(v.clone());
        }
    }
    let n = nodes.len();
    if n == 0 {
        return HashMap::new();
    }
    let mut score: HashMap<String, f64> =
        nodes.iter().map(|k| (k.clone(), 0.0)).collect();
    score.insert(seed_pk.to_string(), 1.0);

    // Pré-calcul out-degree pour normalisation.
    let out_deg: HashMap<&str, usize> = graph
        .iter()
        .map(|(k, v)| (k.as_str(), v.len()))
        .collect();

    for _ in 0..max_iter {
        let mut next: HashMap<String, f64> = nodes.iter().map(|k| (k.clone(), 0.0)).collect();
        // Téléportation vers seed avec proba (1 - damping).
        next.insert(seed_pk.to_string(), 1.0 - damping);

        let mut dangling = 0.0;
        for (node, &s) in &score {
            let deg = out_deg.get(node.as_str()).copied().unwrap_or(0);
            if deg == 0 {
                dangling += s;
                continue;
            }
            let share = damping * s / deg as f64;
            if let Some(neighbors) = graph.get(node) {
                for nb in neighbors {
                    if let Some(slot) = next.get_mut(nb) {
                        *slot += share;
                    }
                }
            }
        }
        // Dangling nodes (pas d'arête sortante) → reversent vers seed.
        if dangling > 0.0 {
            *next.entry(seed_pk.to_string()).or_insert(0.0) += damping * dangling;
        }

        // Convergence ?
        let diff: f64 = nodes
            .iter()
            .map(|k| (next[k] - score[k]).abs())
            .sum();
        score = next;
        if diff < epsilon {
            break;
        }
    }

    // Normalisation pour somme = 1.
    let total: f64 = score.values().sum();
    if total > 0.0 {
        for v in score.values_mut() {
            *v /= total;
        }
    }
    score
}

/// Renvoie le score de confiance ∈ [0,1] que `viewer` accorde à `target`.
/// Si `target` n'est pas atteignable, renvoie 0.
pub fn trust_score(graph: &FollowGraph, viewer: &str, target: &str) -> f64 {
    let scores = personalized_pagerank(graph, viewer, DEFAULT_DAMPING, MAX_ITER, EPSILON);
    scores.get(target).copied().unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph_of(edges: &[(&str, &[&str])]) -> FollowGraph {
        edges
            .iter()
            .map(|(k, vs)| (k.to_string(), vs.iter().map(|s| s.to_string()).collect()))
            .collect()
    }

    #[test]
    fn isolated_user_self_only() {
        let g = graph_of(&[]);
        let r = personalized_pagerank(&g, "alice", DEFAULT_DAMPING, MAX_ITER, EPSILON);
        assert!(r.contains_key("alice"));
        assert!((r["alice"] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn direct_follow_gives_target_score() {
        let g = graph_of(&[("alice", &["bob"])]);
        let r = personalized_pagerank(&g, "alice", DEFAULT_DAMPING, MAX_ITER, EPSILON);
        assert!(r["bob"] > 0.0);
        // alice doit garder un score (téléportation)
        assert!(r["alice"] > 0.0);
    }

    #[test]
    fn distant_node_lower_score() {
        let g = graph_of(&[
            ("alice", &["bob"]),
            ("bob", &["carol"]),
            ("carol", &["dave"]),
        ]);
        let r = personalized_pagerank(&g, "alice", DEFAULT_DAMPING, MAX_ITER, EPSILON);
        assert!(r["bob"] > r["carol"]);
        assert!(r["carol"] > r["dave"]);
    }

    #[test]
    fn unreachable_node_gets_zero() {
        // Eve n'est suivie par personne dans le graphe d'alice
        let g = graph_of(&[("alice", &["bob"]), ("eve", &["mallory"])]);
        let s = trust_score(&g, "alice", "mallory");
        assert!(s < 1e-6);
    }

    #[test]
    fn cycle_handled() {
        let g = graph_of(&[("alice", &["bob"]), ("bob", &["alice"])]);
        let r = personalized_pagerank(&g, "alice", DEFAULT_DAMPING, MAX_ITER, EPSILON);
        assert!(r["alice"] > 0.0 && r["bob"] > 0.0);
    }

    #[test]
    fn scores_sum_to_one() {
        let g = graph_of(&[("alice", &["bob", "carol"]), ("bob", &["dave"])]);
        let r = personalized_pagerank(&g, "alice", DEFAULT_DAMPING, MAX_ITER, EPSILON);
        let total: f64 = r.values().sum();
        assert!((total - 1.0).abs() < 1e-6);
    }

    /// AUDIT-TRUST-1: a popular node (followed by many people in alice's
    /// neighbourhood) MUST score higher than an isolated node. This is the
    /// core invariant of personalised PageRank as a Web-of-Trust signal.
    #[test]
    fn audit_trust_popular_beats_isolated() {
        // alice → bob, alice → carol; bob and carol both follow `popular`.
        // `isolated` is followed by nobody alice can reach.
        let g = graph_of(&[
            ("alice", &["bob", "carol"]),
            ("bob", &["popular"]),
            ("carol", &["popular"]),
        ]);
        let s_popular = trust_score(&g, "alice", "popular");
        let s_isolated = trust_score(&g, "alice", "isolated");
        assert!(
            s_popular > s_isolated,
            "popular ({}) must score higher than isolated ({})",
            s_popular, s_isolated
        );
        assert!(s_isolated < 1e-6, "unreachable node must score ~0");
    }

    /// AUDIT-TRUST-1: a stronger "fan-in" signal yields a strictly higher
    /// score — `mega_popular` has 4 follows from alice's neighbourhood vs.
    /// `mid` with 1.
    #[test]
    fn audit_trust_more_fans_higher_score() {
        // graph_of uses HashMap insert so duplicate keys clobber each other.
        // Build the graph explicitly to ensure alice has 5 outgoing edges.
        let mut g = FollowGraph::new();
        g.insert("alice".into(), vec!["b1".into(), "b2".into(), "b3".into(), "b4".into(), "c1".into()]);
        g.insert("b1".into(), vec!["mega_popular".into()]);
        g.insert("b2".into(), vec!["mega_popular".into()]);
        g.insert("b3".into(), vec!["mega_popular".into()]);
        g.insert("b4".into(), vec!["mega_popular".into()]);
        g.insert("c1".into(), vec!["mid".into()]);
        let s_mega = trust_score(&g, "alice", "mega_popular");
        let s_mid = trust_score(&g, "alice", "mid");
        assert!(
            s_mega > s_mid,
            "mega_popular ({}) > mid ({}) under multi-fan PageRank",
            s_mega, s_mid
        );
    }
}
