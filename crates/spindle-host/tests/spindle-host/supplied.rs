//! A host opened on a store it is given (`Host::supplied`), as a Service Worker gives it a
//! browser's IndexedDB store: the component bound to that store's implementation writes there, a
//! reset removes every instance the store holds and keeps the store, and a component whose state
//! is bound to an implementation no store is given for is refused by name.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, HostError, ManualClock, Stores};
use spindle_kernel::{Change, MemoryStore, Removal, Store, StoreError, Stored};

const NOTEBOOK: &str = "desk.notebooks.Notebook";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/notebooks")
}

/// An environment whose only durable store is `elsewhere`, which this host has none of its own for.
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

/// A memory store the test keeps a handle to while the host writes through it.
#[derive(Clone, Default)]
struct Shared(Arc<Mutex<MemoryStore>>);

impl Shared {
    fn held(&self) -> Vec<(Value, Stored)> {
        self.0
            .lock()
            .expect("the store")
            .scan(NOTEBOOK)
            .expect("the store reads")
            .rows
    }
}

impl Store for Shared {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.0.lock().expect("the store").load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.0.lock().expect("the store").commit(change)
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.0.lock().expect("the store").remove(removal)
    }

    fn scan(&mut self, entity: &str) -> Result<spindle_kernel::Scan, StoreError> {
        self.0.lock().expect("the store").scan(entity)
    }
}

/// What eventlog's `UnknownCommit` reads, as the PostgreSQL store's deadline fires after COMMIT.
const UNKNOWN_COMMIT: &str =
    "commit outcome unknown; reconnect and resolve the same command identity";

/// A [`Shared`] store whose every removal ends with its outcome unknown
/// (`story:unknown-commit-answered-as-unknown`).
struct UnknownRemoval(Shared);

impl Store for UnknownRemoval {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.0.load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.0.commit(change)
    }

    fn remove(&mut self, _removal: &Removal) -> Result<(), StoreError> {
        Err(StoreError::Unknown(UNKNOWN_COMMIT.to_owned()))
    }

    fn scan(&mut self, entity: &str) -> Result<spindle_kernel::Scan, StoreError> {
        self.0.scan(entity)
    }
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

#[test]
fn a_supplied_store_keeps_the_state_and_a_reset_empties_it() {
    let ir = spindle_spec::load(&fixture())
        .expect("the notebooks fixture compiles")
        .ir;
    let shared = Shared::default();
    let stores = Stores::from([(
        "desk".to_owned(),
        Box::new(shared.clone()) as Box<dyn Store>,
    )]);
    let mut host = Host::supplied(ir, &elsewhere(), Arc::new(ManualClock::new()), None, stores)
        .unwrap_or_else(|error| panic!("the host opens on the supplied store: {error}"));

    let report = host
        .submit(
            "desk.notebooks.OpenNotebook",
            object(json!({"title": "Field notes"})),
        )
        .expect("OpenNotebook runs");
    assert_eq!(report.ran[0].executed.outcome, "opened");
    let held = shared.held();
    assert_eq!(held.len(), 1, "the notebook rests in the supplied store");
    let identity = held[0].0.clone();
    assert!(
        host.view("desk.notebooks.NotebookCard", &identity)
            .expect("the view reads")
            .is_some(),
        "the view reads the supplied store"
    );

    assert_eq!(host.reset().expect("the reset"), vec!["desk".to_owned()]);
    assert!(shared.held().is_empty(), "the reset removed every instance");
    assert_eq!(
        host.view("desk.notebooks.NotebookCard", &identity)
            .expect("the view reads"),
        None
    );
    host.submit(
        "desk.notebooks.OpenNotebook",
        object(json!({"title": "Second notes"})),
    )
    .expect("OpenNotebook runs after the reset");
    assert_eq!(
        shared.held().len(),
        1,
        "the host still writes to the store it was given"
    );
}

/// `story:unknown-commit-answered-as-unknown`: a removal whose outcome is unknown may have changed
/// rows, so a reset that meets one loses the supplied store, as any other store error does, and
/// every later command fails naming it lost in a reset.
#[test]
fn a_reset_whose_commit_is_unknown_still_loses_the_store() {
    let ir = spindle_spec::load(&fixture())
        .expect("the notebooks fixture compiles")
        .ir;
    let shared = Shared::default();
    let stores = Stores::from([(
        "desk".to_owned(),
        Box::new(UnknownRemoval(shared.clone())) as Box<dyn Store>,
    )]);
    let mut host = Host::supplied(ir, &elsewhere(), Arc::new(ManualClock::new()), None, stores)
        .unwrap_or_else(|error| panic!("the host opens on the supplied store: {error}"));
    host.submit(
        "desk.notebooks.OpenNotebook",
        object(json!({"title": "Field notes"})),
    )
    .expect("OpenNotebook runs");
    assert_eq!(shared.held().len(), 1, "a notebook for the reset to remove");

    let reset = host.reset().expect_err("the removal's outcome is unknown");
    assert!(
        matches!(
            &reset,
            HostError::Reset { component, error, restored: false }
                if component == "desk" && error.contains(UNKNOWN_COMMIT)
        ),
        "the reset loses the store, naming the unknown outcome: {reset:?}"
    );

    let later = host
        .submit(
            "desk.notebooks.OpenNotebook",
            object(json!({"title": "Second notes"})),
        )
        .expect_err("the store was lost in the reset");
    assert!(
        later
            .to_string()
            .contains("desk's store was lost in a reset"),
        "{later}"
    );
    assert_eq!(shared.held().len(), 1, "nothing more reached the store");
}

#[test]
fn a_state_binding_without_a_supplied_store_is_refused() {
    let ir = spindle_spec::load(&fixture())
        .expect("the notebooks fixture compiles")
        .ir;
    let refused = Host::supplied(
        ir,
        &elsewhere(),
        Arc::new(ManualClock::new()),
        None,
        Stores::new(),
    )
    .expect_err("no store is given for elsewhere");
    assert!(
        matches!(&refused, HostError::Unsupported(why) if why.contains("desk") && why.contains("elsewhere")),
        "the refusal names the component and the binding: {refused}"
    );
}
