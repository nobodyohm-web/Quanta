//! `finality` — epoch / checkpoint skeleton of the finality gadget (**GADGET-1**).
//!
//! This is the **bedrock** of the Casper-FFG-style finality gadget designed in
//! `DESIGN-FINALITY-GADGET.md` (§2 epochs/checkpoints, §4 the finalized set). It
//! is the one piece that does **not** move whatever Alexandre decides for the
//! finer §12 parameters: dividing the chain into epochs and naming the
//! checkpoints at their boundaries is structural to *any* Casper-style gadget.
//!
//! **Nothing finalizes here.** The two-step justify/finalize rule — the part
//! that actually advances finality beyond genesis — is **GADGET-3**. This file
//! only lays:
//!   1. the **pure, deterministic** epoch/checkpoint computation (no clock, no
//!      entropy → the `sm/` sans-IO / C1 determinism guarantee is preserved), and
//!   2. the **finalized set**, initialised to `{genesis}` (genesis is finalized
//!      by definition, design §4) — the *place* where GADGET-3 will branch in.
//!
//! The companion harness invariant (`Sim::check_invariants`,
//! `Violation::FinalitySafety`) is proven to **bite now**, before any real
//! finalization exists, by the planted-conflict teeth test — so this skeleton is
//! verified, not decorative.

use crate::p2p::ledger::Block;
use std::collections::BTreeMap;

/// **E — longueur d'époque, en blocs. Ratifiée *réglable* par ADR-009 = 32**
/// (Gasper/Ethereum 32-slot epoch ; cf. `docs/decisions/ADR-009 …`,
/// `DESIGN-FINALITY-GADGET.md` §12, `AUDIT_QUANTA_2_PROGRESS.md` GADGET-1).
///
/// ADR-009 tranche la **classe** (ajustable, derrière l'abstraction `epoch_len`,
/// modifiable par **fork volontaire** — pas de gouvernance on-chain) et pose **32**
/// comme **défaut ancré**, pas une promesse figée. Le squelette reste
/// **paramétrique** : chaque fonction prend `epoch_len` et est correcte pour *tout*
/// `E ≥ 1`, donc changer la valeur par fork est un changement **d'une ligne sans
/// logique à revisiter** (la latence de finalité vs la fréquence des certificats).
pub const EPOCH_LENGTH_BLOCKS: u64 = 32; // ADR-009 : défaut réglable (fork-only)

/// A **checkpoint**: the `(height, hash)` of an epoch-boundary block, tagged with
/// the `epoch` it opens (design §2). Genesis is the checkpoint of epoch 0.
///
/// `height` is the block's **position** in the chain (genesis = 0), consistent
/// with how the harness `Safety` invariant indexes blocks (`enumerate()`), not
/// the `Block::index` field — in a well-formed chain the two coincide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    /// Epoch number this checkpoint opens (`height / E`).
    pub epoch: u64,
    /// Chain position of the boundary block (a multiple of `E`).
    pub height: u64,
    /// Hash of the boundary block — what two nodes must agree on per epoch.
    pub hash: String,
}

/// Epoch a block at `height` belongs to (pure): `height / E`. `epoch_len` is
/// clamped to `≥ 1` so a misconfigured `E = 0` can never divide-by-zero (panic
/// freedom, Constitution Rust rule 2).
pub fn epoch_of_height(height: u64, epoch_len: u64) -> u64 {
    height / epoch_len.max(1)
}

/// Whether `height` sits on an epoch boundary (pure): a multiple of `E`. Genesis
/// (height 0) is always a boundary.
pub fn is_epoch_boundary(height: u64, epoch_len: u64) -> bool {
    height.is_multiple_of(epoch_len.max(1))
}

/// The checkpoint opening `epoch`, if the chain actually reaches its boundary
/// block (pure, no clock/entropy). Boundary height is `epoch * E`; `None` if the
/// chain is too short (or the multiplication would overflow).
pub fn checkpoint_at_epoch(chain: &[Block], epoch: u64, epoch_len: u64) -> Option<Checkpoint> {
    let height = epoch.checked_mul(epoch_len.max(1))?;
    let pos = usize::try_from(height).ok()?;
    let block = chain.get(pos)?;
    Some(Checkpoint {
        epoch,
        height,
        hash: block.hash.clone(),
    })
}

/// Every epoch-boundary checkpoint present in `chain`, in epoch order (pure,
/// deterministic). For a chain of `len` blocks this yields one checkpoint per
/// boundary `0, E, 2E, …` that the chain reaches — `1 + (len-1)/E` of them. This
/// is the skeleton GADGET-3 will reason over; it has no production caller yet.
pub fn checkpoints(chain: &[Block], epoch_len: u64) -> Vec<Checkpoint> {
    let mut out = Vec::new();
    let mut epoch = 0u64;
    while let Some(cp) = checkpoint_at_epoch(chain, epoch, epoch_len) {
        out.push(cp);
        epoch += 1;
    }
    out
}

/// The set of **finalized checkpoints**, keyed by epoch (design §4).
///
/// Keyed by epoch because finality finalizes **at most one** checkpoint per
/// epoch — and two nodes disagreeing on *which* one at the same epoch is exactly
/// the catastrophe the finality-safety invariant catches. Init is genesis-only;
/// **GADGET-3** adds the rule that lets it grow. A `BTreeMap` keeps `iter()`
/// epoch-ordered so the harness's first violation stays reproducible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalizedSet {
    by_epoch: BTreeMap<u64, Checkpoint>,
}

