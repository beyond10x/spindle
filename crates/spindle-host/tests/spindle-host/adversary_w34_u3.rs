//! Adversary cases of wave 34, unit U3 (`story:reset-on-a-busy-store-keeps-the-store`).
//!
//! The unit keeps a supplied store whose reset step failed [`StoreError::Busy`] and loses it on
//! every other store error. Its own cases pin a busy first step, a busy second store after one
//! that only forgot its events, and a `Backend` removal. These pin what they leave open:
//!
//! - "Only `StoreError::Busy` keeps the store": a classification that also kept the store on a
//!   [`StoreError::Conflict`] (a removal whose instance moved under it) leaves the unit's cases
//!   green. A conflicting removal must lose the store as today.
//! - The busy step at every position of a reset of `n` rows, the reset mark after the last
//!   removal included: the rows removed before it stay removed, the rest are kept,
//!   `reset_changed_rows` is `true` exactly when a removal landed, a command then commits on the
//!   same store, and the reset sent again empties it.
//!
//! The fixture `tests/fixtures/notebooks` declares one component, `desk`, owning `Notebook`,
//! created by `desk.notebooks.OpenNotebook`.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, HostError, ManualClock, Stores};
use spindle_kernel::{Change, MemoryStore, Removal, Scan, Store, StoreError, Stored};

const NOTEBOOK: &str = "desk.notebooks.Notebook";
const OPEN: &str = "desk.notebooks.OpenNotebook";
/// What eventlog's `Deadline` reads, as the PostgreSQL store's lock wait ends it.
const DEADLINE: &str = "event store deadline exceeded during database statement or lock";

fn elsewhere() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"adversary-w34-u3\"\n",
        "os = \"linux\"\n",
        "arch = \"x86_64\"\n",
        "\n",
        "[provides]\n",
        "\"store.durable\" = [\"elsewhere\"]\n",
        "\"store.ephemeral\" = [\"elsewhere\"]\n",
        "\"identity.mint\" = [\"uuid-v4\"]\n",
        "clock = [\"system\"]\n",
        "channel = [\"in-process\"]\n",
        "projection = [\"in-process\"]\n",
    ))
    .expect("the environment parses")
}

/// How a [`Switched`] store fails a reset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fail {
    /// Every step answers.
    Never,
    /// The removal with this index (from 0, counted from when the switch was set) fails busy.
    BusyAtRemoval(usize),
    /// Recording that its events are forgotten fails busy.
    BusyAtMark,
    /// Every removal fails with a conflict, as an instance moved between the reset's scan and its
    /// removal.
    Conflicting,
}

/// A memory store the test keeps a handle to, failing as its switch says.
#[derive(Clone)]
struct Switched {
    store: Arc<Mutex<MemoryStore>>,
    fail: Arc<Mutex<Fail>>,
    removals: Arc<AtomicUsize>,
}

impl Switched {
    fn new() -> Self {
        Self {
            store: Arc::default(),
            fail: Arc::new(Mutex::new(Fail::Never)),
            removals: Arc::default(),
        }
    }

    fn set(&self, fail: Fail) {
        self.removals.store(0, Ordering::SeqCst);
        *self.fail.lock().expect("the switch") = fail;
    }

    fn failing(&self) -> Fail {
        *self.fail.lock().expect("the switch")
    }

    fn rows(&self) -> usize {
        self.store
            .lock()
            .expect("the store")
            .scan(NOTEBOOK)
            .expect("the store reads")
            .rows
            .len()
    }
}

impl Store for Switched {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.store.lock().expect("the store").load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.store.lock().expect("the store").commit(change)
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        let index = self.removals.fetch_add(1, Ordering::SeqCst);
        match self.failing() {
            Fail::BusyAtRemoval(busy) if busy == index => {
                Err(StoreError::Busy(DEADLINE.to_owned()))
            }
            Fail::Conflicting => Err(StoreError::Conflict {
                entity: removal.entity.clone(),
                identity: removal.identity.to_string(),
                expected: Some(removal.expected),
                actual: Some(removal.expected + 1),
            }),
            _ => self.store.lock().expect("the store").remove(removal),
        }
    }

    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError> {
        self.store.lock().expect("the store").scan(entity)
    }

    fn forget_events(&mut self) -> Result<(), StoreError> {
        if self.failing() == Fail::BusyAtMark {
            return Err(StoreError::Busy(DEADLINE.to_owned()));
        }
        self.store.lock().expect("the store").forget_events()
    }
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

