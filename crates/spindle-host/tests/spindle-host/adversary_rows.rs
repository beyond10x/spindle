//! Adversary cases for the rows the host keeps (`story:metrics-read-without-scan`): each entity's
//! count is `store.scan(entity).rows.len()` of the store that holds it.
//!
//! - A store another writer moves: a second host on the same supplied store, and a second host on
//!   the same SQLite state directory. A scan of the store sees the other writer's rows; the counts
//!   the first host kept do not.
//! - The rally example on SQLite: three components, a timer, event bindings between components,
//!   resets with and without a world, and a restart, each component's SQLite file read by a store
//!   of its own after every phase.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock, Seed, Stores, World};
use spindle_kernel::{Change, MemoryStore, Removal, Row, SqliteStore, Store, StoreError, Stored};

const LOAN: &str = "lending.loans.Loan";
const MEMBER: &str = "lending.loans.Member";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rows_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rows")
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

fn environment(store: &str) -> Environment {
    Environment::parse(&format!(
        concat!(
            "format = \"spindle-environment/1\"\n",
            "name = \"rows\"\n",
            "os = \"linux\"\n",
            "arch = \"x86_64\"\n",
            "\n",
            "[provides]\n",
            "\"store.durable\" = [\"{store}\"]\n",
            "\"store.ephemeral\" = [\"{store}\"]\n",
            "\"identity.mint\" = [\"uuid-v4\"]\n",
            "clock = [\"system\"]\n",
            "channel = [\"in-process\"]\n",
            "projection = [\"in-process\"]\n",
        ),
        store = store
    ))
    .expect("the environment parses")
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("laptop.toml reads");
    Environment::parse(&text).expect("laptop.toml is an environment")
}

/// A memory store two hosts write through, as two global scopes of one origin share one
/// IndexedDB database (`spindle-store-browser` hears the other's writes on its channel).
#[derive(Clone, Default)]
struct Shared(Arc<Mutex<MemoryStore>>);

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

fn supplied(shared: &Shared) -> Host {
    let ir = spindle_spec::load(&rows_fixture())
        .expect("the rows fixture compiles")
        .ir;
    Host::supplied(
        ir,
        &environment("elsewhere"),
        Arc::new(ManualClock::new()),
        None,
        Stores::from([(
            "desk".to_owned(),
            Box::new(shared.clone()) as Box<dyn Store>,
        )]),
    )
    .expect("the host opens on the supplied store")
}

fn on_sqlite(state: &Path) -> Host {
    let ir = spindle_spec::load(&rows_fixture())
        .expect("the rows fixture compiles")
        .ir;
    Host::new(
        ir,
        &environment("sqlite"),
        Arc::new(ManualClock::new()),
        state,
    )
    .expect("the host opens on SQLite")
}

fn lend(host: &mut Host, loan: &str) {
    host.submit(
        "lending.loans.Lend",
        object(json!({"loan_id": loan, "borrower": "reader-0", "due": "2026-01-01T00:00:00Z"})),
    )
    .expect("Lend runs");
}

/// What each entity's list view answers: every row a scan of its store answers.
fn listed(host: &mut Host) -> Vec<(String, usize)> {
    [
        (LOAN, "lending.loans.LoanCard"),
        (MEMBER, "lending.loans.MemberCard"),
    ]
    .iter()
    .map(|(entity, view)| {
        let rows = host
            .list(view)
            .unwrap_or_else(|error| panic!("{view} lists: {error}"))
            .len();
        ((*entity).to_owned(), rows)
    })
    .collect()
}

#[test]
#[ignore = "one writer per store: the kept counts see only this host's writes (story:metrics-read-without-scan)"]
fn adversary_rows_a_second_writer_on_a_supplied_store_is_counted() {
    let shared = Shared::default();
    let mut first = supplied(&shared);
    let mut second = supplied(&shared);
    lend(&mut first, "loan-1");
    lend(&mut second, "loan-2");
    lend(&mut second, "loan-3");

    let scanned = listed(&mut first);
    assert_eq!(
        scanned,
        vec![(LOAN.to_owned(), 3), (MEMBER.to_owned(), 0)],
        "the first host's list view scans the shared store"
    );
    assert_eq!(
        first.rows().expect("the rows are read"),
        scanned,
        "the rows the first host answers are the rows a scan of its store answers"
    );
}

