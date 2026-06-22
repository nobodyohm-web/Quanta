//! Inbound I/O, reified as data.
//!
//! Everything that enters the deterministic core is an [`Event`]; the core
//! never performs I/O itself (sans-IO pattern, see [`super`] module docs). The
//! shell translates real I/O — a received packet, a UI action, a fired timer, a
//! clock tick — into these values and feeds them to `Node::handle`.

/// Stable identifier of a peer on the wire.
///
/// Newtype over the hex public key so the core can order peers
/// **deterministically** by string `Ord` (e.g. as `BTreeMap` keys) and never
/// depend on `HashMap` iteration order (Constitution §3).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PeerId(pub String);

/// Identifier of a timer armed via
/// [`Effect::SetTimer`](super::effect::Effect::SetTimer) and later delivered
/// back as [`Event::TimerFired`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimerId(pub u64);

/// A user/UI-originated command (local intent), kept distinct from network
/// input so the core can apply different trust rules to each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalCommand {
    /// Transfer `amount_micro` µQTA from the local wallet to `to_pseudo`.
    Transfer {
        to_pseudo: String,
        amount_micro: u64,
    },
    /// Stake `amount_micro` µQTA toward consensus eligibility.
    Stake { amount_micro: u64 },
    /// Register/claim a human-readable `@pseudo` for the local wallet.
    RegisterUsername { pseudo: String },
}

/// Everything that can enter the deterministic core.
///
/// Adding a variant here is how new inbound I/O is modelled; the core must stay
/// a pure function of `(state, Event, &mut Rng)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Virtual-time advance. The **only** way time enters the core — there are
    /// no clock reads inside it (Constitution §3).
    Tick { now_ms: u64 },
    /// Raw bytes received from a peer. The core runs the full receive pipeline
    /// (size, decode, ban, dedup, freshness, rate-limit, nonce, signature)
    /// before trusting any of it — raw bytes are never trusted
    /// (Constitution §3).
    MessageReceived { from: PeerId, bytes: Vec<u8> },
    /// A local command issued by the UI layer.
    Command(LocalCommand),
    /// A previously-armed timer reached its deadline.
    TimerFired { id: TimerId },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PeerIds order by their hex string — a total, deterministic order
    /// suitable for `BTreeMap`/`BTreeSet` keys (no HashMap-iteration
    /// dependence).
    #[test]
    fn peer_ids_have_total_deterministic_order() {
        let mut peers = [PeerId("c".into()), PeerId("a".into()), PeerId("b".into())];
        peers.sort();
        assert_eq!(
            peers,
            [PeerId("a".into()), PeerId("b".into()), PeerId("c".into())]
        );
    }

    #[test]
    fn events_are_comparable_for_test_assertions() {
        let a = Event::Tick { now_ms: 10 };
        let b = Event::Tick { now_ms: 10 };
        let c = Event::TimerFired { id: TimerId(1) };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
