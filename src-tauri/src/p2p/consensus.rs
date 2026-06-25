//! Moteur de consensus CRDT pour QUANTA
//!
//! Utilise des CRDTs (Conflict-free Replicated Data Types) pour garantir
//! la convergence déterministe entre nœuds sans leader ni votes.
//!
//! Types CRDT utilisés :
//!   - `PNCounter<String>` — balances QUANTA en µQTA (entier, déterministe).
//!
//! STRUCT-2: Le CRDT travaille en µQTA (1 QUANTA = 1_000_000 µQTA) pour s'aligner
//! sur le ledger linéaire et éliminer la dérive f64.
//!
//! Principe de fusion : merge(A, B) = union(max per actor) — idempotent, commutatif.

use crate::p2p::ledger::short;
use crdts::{CmRDT, CvRDT, PNCounter};
use num_traits::ToPrimitive as _;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// CRDT-BOUND-1: hard cap on the number of accounts tracked by the (non-
/// authoritative) CRDT shadow ledger, bounding memory under a flood of distinct
/// addresses from gossip. The kept set is the lexicographically-smallest
/// `MAX_CRDT_ACCOUNTS` keys — an **order-independent**, content-deterministic
/// policy, so the bounded map never becomes an order-dependent counter.
///
/// **§4 policy**: the exact value is a choice. **§4 caveat**: eviction discards a
/// PN-Counter's state, which a CRDT *merge* cannot recover — this is safe TODAY
/// because `ConsensusEngine::merge_peer` is **never called in production** (the
/// CRDT is local-only; balances are write-only, never read). If cross-node CRDT
/// sync is ever revived, a hard size bound on a merging PN-Counter map is a real
/// design decision to revisit (Alexandre).
const MAX_CRDT_ACCOUNTS: usize = 100_000;

// ─── Ledger CRDT ────────────────────────────────────────────────────────────

/// Registre de balances QUANTA convergent basé sur PN-Counter.
///
/// Convention : 1 unité = 1 µQTA (microQTA, déterministe, pas d'arithmétique flottante).
/// Fusion entre nœuds : max par acteur par direction (convergent).
pub struct CrdtLedger {
    /// user_pk → PN-Counter (crédits - débits en µQTA)
    balances: HashMap<String, PNCounter<String>>,
}

impl CrdtLedger {
    pub fn new() -> Self {
        Self { balances: HashMap::new() }
    }

    /// STRUCT-5: Maximum µQTA per single CRDT operation to prevent O(n) DoS.
    /// 10_000_000 µQTA = 10 QUANTA — large transfers should be split into batches.
    const MAX_CRDT_BATCH: u64 = 10_000_000;

    /// Crédite `uqta` au destinataire, émis par `actor`.
    /// STRUCT-5: Capped at MAX_CRDT_BATCH to prevent DoS from large values.
    pub fn credit(&mut self, actor: &str, recipient: &str, uqta: u64) {
        let clamped = uqta.min(Self::MAX_CRDT_BATCH);
        if uqta > Self::MAX_CRDT_BATCH {
            log::warn!(
                "◈ [CRDT] credit capped: {} → {} µQTA for {}",
                uqta, clamped, short(recipient, 12)
            );
        }
        // CRDT-BOUND-1: bounded admission (memory) — track only if `recipient`
        // is in the kept set; otherwise this is a no-op (balances are non-
        // authoritative and never read for their value).
        let Some(counter) = self.admit(recipient) else {
            return;
        };
        // CRDT-BOUND-1: O(1) bulk increment. Was `for _ in 0..clamped` — up to
        // 10M iterations per tx under the consensus write lock (CPU DoS).
        // `inc_many` yields the SAME Op as `clamped` successive `inc()` calls, so
        // `merge()` semantics are unchanged.
        let op = counter.inc_many(actor.to_string(), clamped);
        counter.apply(op);
    }

    /// Débite `uqta` de l'émetteur, opération signée par `actor`.
    /// STRUCT-5: Capped at MAX_CRDT_BATCH to prevent DoS from large values.
    pub fn debit(&mut self, actor: &str, sender: &str, uqta: u64) {
        let clamped = uqta.min(Self::MAX_CRDT_BATCH);
        if uqta > Self::MAX_CRDT_BATCH {
            log::warn!(
                "◈ [CRDT] debit capped: {} → {} µQTA for {}",
                uqta, clamped, short(sender, 12)
            );
        }
        // CRDT-BOUND-1: bounded admission (see `credit`).
        let Some(counter) = self.admit(sender) else {
            return;
        };
        // CRDT-BOUND-1: O(1) bulk decrement (was `for _ in 0..clamped`).
        let op = counter.dec_many(actor.to_string(), clamped);
        counter.apply(op);
    }

