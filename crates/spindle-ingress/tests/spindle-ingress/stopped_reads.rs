//! A reader stopped by its flag (`story:leader-exits-promptly-on-sigterm`,
//! [`spindle_host::Reader::stopped_by`]), as a listener's is by the flag a signal sets: a read of
//! the events or of a history cut off while it waits is answered `503`, as every request that
//! arrives once the run is stopping is; a read that completes is answered as before, the flag set
//! or not.
//!
//! The console fixture is `crates/spindle-cli/tests/fixtures/ingress`: `board` and `ledger`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock, Stores};
use spindle_ingress::{Answer, console};
use spindle_kernel::{
    Change, Committed, Event, MemoryStore, Origin, Position, Reading, Removal, Scan, Scanned,
    Store, StoreError, Stored, Write,
};

/// The events and history reads every case asks.
const EVENTS: [&str; 3] = ["", "limit=1", "after=head"];
const HISTORY: &str = "entity=tasks.board.Task&identity=t1";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

fn environment() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"stopped-reads\"\n",
        "os = \"linux\"\n",
        "arch = \"x86_64\"\n",
        "\n",
        "[provides]\n",
        "\"store.durable\" = [\"elsewhere\"]\n",
        "\"store.ephemeral\" = [\"elsewhere\"]\n",
        "clock = [\"system\"]\n",
        "timer = [\"tokio\"]\n",
        "channel = [\"in-process\"]\n",
        "projection = [\"in-process\"]\n",
        "\"identity.mint\" = [\"uuid-v4\"]\n",
        "\"ingress.network\" = [\"http-loopback\"]\n",
    ))
    .expect("the environment parses")
}

/// A memory store whose reading handle, where `waits`, waits in each stoppable read of its events
/// until the stop flag is set and then gives it up, as a PostgreSQL store's waits for a feed an
/// older transaction holds; without `waits` it reads as a memory store does.
struct Held {
    store: MemoryStore,
    waits: bool,
}

impl Store for Held {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.store.load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.store.commit(change)
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.store.remove(removal)
    }

    fn commit_all(&mut self, writes: &[Write]) -> Result<(), StoreError> {
        self.store.commit_all(writes)
    }

    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError> {
        self.store.scan(entity)
    }

    fn counts(&mut self, entities: &[String]) -> Result<BTreeMap<String, usize>, StoreError> {
        self.store.counts(entities)
    }

    fn reading(&mut self) -> Option<Arc<dyn Reading>> {
        let reading = self.store.reading()?;
        Some(Arc::new(HeldReading {
            reading,
            waits: self.waits,
        }))
    }

    fn attribute(&mut self, origin: Option<Origin>) {
        self.store.attribute(origin);
    }

    fn outcome(&mut self, outcome: &str) {
        self.store.outcome(outcome);
    }

    fn commit_events(&mut self, events: &[Event], scanned: &[Scanned]) -> Result<(), StoreError> {
        self.store.commit_events(events, scanned)
    }

    fn events_after(&mut self, position: u64, limit: usize) -> Result<Vec<Committed>, StoreError> {
        self.store.events_after(position, limit)
    }

    fn events_head(&mut self) -> Result<u64, StoreError> {
        self.store.events_head()
    }

    fn events_id(&mut self) -> Result<u64, StoreError> {
        self.store.events_id()
    }
}

/// The reading handle of a [`Held`] store.
struct HeldReading {
    reading: Arc<dyn Reading>,
    waits: bool,
}

impl HeldReading {
    /// Waits until `stop` is set, at most 10 s: `true` where it was.
    fn held(&self, stop: &AtomicBool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !stop.load(Ordering::SeqCst) {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        true
    }
}

impl Reading for HeldReading {
    fn scan(&self, entity: &str) -> Result<Scan, StoreError> {
        self.reading.scan(entity)
    }

    fn counts(&self, entities: &[String]) -> Result<BTreeMap<String, usize>, StoreError> {
        self.reading.counts(entities)
    }

    fn at(&self, entities: &[String]) -> Result<Box<dyn Position + '_>, StoreError> {
        self.reading.at(entities)
    }

    fn events_after(&self, position: u64, limit: usize) -> Result<Vec<Committed>, StoreError> {
        self.reading.events_after(position, limit)
    }

    fn events_after_unless_stopped(
        &self,
        position: u64,
        limit: usize,
        stop: &AtomicBool,
    ) -> Result<Option<Vec<Committed>>, StoreError> {
        if self.waits && self.held(stop) {
            return Ok(None);
        }
        self.reading.events_after(position, limit).map(Some)
    }

