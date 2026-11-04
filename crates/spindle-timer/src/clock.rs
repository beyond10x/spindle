//! The clock port: where "now" comes from.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A source of monotonic time. The timer never reads a clock on its own: the host passes `now`
/// into [`Timer::due`](crate::Timer::due), and reads activation from the clock it activates with.
pub trait Clock {
    /// The current instant.
    fn now(&self) -> Instant;
}

/// The host's monotonic clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// A clock that moves only when told to, for tests and simulation.
///
/// It starts at the instant it was created and advances by exactly the durations given.
#[derive(Debug)]
pub struct ManualClock(Mutex<Instant>);

impl ManualClock {
    /// A clock standing at the current instant.
    pub fn new() -> Self {
        Self(Mutex::new(Instant::now()))
    }

    /// Moves the clock forward by `by`.
    pub fn advance(&self, by: Duration) {
        let mut now = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *now += by;
    }
}

impl Default for ManualClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Instant {
        *self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
