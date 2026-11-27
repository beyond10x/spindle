//! A served run's own health answers under `/__spindle/`, each answered without the host's thread
//! and without a recorder: `GET /__spindle/seed`, the seed's state and counts
//! (`spindle.observe.SeedState`), and `GET /__spindle/ready`, `200` once the seed has ended, `503`
//! before (`spindle.observe.Readiness`, `story:seed-readiness`).

use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Value, json};

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

/// `GET /__spindle/ready`: `200` where the run has no seed or its seed has ended, `503` while it
/// runs. Its body says which.
pub(crate) fn ready(seed: Option<&SeedState>) -> (u16, Value) {
    let state = seed.map_or("none", |seed| seed.read().phase.name());
    let ready = state != SeedPhase::Running.name();
    (
        if ready { 200 } else { 503 },
        json!({"ready": ready, "seed": state}),
    )
}