fn host_on(store: &Switched) -> Host {
    let ir =
        spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/notebooks"))
            .expect("the notebooks fixture compiles")
            .ir;
    Host::supplied(
        ir,
        &elsewhere(),
        Arc::new(ManualClock::new()),
        None,
        Stores::from([("desk".to_owned(), Box::new(store.clone()) as Box<dyn Store>)]),
    )
    .unwrap_or_else(|error| panic!("the host opens on the supplied store: {error}"))
}

fn open(host: &mut Host, title: &str) {
    host.submit(OPEN, object(json!({"title": title})))
        .unwrap_or_else(|error| panic!("OpenNotebook {title} runs: {error}"));
}

/// The story's "Decided when scoped": "Only `StoreError::Busy` keeps the store. Every other store
/// error loses it and answers `500` as today." A removal refused with a conflict is such an
/// error: the reset answers `HostError::Reset` (`restored: false`), and every later command
/// names the store lost in a reset. Kills the mutant that classifies `Busy | Conflict` as kept,
/// which the unit's own cases (`Backend` only) leave green.
#[test]
fn a_reset_whose_removal_conflicts_still_loses_the_store() {
    let store = Switched::new();
    let mut host = host_on(&store);
    open(&mut host, "Field notes");

    store.set(Fail::Conflicting);
    let error = host
        .reset()
        .expect_err("the conflicting removal fails the reset");
    assert!(
        matches!(&error, HostError::Reset { component, restored: false, .. } if component == "desk"),
        "a removal that conflicts loses the store as today: {error:?}"
    );

    store.set(Fail::Never);
    let later = host
        .submit(OPEN, object(json!({"title": "After the reset"})))
        .expect_err("the lost store refuses every write");
    assert!(
        later.to_string().contains("lost in a reset"),
        "a later command names the store lost in a reset: {later}"
    );
    assert_eq!(store.rows(), 1, "nothing reaches the store the host lost");
}

/// A supplied store busy at any step of its reset is kept, whatever step: for a store holding
/// `n` rows (1 to 3), busy at removal `k` (0 to `n - 1`) or at the reset mark after the last
/// removal (`k = n`), the reset answers the busy error naming `desk` and the step, the `k` rows
/// removed before it stay removed and the rest are kept, `reset_changed_rows` is `true` exactly
/// when a removal landed (`k > 0`), a command then commits on the same store, and the reset sent
/// again once the store answers empties it.
#[test]
fn a_busy_step_at_any_position_keeps_the_store_and_counts_what_landed() {
    for held in 1..=3_usize {
        for busy in 0..=held {
            let case = format!("{held} rows, busy at step {busy}");
            let store = Switched::new();
            let mut host = host_on(&store);
            for index in 0..held {
                open(&mut host, &format!("notes {index}"));
            }
            assert_eq!(store.rows(), held, "{case}");

            let (fail, step) = if busy == held {
                (Fail::BusyAtMark, "forgetting its events")
            } else {
                (Fail::BusyAtRemoval(busy), "removing")
            };
            store.set(fail);
            let error = host.reset().expect_err("the busy store holds the reset");
            assert!(
                matches!(&error, HostError::Store(StoreError::Busy(detail))
                    if detail.contains("desk") && detail.contains(step)
                        && detail.contains(DEADLINE)),
                "{case}: the busy error names desk, {step} and the store's detail: {error:?}"
            );
            assert_eq!(
                host.reset_changed_rows(),
                busy > 0,
                "{case}: rows changed exactly when a removal landed"
            );
            assert_eq!(
                store.rows(),
                held - busy,
                "{case}: the rows removed before the busy step stay removed, the rest are kept"
            );

            store.set(Fail::Never);
            open(&mut host, "after the busy reset");
            assert_eq!(
                store.rows(),
                held - busy + 1,
                "{case}: a command commits on the store the host kept"
            );
            assert_eq!(
                host.reset().expect("the reset sent again"),
                vec!["desk".to_owned()],
                "{case}"
            );
            assert_eq!(
                store.rows(),
                0,
                "{case}: the reset sent again empties the store"
            );
            assert!(host.reset_changed_rows(), "{case}");
        }
    }
}
