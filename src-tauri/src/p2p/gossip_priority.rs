//! NET-3: Priority-aware gossip channel.
//!
//! The outgoing gossip queue is split into four logical lanes (Critical, High,
//! Medium, Low) so consensus-critical traffic (Hello, RequestChain,
//! ChainSegment, NewBlock) drains ahead of bulk traffic (transactions, pages,
//! social actions) and noise (Ping/Pong, ReportPeer, juror commits).
//!
//! Wire format does not change — only the local egress order does. Receiving
//! peers are agnostic to priority; this is purely a local scheduling concern.
//!
//! ## Design
//! - `PrioritySender::send()` inspects the payload, classifies it via
//!   [`priority_for`], and routes to the matching internal `mpsc` channel.
//! - `PriorityReceiver::recv()` first greedily drains any pending message in
//!   priority order via `try_recv`, then falls back to a `biased`
//!   `tokio::select!` so that whichever channel becomes ready first — with
//!   ties always going to the highest-priority lane.
//!
//! Each lane is unbounded (matching the previous single channel) so a slow
//! drain never back-pressures producers. DoS protection lives at the
//! dispatcher layer (rate limit + envelope size cap), not here.

use crate::p2p::gossip::{GossipEnvelope, GossipMessage};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

/// Egress priority level. Smaller value = higher priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Priority {
    /// Consensus + presence: Hello, RequestChain, ChainSegment, NewBlock.
    Critical = 0,
    /// User payload: BroadcastTx, PublishPage, PublishSiteManifest.
    High = 1,
    /// Discovery + metadata: WantNodes, HaveNodes, RequestPage, domains, search,
    /// social actions, forum nodes.
    Medium = 2,
    /// Liveness + moderation noise: Ping, Pong, ReportPeer, broadcast reports,
    /// juror commit/reveal.
    Low = 3,
}

/// Classify a gossip payload into its egress priority lane.
///
/// Mapping rationale:
/// - **Critical** keeps the chain converging — without these every other
///   message is built on stale state.
/// - **High** carries user-visible state changes (money + content).
/// - **Medium** is opportunistic: missing it is recoverable on the next sync.
/// - **Low** is best-effort signal that can drop without correctness loss.
pub fn priority_for(payload: &GossipMessage) -> Priority {
    match payload {
        GossipMessage::Hello { .. }
        | GossipMessage::RequestChain { .. }
        | GossipMessage::ChainSegment { .. }
        | GossipMessage::NewBlock { .. } => Priority::Critical,

        GossipMessage::BroadcastTx { .. }
        | GossipMessage::PublishPage { .. }
        | GossipMessage::PublishSiteManifest { .. } => Priority::High,

        GossipMessage::WantNodes { .. }
        | GossipMessage::HaveNodes { .. }
        | GossipMessage::RequestPage { .. }
        | GossipMessage::PublishDomain { .. }
        | GossipMessage::PublishSubdomain { .. }
        | GossipMessage::PublishSite { .. }
        | GossipMessage::BroadcastSocialAction { .. }
        | GossipMessage::PublishForumNode { .. } => Priority::Medium,

        GossipMessage::Ping { .. }
        | GossipMessage::Pong { .. }
        | GossipMessage::ReportPeer { .. }
        | GossipMessage::BroadcastReport { .. }
        | GossipMessage::BroadcastJurorCommit { .. }
        | GossipMessage::BroadcastJurorReveal { .. } => Priority::Low,
    }
}

/// Cloneable priority-aware sender. Callers use it like an
/// `mpsc::UnboundedSender<GossipEnvelope>` — `send(env)` is the only API.
#[derive(Clone)]
pub struct PrioritySender {
    critical: mpsc::UnboundedSender<GossipEnvelope>,
    high: mpsc::UnboundedSender<GossipEnvelope>,
    medium: mpsc::UnboundedSender<GossipEnvelope>,
    low: mpsc::UnboundedSender<GossipEnvelope>,
}

impl PrioritySender {
    /// Route an envelope to the appropriate priority lane. The error variant
    /// returns the envelope back so callers can observe loss, mirroring the
    /// underlying `mpsc::UnboundedSender::send` contract.
    #[allow(clippy::result_large_err)]
    pub fn send(
        &self,
        env: GossipEnvelope,
    ) -> Result<(), mpsc::error::SendError<GossipEnvelope>> {
        match priority_for(&env.payload) {
            Priority::Critical => self.critical.send(env),
            Priority::High => self.high.send(env),
            Priority::Medium => self.medium.send(env),
            Priority::Low => self.low.send(env),
        }
    }
}

