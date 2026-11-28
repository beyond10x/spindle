//! Adversary cases of wave 34, unit U3 (`story:reset-on-a-busy-store-keeps-the-store`).
//!
//! The story: "A `POST /api/reset` whose store misses its deadline ... keeps the component's
//! store, so the next command runs as before. While the reset has changed no row, it answers
//! `503` with `{"refused", "committed": false}` and nothing reset. Once it has changed rows (a row
//! removed, an earlier store emptied, a world row written), it answers `500`, saying that the
//! store is kept and that the reset can be sent again." The unit's cases send resets without a
//! world. These send a world: a row of it whose write the store holds up busy, after the store's
//! rows were removed and on a store that held none, and a world whose rows the reset checks
//! against a store that is busy before anything is emptied.
//!
//! The fixture `tests/fixtures/desk` declares one component, `desk`, owning `Desk`, created by
//! `study.desk.OpenDesk`.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ess_compiler::EssIr;
use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock, Seed, Stores, World};
use spindle_ingress::{Answer, console};
use spindle_kernel::{
    Change, MemoryStore, Reading, Removal, Row, Scan, Store, StoreError, Stored, Write as Written,
};

const COMPONENT: &str = "desk";
const DESK: &str = "study.desk.Desk";
const OPEN: &str = "study.desk.OpenDesk";
/// What eventlog's `Deadline` reads, as the PostgreSQL store's lock wait ends it.
const DEADLINE: &str = "event store deadline exceeded during database statement or lock";

fn environment() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"adversary-w34-u3\"\n",
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

/// A memory store whose loads, or whose row writes (not its removals or its reset mark), fail
/// busy while their switch is on.
struct Switched {
    inner: MemoryStore,
    loads_busy: Arc<AtomicBool>,
    writes_busy: Arc<AtomicBool>,
}

fn busy() -> StoreError {
    StoreError::Busy(DEADLINE.to_owned())
}

impl Store for Switched {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        if self.loads_busy.load(Ordering::SeqCst) {
            return Err(busy());
        }
        self.inner.load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        if self.writes_busy.load(Ordering::SeqCst) {
            return Err(busy());
        }
        self.inner.commit(change)
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.inner.remove(removal)
    }

    fn commit_all(&mut self, writes: &[Written]) -> Result<(), StoreError> {
        if self.writes_busy.load(Ordering::SeqCst) {
            return Err(busy());
        }
        self.inner.commit_all(writes)
    }

    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError> {
        self.inner.scan(entity)
    }

    fn reading(&mut self) -> Option<Arc<dyn Reading>> {
        self.inner.reading()
    }

    fn forget_events(&mut self) -> Result<(), StoreError> {
        self.inner.forget_events()
    }
}

struct Built {
    host: Host,
    loads_busy: Arc<AtomicBool>,
    writes_busy: Arc<AtomicBool>,
}

