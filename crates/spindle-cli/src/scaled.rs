//! A clock that runs a whole number of times faster than real time, for `play --speed`.

use std::time::{Duration, Instant, SystemTime};

use spindle_host::Clock;

/// The host's clock, sped up: from the instant it was made, each real second is `speed` seconds.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Scaled {
    origin: Instant,
    wall: SystemTime,
    speed: u32,
}

impl Scaled {
    /// A clock standing at the current instant and civil time, running `speed` times faster.
    pub(crate) fn new(speed: u32) -> Self {
        Self {
            origin: Instant::now(),
            wall: SystemTime::now(),
            speed: speed.max(1),
        }
    }

    fn elapsed(&self) -> Duration {
        self.origin.elapsed().saturating_mul(self.speed)
    }

    /// How much real time is left until the clock reads `deadline`.
    pub(crate) fn until(&self, deadline: Instant) -> Duration {
        deadline.saturating_duration_since(self.now()) / self.speed
    }
}

impl Clock for Scaled {
    fn now(&self) -> Instant {
        self.origin + self.elapsed()
    }

    fn wall(&self) -> SystemTime {
        self.wall + self.elapsed()
    }
}
