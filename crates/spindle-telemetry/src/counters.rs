//! The default sink: atomic counters and fixed-bucket histograms, one set per instance.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::catalogue::{Catalogue, Key, Kind};
use crate::port::{Carried, Failure, Outcome, Record, Retried, Telemetry, Timed};

/// The upper bounds, in microseconds, of a duration histogram's buckets. A histogram has one
/// count per bound, each counting the values above the previous bound and at most this one, and
/// one more for the values above the last.
pub const DURATION_BOUNDS_US: [u64; 12] = [
    10, 50, 100, 250, 500, 1_000, 2_500, 5_000, 10_000, 50_000, 250_000, 1_000_000,
];

/// One instance's counters, laid out by its [`Catalogue`].
///
/// Recording looks a name up in maps built once, here, and increments one atomic: it takes no
/// lock and allocates nothing. A record whose name the catalogue lacks is counted in
/// [`Self::uncatalogued`]: the IR does not name it, which is a bug in whatever reported it.
#[derive(Debug)]
pub struct Counters {
    catalogue: Catalogue,
    index: Index,
    slots: Box<[Slot]>,
    uncatalogued: AtomicU64,
}

#[derive(Debug)]
enum Slot {
    Counter(AtomicU64),
    Histogram(Box<[AtomicU64]>),
}

#[derive(Debug, Default)]
struct Index {
    commands: HashMap<String, CommandIndex>,
    appended: HashMap<String, usize>,
    carried: HashMap<String, [Option<usize>; Carried::ALL.len()]>,
    retried: HashMap<String, [Option<usize>; Retried::ALL.len()]>,
    timed: HashMap<String, [Option<usize>; Timed::ALL.len()]>,
    rows_written: Option<usize>,
    bytes_written: Option<usize>,
}

#[derive(Debug, Default)]
struct CommandIndex {
    outcomes: HashMap<String, usize>,
    failed: [Option<usize>; Failure::ALL.len()],
    duration: Option<usize>,
}

/// Every value of one instance's counters at one moment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Snapshot {
    /// The component.
    pub component: String,
    /// Each counter, in catalogue order.
    pub counters: Vec<CounterValue>,
    /// Each histogram, in catalogue order.
    pub histograms: Vec<HistogramValue>,
    /// Records whose name the catalogue lacks.
    pub uncatalogued: u64,
}

/// One counter's value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CounterValue {
    /// Its name.
    pub name: String,
    /// Its cumulative value.
    pub value: u64,
}

/// One histogram's buckets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistogramValue {
    /// Its name.
    pub name: String,
    /// The buckets' upper bounds; see [`DURATION_BOUNDS_US`].
    pub bounds: Vec<u64>,
    /// One cumulative count per bound, then one for the values above the last.
    pub counts: Vec<u64>,
}

impl Counters {
    /// Zeroed counters for every entry of `catalogue`.
    pub fn new(catalogue: Catalogue) -> Self {
        let mut index = Index::default();
        let mut slots = Vec::with_capacity(catalogue.counters.len());
        for (position, counter) in catalogue.counters.iter().enumerate() {
            slots.push(match counter.kind {
                Kind::Counter => Slot::Counter(AtomicU64::new(0)),
                Kind::Histogram => Slot::Histogram(
                    (0..=DURATION_BOUNDS_US.len())
                        .map(|_| AtomicU64::new(0))
                        .collect(),
                ),
            });
            index.insert(&counter.key, position);
        }
        Self {
            catalogue,
            index,
            slots: slots.into_boxed_slice(),
            uncatalogued: AtomicU64::new(0),
        }
    }

    /// The catalogue these counters follow.
    pub fn catalogue(&self) -> &Catalogue {
        &self.catalogue
    }

    /// The value of the counter `name`; `None` when the catalogue has no counter of that name.
    pub fn value(&self, name: &str) -> Option<u64> {
        let position = self.position(name)?;
        match &self.slots[position] {
            Slot::Counter(value) => Some(value.load(Ordering::Relaxed)),
            Slot::Histogram(_) => None,
        }
    }

    /// The buckets of the histogram `name`; `None` when the catalogue has no histogram of that
    /// name.
    pub fn histogram(&self, name: &str) -> Option<HistogramValue> {
        let position = self.position(name)?;
        match &self.slots[position] {
            Slot::Histogram(counts) => Some(HistogramValue {
                name: name.to_owned(),
                bounds: DURATION_BOUNDS_US.to_vec(),
                counts: counts
                    .iter()
                    .map(|count| count.load(Ordering::Relaxed))
                    .collect(),
            }),
            Slot::Counter(_) => None,
        }
    }