fn built() -> Built {
    let ir = spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/desk"))
        .expect("the desk fixture compiles")
        .ir;
    let loads_busy = Arc::new(AtomicBool::new(false));
    let writes_busy = Arc::new(AtomicBool::new(false));
    let store = Switched {
        inner: MemoryStore::default(),
        loads_busy: Arc::clone(&loads_busy),
        writes_busy: Arc::clone(&writes_busy),
    };
    let host = Host::supplied(
        ir,
        &environment(),
        Arc::new(ManualClock::new()),
        None,
        Stores::from([(COMPONENT.to_owned(), Box::new(store) as Box<dyn Store>)]),
    )
    .unwrap_or_else(|error| panic!("the host opens on the store: {error}"));
    Built {
        host,
        loads_busy,
        writes_busy,
    }
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

fn open(host: &mut Host, id: &str) -> Result<String, String> {
    host.submit(OPEN, object(json!({"desk_id": id, "holder": "a reader"})))
        .map(|report| report.ran[0].executed.outcome.clone())
        .map_err(|error| error.to_string())
}

/// The world every reset body here reads as, whatever the body: one free desk, `desk-9`.
fn one_desk(_: &EssIr, _: &Value) -> Result<World, String> {
    Ok(World {
        rows: vec![Seed {
            entity: DESK.to_owned(),
            identity: json!("desk-9"),
            row: Row {
                state: "Free".to_owned(),
                fields: [("holder".to_owned(), json!("a seeded reader"))].into(),
            },
        }],
        ..World::default()
    })
}

fn reset_with_world(host: &mut Host) -> Answer {
    console::reset(host, br#"{"rows": "one desk"}"#, Some(&one_desk))
}

fn body(answer: &Answer) -> Value {
    answer.body.clone().unwrap_or(Value::Null)
}

/// The path the unit reports no test covers: a world row whose write the store holds up busy
/// once the reset removed the store's rows. The reset changed rows, so it answers `500` saying the
/// store is kept and the reset can be sent again; the store is kept (a command runs on it), and
/// the reset sent once the store answers loads the world.
#[test]
fn a_world_row_held_busy_after_the_rows_were_removed_answers_500_and_keeps_the_store() {
    let Built {
        mut host,
        writes_busy,
        ..
    } = built();
    assert_eq!(open(&mut host, "desk-1"), Ok("opened".to_owned()));

    writes_busy.store(true, Ordering::SeqCst);
    let held = reset_with_world(&mut host);
    writes_busy.store(false, Ordering::SeqCst);
    let held_body = body(&held);
    let refused = held_body["refused"].as_str().unwrap_or_default();
    assert_eq!(held.status, 500, "{held_body}");
    assert!(
        refused.contains(DEADLINE) && refused.contains("kept") && refused.contains("sent again"),
        "the 500 names the busy store, that it is kept and that the reset can be sent again: \
         {held_body}"
    );
    assert!(host.reset_changed_rows());

    assert_eq!(
        open(&mut host, "desk-2"),
        Ok("opened".to_owned()),
        "the next command is answered by the model on the kept store"
    );
    let again = reset_with_world(&mut host);
    assert_eq!(
        (again.status, body(&again)),
        (200, json!({"emptied": [COMPONENT]}))
    );
    assert_ne!(
        open(&mut host, "desk-9"),
        Ok("opened".to_owned()),
        "desk-9 rests in the store once the world loaded, so it does not open again"
    );
}

/// The same busy world row on a store that held no row: the reset removed nothing and only
/// forgot the store's events, so it counts as nothing reset (D3 A) and answers the `503`
/// `{"refused", "committed": false}` of a write that changed nothing.
#[test]
fn a_world_row_held_busy_on_a_store_that_held_no_row_answers_503() {
    let Built {
        mut host,
        writes_busy,
        ..
    } = built();

    writes_busy.store(true, Ordering::SeqCst);
    let held = reset_with_world(&mut host);
    writes_busy.store(false, Ordering::SeqCst);
    let held_body = body(&held);
    assert_eq!(
        (held.status, held_body["committed"].clone()),
        (503, json!(false)),
        "{held_body}"
    );
    assert!(!host.reset_changed_rows());
    assert_eq!(open(&mut host, "desk-1"), Ok("opened".to_owned()));
}

/// The story's Outcome, read literally: a reset whose store misses its deadline "while the reset
/// has changed no row ... answers `503` with `{"refused", "committed": false}`". A reset with a
/// world reads each row's instance from the store before any store is emptied
/// (`Host::reset_to`); where that read is busy, the reset changed nothing, yet it answers `400`,
/// the status of a world the client got wrong, naming the row as one the store "cannot hold".
/// A client that sends it again unchanged once the store answers is told it should not.
#[test]
fn a_reset_whose_world_check_reads_a_busy_store_answers_503_not_400() {
    let Built {
        mut host,
        loads_busy,
        ..
    } = built();
    assert_eq!(open(&mut host, "desk-1"), Ok("opened".to_owned()));

    loads_busy.store(true, Ordering::SeqCst);
    let held = reset_with_world(&mut host);
    loads_busy.store(false, Ordering::SeqCst);
    let held_body = body(&held);
    assert!(
        !host.reset_changed_rows(),
        "nothing was emptied: {held_body}"
    );
    assert_eq!(
        (held.status, held_body["committed"].clone()),
        (503, json!(false)),
        "a reset held by a busy store before it changed a row answers the documented 503: \
         {held_body}"
    );
}
