//! Moteur de consensus CRDT pour SOVA
//!
//! Utilise des CRDTs (Conflict-free Replicated Data Types) pour garantir
//! la convergence déterministe entre nœuds sans leader ni votes.
//!
//! Types CRDT utilisés :
//!   - `PNCounter<String>` — balances SOVA (positif/négatif, entier millième)
//!
//! Principe de fusion : merge(A, B) = union(max per actor) — idempotent, commutatif.

use crdts::{CmRDT, CvRDT, PNCounter};
use num_traits::ToPrimitive as _;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Ledger CRDT ────────────────────────────────────────────────────────────

/// Registre de balances ATN convergent basé sur PN-Counter.
///
/// Convention : 1 unité = 1 milliATN (pour éviter l'arithmétique flottante).
/// Fusion entre nœuds : max par acteur par direction (convergent).
pub struct CrdtLedger {
    /// user_pk → PN-Counter (crédits - débits en milliATN)
    balances: HashMap<String, PNCounter<String>>,
}

impl CrdtLedger {
    pub fn new() -> Self {
        Self { balances: HashMap::new() }
    }

    /// Crédite `milliATN` au destinataire, émis par `actor`.
    pub fn credit(&mut self, actor: &str, recipient: &str, milli_atn: u64) {
        let counter = self.balances
            .entry(recipient.to_string())
            .or_default();
        for _ in 0..milli_atn {
            let op = counter.inc(actor.to_string());
            counter.apply(op);
        }
    }

    /// Débite `milli_atn` de l'émetteur, opération signée par `actor`.
    pub fn debit(&mut self, actor: &str, sender: &str, milli_atn: u64) {
        let counter = self.balances
            .entry(sender.to_string())
            .or_default();
        for _ in 0..milli_atn {
            let op = counter.dec(actor.to_string());
            counter.apply(op);
        }
    }

    /// Balance en ATN (plancher à 0 — pas de balance négative).
    pub fn balance_of(&self, pk: &str) -> f64 {
        self.balances
            .get(pk)
            .map(|c| c.read().to_i64().unwrap_or(0).max(0) as f64 / 1000.0)
            .unwrap_or(0.0)
    }

    /// Fusionne deux ledgers CRDT — commmutatif, associatif, idempotent.
    pub fn merge(&mut self, other: &Self) {
        for (pk, other_c) in &other.balances {
            let entry = self.balances
                .entry(pk.clone())
                .or_default();
            entry.merge(other_c.clone());
        }
    }

    /// Nombre de comptes connus par le CRDT (pour les stats frontend).
    pub fn account_count(&self) -> usize {
        self.balances.len()
    }
}

impl Default for CrdtLedger {
    fn default() -> Self { Self::new() }
}

// ─── ConsensusEngine ────────────────────────────────────────────────────────

/// Moteur de consensus global — CRDT ledger uniquement (V2: plus de likes/vues).
pub struct ConsensusEngine {
    pub ledger: CrdtLedger,
}

impl ConsensusEngine {
    pub fn new() -> Self {
        Self { ledger: CrdtLedger::new() }
    }

    /// Fusionne l'état d'un pair distant dans l'état local.
    /// Appel idempotent : merger deux fois le même pair n'a aucun effet.
    pub fn merge_peer(&mut self, peer_ledger: CrdtLedger) {
        self.ledger.merge(&peer_ledger);
    }
}

impl Default for ConsensusEngine {
    fn default() -> Self { Self::new() }
}

// ─── Snapshot sérialisable / persistance disque ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrdtLedgerSnapshot {
    pub balances: HashMap<String, PNCounter<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusSnapshot {
    pub ledger: CrdtLedgerSnapshot,
}

impl CrdtLedger {
    pub fn snapshot(&self) -> CrdtLedgerSnapshot {
        CrdtLedgerSnapshot { balances: self.balances.clone() }
    }
    pub fn restore(snap: CrdtLedgerSnapshot) -> Self {
        Self { balances: snap.balances }
    }
}

impl ConsensusEngine {
    pub fn snapshot(&self) -> ConsensusSnapshot {
        ConsensusSnapshot {
            ledger: self.ledger.snapshot(),
        }
    }
    pub fn restore(snap: ConsensusSnapshot) -> Self {
        Self {
            ledger: CrdtLedger::restore(snap.ledger),
        }
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credit_debit_balance() {
        let mut ledger = CrdtLedger::new();
        ledger.credit("node_a", "user_1", 5_000);
        assert!((ledger.balance_of("user_1") - 5.0).abs() < 0.001);
        ledger.debit("node_a", "user_1", 2_000);
        assert!((ledger.balance_of("user_1") - 3.0).abs() < 0.001);
    }

    #[test]
    fn merge_is_idempotent() {
        let mut a = CrdtLedger::new();
        let mut b = CrdtLedger::new();
        a.credit("node_a", "alice", 1_000);
        b.credit("node_b", "alice", 500);
        a.merge(&b);
        let bal1 = a.balance_of("alice");
        a.merge(&b);
        let bal2 = a.balance_of("alice");
        assert!((bal1 - bal2).abs() < 0.001, "Merge doit être idempotent");
    }

    #[test]
    fn account_count_grows_with_transfers() {
        let mut ledger = CrdtLedger::new();
        assert_eq!(ledger.account_count(), 0);
        ledger.credit("node_a", "alice", 1_000);
        ledger.debit("node_a", "alice", 200);
        ledger.credit("node_a", "bob", 500);
        assert_eq!(ledger.account_count(), 2, "alice + bob");
    }
}
