//! The clock port: where "now" comes from.
//!
//! On `wasm32-unknown-unknown`, where the standard library has no clock, [`Instant`] and the civil
//! time [`SystemClock`] and [`ManualClock::new`] read come from the browser (`performance.now()`
//! and `Date.now()`, through `web-time`). Everywhere else [`Instant`] is `std::time::Instant`.

use std::sync::Mutex;
use std::time::{Duration, SystemTime};

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub use std::time::Instant;
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub use web_time::Instant;

/// The current civil time, from the operating system.
#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn civil_now() -> SystemTime {
    SystemTime::now()
}

/// The current civil time, from the browser's `Date.now()`.
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn civil_now() -> SystemTime {
    let since = web_time::SystemTime::now()
        .duration_since(web_time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    SystemTime::UNIX_EPOCH + since
}

/// A source of time. The timer never reads a clock on its own: the host passes `now` into
/// [`Timer::due`](crate::Timer::due), and reads activation from the clock it activates with.
///
/// [`Clock::now`] is monotonic and places ticks; [`Clock::wall`] is the civil time a
/// `{generated: true}` `Timestamp` is stamped with. Both move together. A clock is shared with
/// the threads that read beside the host (`spindle_host::Reader`), so it is `Send` and `Sync`.
pub trait Clock: Send + Sync {
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
        civil_now()
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
        Self::at(civil_now())
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
