//! Merkle-DAG — Structure de données pour le consensus distribué
//!
//! Basé sur : Martin Kleppmann, "Making CRDTs Byzantine Fault Tolerant" (2022)
//!            Hector Sanjuan, "Merkle-CRDTs" (Protocol Labs, 2020)
//!
//! Propriétés :
//!   - Content-addressed : chaque nœud est identifié par son hash BLAKE3
//!   - Tolérant aux forks  : le DAG accepte toutes les branches valides
//!   - Delta sync          : seuls les nœuds manquants sont échangés
//!   - Pas de leader       : aucun coordinateur central requis

use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};
use chrono::Utc;

// ─── Types ──────────────────────────────────────────────────────────────────

/// Un nœud dans le Merkle-DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNode {
    /// Hash BLAKE3 de (parents + payload + author + timestamp)
    pub id: String,
    /// Hashes des nœuds parents (vide = racine/genèse)
    pub parents: Vec<String>,
    /// Données sérialisées (transaction, état, etc.)
    pub payload: Vec<u8>,
    /// Clé publique Ed25519 (hex) de l'auteur
    pub author: String,
    /// Horodatage RFC3339
    pub timestamp: String,
    /// Signature Ed25519 de (parents || payload)
    pub signature: String,
}

impl DagNode {
    /// Calcule l'identifiant BLAKE3 du nœud.
    pub fn compute_id(parents: &[String], payload: &[u8], author: &str, timestamp: &str) -> String {
        let mut hasher = blake3::Hasher::new();
        for p in parents { hasher.update(p.as_bytes()); }
        hasher.update(payload);
        hasher.update(author.as_bytes());
        hasher.update(timestamp.as_bytes());
        hex::encode(hasher.finalize().as_bytes())
    }

    /// Crée un nouveau nœud non signé (signature à compléter par l'appelant).
    pub fn new(parents: Vec<String>, payload: Vec<u8>, author: String) -> Self {
        let timestamp = Utc::now().to_rfc3339();
        let id = Self::compute_id(&parents, &payload, &author, &timestamp);
        Self { id, parents, payload, author, timestamp, signature: String::new() }
    }
}

// ─── MerkleDAG ──────────────────────────────────────────────────────────────

/// Store Merkle-DAG — tolérant aux forks, déterministe, offline-first.
pub struct MerkleDAG {
    /// id → nœud
    nodes: HashMap<String, DagNode>,
    /// Nœuds sans enfants (frontier) — utilisés pour le sync P2P
    heads: HashSet<String>,
}

impl MerkleDAG {
    pub fn new() -> Self {
        Self { nodes: HashMap::new(), heads: HashSet::new() }
    }

    /// Insère un nœud dans le DAG.
    ///
    /// Vérifie que tous les parents sont présents (protection orphelin).
    /// Le nœud genèse ("genesis") est toujours accepté sans parents.
    pub fn insert(&mut self, node: DagNode) -> Result<(), String> {
        // Vérification des parents (sauf genèse)
        for parent in &node.parents {
            if parent != "genesis" && !self.nodes.contains_key(parent) {
                return Err(format!(
                    "Parent {} introuvable — sync incomplet",
                    &parent[..parent.len().min(12)]
                ));
            }
        }

        // Vérification d'intégrité BLAKE3
        let expected = DagNode::compute_id(&node.parents, &node.payload, &node.author, &node.timestamp);
        if node.id != expected {
            return Err("Hash BLAKE3 invalide — nœud corrompu".into());
        }

        // Mise à jour des heads : les parents ne sont plus au frontier
        for parent in &node.parents {
            self.heads.remove(parent);
        }
        self.heads.insert(node.id.clone());
        self.nodes.insert(node.id.clone(), node);
        Ok(())
    }

    /// Récupère un nœud par son identifiant.
    pub fn get(&self, id: &str) -> Option<&DagNode> {
        self.nodes.get(id)
    }

    /// Retourne les nœuds que le pair `known` ne possède pas (delta sync).
    pub fn missing_from<'a>(&'a self, known: &HashSet<String>) -> Vec<&'a DagNode> {
        self.nodes.values().filter(|n| !known.contains(&n.id)).collect()
    }

    /// Frontier courant — utilisé comme "témoins" pour le sync P2P.
    pub fn heads(&self) -> Vec<String> {
        self.heads.iter().cloned().collect()
    }

    /// Ensemble de tous les identifiants connus.
    pub fn known_ids(&self) -> HashSet<String> {
        self.nodes.keys().cloned().collect()
    }

    pub fn node_count(&self) -> usize { self.nodes.len() }
    pub fn head_count(&self) -> usize { self.heads.len() }
}

impl Default for MerkleDAG {
    fn default() -> Self { Self::new() }
}

// ─── Snapshot ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagSnapshot {
    pub nodes: Vec<DagNode>,
}

impl MerkleDAG {
    pub fn snapshot(&self) -> DagSnapshot {
        DagSnapshot { nodes: self.nodes.values().cloned().collect() }
    }

    pub fn restore(snap: DagSnapshot) -> Self {
        let mut dag = Self::new();
        // Trier par nombre de parents pour insérer dans le bon ordre (racines en premier)
        let mut sorted = snap.nodes;
        sorted.sort_by_key(|n| n.parents.len());
        for node in sorted {
            // En restauration, on ignore les erreurs de parent manquant
            // (le snapshot peut être partiel)
            let _ = dag.nodes.insert(node.id.clone(), node.clone());
            dag.heads.insert(node.id.clone());
        }
        // Recalculer les heads (supprimer les non-heads)
        let all_parents: HashSet<String> = dag.nodes.values()
            .flat_map(|n| n.parents.iter().cloned())
            .collect();
        dag.heads.retain(|id| !all_parents.contains(id));
        dag
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_heads() {
        let mut dag = MerkleDAG::new();

        let n1 = DagNode::new(vec!["genesis".into()], b"block1".to_vec(), "pk_a".into());
        let n2 = DagNode::new(vec!["genesis".into()], b"block2".to_vec(), "pk_b".into());
        let id1 = n1.id.clone();
        let id2 = n2.id.clone();

        dag.insert(n1).unwrap();
        dag.insert(n2).unwrap();

        assert_eq!(dag.node_count(), 2);
        assert_eq!(dag.head_count(), 2, "Deux branches indépendantes");

        // Merge block referencing both heads
        let n3 = DagNode::new(vec![id1.clone(), id2.clone()], b"merge".to_vec(), "pk_a".into());
        dag.insert(n3).unwrap();

        assert_eq!(dag.head_count(), 1, "Après merge, un seul head");
    }

    #[test]
    fn reject_orphan() {
        let mut dag = MerkleDAG::new();
        let n = DagNode::new(vec!["nonexistent_parent".into()], b"orphan".to_vec(), "pk".into());
        assert!(dag.insert(n).is_err(), "Nœud orphelin doit être rejeté");
    }
}
