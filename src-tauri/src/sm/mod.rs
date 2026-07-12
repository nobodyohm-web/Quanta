//! `sm` — the deterministic, **sans-IO state-machine core** of Quanta.
//!
//! Constitution Phase 0 / harness task **T0.1–T0.2**
//! (`QUANTA_T0_DST_HARNESS.md`).
//!
//! # Why sans-IO
//!
//! Determinism is impossible with Iroh and multi-threaded tokio inside the
//! consensus loop: the runtime schedules tasks non-deterministically and Iroh
//! does real network I/O. The fix is the **sans-IO pattern** — a pure
//! functional core with an imperative shell around it:
//!
//! ```text
//!   ┌────────────────────────────────────────────────┐
//!   │  Deterministic core (NO I/O, synchronous)       │
//!   │  fn handle(state, Event, &mut Rng) -> Vec<Effect>│
//!   │  ledger · consensus · mempool · @pseudo          │
//!   └───────────────┬──────────────────────────────────┘
//!         Events (in) │ Effects (out)
//!         ┌───────────┴────────────┐
//!         ▼                        ▼
//!   production shell          simulation shell
//!   (Iroh+tokio+OsRng+libSQL) (virtual net+clock+seeded RNG+faults)
//! ```
//!
//! The core never reads the system clock, never calls `OsRng`, never touches
//! the network or disk. Time is an input ([`Event::Tick`]), randomness is
//! injected (`&mut dyn `[`Rng`]), and sending / persisting / emitting are
//! outputs ([`Effect`]). Because both shells speak the same `Event`/`Effect`
//! language, the simulation shell can drive everything from a single seed,
//! making **every run replayable byte-for-byte**.
//!
//! # Naming
//!
//! The harness spec calls this the "core" and permits the name `sm/`. We use
//! `sm` (state machine) to avoid shadowing Rust's `::core` prelude crate.
//!
//! # Build status (incremental)
//!
//! T0.1 is a large extraction and is landed in verifiable slices (Constitution
//! §4/§8: never transform everything at once). **This slice lands the boundary
//! types and determinism abstractions** — [`Event`], [`Effect`], [`Clock`],
//! [`Rng`]. Subsequent slices migrate the ledger / consensus / mempool /
//! @pseudo logic behind `Node::handle`. Until then, the existing `p2p` modules
//! remain the source of truth and behaviour is unchanged.

pub mod clock;
pub mod effect;
pub mod event;
pub mod finality;
pub mod finality_rule;
pub mod finality_slashing;
pub mod finality_vote;
pub mod fork_choice;
pub mod node;
pub mod rng;

/// Deterministic simulation shell (DST harness, T0.4+). Test-only: virtual
/// clock + seeded scheduler + virtual network drive N [`Node`]s reproducibly.
#[cfg(test)]
mod sim;

pub use clock::{Clock, ManualClock};
pub use effect::{Effect, Snapshot, UiEvent};
pub use finality::{Checkpoint, FinalizedSet, EPOCH_LENGTH_BLOCKS};
pub use finality_rule::{FinalityState, JustifiedSet, StepOutcome};
pub use finality_slashing::{
    apply_slash, detect_fault, slash_for_proof, slashable_weight, verify_proof, Fault, FaultProof,
    SlashOutcome,
};
pub use fork_choice::{anchors, ghost_head, BlockTree, LatestVotes};
pub use finality_vote::{FinalityCertificate, MlDsaCertificate, Vote};
pub use event::{Event, LocalCommand, PeerId, TimerId};
pub use node::Node;
pub use rng::{Blake3Rng, Rng};
