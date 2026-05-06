#![allow(dead_code)] // Module Phase 3 — pas encore intégré
//! Marketplace de Calcul Distribué — QUANTA V2
//!
//! Permet aux utilisateurs de soumettre des tâches de calcul (IA training,
//! rendu 3D, simulation scientifique) et aux nœuds de les exécuter en échange
//! de QUANTA tokens.
//!
//! Flux :
//!   1. Client soumet Task { payload, quanta_reward, deadline }
//!   2. Le réseau gossipe la tâche
//!   3. Les nœuds disponibles clament la tâche (premier arrivé)
//!   4. Le nœud exécute, soumet le résultat + proof
//!   5. Les validateurs vérifient (ZK proof ou consensus)
//!   6. Le paiement est libéré (reward - burn 2%)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::Utc;
use super::ledger::MICRO;

/// Burn rate pour soumission de tâche (2% — calculé en intégers : amount * 2 / 100).
const TASK_BURN_NUM: u64 = 2;
const TASK_BURN_DEN: u64 = 100;

#[inline]
fn compute_task_burn(reward_uqta: u64) -> u64 {
    reward_uqta * TASK_BURN_NUM / TASK_BURN_DEN
}

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

/// Une tâche de calcul distribuée. Montants en µQTA.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeTask {
    /// ID unique (BLAKE3 du contenu)
    pub id: String,
    /// Clé publique du soumetteur
    pub submitter: String,
    /// Type et paramètres du calcul
    pub task_type: TaskType,
    /// Récompense en µQTA (le soumetteur paye)
    pub reward_qta: u64,
    /// Montant brûlé en µQTA (2% du reward)
    pub burn_amount: u64,
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

/// Stats marketplace. Montants monétaires en µQTA.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MarketplaceStats {
    pub tasks_submitted: u64,
    pub tasks_completed: u64,
    pub tasks_failed: u64,
    /// µQTA payés cumulés
    pub total_qta_paid: u64,
    /// µQTA brûlés cumulés
    pub total_qta_burned: u64,
    pub total_watt_minutes: f64,
    /// CRIT-2: µQTA currently locked in escrow (not yet paid or refunded)
    pub escrow_locked: u64,
}