    fn events_head(&self) -> Result<u64, StoreError> {
        self.reading.events_head()
    }

    fn events_head_unless_stopped(&self, stop: &AtomicBool) -> Result<Option<u64>, StoreError> {
        if self.waits && self.held(stop) {
            return Ok(None);
        }
        self.reading.events_head().map(Some)
    }

    fn events_id(&self) -> Result<u64, StoreError> {
        self.reading.events_id()
    }
}

/// A host whose board's store holds two opened tasks, its reads waiting where `waits`.
fn host(waits: bool) -> Host {
    let mut board = MemoryStore::default();
    for (index, task) in ["t1", "t2"].into_iter().enumerate() {
        board.attribute(Some(Origin {
            command: "tasks.board.OpenTask".to_owned(),
            sequence: u64::try_from(index).expect("an index fits") + 1,
            cause: None,
        }));
        board
            .commit_events(
                &[Event {
                    name: "tasks.board.TaskOpened".to_owned(),
                    payload: object(json!({"task_id": task, "title": "Open"})),
                }],
                &[],
            )
            .expect("the board records the event");
    }
    board.attribute(None);
    let given: Stores = [
        (
            "board".to_owned(),
            Box::new(Held {
                store: board,
                waits,
            }) as Box<dyn Store>,
        ),
        (
            "ledger".to_owned(),
            Box::new(MemoryStore::default()) as Box<dyn Store>,
        ),
    ]
    .into_iter()
    .collect();
    let ir = spindle_spec::load(&root().join("crates/spindle-cli/tests/fixtures/ingress"))
        .expect("the console fixture compiles")
        .ir;
    Host::supplied(
        ir,
        &environment(),
        Arc::new(ManualClock::new()),
        None,
        given,
    )
    .expect("the host on given stores opens")
}

/// Every read [`EVENTS`] and [`HISTORY`] name, through a reader stopped by `stop`, each on a
/// thread of its own: their answers, in that order.
fn read_all(host: &Host, stop: &Arc<AtomicBool>) -> Vec<std::thread::JoinHandle<Answer>> {
    let mut reads = Vec::new();
    for query in EVENTS {
        let reader = host.reader().stopped_by(Arc::clone(stop));
        reads.push(std::thread::spawn(move || {
            console::events_read_queried(&reader, query)
        }));
    }
    let reader = host.reader().stopped_by(Arc::clone(stop));
    reads.push(std::thread::spawn(move || {
        console::history_read_queried(&reader, HISTORY)
    }));
    reads
}

/// A read cut off while it waits for the store, the flag set under it, is answered `503`
/// `{"refused": "the run is stopping"}`; each returns once the flag is set.
#[test]
fn a_read_cut_off_while_it_waits_is_answered_503() {
    let host = host(true);
    let stop = Arc::new(AtomicBool::new(false));
    let reads = read_all(&host, &stop);
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        reads.iter().all(|read| !read.is_finished()),
        "every read waits for the store"
    );
    let stopped = Instant::now();
    stop.store(true, Ordering::SeqCst);
    for read in reads {
        let answer = read.join().expect("the read ends");
        assert_eq!(
            (answer.status, answer.body),
            (503, Some(json!({"refused": "the run is stopping"}))),
            "a read cut off is refused as the run is stopping"
        );
    }
    let took = stopped.elapsed();
    assert!(
        took < Duration::from_secs(1),
        "the reads ended {took:?} after the flag"
    );
}

/// A read that waits for nothing completes and is answered as without the flag, though the flag
/// is set before it starts: `200` with its events and the history.
#[test]
fn a_read_that_completes_is_answered_with_the_flag_set() {
    let host = host(false);
    let down = Arc::new(AtomicBool::new(false));
    let unflagged: Vec<(u16, Option<Value>)> = read_all(&host, &down)
        .into_iter()
        .map(|read| {
            let answer = read.join().expect("the read ends");
            (answer.status, answer.body)
        })
        .collect();
    assert!(
        unflagged.iter().all(|(status, _)| *status == 200),
        "{unflagged:?}"
    );
    let up = Arc::new(AtomicBool::new(true));
    let flagged: Vec<(u16, Option<Value>)> = read_all(&host, &up)
        .into_iter()
        .map(|read| {
            let answer = read.join().expect("the read ends");
            (answer.status, answer.body)
        })
        .collect();
    assert_eq!(
        flagged, unflagged,
        "the flag changes no read that completes"
    );
}
