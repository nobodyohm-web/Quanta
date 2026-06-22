//! Injected clock for the deterministic core.
//!
//! The core reads time **only** via [`Event::Tick`](super::event::Event::Tick)
//! — it never calls `SystemTime::now` / `Instant::now` (Constitution §3: no
//! system-clock reads inside consensus/ledger). A `Clock` lives in the *shell*,
//! which turns its readings into `Tick` events. The production shell will back
//! this with the OS clock; the simulation shell uses [`ManualClock`] so virtual
//! time only advances when the event loop says so.

/// Wall-clock source in **milliseconds since the Unix epoch**.
///
/// Millisecond granularity matches the existing ±90 s freshness window and the
/// gossip timers, while fitting comfortably in `u64`.
pub trait Clock {
    /// Current time in milliseconds since the Unix epoch.
    fn now_ms(&self) -> u64;
}

/// Deterministic clock for simulation and tests: time advances only when the
/// shell sets it, never by reading the OS. This is what makes a run replayable.
#[derive(Debug, Clone)]
pub struct ManualClock {
    now_ms: u64,
}

impl ManualClock {
    /// Start the clock at `start_ms` (ms since the Unix epoch).
    pub fn new(start_ms: u64) -> Self {
        Self { now_ms: start_ms }
    }

    /// Jump to an absolute virtual time.
    pub fn set(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
    }

    /// Advance virtual time by `delta_ms`. Saturates at `u64::MAX` rather than
    /// panicking — a clock is not an amount, and overflow is unreachable in any
    /// real run (`u64::MAX` ms ≈ 5×10^8 years).
    pub fn advance(&mut self, delta_ms: u64) {
        self.now_ms = self.now_ms.saturating_add(delta_ms);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.now_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_clock_only_moves_when_told() {
        let mut c = ManualClock::new(1_000);
        assert_eq!(c.now_ms(), 1_000);
        c.advance(500);
        assert_eq!(c.now_ms(), 1_500);
        c.set(42);
        assert_eq!(c.now_ms(), 42);
    }

    #[test]
    fn advance_saturates_without_panicking() {
        let mut c = ManualClock::new(u64::MAX - 5);
        c.advance(1_000);
        assert_eq!(c.now_ms(), u64::MAX);
    }
}