#[test]
#[ignore = "one writer per store: the kept counts see only this host's writes (story:metrics-read-without-scan)"]
fn adversary_rows_a_second_writer_on_a_sqlite_state_directory_is_counted() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut first = on_sqlite(state.path());
    let mut second = on_sqlite(state.path());
    lend(&mut first, "loan-1");
    lend(&mut second, "loan-2");

    let scanned = listed(&mut first);
    assert_eq!(
        scanned,
        vec![(LOAN.to_owned(), 2), (MEMBER.to_owned(), 0)],
        "the first host's list view scans the SQLite file"
    );
    assert_eq!(
        first.rows().expect("the rows are read"),
        scanned,
        "the rows the first host answers are the rows a scan of its store answers"
    );
}

/// The rows of every entity `host` answers, against those a store of its own opened on each
/// component's SQLite file in `state` scans: an entity rests in one component's file, so its
/// rows are the sum over every file.
fn holds_what_each_file_scans(host: &mut Host, state: &Path, phase: &str) {
    let rows = host
        .rows()
        .unwrap_or_else(|error| panic!("{phase}: the rows are read: {error}"));
    let mut files = std::fs::read_dir(state)
        .expect("the state directory reads")
        .map(|entry| entry.expect("an entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "sqlite")
        })
        .collect::<Vec<_>>();
    files.sort();
    assert_eq!(files.len(), 3, "{phase}: one file per rally component");
    let mut stores = files
        .iter()
        .map(|file| SqliteStore::open(file).expect("the file opens"))
        .collect::<Vec<_>>();
    let scanned = rows
        .iter()
        .map(|(entity, _)| {
            let held = stores
                .iter_mut()
                .map(|store| store.scan(entity).expect("the file scans").rows.len())
                .sum::<usize>();
            (entity.clone(), held)
        })
        .collect::<Vec<_>>();
    assert_eq!(rows, scanned, "{phase}");
}

fn rally(state: &Path, clock: &Arc<ManualClock>) -> Host {
    let loaded = spindle_spec::load(&root().join("examples/rally/ess")).expect("rally compiles");
    Host::new(loaded.ir, &laptop(), clock.clone(), state).expect("laptop admits rally")
}

fn play(host: &mut Host, clock: &ManualClock, ticks: usize) {
    for _ in 0..ticks {
        clock.advance(Duration::from_secs(1));
        host.advance().expect("the host advances");
    }
}

#[test]
fn adversary_rows_rally_on_sqlite_counts_what_each_file_scans_through_ticks_bindings_resets_and_a_restart()
 {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let state = dir.path().join("state");
    std::fs::create_dir(&state).expect("the state directory");
    let clock = Arc::new(ManualClock::new());
    let mut host = rally(&state, &clock);
    holds_what_each_file_scans(&mut host, &state, "opened");

    for id in ["m1", "m2"] {
        host.submit(
            "rally.table.OpenMatch",
            object(json!({"match_id": id, "target_score": 2})),
        )
        .expect("the match opens");
    }
    host.start("tick", object(json!({"match_id": "m1"})))
        .expect("the tick binding starts");
    play(&mut host, &clock, 40);
    holds_what_each_file_scans(&mut host, &state, "after 40 ticks of m1");

    host.reset().expect("the reset");
    holds_what_each_file_scans(&mut host, &state, "after a reset");

    host.submit(
        "rally.table.OpenMatch",
        object(json!({"match_id": "m1", "target_score": 2})),
    )
    .expect("the match opens again, the tick binding still running on m1");
    play(&mut host, &clock, 25);
    holds_what_each_file_scans(&mut host, &state, "after 25 ticks of m1 opened again");

    drop(host);
    let mut host = rally(&state, &clock);
    holds_what_each_file_scans(&mut host, &state, "after a restart");
    host.start("tick", object(json!({"match_id": "m1"})))
        .expect("the tick binding starts");
    play(&mut host, &clock, 25);
    holds_what_each_file_scans(&mut host, &state, "after 25 more ticks of m1");

    let row = stored(&state, "rally-table", "rally.table.Match", "m1");
    host.reset_to(World {
        rows: vec![Seed {
            entity: "rally.table.Match".to_owned(),
            identity: json!("m9"),
            row,
        }],
        answers: None,
        mint: None,
    })
    .expect("the reset to a world");
    holds_what_each_file_scans(&mut host, &state, "after a reset to a world of one match");
    play(&mut host, &clock, 5);
    holds_what_each_file_scans(&mut host, &state, "after 5 ticks past the world");
}

/// The row the instance `id` of `entity` holds in `component`'s SQLite file in `state`.
fn stored(state: &Path, component: &str, entity: &str, id: &str) -> Row {
    SqliteStore::open(&state.join(format!("{component}.sqlite")))
        .expect("the file opens")
        .load(entity, &json!(id))
        .expect("the file reads")
        .unwrap_or_else(|| panic!("{entity} {id} rests in {component}"))
        .row
}
