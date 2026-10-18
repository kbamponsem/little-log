//! Hybrid logical clock.
//!
//! Wall time alone is not enough for a peer-to-peer log: devices disagree about
//! the time, and a device's own clock can be stepped backwards (NTP, a reboot,
//! a user changing it). The HLC keeps timestamps **monotonic** — a local tick
//! is always strictly later than the last one, and observing a peer pushes ours
//! past theirs — while still tracking wall time closely enough to be readable.
//!
//! Ordering is total and identical on every device: `(wall_ms, counter, node)`.
//! The counter separates events made in the same millisecond; `node` breaks
//! ties between devices, so two devices never need to agree on arrival order.

use serde::{Deserialize, Serialize};

use crate::model::Millis;

/// A hybrid logical clock timestamp.
///
/// Derived `Ord` is the merge order: field order matters here, and `node` is
/// compared as bytes so every device sorts the same way.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Hlc {
    pub wall_ms: Millis,
    pub counter: u32,
    /// The device that produced this timestamp — its public key id.
    pub node: String,
}

impl Hlc {
    pub fn new(wall_ms: Millis, counter: u32, node: impl Into<String>) -> Self {
        Self {
            wall_ms,
            counter,
            node: node.into(),
        }
    }
}

/// A device's local clock. Tick it for events you author; observe it for
/// timestamps that arrive from peers.
#[derive(Clone, Debug)]
pub struct Clock {
    node: String,
    state: Hlc,
}

impl Clock {
    pub fn new(node: impl Into<String>) -> Self {
        let node = node.into();
        Self {
            state: Hlc::new(0, 0, node.clone()),
            node,
        }
    }

    pub fn node(&self) -> &str {
        &self.node
    }

    /// The newest timestamp this clock has issued.
    pub fn current(&self) -> &Hlc {
        &self.state
    }

    /// Timestamp a local event made at physical time `physical_ms`.
    ///
    /// `physical_ms` is a parameter rather than a call to the system clock so
    /// behaviour under skew and step-backs can be tested deterministically.
    pub fn tick(&mut self, physical_ms: Millis) -> Hlc {
        if physical_ms > self.state.wall_ms {
            // A fresh millisecond: counter 0 is already ordered after
            // everything this clock has issued.
            self.state.wall_ms = physical_ms;
            self.state.counter = 0;
        } else {
            self.bump();
        }
        self.state.clone()
    }

    /// Fold in a timestamp seen from a peer, then timestamp a local event.
    ///
    /// The result is strictly later than both the previous state and `remote`,
    /// which is what keeps merge order consistent in both directions.
    pub fn observe(&mut self, remote: &Hlc, physical_ms: Millis) -> Hlc {
        let wall_ms = physical_ms.max(self.state.wall_ms).max(remote.wall_ms);
        let counter = if wall_ms > self.state.wall_ms && wall_ms > remote.wall_ms {
            0
        } else if wall_ms > remote.wall_ms {
            self.state.counter
        } else if wall_ms > self.state.wall_ms {
            remote.counter
        } else {
            self.state.counter.max(remote.counter)
        };
        self.state = Hlc::new(wall_ms, counter, self.node.clone());
        self.bump();
        self.state.clone()
    }

    /// Advance the counter, spilling into the wall clock rather than wrapping.
    ///
    /// Reaching `u32::MAX` ticks in one millisecond is not reachable in
    /// practice, but silently saturating would break the guarantee that a tick
    /// is strictly greater than the last one — and merge order depends on it.
    fn bump(&mut self) {
        if self.state.counter == u32::MAX {
            self.state.wall_ms = self.state.wall_ms.saturating_add(1);
            self.state.counter = 0;
        } else {
            self.state.counter += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_in_the_same_millisecond_are_ordered_by_counter() {
        let mut clock = Clock::new("phone");
        let first = clock.tick(1_000);
        let second = clock.tick(1_000);

        assert_eq!(first, Hlc::new(1_000, 0, "phone"));
        assert_eq!(second, Hlc::new(1_000, 1, "phone"));
        assert!(second > first);
    }

    #[test]
    fn a_backwards_wall_clock_never_goes_backwards() {
        let mut clock = Clock::new("phone");
        let before = clock.tick(5_000);
        let after = clock.tick(1_000); // the clock was stepped back

        assert_eq!(after.wall_ms, 5_000, "wall time must not retreat");
        assert!(after > before);
    }

    #[test]
    fn a_timestamp_from_the_future_pulls_the_clock_forward() {
        let mut clock = Clock::new("phone");
        clock.tick(1_000);
        let remote = Hlc::new(9_999, 5, "tablet");

        let merged = clock.observe(&remote, 1_000);

        assert_eq!(merged.wall_ms, 9_999);
        assert_eq!(merged.counter, 6);
        assert_eq!(merged.node, "phone", "the timestamp stays ours");
        assert!(merged > remote);
    }

    #[test]
    fn observing_a_stale_timestamp_still_advances_us() {
        let mut clock = Clock::new("phone");
        let ours = clock.tick(5_000);
        let remote = Hlc::new(1_000, 0, "tablet");

        let merged = clock.observe(&remote, 5_000);

        assert!(merged > ours);
        assert!(merged > remote);
    }

    #[test]
    fn two_devices_converge_after_exchanging_timestamps() {
        let mut phone = Clock::new("phone");
        let mut tablet = Clock::new("tablet");

        let first = phone.tick(100);
        let second = tablet.observe(&first, 100);
        let third = phone.observe(&second, 100);

        // Both devices now agree on the order; neither had to agree on the
        // wall clock or on who arrived first.
        assert!(first < second, "{first:?} should precede {second:?}");
        assert!(second < third, "{second:?} should precede {third:?}");
        assert_eq!(phone.current(), &third);
    }

    #[test]
    fn equal_wall_and_counter_are_broken_by_the_node_id() {
        // Two devices ticking in the same millisecond produce the same
        // (wall, counter) — the node id must decide, identically everywhere.
        let a = Hlc::new(1_000, 3, "phone");
        let b = Hlc::new(1_000, 3, "tablet");
        assert!(a < b);
        assert!(b > a);
        assert!(!(a == b));
    }

    #[test]
    fn a_saturated_counter_spills_into_the_wall_clock() {
        let mut clock = Clock::new("phone");
        let remote = Hlc::new(1_000, u32::MAX, "tablet");

        let merged = clock.observe(&remote, 1_000);

        assert!(merged > remote, "must not tie with the remote timestamp");
        assert_eq!(merged.wall_ms, 1_001);
        assert_eq!(merged.counter, 0);
    }
}
