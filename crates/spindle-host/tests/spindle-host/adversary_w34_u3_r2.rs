//! Adversary cases of wave 34, unit U3, round 2 (`story:reset-on-a-busy-store-keeps-the-store`).
//!
//! The story's D2 A: a busy reset is reported as `HostError::Store(StoreError::Busy(..))`, "the
//! component and the failed step in its text". The unit names them for the world check's read,
//! each removal and the reset mark. A reset with a world has one more store call after the stores
//! are emptied: the write of each world row (`Host::load`). A busy write there reaches
//! `console::reset` as the store's own error, naming neither the component nor the step, so the
//! `503` or `500` the console answers does not say which store to wait for.
//!
//! The fixture `tests/fixtures/notebooks` declares one component, `desk`, owning `Notebook`,
//! created by `desk.notebooks.OpenNotebook`.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, HostError, ManualClock, Seed, Stores, World};
use spindle_kernel::{Change, MemoryStore, Removal, Scan, Store, StoreError, Stored, Write};

const NOTEBOOK: &str = "desk.notebooks.Notebook";
const OPEN: &str = "desk.notebooks.OpenNotebook";
/// What eventlog's `Deadline` reads, as the PostgreSQL store's lock wait ends it.
const DEADLINE: &str = "event store deadline exceeded during database statement or lock";

fn elsewhere() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"adversary-w34-u3-r2\"\n",
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

/// A memory store the test keeps a handle to, whose row writes (not its removals or its reset
/// mark) fail busy while its switch is on.
#[derive(Clone, Default)]
struct WritesBusy {
    store: Arc<Mutex<MemoryStore>>,
    busy: Arc<AtomicBool>,
}

impl WritesBusy {
    fn busy(&self) -> Result<(), StoreError> {
        if self.busy.load(Ordering::SeqCst) {
            return Err(StoreError::Busy(DEADLINE.to_owned()));
        }
        Ok(())
    }
}

impl Store for WritesBusy {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.store.lock().expect("the store").load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.busy()?;
        self.store.lock().expect("the store").commit(change)
    }

    fn commit_all(&mut self, writes: &[Write]) -> Result<(), StoreError> {
        self.busy()?;
        self.store.lock().expect("the store").commit_all(writes)
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.store.lock().expect("the store").remove(removal)
    }

    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError> {
        self.store.lock().expect("the store").scan(entity)
    }

    fn forget_events(&mut self) -> Result<(), StoreError> {
        self.store.lock().expect("the store").forget_events()
    }
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

fn host_on(store: &WritesBusy) -> Host {
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

/// A world of one notebook, its row taken from one the model wrote, so it passes the entity's
/// invariants.
fn one_notebook(host: &mut Host, store: &WritesBusy) -> World {
    host.submit(OPEN, object(json!({"title": "Field notes"})))
        .unwrap_or_else(|error| panic!("OpenNotebook runs: {error}"));
    let row = store
        .store
        .lock()
        .expect("the store")
        .scan(NOTEBOOK)
        .expect("the store reads")
        .rows[0]
        .1
        .row
        .clone();
    World {
        rows: vec![Seed {
            entity: NOTEBOOK.to_owned(),
            identity: json!("00000000-0000-4000-8000-000000000009"),
            row,
        }],
        ..World::default()
    }
}

/// D2 A read literally, at the world row's write: the reset's busy error names the component
/// and the step, as it does at the world check, a removal and the reset mark. The reset first
/// removes the notebook the model wrote (it changed rows), then the write of the world's row is
/// busy. The store is kept either way; only the text is short.
#[test]
fn a_busy_world_row_write_names_the_component_and_the_step() {
    let store = WritesBusy::default();
    let mut host = host_on(&store);
    let world = one_notebook(&mut host, &store);

    store.busy.store(true, Ordering::SeqCst);
    let error = host
        .reset_to(world)
        .expect_err("the busy store holds the world's row");
    store.busy.store(false, Ordering::SeqCst);

    assert!(
        matches!(&error, HostError::Store(StoreError::Busy(_))),
        "a busy world row keeps the store's busy error: {error:?}"
    );
    let HostError::Store(StoreError::Busy(detail)) = &error else {
        unreachable!()
    };
    assert!(
        detail.contains("desk") && detail.contains("row 0"),
        "a busy reset names the component and the step (D2 A), as the world check's read does \
         (\"resetting desk: reading row 0, ...\"); the world row's write answers only the \
         store's own text: {detail:?}"
    );
}
