//! The samples the control process keeps: the last [`WINDOW`] per instance, in memory.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde_json::Value;

/// How long a sample is kept: the last 15 minutes (`docs/design/observe.md` § 5).
pub const WINDOW: Duration = Duration::from_secs(15 * 60);

/// Where the time comes from, injected so a test can move it.
pub trait Clock: Send + Sync {
    /// Now.
    fn now(&self) -> SystemTime;
}

/// The system's wall clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// The samples of every instance taken within the window before now, oldest first.
///
/// A sample is stamped with the clock when it is pushed. One stamped more than the window before
/// now is dropped; one stamped exactly the window before now is still held.
pub struct History {
    window: Duration,
    clock: Arc<dyn Clock>,
    samples: BTreeMap<String, VecDeque<(SystemTime, Value)>>,
}

impl std::fmt::Debug for History {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("History")
            .field("window", &self.window)
            .field("instances", &self.samples.len())
            .finish()
    }
}

impl History {
    /// An empty history keeping `window` on `clock`.
    pub fn new(window: Duration, clock: Arc<dyn Clock>) -> Self {
        Self {
            window,
            clock,
            samples: BTreeMap::new(),
        }
    }

    /// Keeps `sample` of `instance`, stamped now, and drops every sample of every instance that
    /// has left the window.
    pub fn push(&mut self, instance: &str, sample: Value) {
        let now = self.clock.now();
        self.samples
            .entry(instance.to_owned())
            .or_default()
            .push_back((now, sample));
        self.prune(now);
    }

    /// The samples of `instance` within the window before now, oldest first.
    pub fn samples(&self, instance: &str) -> Vec<Value> {
        let oldest = self.oldest(self.clock.now());
        self.samples
            .get(instance)
            .map(|held| {
                held.iter()
                    .filter(|(at, _)| oldest.is_none_or(|oldest| *at >= oldest))
                    .map(|(_, sample)| sample.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Every instance with a sample held, in name order.
    pub fn instances(&self) -> Vec<String> {
        self.samples.keys().cloned().collect()
    }

    fn oldest(&self, now: SystemTime) -> Option<SystemTime> {
        now.checked_sub(self.window)
    }

    fn prune(&mut self, now: SystemTime) {
        let Some(oldest) = self.oldest(now) else {
            return;
        };
        for held in self.samples.values_mut() {
            while held.front().is_some_and(|(at, _)| *at < oldest) {
                held.pop_front();
            }
        }
        self.samples.retain(|_, held| !held.is_empty());
    }
}
