//! A served run's own health answers under `/__spindle/`, each answered without the host's thread
//! and without a recorder: `GET /__spindle/seed`, the seed's state and counts
//! (`spindle.observe.SeedState`), and `GET /__spindle/ready`, `200` once the seed has ended and
//! until the run stops, `503` otherwise (`spindle.observe.Readiness`, `story:seed-readiness`); and
//! `GET /__spindle/live`, `200` while the host's thread holds a request it took or asks for
//! requests within a bound (`spindle.observe.Liveness`, `story:health-and-graceful-drain`).

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde_json::{Value, json};

/// How long the host's thread, holding no request it took, may go without asking the listener for
/// one before `GET /__spindle/live` answers `503`, unless the listener is given another bound.
/// While it holds one, answering it however long that takes, it reads live.
pub const LIVENESS: Duration = Duration::from_secs(60);

/// Where a seed run is (`spindle.observe.SeedStateKind`, but for `none`, which is a run without a
/// seed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedPhase {
    /// An entry is left to submit, or the last one has not returned yet.
    Running,
    /// Every entry has been submitted and has returned: whatever an entry wrote is in place.
    Ended,
}

impl SeedPhase {
    /// The word `GET /__spindle/seed` answers it with.
    pub fn name(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Ended => "ended",
        }
    }
}

/// The seed run under way, or the last one, as `GET /__spindle/seed` answers it: a start's or a
/// reset's, its counts that run's alone, as its end line prints them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeedProgress {
    /// Whether it has ended.
    pub phase: SeedPhase,
    /// The entries of the seed file.
    pub entries: u64,
    /// The entries of this run not yet done: not yet sent, or to be sent again first.
    pub left: u64,
    /// Entries sent.
    pub sent: u64,
    /// Entries whose outcome took effect.
    pub applied: u64,
    /// Entries an `existing_instance:` outcome or an earlier request id answered.
    pub already_there: u64,
    /// Entries a declared refusing outcome answered.
    pub refused: u64,
    /// Entries that did not run: the host or the kernel refused them before an outcome.
    pub rejected: u64,
}

impl SeedProgress {
    /// A seed of `entries` entries about to be submitted from its first.
    pub fn starting(entries: u64) -> Self {
        Self {
            phase: SeedPhase::Running,
            entries,
            left: entries,
            sent: 0,
            applied: 0,
            already_there: 0,
            refused: 0,
            rejected: 0,
        }
    }
}

/// The state of a run's seed, published by the thread that submits it and read by the listener's
/// threads; clones share it. A surface given one ([`crate::Surface::with_seed`]) answers it at
/// `GET /__spindle/seed` and is ready only once it reads [`SeedPhase::Ended`].
#[derive(Debug, Clone)]
pub struct SeedState(Arc<Mutex<SeedProgress>>);

impl SeedState {
    /// A seed of `entries` entries, running, none sent.
    pub fn new(entries: u64) -> Self {
        Self(Arc::new(Mutex::new(SeedProgress::starting(entries))))
    }

    /// Publishes `progress`: every request read after this returns reads it.
    pub fn publish(&self, progress: SeedProgress) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = progress;
    }

    /// What was published last.
    pub fn read(&self) -> SeedProgress {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl PartialEq for SeedState {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for SeedState {}

/// What the health routes read for one request: the run's seed, whether the run is stopping,
/// whether the host's thread holds a request it took, and how long it has gone without asking
/// for one, against its bound.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Probed<'a> {
    pub seed: Option<&'a SeedState>,
    pub stopping: bool,
    pub holding: bool,
    pub idle: Duration,
    pub bound: Duration,
}

impl Probed<'_> {
    /// What a request answered on the host's thread reads ([`crate::route`]): that thread is
    /// answering it, and nothing stops the run.
    pub(crate) fn hosted(seed: Option<&SeedState>) -> Probed<'_> {
        Probed {
            seed,
            stopping: false,
            holding: true,
            idle: Duration::ZERO,
            bound: LIVENESS,
        }
    }
}

/// `GET /__spindle/seed`: `{"state": "none"}` without a seed; otherwise its state and counts.
pub(crate) fn seed(seed: Option<&SeedState>) -> Value {
    let Some(seed) = seed else {
        return json!({"state": "none"});
    };
    let progress = seed.read();
    json!({
        "state": progress.phase.name(),
        "entries": progress.entries,
        "left": progress.left,
        "sent": progress.sent,
        "applied": progress.applied,
        "already_there": progress.already_there,
        "refused": progress.refused,
        "rejected": progress.rejected,
    })
}

/// `GET /__spindle/ready`: `200` where the run has no seed or its seed has ended, and it is not
/// stopping; `503` while the seed runs and once the run stops. Its body says which.
pub(crate) fn ready(probed: &Probed<'_>) -> (u16, Value) {
    let state = probed.seed.map_or("none", |seed| seed.read().phase.name());
    let ready = state != SeedPhase::Running.name() && !probed.stopping;
    (
        if ready { 200 } else { 503 },
        json!({"ready": ready, "seed": state, "stopping": probed.stopping}),
    )
}

/// `GET /__spindle/live`: `200` while the host's thread holds a request it took, however long it
/// takes to answer it, or asked for one within the bound; `503` once it has done neither. Its body
/// gives the time since it last asked and the bound, in milliseconds.
pub(crate) fn live(probed: &Probed<'_>) -> (u16, Value) {
    let live = probed.holding || probed.idle <= probed.bound;
    let millis = |duration: Duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
    (
        if live { 200 } else { 503 },
        json!({"live": live, "idle_ms": millis(probed.idle), "bound_ms": millis(probed.bound)}),
    )
}
