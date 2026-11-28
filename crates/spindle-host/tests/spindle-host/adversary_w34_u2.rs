//! Adversary, wave 34 unit U2 (`story:unknown-commit-answered-as-unknown`): a reset that meets a
//! write whose outcome is unknown (`StoreError::Unknown`). The story's own reason for losing the
//! store is that "an unknown commit may have changed rows"; these cases hold the host's other
//! answers about that reset to the same reading.
//!
//! - `Host::reset_changed_rows` is documented `false` only where no store's rows changed, and a
//!   caller that removed something beside the stores for the reset puts it back when it reads
//!   `false` (`spindle-cli`'s leased reset puts the seed records back). A removal or a world row
//!   whose outcome is unknown may have changed rows, so it reads `true`.
//! - The CHANGELOG bullet of the story says "a reset that meets it loses the store, as before";
//!   a world row whose commit outcome is unknown is met by the reset too.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock, Seed, Stores, World};
use spindle_kernel::{Change, MemoryStore, Removal, Store, StoreError, Stored};

const NOTEBOOK: &str = "desk.notebooks.Notebook";
const OPEN: &str = "desk.notebooks.OpenNotebook";
/// What eventlog's `UnknownCommit` reads.
const UNKNOWN: &str = "commit outcome unknown; reconnect and resolve the same command identity";

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

/// A memory store whose next removal, or next commit, once armed, ends with its outcome unknown
/// after it landed: the write is applied, then reported unknown, as a PostgreSQL COMMIT that
/// landed before the store's deadline fired.
#[derive(Clone, Default)]
struct LandsThenUnknown {
    inner: Arc<Mutex<MemoryStore>>,
    removal: Arc<AtomicBool>,
    commit: Arc<AtomicBool>,
}

impl LandsThenUnknown {
    fn held(&self) -> Vec<(Value, Stored)> {
        self.inner
            .lock()
            .expect("the store")
            .scan(NOTEBOOK)
            .expect("the store reads")
            .rows
    }
}

impl Store for LandsThenUnknown {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.inner.lock().expect("the store").load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.inner.lock().expect("the store").commit(change)?;
        if self.commit.swap(false, Ordering::SeqCst) {
            return Err(StoreError::Unknown(UNKNOWN.to_owned()));
        }
        Ok(())
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.inner.lock().expect("the store").remove(removal)?;
        if self.removal.swap(false, Ordering::SeqCst) {
            return Err(StoreError::Unknown(UNKNOWN.to_owned()));
        }
        Ok(())
    }

    fn scan(&mut self, entity: &str) -> Result<spindle_kernel::Scan, StoreError> {
        self.inner.lock().expect("the store").scan(entity)
    }
}

fn host(store: &LandsThenUnknown) -> Host {
    let ir = spindle_spec::load(&fixture())
        .expect("the notebooks fixture compiles")
        .ir;
    let stores = Stores::from([(
        "desk".to_owned(),
        Box::new(store.clone()) as Box<dyn Store>,
    )]);
    Host::supplied(ir, &elsewhere(), Arc::new(ManualClock::new()), None, stores)
        .unwrap_or_else(|error| panic!("the host opens on the supplied store: {error}"))
}

/// One notebook's identity and row, as `OpenNotebook` writes it, for a world to carry.
fn a_notebook_row() -> Seed {
    let store = LandsThenUnknown::default();
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

/// The first removal of the reset lands and is reported unknown: the store's rows changed, and
/// the host says they did not.
#[test]
fn a_reset_whose_first_removal_is_unknown_may_have_changed_rows() {
    let store = LandsThenUnknown::default();
    let mut host = host(&store);
    host.submit(OPEN, object(json!({"title": "Field notes"})))
        .expect("OpenNotebook runs");
    assert_eq!(store.held().len(), 1, "a notebook for the reset to remove");

    store.removal.store(true, Ordering::SeqCst);
    let reset = host.reset().expect_err("the removal's outcome is unknown");
    assert!(reset.to_string().contains(UNKNOWN), "{reset}");
    assert!(
        store.held().is_empty(),
        "the removal landed, so the store's rows changed"
    );
    assert!(
        host.reset_changed_rows(),
        "a removal whose outcome is unknown may have changed rows, yet the host reports the \
         reset changed none, so a caller puts back what it removed for the reset: {reset}"
    );
}

/// The world's only row lands in an empty store and is reported unknown: the store's rows
/// changed, and the host says they did not.
#[test]
fn a_reset_whose_world_row_is_unknown_may_have_changed_rows() {
    let seed = a_notebook_row();
    let store = LandsThenUnknown::default();
    let mut host = host(&store);

    store.commit.store(true, Ordering::SeqCst);
    let reset = host
        .reset_to(World {
            rows: vec![seed],
            answers: None,
            mint: None,
            clock: None,
        })
        .expect_err("the world row's outcome is unknown");
    assert!(reset.to_string().contains(UNKNOWN), "{reset}");
    assert_eq!(store.held().len(), 1, "the world row landed");
    assert!(
        host.reset_changed_rows(),
        "a world row whose outcome is unknown may have changed rows, yet the host reports the \
         reset changed none: {reset}"
    );
}

/// The CHANGELOG bullet of the story: "a reset that meets it loses the store, as before". A
/// world row whose commit outcome is unknown is met by the reset, and the store stays in use.
#[test]
fn a_reset_whose_world_row_is_unknown_loses_the_store() {
    let seed = a_notebook_row();
    let store = LandsThenUnknown::default();
    let mut host = host(&store);

    store.commit.store(true, Ordering::SeqCst);
    let reset = host
        .reset_to(World {
            rows: vec![seed],
            answers: None,
            mint: None,
            clock: None,
        })
        .expect_err("the world row's outcome is unknown");

    let later = host.submit(OPEN, object(json!({"title": "Second notes"})));
    assert!(
        later
            .as_ref()
            .is_err_and(|error| error.to_string().contains("lost in a reset")),
        "a reset that met an unknown outcome ({reset}) loses the store, as the CHANGELOG says; \
         the next command answered {:?} and the store holds {} rows",
        later.map(|report| report.ran.len()),
        store.held().len()
    );
}