impl Marketplace {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            results: HashMap::new(),
            stats: MarketplaceStats::default(),
        }
    }

    /// Soumettre une nouvelle tâche de calcul (reward QUANTA).
    /// Le soumetteur doit avoir reward_qta + burn sur son solde.
    pub fn submit_task(
        &mut self,
        submitter: &str,
        task_type: TaskType,
        reward_qta: u64,
        deadline: &str,
        estimated_watt_minutes: f64,
    ) -> Result<ComputeTask, String> {
        if reward_qta == 0 {
            return Err("Reward must be positive".into());
        }

        let burn_amount = compute_task_burn(reward_qta);
        let payload = format!("{}:{}:{:?}:{}", submitter, reward_qta, task_type, deadline);
        let id = hex::encode(blake3::hash(payload.as_bytes()).as_bytes());

        let task = ComputeTask {
            id: id[..32].to_string(),
            submitter: submitter.to_string(),
            task_type,
            reward_qta,
            burn_amount,
            deadline: deadline.to_string(),
            status: TaskStatus::Pending,
            created_at: Utc::now().to_rfc3339(),
            estimated_watt_minutes,
        };

        self.tasks.insert(task.id.clone(), task.clone());
        self.stats.tasks_submitted += 1;
        self.stats.total_qta_burned += burn_amount;
        Ok(task)
    }

    /// B2 — Submit a task WITH balance verification and escrow locking.
    ///
    /// This is the secure version of `submit_task()` that prevents submitting
    /// tasks without sufficient funds.
    ///
    /// Flow:
    /// 1. Verify submitter has balance ≥ reward + burn (2%)
    /// 2. Lock funds by moving them to ESCROW account in the ledger
    /// 3. Register the task in the marketplace
    pub fn submit_task_with_escrow(
        &mut self,
        ledger: &mut super::ledger::Ledger,
        submitter: &str,
        task_type: TaskType,
        reward_qta: u64,
        deadline: &str,
        estimated_watt_minutes: f64,
    ) -> Result<ComputeTask, String> {
        if reward_qta == 0 {
            return Err("Reward must be positive".into());
        }

        let burn_amount = compute_task_burn(reward_qta);
        let total_cost = reward_qta + burn_amount; // reward to worker + 2% burn

        // B2: Check balance BEFORE creating the task
        let balance = ledger.balance_of(submitter);
        if balance < total_cost {
            return Err(format!(
                "Insufficient balance for task: need {:.6} QUANTA (reward {:.6} + burn {:.6}), have {:.6}",
                total_cost as f64 / MICRO as f64,
                reward_qta as f64 / MICRO as f64,
                burn_amount as f64 / MICRO as f64,
                balance as f64 / MICRO as f64,
            ));
        }

        // CRIT-2 fix: lock the submitter's balance into ESCROW.
        let _escrow_tx = ledger.build_escrow_lock_tx(submitter, total_cost);
        self.stats.escrow_locked += total_cost;

        // Create the task (delegates to internal submit_task)
        self.submit_task(submitter, task_type, reward_qta, deadline, estimated_watt_minutes)
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

    /// Valide un résultat et libère le paiement depuis l'escrow.
    /// En Phase 2 : validation simple (le résultat existe).
    /// En Phase 4 : vérification ZK proof via RISC Zero.
    /// CRIT-2 fix: now takes a mutable ledger to credit the worker from escrow.
    pub fn validate_and_pay(
        &mut self,
        task_id: &str,
        ledger: &mut super::ledger::Ledger,
    ) -> Result<(String, u64), String> {
        let task = self.tasks.get_mut(task_id).ok_or("Task not found")?;

        match &task.status {
            TaskStatus::Submitted { worker_id, result_hash } => {
                let worker = worker_id.clone();
                let _hash = result_hash.clone();
                // task.burn_amount is already 2% of reward_qta; net = reward - burn
                let net_reward = task.reward_qta - task.burn_amount;

                // CRIT-2: Release escrow funds to worker via ledger
                ledger.escrow_release_to(&worker, net_reward);
                // The burn_amount stays burned (already debited from submitter via ESCROW)

                task.status = TaskStatus::Completed {
                    worker_id: worker.clone(),
                    result_hash: _hash,
                };

                self.stats.tasks_completed += 1;
                self.stats.total_qta_paid += net_reward;
                self.stats.escrow_locked = self.stats.escrow_locked
                    .saturating_sub(task.reward_qta + task.burn_amount);

                // Accumuler les watt-minutes pour le Shapley
                if let Some(result) = self.results.get(task_id) {
                    self.stats.total_watt_minutes += result.watts_used * result.execution_secs / 60.0;
                }

                Ok((worker, net_reward))
            }
            _ => Err("Task not in submitted state".into()),
        }
    }

    /// Expire les tâches dont la deadline est dépassée.
    /// CRIT-2 fix: now takes a ledger to refund escrowed funds to the submitter.
    pub fn expire_tasks(&mut self, ledger: &mut super::ledger::Ledger) {
        let now = Utc::now().to_rfc3339();
        for task in self.tasks.values_mut() {
            if matches!(task.status, TaskStatus::Pending | TaskStatus::Claimed { .. })
                && task.deadline < now
            {
                // CRIT-2: Refund escrowed funds to submitter on expiry
                let total_cost = task.reward_qta + task.burn_amount;
                ledger.escrow_release_to(&task.submitter, total_cost);
                self.stats.escrow_locked = self.stats.escrow_locked.saturating_sub(total_cost);

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
        // 10 QUANTA reward QUANTA
        let task = mp.submit_task(
            "submitter_pk", TaskType::Scientific { program_hash: "abc".into() },
            10 * MICRO, "2030-01-01T00:00:00Z", 500.0
        ).unwrap();
        assert_eq!(task.burn_amount, MICRO / 5); // 2% of 10 QUANTA = 0.2 QUANTA = 200_000 µQTA
        assert!(matches!(task.status, TaskStatus::Pending));

        mp.claim_task(&task.id, "worker_pk").unwrap();
        let t = mp.get_task(&task.id).unwrap();
        assert!(matches!(&t.status, TaskStatus::Claimed { worker_id, .. } if worker_id == "worker_pk"));
    }

    #[test]
    fn test_full_lifecycle() {
        let mut mp = Marketplace::new();
        let mut ledger = super::super::ledger::Ledger::new();
        // 100 QUANTA reward
        let task = mp.submit_task(
            "sub", TaskType::GenericWasm { wasm_hash: "hash".into() },
            100 * MICRO, "2030-01-01T00:00:00Z", 1000.0
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

        let (worker, reward) = mp.validate_and_pay(&task.id, &mut ledger).unwrap();
        assert_eq!(worker, "worker");
        // 100 QUANTA - 2% burn = 98 QUANTA = 98 * MICRO µQTA
        assert_eq!(reward, 98 * MICRO);
        assert_eq!(mp.stats.tasks_completed, 1);
    }

    #[test]
    fn test_wrong_worker_rejected() {
        let mut mp = Marketplace::new();
        let task = mp.submit_task(
            "sub", TaskType::Scientific { program_hash: "x".into() },
            50 * MICRO, "2030-01-01T00:00:00Z", 200.0
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