    /// CRDT-BOUND-1: bound the balances map. Returns the PN-Counter for `addr` to
    /// mutate, or `None` if the map is full and `addr` does not rank among the
    /// kept accounts. KEPT SET = the `MAX_CRDT_ACCOUNTS` accounts with the
    /// lexicographically-SMALLEST key — a content-deterministic, **order-
    /// independent** policy (same kept set on every node regardless of arrival
    /// order; it never becomes an order-dependent counter, the `@pseudo`-cap
    /// constraint). `merge()` itself is unchanged; see the `MAX_CRDT_ACCOUNTS`
    /// §4 caveat about reviving cross-node merge.
    fn admit(&mut self, addr: &str) -> Option<&mut PNCounter<String>> {
        if !self.balances.contains_key(addr) && self.balances.len() >= MAX_CRDT_ACCOUNTS {
            // New address, map full → keep the lexicographically-smaller one.
            match self.balances.keys().max().cloned() {
                Some(max_key) if addr < max_key.as_str() => {
                    self.balances.remove(&max_key);
                }
                _ => return None, // addr ranks outside the kept set → don't track
            }
        }
        Some(self.balances.entry(addr.to_string()).or_default())
    }

    /// Balance en µQTA (plancher à 0 — pas de balance négative).
    pub fn balance_of(&self, pk: &str) -> u64 {
        self.balances
            .get(pk)
            .map(|c| c.read().to_i64().unwrap_or(0).max(0) as u64)
            .unwrap_or(0)
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
        // 5_000 µQTA = 0.0.5 QUANTA
        ledger.credit("node_a", "user_1", 5_000);
        assert_eq!(ledger.balance_of("user_1"), 5_000);
        ledger.debit("node_a", "user_1", 2_000);
        assert_eq!(ledger.balance_of("user_1"), 3_000);
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
        assert_eq!(bal1, bal2, "Merge doit être idempotent");
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

    #[test]
    fn crdt_large_amount_is_o1_no_loop_dos() {
        // §5: a huge µQTA value must NOT loop ~10M times under the lock —
        // credit/debit are O(1) now (inc_many/dec_many). The test completing
        // instantly is the proof; the balance is the clamped value.
        let mut ledger = CrdtLedger::new();
        ledger.credit("net", "whale", u64::MAX);
        assert_eq!(
            ledger.balance_of("whale"),
            CrdtLedger::MAX_CRDT_BATCH,
            "credit clamped to MAX_CRDT_BATCH and applied in O(1)"
        );
        ledger.debit("net", "whale", 4_000_000);
        assert_eq!(
            ledger.balance_of("whale"),
            CrdtLedger::MAX_CRDT_BATCH - 4_000_000,
            "debit O(1)"
        );
    }

    #[test]
    fn crdt_balances_bounded_and_kept_set_is_order_independent() {
        // §5: flood > the cap with distinct addresses → the map stays BOUNDED,
        // and the kept set is the lexicographically-smallest MAX_CRDT_ACCOUNTS —
        // a pure function of the SET, not the insertion order (inserted here in
        // REVERSE, yet the kept set is still the smallest-K).
        let mut ledger = CrdtLedger::new();
        let overflow = 50usize;
        let total = MAX_CRDT_ACCOUNTS + overflow;
        for i in (0..total).rev() {
            ledger.credit("net", &format!("addr{i:07}"), 1);
        }
        assert_eq!(
            ledger.account_count(),
            MAX_CRDT_ACCOUNTS,
            "balances map stays bounded at the cap under an adversarial flood"
        );
        let kept: std::collections::BTreeSet<String> = ledger.balances.keys().cloned().collect();
        let expected: std::collections::BTreeSet<String> =
            (0..MAX_CRDT_ACCOUNTS).map(|i| format!("addr{i:07}")).collect();
        assert_eq!(
            kept, expected,
            "kept set = lex-smallest K, independent of insertion order (convergence-friendly)"
        );
    }
}
