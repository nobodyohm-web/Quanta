//! Outbound I/O, reified as data.
//!
//! The core returns `Vec<Effect>` from `Node::handle`; the **shell** is the
//! only place that actually sends bytes, persists, arms timers, or emits UI
//! events (sans-IO pattern, see [`super`] module docs). This keeps the core a
//! pure, synchronous, replayable function — the effects it returns are
//! *requests*, not actions.

use super::event::{PeerId, TimerId};

/// An opaque, deterministic snapshot of node state to be persisted.
///
/// Modelled as bytes so the core stays agnostic to the storage backend — libSQL
/// in production, an in-memory store in simulation (harness spec §3.4). The
/// core produces the bytes; the shell decides where they go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot(pub Vec<u8>);

/// A UI-facing event, translated by the shell into a Tauri `torus://…` event.
///
/// `name` keeps the existing wire-compatible event identifiers; `payload_json`
/// is the already-serialised body. Keeping it as a string here means the core
/// owns no Tauri dependency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiEvent {
    pub name: String,
    pub payload_json: String,
}

/// Everything the core asks the shell to do with the outside world.
///
/// `Effect` is data, never a callback — so a simulation shell can inspect,
/// reorder, drop, or delay each one to inject faults deterministically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Send `bytes` to exactly one peer.
    Send { to: PeerId, bytes: Vec<u8> },
    /// Broadcast `bytes` to all currently-connected peers.
    Broadcast { bytes: Vec<u8> },
    /// Arm a timer to fire at virtual time `fire_at_ms`, delivered back to the
    /// core as [`Event::TimerFired`](super::event::Event::TimerFired).
    SetTimer { id: TimerId, fire_at_ms: u64 },
    /// Cancel a previously-armed timer.
    CancelTimer { id: TimerId },
    /// Persist a state snapshot (storage-agnostic).
    Persist { snapshot: Snapshot },
    /// Emit a UI event.
    Emit(UiEvent),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Effects are plain comparable data — the property that lets a simulation
    /// shell assert on, and inject faults into, the core's outputs.
    #[test]
    fn effects_are_inspectable_data() {
        let e = Effect::Send {
            to: PeerId("abcd".into()),
            bytes: vec![1, 2, 3],
        };
        assert_eq!(
            e,
            Effect::Send {
                to: PeerId("abcd".into()),
                bytes: vec![1, 2, 3]
            }
        );
        assert_ne!(
            e,
            Effect::Broadcast {
                bytes: vec![1, 2, 3]
            }
        );
    }

    #[test]
    fn timer_effects_round_trip_their_id() {
        let set = Effect::SetTimer {
            id: TimerId(7),
            fire_at_ms: 1_500,
        };
        let cancel = Effect::CancelTimer { id: TimerId(7) };
        match (set, cancel) {
            (Effect::SetTimer { id: a, .. }, Effect::CancelTimer { id: b }) => assert_eq!(a, b),
            _ => panic!("unexpected effect shapes"),
        }
    }
}
