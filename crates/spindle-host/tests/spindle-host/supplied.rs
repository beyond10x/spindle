//! A host opened on a store it is given (`Host::supplied`), as a Service Worker gives it a
//! browser's IndexedDB store: the component bound to that store's implementation writes there, a
//! reset removes every instance the store holds and keeps the store, and a component whose state
//! is bound to an implementation no store is given for is refused by name.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, HostError, ManualClock, Seed, Stores, World};
use spindle_kernel::{Change, MemoryStore, Removal, Row, Store, StoreError, Stored};

const NOTEBOOK: &str = "desk.notebooks.Notebook";
const GRANT: &str = "access.grants.Grant";

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

/// What eventlog's `Deadline` reads, as the PostgreSQL store's lock wait ends it.
const DEADLINE: &str = "event store deadline exceeded during database statement or lock";
/// What eventlog's PostgreSQL store returns for a connection that went away mid-write: a store
/// failure that is not a deadline.
const DOWN: &str = "PostgreSQL connection unavailable";

/// Which step of a reset a [`Stalled`] store fails, and how
/// (`story:reset-on-a-busy-store-keeps-the-store`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Stall {
    /// Every step answers.
    #[default]
    Answering,
    /// Every scan fails busy, as a read waiting out its lock wait.
    ScanBusy,
    /// Every removal fails busy, as an append waiting out its lock wait behind the publication
    /// lock another session holds.
    RemoveBusy,
    /// Recording that its events are forgotten (the reset mark) fails busy.
    ForgetBusy,
    /// Every removal fails `Backend`, as a connection lost mid-write.
    RemoveDown,
}

/// A memory store the test keeps a handle to, whose reset steps fail as its switch says, counting
/// the times its events were forgotten.
#[derive(Clone, Default)]
struct Stalled {
    store: Arc<Mutex<MemoryStore>>,
    stall: Arc<Mutex<Stall>>,
    forgotten: Arc<AtomicUsize>,
}

impl Stalled {
    fn stall(&self, stall: Stall) {
        *self.stall.lock().expect("the switch") = stall;
    }

    fn stalled(&self) -> Stall {
        *self.stall.lock().expect("the switch")
    }

    /// The rows of `entity` the store holds, read past the switch.
    fn rows(&self, entity: &str) -> usize {
        self.store
            .lock()
            .expect("the store")
            .scan(entity)
            .expect("the store reads")
            .rows
            .len()
    }
}

