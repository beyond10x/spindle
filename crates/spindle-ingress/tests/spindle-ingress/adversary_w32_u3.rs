//! Adversary of wave 32 U3 (`story:leader-exits-promptly-on-sigterm`), round 2: the listener's
//! reader is stopped by the flag a signal sets ([`spindle_host::Reader::stopped_by`]), so an
//! events read waiting for a store's feed does not hold the exit.
//!
//! `run --help` says of the signal: "every request that arrives is answered `503` at once; the
//! request under way finishes". A read whose store needs time and waits for no feed, as a
//! SQLite store reading a long log does, has every event in hand when it returns; the case reads
//! the contract literally: a `GET /api/events` that arrived before the flag was set and read its
//! whole answer is answered with it.
//!
//! The console fixture is `crates/spindle-cli/tests/fixtures/ingress`: `board` and `ledger`.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock, Stores};
use spindle_ingress::{HTTP_LOOPBACK, Listener, Surface};
use spindle_kernel::{
    Change, Committed, Event, MemoryStore, Origin, Position, Reading, Removal, Scan, Store,
    StoreError, Stored, Write as Written,
};

/// How long each read of the slow store's events takes.
const SLOW: Duration = Duration::from_millis(800);

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
        "name = \"slow-events\"\n",
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

/// A memory store whose reading handle takes [`SLOW`] for each read of its events: a store that
/// needs time for a long log and waits for no feed.
struct SlowEvents(MemoryStore);

impl Store for SlowEvents {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.0.load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.0.commit(change)
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.0.remove(removal)
    }

    fn commit_all(&mut self, writes: &[Written]) -> Result<(), StoreError> {
        self.0.commit_all(writes)
    }

    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError> {
        self.0.scan(entity)
    }

    fn counts(&mut self, entities: &[String]) -> Result<BTreeMap<String, usize>, StoreError> {
        self.0.counts(entities)
    }

    fn reading(&mut self) -> Option<Arc<dyn Reading>> {
        let inner = self.0.reading()?;
        Some(Arc::new(SlowReading(inner)))
    }

    fn attribute(&mut self, origin: Option<Origin>) {
        self.0.attribute(origin);
    }

    fn outcome(&mut self, outcome: &str) {
        self.0.outcome(outcome);
    }

    fn commit_events(
        &mut self,
        events: &[Event],
        scanned: &[spindle_kernel::Scanned],
    ) -> Result<(), StoreError> {
        self.0.commit_events(events, scanned)
    }

    fn events_after(&mut self, position: u64, limit: usize) -> Result<Vec<Committed>, StoreError> {
        self.0.events_after(position, limit)
    }

    fn events_head(&mut self) -> Result<u64, StoreError> {
        self.0.events_head()
    }

    fn events_id(&mut self) -> Result<u64, StoreError> {
        self.0.events_id()
    }
}

/// The reading handle of a [`SlowEvents`] store.
struct SlowReading(Arc<dyn Reading>);

impl Reading for SlowReading {
    fn scan(&self, entity: &str) -> Result<Scan, StoreError> {
        self.0.scan(entity)
    }

    fn counts(&self, entities: &[String]) -> Result<BTreeMap<String, usize>, StoreError> {
        self.0.counts(entities)
    }

    fn at(&self, entities: &[String]) -> Result<Box<dyn Position + '_>, StoreError> {
        self.0.at(entities)
    }

    fn events_after(&self, position: u64, limit: usize) -> Result<Vec<Committed>, StoreError> {
        std::thread::sleep(SLOW);
        self.0.events_after(position, limit)
    }

    fn events_head(&self) -> Result<u64, StoreError> {
        std::thread::sleep(SLOW);
        self.0.events_head()
    }

    fn events_id(&self) -> Result<u64, StoreError> {
        self.0.events_id()
    }
}

/// A host whose board's store holds three events and reads them slowly beside the host's thread.
fn slow_host() -> Host {
    let mut board = MemoryStore::default();
    for index in 0..3_u64 {
        board.attribute(Some(Origin {
            command: "tasks.board.OpenTask".to_owned(),
            sequence: index + 1,
            cause: None,
        }));
        board
            .commit_events(
                &[Event {
                    name: "tasks.board.TaskOpened".to_owned(),
                    payload: object(json!({"task_id": format!("t{index}"), "title": "Old"})),
                }],
                &[],
            )
            .expect("the board records the event");
    }
    board.attribute(None);
    let given: Stores = [
        (
            "board".to_owned(),
            Box::new(SlowEvents(board)) as Box<dyn Store>,
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

/// `GET target`: the status and the body.
fn get(address: SocketAddr, target: &str) -> (u16, Value) {
    let mut stream = TcpStream::connect(address).expect("the listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("a read timeout");
    write!(
        stream,
        "GET {target} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    )
    .expect("the request is written");
    let mut response = String::new();
    let _ = stream.read_to_string(&mut response);
    let (head, body) = response.split_once("\r\n\r\n").unwrap_or((&response, ""));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or(0);
    (status, serde_json::from_str(body).unwrap_or(Value::Null))
}

/// `run --help`: on the signal "the request under way finishes". A `GET /api/events` that
/// arrived before the flag the signal sets ([`Listener::stopping`]) and that reads every event
/// it lists, waiting for no feed, is answered `200` with the three events, as it was before the
/// listener's reader was stopped by that flag.
#[test]
fn an_events_read_under_way_that_read_every_event_is_answered_with_them_after_the_signal() {
    let host = slow_host();
    let ir = host.ir().clone();
    let surface = Surface::new(&ir, &["board".to_owned()], None)
        .and_then(|surface| surface.with_console(HTTP_LOOPBACK))
        .unwrap_or_else(|refused| panic!("the board surface is served: {refused}"));
    let listener = Listener::bind_reading(0, surface, host.reader()).expect("the listener binds");
    let address = listener.local_addr();

    let (status, body) = get(address, "/api/events");
    assert_eq!(status, 200, "the read answers before the signal: {body}");
    let listed = body["events"].as_array().map_or(0, Vec::len);
    assert_eq!(listed, 3, "{body}");

    let started = Instant::now();
    let reader = std::thread::spawn(move || get(address, "/api/events"));
    std::thread::sleep(SLOW / 4);
    listener.stopping().store(true, Ordering::SeqCst);
    let (status, body) = reader.join().expect("the read's thread ends");
    let took = started.elapsed();
    listener.close(Duration::from_secs(2));
    assert!(
        took >= SLOW,
        "the read took {took:?}: it read its events before it answered"
    );
    assert_eq!(
        (status, body["events"].as_array().map_or(0, Vec::len)),
        (200, 3),
        "the read under way when the run was signalled had all three events in hand after \
         {took:?} and answered {status} {body}"
    );
}
