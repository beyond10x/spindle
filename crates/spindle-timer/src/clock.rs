//! The clock port: where "now" comes from.

use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

/// A source of time. The timer never reads a clock on its own: the host passes `now` into
/// [`Timer::due`](crate::Timer::due), and reads activation from the clock it activates with.
///
/// [`Clock::now`] is monotonic and places ticks; [`Clock::wall`] is the civil time a
/// `{generated: true}` `Timestamp` is stamped with. Both move together.
pub trait Clock {
    /// The current instant.
    fn now(&self) -> Instant;

    /// The current civil time.
    fn wall(&self) -> SystemTime;
}

/// The host's monotonic clock, and its civil time.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn wall(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// A clock that moves only when told to, for tests and simulation.
///
/// It starts at the instant it was created, at the civil time it was created or the one
/// [`ManualClock::at`] names, and both advance by exactly the durations given.
#[derive(Debug)]
pub struct ManualClock(Mutex<(Instant, SystemTime)>);

impl ManualClock {
    /// A clock standing at the current instant and civil time.
    pub fn new() -> Self {
        Self::at(SystemTime::now())
    }

    /// A clock standing at the current instant and at the civil time `wall`.
    pub fn at(wall: SystemTime) -> Self {
        Self(Mutex::new((Instant::now(), wall)))
    }

    /// Moves the clock forward by `by`.
    pub fn advance(&self, by: Duration) {
        let mut now = self.held();
        now.0 += by;
        now.1 += by;
    }

    fn held(&self) -> std::sync::MutexGuard<'_, (Instant, SystemTime)> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Default for ManualClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Instant {
        self.held().0
    }

    fn wall(&self) -> SystemTime {
        self.held().1
    }
}
