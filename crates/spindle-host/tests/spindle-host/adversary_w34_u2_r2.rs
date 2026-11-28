//! Adversary round 2, wave 34 unit U2 (`story:unknown-commit-answered-as-unknown`): the fix that
//! makes a reset's removal or world row whose outcome is unknown count as a changed row
//! (`Host::reset_changed_rows`). These cases hold the other side of that fix: only an unknown
//! outcome, or a write that landed, counts; a write that failed before it landed, and forgetting
//! events alone, do not, and one reset's reading does not carry into the next. A caller that
//! removed something beside the stores for the reset (`spindle-cli`'s leased reset and its seed
//! records) puts it back only where the reading is `false`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, HostError, ManualClock, Seed, Stores, World};
use spindle_kernel::{Change, MemoryStore, Removal, Store, StoreError, Stored};

const NOTEBOOK: &str = "desk.notebooks.Notebook";
const OPEN: &str = "desk.notebooks.OpenNotebook";
/// What eventlog's `UnknownCommit` reads.
const UNKNOWN: &str = "commit outcome unknown; reconnect and resolve the same command identity";
/// What eventlog's deadline reads before COMMIT was sent.
const BUSY: &str = "deadline exceeded before the commit began";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/notebooks")
}

fn elsewhere() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"elsewhere\"\n",
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

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

/// A memory store whose next write, once armed, fails as armed: a removal that lands and is then
/// reported unknown, a commit refused busy before anything landed, or a forgetting of events
/// reported unknown.
#[derive(Clone, Default)]
struct Armed {
    inner: Arc<Mutex<MemoryStore>>,
    removal_unknown: Arc<AtomicBool>,
    commit_busy: Arc<AtomicBool>,
    forget_unknown: Arc<AtomicBool>,
}

impl Armed {
    fn held(&self) -> Vec<(Value, Stored)> {
        self.inner
            .lock()
            .expect("the store")
            .scan(NOTEBOOK)
            .expect("the store reads")
            .rows
    }
}

impl Store for Armed {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.inner.lock().expect("the store").load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        if self.commit_busy.swap(false, Ordering::SeqCst) {
            return Err(StoreError::Busy(BUSY.to_owned()));
        }
        self.inner.lock().expect("the store").commit(change)
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.inner.lock().expect("the store").remove(removal)?;
        if self.removal_unknown.swap(false, Ordering::SeqCst) {
            return Err(StoreError::Unknown(UNKNOWN.to_owned()));
        }
        Ok(())
    }

    fn scan(&mut self, entity: &str) -> Result<spindle_kernel::Scan, StoreError> {
        self.inner.lock().expect("the store").scan(entity)
    }

    fn forget_events(&mut self) -> Result<(), StoreError> {
        if self.forget_unknown.swap(false, Ordering::SeqCst) {
            return Err(StoreError::Unknown(UNKNOWN.to_owned()));
        }
        Ok(())
    }
}

fn host(store: &Armed) -> Host {
    let ir = spindle_spec::load(&fixture())
        .expect("the notebooks fixture compiles")
        .ir;
    let stores = Stores::from([("desk".to_owned(), Box::new(store.clone()) as Box<dyn Store>)]);
    Host::supplied(ir, &elsewhere(), Arc::new(ManualClock::new()), None, stores)
        .unwrap_or_else(|error| panic!("the host opens on the supplied store: {error}"))
}

/// One notebook's identity and row, as `OpenNotebook` writes it, for a world to carry.
fn a_notebook_row() -> Seed {
    let store = Armed::default();
    let mut host = host(&store);
    host.submit(OPEN, object(json!({"title": "Field notes"})))
        .expect("OpenNotebook runs");
    let (identity, stored) = store.held().remove(0);
    Seed {
        entity: NOTEBOOK.to_owned(),
        identity,
        row: stored.row,
    }
}

fn world(rows: Vec<Seed>) -> World {
    World {
        rows,
        answers: None,
        mint: None,
        clock: None,
    }
}

/// The fix counts a world row as written only where it landed or its outcome is unknown: the
/// only row of a world, refused busy before it landed, on a store that held nothing to remove,
/// changed no row. A fix that counted every failed row would read `true`, and the leased reset
/// would not put back the seed records it removed. No case of the suite holds this side: the
/// mutant `.inspect_err(|_| self.reset_rows = true)` in `Host::load` passes every other case.
#[test]
fn a_world_row_refused_busy_on_an_empty_store_changed_no_rows() {
    let seed = a_notebook_row();
    let store = Armed::default();
    let mut host = host(&store);

    store.commit_busy.store(true, Ordering::SeqCst);
    let reset = host
        .reset_to(world(vec![seed]))
        .expect_err("the world row is refused busy");
    assert!(
        matches!(&reset, HostError::Store(StoreError::Busy(detail))
            if detail.starts_with("resetting desk: writing row 0") && detail.ends_with(BUSY)),
        "{reset:?}"
    );
    assert!(store.held().is_empty(), "nothing landed");
    assert!(
        !host.reset_changed_rows(),
        "a world row refused before it landed, on a store with nothing to remove, changed no row"
    );
}

/// The CHANGELOG: "A reset whose removal or forgetting of events meets it loses the store". On
/// an empty store no removal runs, and forgetting events alone changes no row
/// (`Host::reset_changed_rows`): the store is lost and the reading stays `false`.
#[test]
fn forgetting_events_unknown_on_an_empty_store_loses_it_and_changed_no_rows() {
    let store = Armed::default();
    let mut host = host(&store);

    store.forget_unknown.store(true, Ordering::SeqCst);
    let reset = host
        .reset()
        .expect_err("forgetting the events ends unknown");
    assert!(
        matches!(
            &reset,
            HostError::Reset { component, error, restored: false }
                if component == "desk" && error.contains(UNKNOWN)
        ),
        "the reset loses the store, naming the unknown outcome: {reset:?}"
    );
    assert!(
        !host.reset_changed_rows(),
        "forgetting events alone changes no row"
    );
    let later = host
        .submit(OPEN, object(json!({"title": "Second notes"})))
        .expect_err("the store was lost in the reset");
    assert!(
        later
            .to_string()
            .contains("desk's store was lost in a reset"),
        "{later}"
    );
}

/// A reset whose removal ends unknown reads `true`; the next reset, over the store it lost,
/// removes nothing and reads `false`: the first reset's reading is not carried into the next.
#[test]
fn an_unknown_removal_leaves_no_reading_for_the_next_reset() {
    let store = Armed::default();
    let mut host = host(&store);
    host.submit(OPEN, object(json!({"title": "Field notes"})))
        .expect("OpenNotebook runs");

    store.removal_unknown.store(true, Ordering::SeqCst);
    host.reset().expect_err("the removal's outcome is unknown");
    assert!(
        host.reset_changed_rows(),
        "the unknown removal may have landed"
    );

    let again = host
        .reset()
        .expect_err("the store was lost in the first reset");
    assert!(
        matches!(
            &again,
            HostError::Reset {
                restored: false,
                ..
            }
        ),
        "{again:?}"
    );
    assert!(
        !host.reset_changed_rows(),
        "a reset over a lost store changes no row: {again}"
    );
}