/// Drain side of the priority channel. Held by exactly one consumer
/// (the outgoing gossip drain task).
pub struct PriorityReceiver {
    critical: mpsc::UnboundedReceiver<GossipEnvelope>,
    high: mpsc::UnboundedReceiver<GossipEnvelope>,
    medium: mpsc::UnboundedReceiver<GossipEnvelope>,
    low: mpsc::UnboundedReceiver<GossipEnvelope>,
}

impl PriorityReceiver {
    /// Receive the next envelope, draining higher-priority lanes first.
    ///
    /// Behaviour:
    /// 1. Non-blocking sweep: poll `try_recv` from Critical→Low and return the
    ///    first hit so a backlog of low-priority traffic can never overtake a
    ///    fresh critical message.
    /// 2. If every lane is empty, await a biased `select!` over all four
    ///    receivers. `biased` evaluates branches in priority order, so when
    ///    multiple lanes become ready simultaneously the highest one wins.
    ///
    /// Returns `None` only when every lane is closed AND drained — i.e. all
    /// senders have been dropped (graceful shutdown).
    pub async fn recv(&mut self) -> Option<GossipEnvelope> {
        loop {
            if let Ok(env) = self.critical.try_recv() {
                return Some(env);
            }
            if let Ok(env) = self.high.try_recv() {
                return Some(env);
            }
            if let Ok(env) = self.medium.try_recv() {
                return Some(env);
            }
            if let Ok(env) = self.low.try_recv() {
                return Some(env);
            }

            tokio::select! {
                biased;
                res = self.critical.recv() => {
                    if let Some(e) = res { return Some(e); }
                }
                res = self.high.recv() => {
                    if let Some(e) = res { return Some(e); }
                }
                res = self.medium.recv() => {
                    if let Some(e) = res { return Some(e); }
                }
                res = self.low.recv() => {
                    if let Some(e) = res { return Some(e); }
                }
            }

            // A branch fired with `None` (lane closed). Loop and let the
            // remaining lanes drain. If all are exhausted, the next iteration's
            // `try_recv` will all be Disconnected/Empty and `select!` will keep
            // returning `None` — at which point we'd loop forever.
            // Guard against that by checking whether any lane still has a
            // sender attached.
            if self.all_closed() {
                return None;
            }
        }
    }

    /// `true` once every lane has had its last sender dropped. Used to break
    /// out of `recv()` when all producers have shut down.
    fn all_closed(&mut self) -> bool {
        // tokio's UnboundedReceiver doesn't expose `is_closed()` directly,
        // but `try_recv` returns `Disconnected` when the channel is closed
        // AND empty. We invoke it on each lane; if all four say Disconnected
        // we're done. Empty lanes that still have a live sender return Empty,
        // so this discriminates cleanly.
        use mpsc::error::TryRecvError;
        matches!(self.critical.try_recv(), Err(TryRecvError::Disconnected))
            && matches!(self.high.try_recv(), Err(TryRecvError::Disconnected))
            && matches!(self.medium.try_recv(), Err(TryRecvError::Disconnected))
            && matches!(self.low.try_recv(), Err(TryRecvError::Disconnected))
    }
}