impl FinalizedSet {
    /// Initial state — **genesis only** (epoch 0, height 0), finalized by
    /// definition (design §4). Nothing else is finalized until GADGET-3.
    pub fn genesis_only(genesis_hash: String) -> Self {
        let mut by_epoch = BTreeMap::new();
        by_epoch.insert(
            0,
            Checkpoint {
                epoch: 0,
                height: 0,
                hash: genesis_hash,
            },
        );
        Self { by_epoch }
    }

    /// The finalized checkpoint at `epoch`, if any.
    pub fn get(&self, epoch: u64) -> Option<&Checkpoint> {
        self.by_epoch.get(&epoch)
    }

    /// Finalized checkpoints in epoch order (deterministic).
    pub fn iter(&self) -> impl Iterator<Item = &Checkpoint> {
        self.by_epoch.values()
    }

    /// Record `cp` as finalized. **Data-structure hook only** — this is the
    /// branch point GADGET-3 will call, *not* the finalization decision: it
    /// performs **no** justification check (that rule is GADGET-3, deliberately
    /// not coded here). Until that rule exists, the only caller is the harness
    /// teeth test, which plants a conflict to prove the invariant bites.
    pub fn insert(&mut self, cp: Checkpoint) {
        self.by_epoch.insert(cp.epoch, cp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::p2p::ledger::{Block, Transaction};

    /// A bare block carrying just a position + hash — enough to exercise the
    /// pure checkpoint maths without sealing real blocks.
    fn blk(index: u64, hash: &str) -> Block {
        Block {
            index,
            timestamp: String::new(),
            transactions: Vec::<Transaction>::new(),
            prev_hash: String::new(),
            hash: hash.to_string(),
            miner: String::new(),
            energy_kwh: 0.0,
        }
    }

    fn chain_of(n: u64) -> Vec<Block> {
        (0..n).map(|i| blk(i, &format!("h{i}"))).collect()
    }

    #[test]
    fn epoch_of_height_is_floor_division() {
        // E = 4: heights 0..3 → epoch 0, 4..7 → epoch 1, 8 → epoch 2.
        assert_eq!(epoch_of_height(0, 4), 0);
        assert_eq!(epoch_of_height(3, 4), 0);
        assert_eq!(epoch_of_height(4, 4), 1);
        assert_eq!(epoch_of_height(7, 4), 1);
        assert_eq!(epoch_of_height(8, 4), 2);
    }

    #[test]
    fn boundaries_are_multiples_of_e_and_genesis_is_one() {
        assert!(is_epoch_boundary(0, 4), "genesis is always a boundary");
        assert!(!is_epoch_boundary(1, 4));
        assert!(is_epoch_boundary(4, 4));
        assert!(is_epoch_boundary(8, 4));
    }

    #[test]
    fn misconfigured_zero_epoch_len_never_panics() {
        // E clamped to ≥1 → no divide-by-zero, deterministic fallback.
        assert_eq!(epoch_of_height(5, 0), 5);
        assert!(is_epoch_boundary(0, 0));
    }

    #[test]
    fn checkpoints_are_the_boundary_blocks_in_epoch_order() {
        // 9 blocks (heights 0..8), E = 4 → boundaries at 0, 4, 8 → epochs 0,1,2.
        let chain = chain_of(9);
        let cps = checkpoints(&chain, 4);
        assert_eq!(
            cps,
            vec![
                Checkpoint { epoch: 0, height: 0, hash: "h0".into() },
                Checkpoint { epoch: 1, height: 4, hash: "h4".into() },
                Checkpoint { epoch: 2, height: 8, hash: "h8".into() },
            ]
        );
    }

    #[test]
    fn checkpoint_at_epoch_is_none_past_the_chain_tip() {
        let chain = chain_of(5); // heights 0..4
        // E = 4: epoch 1 boundary is height 4 (present), epoch 2 is height 8 (absent).
        assert_eq!(
            checkpoint_at_epoch(&chain, 1, 4),
            Some(Checkpoint { epoch: 1, height: 4, hash: "h4".into() })
        );
        assert_eq!(checkpoint_at_epoch(&chain, 2, 4), None);
    }

    #[test]
    fn finalized_set_starts_at_genesis_only() {
        let fs = FinalizedSet::genesis_only("GENESIS".into());
        let g = fs.get(0).expect("genesis is finalized by definition");
        assert_eq!(g.height, 0);
        assert_eq!(g.hash, "GENESIS");
        assert!(fs.get(1).is_none(), "nothing past genesis finalizes in GADGET-1");
        assert_eq!(fs.iter().count(), 1);
    }

    #[test]
    fn insert_records_one_checkpoint_per_epoch() {
        let mut fs = FinalizedSet::genesis_only("GENESIS".into());
        fs.insert(Checkpoint { epoch: 1, height: 4, hash: "h4".into() });
        assert_eq!(fs.get(1).map(|c| c.hash.as_str()), Some("h4"));
        // Same-epoch re-insert replaces (GADGET-3 yields ≤1 honest checkpoint
        // per epoch; a genuine cross-node conflict is the invariant's job).
        fs.insert(Checkpoint { epoch: 1, height: 4, hash: "h4bis".into() });
        assert_eq!(fs.get(1).map(|c| c.hash.as_str()), Some("h4bis"));
        assert_eq!(fs.iter().count(), 2, "epoch 0 (genesis) + epoch 1");
    }
}