    /// Records whose name the catalogue lacks.
    pub fn uncatalogued(&self) -> u64 {
        self.uncatalogued.load(Ordering::Relaxed)
    }

    /// Every value now. Each value is read on its own, so a snapshot taken while commands run
    /// is not one instant across counters; each counter in it only ever grows.
    pub fn snapshot(&self) -> Snapshot {
        let mut counters = Vec::new();
        let mut histograms = Vec::new();
        for (counter, slot) in self.catalogue.counters.iter().zip(self.slots.iter()) {
            match slot {
                Slot::Counter(value) => counters.push(CounterValue {
                    name: counter.name.clone(),
                    value: value.load(Ordering::Relaxed),
                }),
                Slot::Histogram(counts) => histograms.push(HistogramValue {
                    name: counter.name.clone(),
                    bounds: DURATION_BOUNDS_US.to_vec(),
                    counts: counts
                        .iter()
                        .map(|count| count.load(Ordering::Relaxed))
                        .collect(),
                }),
            }
        }
        Snapshot {
            component: self.catalogue.component.clone(),
            counters,
            histograms,
            uncatalogued: self.uncatalogued(),
        }
    }

    fn position(&self, name: &str) -> Option<usize> {
        self.catalogue
            .counters
            .iter()
            .position(|counter| counter.name == name)
    }
}

impl Index {
    fn insert(&mut self, key: &Key, position: usize) {
        match key {
            Key::Outcome { command, outcome } => {
                self.commands
                    .entry(command.clone())
                    .or_default()
                    .outcomes
                    .insert(outcome.clone(), position);
            }
            Key::Failed { command, failure } => {
                self.commands.entry(command.clone()).or_default().failed[failure.index()] =
                    Some(position);
            }
            Key::Duration { command } => {
                self.commands.entry(command.clone()).or_default().duration = Some(position);
            }
            Key::Appended { event } => {
                self.appended.insert(event.clone(), position);
            }
            Key::Carried { binding, carried } => {
                self.carried.entry(binding.clone()).or_default()[carried.index()] = Some(position);
            }
            Key::Retried { binding, retried } => {
                self.retried.entry(binding.clone()).or_default()[retried.index()] = Some(position);
            }
            Key::Timed { binding, timed } => {
                self.timed.entry(binding.clone()).or_default()[timed.index()] = Some(position);
            }
            Key::RowsWritten => self.rows_written = Some(position),
            Key::BytesWritten => self.bytes_written = Some(position),
        }
    }
}

impl Counters {
    fn add(&self, position: Option<usize>, by: u64) -> bool {
        match position.map(|position| &self.slots[position]) {
            Some(Slot::Counter(value)) => {
                value.fetch_add(by, Ordering::Relaxed);
                true
            }
            _ => false,
        }
    }

    fn observe(&self, position: Option<usize>, elapsed: Duration) {
        if let Some(Slot::Histogram(counts)) = position.map(|position| &self.slots[position]) {
            let micros = u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX);
            let bucket = DURATION_BOUNDS_US
                .iter()
                .position(|bound| micros <= *bound)
                .unwrap_or(DURATION_BOUNDS_US.len());
            counts[bucket].fetch_add(1, Ordering::Relaxed);
        }
    }

    fn miss(&self) {
        self.uncatalogued.fetch_add(1, Ordering::Relaxed);
    }
}

impl Telemetry for Counters {
    fn record(&self, record: Record<'_>) {
        let counted = match record {
            Record::Command {
                command,
                outcome,
                elapsed,
            } => match self.index.commands.get(command) {
                None => false,
                Some(index) => {
                    let position = match outcome {
                        Outcome::Declared(outcome) => index.outcomes.get(outcome).copied(),
                        Outcome::Failed(failure) => index.failed[failure.index()],
                    };
                    let counted = self.add(position, 1);
                    if counted {
                        self.observe(index.duration, elapsed);
                    }
                    counted
                }
            },
            Record::Appended { event } => self.add(self.index.appended.get(event).copied(), 1),
            Record::Written { rows, bytes } => {
                self.add(self.index.rows_written, rows) & self.add(self.index.bytes_written, bytes)
            }
            Record::Carried { binding, carried } => self.add(
                self.index
                    .carried
                    .get(binding)
                    .and_then(|positions| positions[carried.index()]),
                1,
            ),
            Record::Retried { binding, retried } => self.add(
                self.index
                    .retried
                    .get(binding)
                    .and_then(|positions| positions[retried.index()]),
                1,
            ),
            Record::Timed {
                binding,
                timed,
                count,
            } => self.add(
                self.index
                    .timed
                    .get(binding)
                    .and_then(|positions| positions[timed.index()]),
                count,
            ),
        };
        if !counted {
            self.miss();
        }
    }
}