/// Build a fresh priority channel with all four lanes wired up.
pub fn priority_channel() -> (PrioritySender, PriorityReceiver) {
    let (ctx, crx) = mpsc::unbounded_channel();
    let (htx, hrx) = mpsc::unbounded_channel();
    let (mtx, mrx) = mpsc::unbounded_channel();
    let (ltx, lrx) = mpsc::unbounded_channel();
    (
        PrioritySender { critical: ctx, high: htx, medium: mtx, low: ltx },
        PriorityReceiver { critical: crx, high: hrx, medium: mrx, low: lrx },
    )
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::p2p::gossip::{GossipMessage, GossipRouter};

    fn env(payload: GossipMessage) -> GossipEnvelope {
        GossipRouter::build_signed_envelope(
            "deadbeef".repeat(8),
            payload,
            1,
            chrono::Utc::now().to_rfc3339(),
            &[0u8; 64],
        )
        .expect("build envelope")
    }

    #[test]
    fn priority_classification_is_complete() {
        // Critical
        assert_eq!(
            priority_for(&GossipMessage::Hello {
                heads: vec![],
                node_id: "n".into(),
                version: 1,
                watts: 1.0,
                country: "FR".into(),
                tasks_completed: 0,
                blocks_verified: 0,
                uptime_minutes: 0,
                chain_height: 0,
                known_peer_ids: vec![],
                display_name: None,
            }),
            Priority::Critical
        );
        assert_eq!(
            priority_for(&GossipMessage::RequestChain { from_height: 0, max_blocks: 50 }),
            Priority::Critical
        );
        assert_eq!(
            priority_for(&GossipMessage::ChainSegment {
                blocks_json: vec![],
                sender_height: 0,
                blocks_compressed: None,
            }),
            Priority::Critical
        );
        assert_eq!(
            priority_for(&GossipMessage::NewBlock { block_json: "{}".into() }),
            Priority::Critical
        );

        // High
        assert_eq!(
            priority_for(&GossipMessage::BroadcastTx { tx_json: "{}".into() }),
            Priority::High
        );
        assert_eq!(
            priority_for(&GossipMessage::PublishPage { page_json: "{}".into() }),
            Priority::High
        );
        assert_eq!(
            priority_for(&GossipMessage::PublishSiteManifest { manifest_json: "{}".into() }),
            Priority::High
        );

        // Medium
        assert_eq!(
            priority_for(&GossipMessage::WantNodes { ids: vec![] }),
            Priority::Medium
        );
        assert_eq!(
            priority_for(&GossipMessage::HaveNodes { nodes: vec![] }),
            Priority::Medium
        );
        assert_eq!(
            priority_for(&GossipMessage::RequestPage { author_pk: "p".into() }),
            Priority::Medium
        );
        assert_eq!(
            priority_for(&GossipMessage::BroadcastSocialAction { action_json: "{}".into() }),
            Priority::Medium
        );

        // Low
        assert_eq!(priority_for(&GossipMessage::Ping { nonce: 1 }), Priority::Low);
        assert_eq!(priority_for(&GossipMessage::Pong { nonce: 1 }), Priority::Low);
        assert_eq!(
            priority_for(&GossipMessage::BroadcastJurorCommit { commit_json: "{}".into() }),
            Priority::Low
        );
    }

    #[test]
    fn priority_ordering_holds() {
        assert!(Priority::Critical < Priority::High);
        assert!(Priority::High < Priority::Medium);
        assert!(Priority::Medium < Priority::Low);
    }

    #[tokio::test]
    async fn higher_priority_drains_first() {
        let (tx, mut rx) = priority_channel();

        // Push a low, then medium, then high, then critical — reverse priority
        // order. Drain MUST come out in priority order regardless of insertion.
        tx.send(env(GossipMessage::Ping { nonce: 1 })).unwrap();
        tx.send(env(GossipMessage::WantNodes { ids: vec!["x".into()] })).unwrap();
        tx.send(env(GossipMessage::BroadcastTx { tx_json: "{}".into() })).unwrap();
        tx.send(env(GossipMessage::NewBlock { block_json: "{}".into() })).unwrap();

        // Order out: Critical → High → Medium → Low
        let mut order = Vec::with_capacity(4);
        for _ in 0..4 {
            let e = rx.recv().await.expect("envelope");
            order.push(priority_for(&e.payload));
        }

        assert_eq!(
            order,
            vec![Priority::Critical, Priority::High, Priority::Medium, Priority::Low],
            "drain must yield highest-priority message first"
        );
    }

    #[tokio::test]
    async fn drain_returns_none_when_all_senders_dropped() {
        let (tx, mut rx) = priority_channel();
        drop(tx);
        assert!(rx.recv().await.is_none(), "closed channel must yield None");
    }

    #[tokio::test]
    async fn fifo_within_same_priority() {
        let (tx, mut rx) = priority_channel();
        // Three High-priority messages in order
        tx.send(env(GossipMessage::BroadcastTx { tx_json: "1".into() })).unwrap();
        tx.send(env(GossipMessage::BroadcastTx { tx_json: "2".into() })).unwrap();
        tx.send(env(GossipMessage::BroadcastTx { tx_json: "3".into() })).unwrap();

        for expected in ["1", "2", "3"] {
            let e = rx.recv().await.unwrap();
            match e.payload {
                GossipMessage::BroadcastTx { tx_json } => assert_eq!(tx_json, expected),
                _ => panic!("wrong payload"),
            }
        }
    }

    #[tokio::test]
    async fn critical_overtakes_low_backlog() {
        let (tx, mut rx) = priority_channel();
        // Build up a backlog of Low-priority traffic
        for i in 0..50 {
            tx.send(env(GossipMessage::Ping { nonce: i })).unwrap();
        }
        // Fresh critical message arrives mid-backlog
        tx.send(env(GossipMessage::NewBlock { block_json: "block".into() })).unwrap();

        // First drained must be the critical one, not a backlog Ping
        let first = rx.recv().await.unwrap();
        assert_eq!(priority_for(&first.payload), Priority::Critical);
    }
}
