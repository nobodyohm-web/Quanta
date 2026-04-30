#![allow(dead_code)] // Module Phase 3 — pas encore intégré
//! Marketplace de Calcul Distribué — SOVA V2
//!
//! Permet aux utilisateurs de soumettre des tâches de calcul (IA training,
//! rendu 3D, simulation scientifique) et aux nœuds de les exécuter en échange
//! de SOVA tokens.
//!
//! Flux :
//!   1. Client soumet Task { payload, sova_reward, deadline }
//!   2. Le réseau gossipe la tâche
//!   3. Les nœuds disponibles clament la tâche (premier arrivé)
//!   4. Le nœud exécute, soumet le résultat + proof
//!   5. Les validateurs vérifient (ZK proof ou consensus)
//!   6. Le paiement est libéré (reward - burn 2%)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::Utc;

/// Burn rate pour soumission de tâche (2%)
const TASK_BURN_RATE: f64 = 0.02;

/// Statut d'une tâche de calcul
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    /// Soumise, en attente d'exécution
    Pending,
    /// Réclamée par un nœud
    Claimed { worker_id: String, claimed_at: String },
    /// En cours d'exécution
    Running { worker_id: String, progress: f64 },
    /// Résultat soumis, en attente de validation
    Submitted { worker_id: String, result_hash: String },
    /// Validé et payé
    Completed { worker_id: String, result_hash: String },
    /// Échouée (timeout, erreur, résultat invalide)
    Failed { reason: String },
    /// Expirée (deadline dépassée)
    Expired,
}

/// Type de calcul
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskType {
    /// Entraînement ML (BOINC-like)
    MLTraining { model_hash: String, dataset_hash: String },
    /// Rendu 3D
    Render3D { scene_hash: String, resolution: (u32, u32) },
    /// Simulation scientifique (repliement protéines, climat, etc.)
    Scientific { program_hash: String },
    /// Calcul générique (WASM sandboxed)
    GenericWasm { wasm_hash: String },
}

/// Une tâche de calcul distribuée
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeTask {
    /// ID unique (BLAKE3 du contenu)
    pub id: String,
    /// Clé publique du soumetteur
    pub submitter: String,
    /// Type et paramètres du calcul
    pub task_type: TaskType,
    /// Récompense en SOVA (le soumetteur paye)
    pub reward_sova: f64,
    /// Montant brûlé (2% du reward)
    pub burn_amount: f64,
    /// Deadline (RFC3339)
    pub deadline: String,
    /// Statut courant
    pub status: TaskStatus,
    /// Timestamp de création
    pub created_at: String,
    /// Estimation des ressources requises (watts × minutes)
    pub estimated_watt_minutes: f64,
}

/// Résultat soumis par un worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub worker_id: String,
    /// Hash BLAKE3 du résultat
    pub result_hash: String,
    /// Temps d'exécution en secondes
    pub execution_secs: f64,
    /// Watts consommés pendant l'exécution
    pub watts_used: f64,
    /// Preuve ZK (Phase 4: RISC Zero receipt, optionnel pour Phase 2)
    pub zk_proof: Option<Vec<u8>>,
    pub submitted_at: String,
}