impl Store for Stalled {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.store.lock().expect("the store").load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.store.lock().expect("the store").commit(change)
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        match self.stalled() {
            Stall::RemoveBusy => Err(StoreError::Busy(DEADLINE.to_owned())),
            Stall::RemoveDown => Err(StoreError::Backend(DOWN.to_owned())),
            _ => self.store.lock().expect("the store").remove(removal),
        }
    }

    fn scan(&mut self, entity: &str) -> Result<spindle_kernel::Scan, StoreError> {
        if self.stalled() == Stall::ScanBusy {
            return Err(StoreError::Busy(DEADLINE.to_owned()));
        }
        self.store.lock().expect("the store").scan(entity)
    }

    fn forget_events(&mut self) -> Result<(), StoreError> {
        if self.stalled() == Stall::ForgetBusy {
            return Err(StoreError::Busy(DEADLINE.to_owned()));
        }
        self.store.lock().expect("the store").forget_events()?;
        self.forgotten.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// A host on the notebooks fixture whose component `desk` keeps its state in `store`.
fn notebooks_on(store: &Stalled) -> Host {
    let ir = spindle_spec::load(&fixture())
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

/// `story:reset-on-a-busy-store-keeps-the-store`, D2 A: a reset whose supplied store is busy at
/// its first step (the scan, the first removal, or, on a store holding no row, the reset mark)
/// answers the store's busy error naming the component and the step, changes no row, and keeps
/// the store: the row is still read and a command then commits on the same store.
#[test]
fn a_reset_whose_first_step_is_busy_keeps_the_supplied_store() {
    for (stall, step, held) in [
        (Stall::ScanBusy, "reading", 1),
        (Stall::RemoveBusy, "removing", 1),
        (Stall::ForgetBusy, "forgetting its events", 0),
    ] {
        let store = Stalled::default();
        let mut host = notebooks_on(&store);
        let identity = (held == 1).then(|| {
            host.submit(
                "desk.notebooks.OpenNotebook",
                object(json!({"title": "Field notes"})),
            )
            .expect("OpenNotebook runs");
            store
                .store
                .lock()
                .expect("the store")
                .scan(NOTEBOOK)
                .expect("the store reads")
                .rows[0]
                .0
                .clone()
        });

        store.stall(stall);
        let error = host.reset().expect_err("the busy store holds the reset");
        assert!(
            matches!(&error, HostError::Store(StoreError::Busy(detail))
                if detail.contains("desk") && detail.contains(step)),
            "{stall:?}: a busy reset answers the store's busy error naming desk and {step}: {error}"
        );
        assert!(
            !host.reset_changed_rows(),
            "{stall:?}: a reset busy at its first step changed no row"
        );

        store.stall(Stall::Answering);
        assert_eq!(
            store.rows(NOTEBOOK),
            held,
            "{stall:?}: the store keeps its rows"
        );
        if let Some(identity) = &identity {
            assert!(
                host.view("desk.notebooks.NotebookCard", identity)
                    .expect("the view reads")
                    .is_some(),
                "{stall:?}: the row is still read"
            );
        }
        host.submit(
            "desk.notebooks.OpenNotebook",
            object(json!({"title": "After the reset"})),
        )
        .unwrap_or_else(|error| panic!("{stall:?}: a command commits on the same store: {error}"));
        assert_eq!(
            store.rows(NOTEBOOK),
            held + 1,
            "{stall:?}: the command wrote to the store the host was given"
        );
    }
}

/// `story:reset-on-a-busy-store-keeps-the-store`, D3 A: with two supplied stores, the first
/// holding no row, whose events alone the reset forgets, and the second busy at its first removal,
/// the reset answers the busy error and counts as nothing reset, which `console::reset` answers
/// `503`.
#[test]
fn a_busy_store_after_one_that_only_forgot_its_events_is_nothing_reset() {
    let ir = spindle_spec::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cross-component-reads"),
    )
    .expect("the cross-component-reads fixture compiles")
    .ir;
    let catalogue = Stalled::default();
    let grants = Stalled::default();
    let mut host = Host::supplied(
        ir,
        &elsewhere(),
        Arc::new(ManualClock::new()),
        None,
        Stores::from([
            (
                "catalogue".to_owned(),
                Box::new(catalogue.clone()) as Box<dyn Store>,
            ),
            (
                "grants".to_owned(),
                Box::new(grants.clone()) as Box<dyn Store>,
            ),
        ]),
    )
    .unwrap_or_else(|error| panic!("the host opens on the supplied stores: {error}"));
    host.reset_to(World {
        rows: vec![Seed {
            entity: GRANT.to_owned(),
            identity: json!("grant-1"),
            row: Row {
                state: "Issued".to_owned(),
                fields: [("holder".to_owned(), json!("a holder"))].into(),
            },
        }],
        ..World::default()
    })
    .expect("the world loads");
    assert_eq!(grants.rows(GRANT), 1);
    let forgotten = catalogue.forgotten.load(Ordering::SeqCst);

    grants.stall(Stall::RemoveBusy);
    let error = host
        .reset()
        .expect_err("the busy grants store holds the reset");
    assert!(
        matches!(&error, HostError::Store(StoreError::Busy(detail))
            if detail.contains("grants") && detail.contains("removing")),
        "the busy removal answers the store's busy error naming grants: {error}"
    );
    assert_eq!(
        catalogue.forgotten.load(Ordering::SeqCst),
        forgotten + 1,
        "the catalogue's events were forgotten before the grants store was busy"
    );
    assert!(
        !host.reset_changed_rows(),
        "a store whose events alone were forgotten counts as nothing reset"
    );
    assert_eq!(grants.rows(GRANT), 1, "the grants store keeps its row");

    grants.stall(Stall::Answering);
    assert_eq!(
        host.reset().expect("the reset sent again"),
        vec!["catalogue".to_owned(), "grants".to_owned()]
    );
    assert_eq!(grants.rows(GRANT), 0);
}

/// `story:reset-on-a-busy-store-keeps-the-store`: only a busy store is kept. A supplied store
/// whose removal fails otherwise is lost, as before: the reset answers `HostError::Reset`
/// (`restored: false`), which `console::reset` answers `500`, and every later command fails
/// naming the store lost in a reset, the store answering again or not.
#[test]
fn a_reset_whose_store_fails_otherwise_still_loses_it() {
    let store = Stalled::default();
    let mut host = notebooks_on(&store);
    host.submit(
        "desk.notebooks.OpenNotebook",
        object(json!({"title": "Field notes"})),
    )
    .expect("OpenNotebook runs");

    store.stall(Stall::RemoveDown);
    let error = host.reset().expect_err("the store fails the removal");
    assert!(
        matches!(&error, HostError::Reset { component, restored: false, .. } if component == "desk"),
        "a removal that fails otherwise loses the store: {error}"
    );

    store.stall(Stall::Answering);
    let later = host
        .submit(
            "desk.notebooks.OpenNotebook",
            object(json!({"title": "After the reset"})),
        )
        .expect_err("the lost store refuses every write");
    assert!(
        later.to_string().contains("lost in a reset"),
        "a later command names the store lost in a reset: {later}"
    );
    assert_eq!(
        store.rows(NOTEBOOK),
        1,
        "nothing reaches the store the host lost"
    );
}