/// Marketplace — gère les tâches de calcul
pub struct Marketplace {
    /// Toutes les tâches par ID
    tasks: HashMap<String, ComputeTask>,
    /// Résultats soumis par task_id
    results: HashMap<String, TaskResult>,
    /// Statistiques
    pub stats: MarketplaceStats,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MarketplaceStats {
    pub tasks_submitted: u64,
    pub tasks_completed: u64,
    pub tasks_failed: u64,
    pub total_sova_paid: f64,
    pub total_sova_burned: f64,
    pub total_watt_minutes: f64,
}

impl Marketplace {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            results: HashMap::new(),
            stats: MarketplaceStats::default(),
        }
    }

    /// Soumettre une nouvelle tâche de calcul.
    /// Le soumetteur doit avoir reward_sova + burn sur son solde.
    pub fn submit_task(
        &mut self,
        submitter: &str,
        task_type: TaskType,
        reward_sova: f64,
        deadline: &str,
        estimated_watt_minutes: f64,
    ) -> Result<ComputeTask, String> {
        if reward_sova <= 0.0 {
            return Err("Reward must be positive".into());
        }

        let burn_amount = reward_sova * TASK_BURN_RATE;
        let payload = format!("{}:{}:{:?}:{}", submitter, reward_sova, task_type, deadline);
        let id = hex::encode(blake3::hash(payload.as_bytes()).as_bytes());

        let task = ComputeTask {
            id: id[..32].to_string(),
            submitter: submitter.to_string(),
            task_type,
            reward_sova,
            burn_amount,
            deadline: deadline.to_string(),
            status: TaskStatus::Pending,
            created_at: Utc::now().to_rfc3339(),
            estimated_watt_minutes,
        };

        self.tasks.insert(task.id.clone(), task.clone());
        self.stats.tasks_submitted += 1;
        self.stats.total_sova_burned += burn_amount;
        Ok(task)
    }

    /// Un worker réclame une tâche en attente.
    pub fn claim_task(&mut self, task_id: &str, worker_id: &str) -> Result<(), String> {
        let task = self.tasks.get_mut(task_id).ok_or("Task not found")?;
        match &task.status {
            TaskStatus::Pending => {
                task.status = TaskStatus::Claimed {
                    worker_id: worker_id.to_string(),
                    claimed_at: Utc::now().to_rfc3339(),
                };
                Ok(())
            }
            _ => Err("Task already claimed or completed".into()),
        }
    }

    /// Le worker soumet le résultat d'une tâche.
    pub fn submit_result(&mut self, result: TaskResult) -> Result<(), String> {
        let task = self.tasks.get_mut(&result.task_id).ok_or("Task not found")?;
        
        // Vérifier que c'est bien le worker qui a réclamé la tâche
        match &task.status {
            TaskStatus::Claimed { worker_id, .. } | TaskStatus::Running { worker_id, .. } => {
                if worker_id != &result.worker_id {
                    return Err("Not the assigned worker".into());
                }
            }
            _ => return Err("Task not in claimable state".into()),
        }

        task.status = TaskStatus::Submitted {
            worker_id: result.worker_id.clone(),
            result_hash: result.result_hash.clone(),
        };

        self.results.insert(result.task_id.clone(), result);
        Ok(())
    }

    /// Valide un résultat et libère le paiement.
    /// En Phase 2 : validation simple (le résultat existe).
    /// En Phase 4 : vérification ZK proof via RISC Zero.
    pub fn validate_and_pay(&mut self, task_id: &str) -> Result<(String, f64), String> {
        let task = self.tasks.get_mut(task_id).ok_or("Task not found")?;

        match &task.status {
            TaskStatus::Submitted { worker_id, result_hash } => {
                let worker = worker_id.clone();
                let _hash = result_hash.clone();
                let reward = task.reward_sova - task.burn_amount;

                task.status = TaskStatus::Completed {
                    worker_id: worker.clone(),
                    result_hash: _hash,
                };

                self.stats.tasks_completed += 1;
                self.stats.total_sova_paid += reward;

                // Accumuler les watt-minutes pour le Shapley
                if let Some(result) = self.results.get(task_id) {
                    self.stats.total_watt_minutes += result.watts_used * result.execution_secs / 60.0;
                }

                Ok((worker, reward))
            }
            _ => Err("Task not in submitted state".into()),
        }
    }

    /// Expire les tâches dont la deadline est dépassée.
    pub fn expire_tasks(&mut self) {
        let now = Utc::now().to_rfc3339();
        for task in self.tasks.values_mut() {
            if matches!(task.status, TaskStatus::Pending | TaskStatus::Claimed { .. })
                && task.deadline < now
            {
                task.status = TaskStatus::Expired;
                self.stats.tasks_failed += 1;
            }
        }
    }

    /// Liste les tâches en attente d'exécution.
    pub fn pending_tasks(&self) -> Vec<&ComputeTask> {
        self.tasks.values()
            .filter(|t| matches!(t.status, TaskStatus::Pending))
            .collect()
    }

    /// Tâches complétées par un worker donné.
    pub fn completed_by(&self, worker_id: &str) -> u64 {
        self.tasks.values()
            .filter(|t| matches!(&t.status, TaskStatus::Completed { worker_id: w, .. } if w == worker_id))
            .count() as u64
    }

    pub fn get_task(&self, task_id: &str) -> Option<&ComputeTask> {
        self.tasks.get(task_id)
    }
}

// ─── Sérialization ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceSnapshot {
    pub tasks: Vec<ComputeTask>,
    pub stats: MarketplaceStats,
}

impl Marketplace {
    pub fn snapshot(&self) -> MarketplaceSnapshot {
        MarketplaceSnapshot {
            tasks: self.tasks.values().cloned().collect(),
            stats: self.stats.clone(),
        }
    }

    pub fn restore(snap: MarketplaceSnapshot) -> Self {
        let mut tasks = HashMap::new();
        for t in snap.tasks { tasks.insert(t.id.clone(), t); }
        Self { tasks, results: HashMap::new(), stats: snap.stats }
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_submit_and_claim() {
        let mut mp = Marketplace::new();
        let task = mp.submit_task(
            "submitter_pk", TaskType::Scientific { program_hash: "abc".into() },
            10.0, "2030-01-01T00:00:00Z", 500.0
        ).unwrap();
        assert_eq!(task.burn_amount, 0.2); // 2% of 10
        assert!(matches!(task.status, TaskStatus::Pending));

        mp.claim_task(&task.id, "worker_pk").unwrap();
        let t = mp.get_task(&task.id).unwrap();
        assert!(matches!(&t.status, TaskStatus::Claimed { worker_id, .. } if worker_id == "worker_pk"));
    }

    #[test]
    fn test_full_lifecycle() {
        let mut mp = Marketplace::new();
        let task = mp.submit_task(
            "sub", TaskType::GenericWasm { wasm_hash: "hash".into() },
            100.0, "2030-01-01T00:00:00Z", 1000.0
        ).unwrap();

        mp.claim_task(&task.id, "worker").unwrap();

        let result = TaskResult {
            task_id: task.id.clone(),
            worker_id: "worker".into(),
            result_hash: "result_hash".into(),
            execution_secs: 120.0,
            watts_used: 50.0,
            zk_proof: None,
            submitted_at: Utc::now().to_rfc3339(),
        };
        mp.submit_result(result).unwrap();

        let (worker, reward) = mp.validate_and_pay(&task.id).unwrap();
        assert_eq!(worker, "worker");
        assert!((reward - 98.0).abs() < 0.01); // 100 - 2% burn = 98
        assert_eq!(mp.stats.tasks_completed, 1);
    }

    #[test]
    fn test_wrong_worker_rejected() {
        let mut mp = Marketplace::new();
        let task = mp.submit_task(
            "sub", TaskType::Scientific { program_hash: "x".into() },
            50.0, "2030-01-01T00:00:00Z", 200.0
        ).unwrap();
        mp.claim_task(&task.id, "worker_a").unwrap();

        let result = TaskResult {
            task_id: task.id.clone(),
            worker_id: "worker_b".into(), // wrong worker!
            result_hash: "h".into(),
            execution_secs: 60.0, watts_used: 30.0,
            zk_proof: None, submitted_at: Utc::now().to_rfc3339(),
        };
        assert!(mp.submit_result(result).is_err());
    }
}
