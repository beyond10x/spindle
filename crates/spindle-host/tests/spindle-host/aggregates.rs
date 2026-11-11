//! A view with aggregate fields (`story:view-aggregate-fields`): `count`, `sum` and `avg`, with
//! and without `group_by`, behind a filter, with a measure's own `where:`, answered with the rows
//! ESS's interpreter answers for the same sessions, on the host's thread and through its reader,
//! on the memory and the SQLite store.
//!
//! `tests/fixtures/aggregates` declares one component, `desk`, keeping sessions: each on an
//! account, with its minutes, a `Decimal` charge and an optional rating, open or closed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{
    ConformanceTarget, Deadline, SemanticCommandRequest, SemanticViewRequest,
};
use ess_primitives::consistency::QueryConsistency;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, HostError, ManualClock, Stores};
use spindle_kernel::{Change, MemoryStore, Removal, SqliteStore, Store, StoreError, Stored};

const OPEN_PER_ACCOUNT: &str = "ledger.sessions.OpenPerAccount";
const TOTALS: &str = "ledger.sessions.Totals";
const OPEN_TOTALS: &str = "ledger.sessions.OpenTotals";
const PER_STATE: &str = "ledger.sessions.PerState";
const PER_RATING: &str = "ledger.sessions.PerRating";
const CLOSED_PER_ACCOUNT: &str = "ledger.sessions.ClosedPerAccount";
const ACCOUNT_TOTALS: &str = "ledger.sessions.AccountTotals";
const SESSION: &str = "ledger.sessions.Session";

/// Every view of the fixture with each set of parameters it is read with here.
const READS: [(&str, &str); 8] = [
    (OPEN_PER_ACCOUNT, "{}"),
    (TOTALS, "{}"),
    (OPEN_TOTALS, "{}"),
    (PER_STATE, "{}"),
    (PER_RATING, "{}"),
    (CLOSED_PER_ACCOUNT, "{}"),
    (ACCOUNT_TOTALS, r#"{"account": "a2"}"#),
    (ACCOUNT_TOTALS, r#"{"account": "a9"}"#),
];

const STORES: [&str; 2] = ["memory", "sqlite"];

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/aggregates")
}

fn ir() -> ess_compiler::EssIr {
    spindle_spec::load(&fixture())
        .expect("the aggregates fixture compiles")
        .ir
}

/// An environment whose only store is `supplied`, which the host is given one of.
fn supplied() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"supplied\"\n",
        "os = \"linux\"\n",
        "arch = \"x86_64\"\n",
        "\n",
        "[provides]\n",
        "\"store.durable\" = [\"supplied\"]\n",
        "\"store.ephemeral\" = [\"supplied\"]\n",
        "clock = [\"system\"]\n",
        "channel = [\"in-process\"]\n",
        "projection = [\"in-process\"]\n",
    ))
    .expect("the supplied environment parses")
}

/// The commands every case sends, in order. Open at the end: `s1`, `s6` (`a1`), `s4` (`a2`) and
/// `s5` (`a3`); closed: `s2` (`a1`) and `s3` (`a2`). `s2`, `s5` and `s6` hold no rating; `s4` has
/// no minutes.
fn script() -> Vec<(&'static str, Value)> {
    let open = "ledger.sessions.OpenSession";
    vec![
        (
            open,
            json!({"session_id": "s1", "account_id": "a1", "minutes": 10, "charge": 12.5,
                   "rating": 4}),
        ),
        (
            open,
            json!({"session_id": "s2", "account_id": "a1", "minutes": 20, "charge": 3.25}),
        ),
        (
            open,
            json!({"session_id": "s3", "account_id": "a2", "minutes": 16, "charge": 0.1,
                   "rating": 5}),
        ),
        (
            open,
            json!({"session_id": "s4", "account_id": "a2", "minutes": 0, "charge": 0,
                   "rating": 3}),
        ),
        (
            open,
            json!({"session_id": "s5", "account_id": "a3", "minutes": 7, "charge": 1.15}),
        ),
        (
            open,
            json!({"session_id": "s6", "account_id": "a1", "minutes": 5, "charge": 2}),
        ),
        ("ledger.sessions.CloseSession", json!({"session_id": "s2"})),
        ("ledger.sessions.CloseSession", json!({"session_id": "s3"})),
    ]
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

/// A store `store` names: memory, or a SQLite file under `state`.
fn open(store: &str, state: &Path) -> Box<dyn Store> {
    match store {
        "memory" => Box::new(MemoryStore::default()),
        _ => {
            Box::new(SqliteStore::open(&state.join("desk.sqlite")).expect("the SQLite file opens"))
        }
    }
}

/// A host on `store` with `script` run on it.
fn host(store: &str, state: &Path, script: &[(&str, Value)]) -> Host {
    let stores = Stores::from([("desk".to_owned(), open(store, state))]);
    let mut host = Host::supplied(
        ir(),
        &supplied(),
        Arc::new(ManualClock::new()),
        None,
        stores,
    )
    .unwrap_or_else(|error| panic!("{store}: the host opens: {error}"));
    for (command, input) in script {
        host.submit(command, object(input.clone()))
            .unwrap_or_else(|error| panic!("{store}: {command} {input} runs: {error}"));
    }
    host
}

/// ESS's interpreter with `script` run on it.
fn interpreter(script: &[(&str, Value)]) -> Interpreted {
    let interpreter = Interpreted::for_model(ir());
    let correlation = CorrelationId::new("aggregates").expect("a correlation");
    for (command, input) in script {
        let fields: BTreeMap<String, Node> =
            serde_json::from_value(input.clone()).expect("the input as ESS values");
        interpreter
            .execute_command(SemanticCommandRequest {
                command: command.parse().expect("a command name"),
                actor: None,
                caller: None,
                input: fields,
                correlation: correlation.clone(),
            })
            .unwrap_or_else(|error| panic!("{command} {input} runs on the interpreter: {error:?}"));
    }
    interpreter
}

/// The rows the interpreter answers for `view` read with `params`, in its order, every member
/// it answers `null` included. `None` where it refuses the read.
fn interpreted(interpreter: &Interpreted, view: &str, params: &str) -> Option<Vec<Row>> {
    let params: BTreeMap<String, Node> =
        serde_json::from_str(params).expect("the parameters as ESS values");
    let answer = interpreter
        .query_view(SemanticViewRequest {
            view: view.parse().expect("a view name"),
            params,
            consistency: QueryConsistency::Current,
            correlation: CorrelationId::new("aggregates").expect("a correlation"),
            deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
        })
        .ok()?;
    Some(answer.rows)
}

/// One row as ESS values: numbers compare by value, as ESS compares them (`5` and `5.0` are one).
type Row = BTreeMap<String, Node>;

fn nodes(rows: &[Map<String, Value>]) -> Vec<Row> {
    rows.iter()
        .map(|row| {
            serde_json::from_value(Value::Object(row.clone())).expect("a served row as ESS values")
        })
        .collect()
}

fn parameters(params: &str) -> Map<String, Value> {
    object(serde_json::from_str(params).expect("the parameters as JSON"))
}

/// `view` read with `params` on the host's thread and through its reader, on another thread:
/// the two answers, which must be one.
fn read(host: &mut Host, store: &str, view: &str, params: &str) -> Vec<Map<String, Value>> {
    let given = parameters(params);
    let rows = host
        .list_with(view, &given)
        .unwrap_or_else(|error| panic!("{store}: {view} {params} answers: {error}"));
    let reader = host.reader();
    assert!(
        reader.reads(view),
        "{store}: the reader answers {view} beside the host's thread"
    );
    let (owned, beside) = (view.to_owned(), given.clone());
    let read = std::thread::spawn(move || reader.list_with(&owned, &beside))
        .join()
        .expect("the reading thread ends")
        .unwrap_or_else(|error| panic!("{store}: {view} {params} is read beside: {error}"));
    assert_eq!(
        read, rows,
        "{store}: {view} {params}: the reader answers the rows the host's thread answers"
    );
    rows
}

fn rows(values: Value) -> Vec<Map<String, Value>> {
    match values {
        Value::Array(rows) => rows.into_iter().map(object).collect(),
        other => panic!("not an array: {other}"),
    }
}

/// The story's acceptance: `OpenPerAccount`, filtered `state == Open` and grouped by
/// `account_id`, answers one row per account with its count of open sessions, in ESS's order, as
/// ESS's interpreter answers it.
#[test]
fn open_sessions_are_counted_per_account() {
    let interpreter = interpreter(&script());
    for store in STORES {
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let mut host = host(store, state.path(), &script());
        let served = read(&mut host, store, OPEN_PER_ACCOUNT, "{}");
        assert_eq!(
            served,
            rows(json!([
                {"account_id": "a1", "open_sessions": 2},
                {"account_id": "a2", "open_sessions": 1},
                {"account_id": "a3", "open_sessions": 1},
            ])),
            "{store}"
        );
        assert_eq!(
            Some(nodes(&served)),
            interpreted(&interpreter, OPEN_PER_ACCOUNT, "{}"),
            "{store}: the rows ESS's interpreter answers"
        );
    }
}

/// A view without `group_by` answers one row: the `count`, the exact `sum` of an `Integer` and of
/// a `Decimal`, the `avg` rounded to six places half-even, and the measures skipping an absent
/// rating. Each `Integer` is written as an integer.
#[test]
fn totals_count_sum_and_average_without_group_by() {
    for store in STORES {
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let mut host = host(store, state.path(), &script());
        assert_eq!(
            read(&mut host, store, TOTALS, "{}"),
            rows(
                json!([{"sessions": 6, "minutes": 58, "charge": 19, "mean_minutes": 9.666667,
                         "rated": 12, "mean_rating": 4}])
            ),
            "{store}"
        );
        assert_eq!(
            read(&mut host, store, OPEN_TOTALS, "{}"),
            rows(json!([{"open_sessions": 4, "open_charge": 15.65,
                         "mean_open_charge": 3.9125}])),
            "{store}"
        );
        assert_eq!(
            read(&mut host, store, PER_STATE, "{}"),
            rows(json!([
                {"state": "Closed", "sessions": 2, "mean_charge": 1.675},
                {"state": "Open", "sessions": 4, "mean_charge": 3.9125},
            ])),
            "{store}"
        );
        assert_eq!(
            read(&mut host, store, PER_RATING, "{}"),
            rows(json!([
                {"rating": null, "sessions": 3, "minutes": 32},
                {"rating": 3, "sessions": 1, "minutes": 0},
                {"rating": 4, "sessions": 1, "minutes": 10},
                {"rating": 5, "sessions": 1, "minutes": 16},
            ])),
            "{store}: the sessions without a rating are one group, first, as ESS orders `null`"
        );
        assert_eq!(
            read(&mut host, store, CLOSED_PER_ACCOUNT, "{}"),
            rows(json!([
                {"account_id": "a1", "sessions": 3, "closed": 1, "closed_minutes": 20},
                {"account_id": "a2", "sessions": 1, "closed": 1, "closed_minutes": 16},
                {"account_id": "a3", "sessions": 1, "closed": 0, "closed_minutes": 0},
            ])),
            "{store}: `minutes > 0` keeps s4 out, each `where:` counts the closed sessions alone"
        );
        assert_eq!(
            read(&mut host, store, ACCOUNT_TOTALS, r#"{"account": "a2"}"#),
            rows(json!([{"sessions": 2, "minutes": 16}])),
            "{store}"
        );
    }
}

/// Every view of the fixture, with each parameter set, answers the rows ESS's interpreter
/// answers, in its order, on both stores and both paths.
#[test]
fn every_aggregate_view_agrees_with_the_interpreter() {
    let views = ir()
        .views()
        .keys()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    for view in &views {
        assert!(
            READS.iter().any(|(read, _)| read == view),
            "every view of the fixture is read here; {view} is not"
        );
    }
    let interpreter = interpreter(&script());
    for store in STORES {
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let mut host = host(store, state.path(), &script());
        let mut differences = Vec::new();
        for (view, params) in READS {
            let expected = interpreted(&interpreter, view, params);
            if expected.is_none() {
                differences.push(format!("{view} {params}: the interpreter refuses the read"));
            }
            let served = nodes(&read(&mut host, store, view, params));
            if Some(&served) != expected.as_ref() {
                differences.push(format!(
                    "{view} {params}: host {served:?}, interpreter {expected:?}"
                ));
            }
        }
        assert!(
            differences.is_empty(),
            "{store}: views whose rows differ from ESS's interpreter: {differences:#?}"
        );
    }
}

/// Over no session, a view without `group_by` answers its one row, each `count` and required
/// `sum` `0` and each `avg` and skipping `sum` `null`, and a grouped view answers no row; a filter
/// that keeps no session reads the same. As ESS's interpreter answers each.
#[test]
fn an_empty_source_answers_one_totals_row_and_no_groups() {
    let interpreter = interpreter(&[]);
    for store in STORES {
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let mut host = host(store, state.path(), &[]);
        assert_eq!(
            read(&mut host, store, TOTALS, "{}"),
            rows(
                json!([{"sessions": 0, "minutes": 0, "charge": 0, "mean_minutes": null,
                         "rated": null, "mean_rating": null}])
            ),
            "{store}"
        );
        assert_eq!(
            read(&mut host, store, OPEN_PER_ACCOUNT, "{}"),
            Vec::<Map<String, Value>>::new(),
            "{store}"
        );
        assert_eq!(
            read(&mut host, store, PER_STATE, "{}"),
            Vec::<Map<String, Value>>::new(),
            "{store}"
        );
        for (view, params) in READS {
            assert_eq!(
                Some(nodes(&read(&mut host, store, view, params))),
                interpreted(&interpreter, view, params),
                "{store}: {view} {params} over no session"
            );
        }
    }
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = host("memory", state.path(), &script());
    assert_eq!(
        read(&mut host, "memory", ACCOUNT_TOTALS, r#"{"account": "a9"}"#),
        rows(json!([{"sessions": 0, "minutes": 0}])),
        "a filter that keeps no session answers the one row over none"
    );
}

/// An aggregate view's rows are groups, not instances: it has no row for an identity, and a read
/// of one is refused naming that, never answered with a session's own fields.
#[test]
fn an_aggregate_view_has_no_row_by_identity() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = host("memory", state.path(), &script());
    match host.view(OPEN_PER_ACCOUNT, &json!("s1")) {
        Err(HostError::Unsupported(text)) => assert!(
            text.contains(OPEN_PER_ACCOUNT) && text.contains("aggregat"),
            "the refusal names the view and its aggregation: {text}"
        ),
        other => panic!("a read by identity of an aggregate view is refused: {other:?}"),
    }
}

/// What a store was asked: each scan and each load, by entity.
#[derive(Default)]
struct Asked {
    scans: Vec<String>,
    loads: Vec<String>,
}

/// A SQLite store, shared, that notes every scan and load it answers.
#[derive(Clone)]
struct Counted {
    store: Arc<Mutex<SqliteStore>>,
    asked: Arc<Mutex<Asked>>,
}

impl Store for Counted {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.asked
            .lock()
            .expect("the tally")
            .loads
            .push(entity.to_owned());
        self.store.lock().expect("the store").load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.store.lock().expect("the store").commit(change)
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.store.lock().expect("the store").remove(removal)
    }

    fn commit_all(&mut self, writes: &[spindle_kernel::Write]) -> Result<(), StoreError> {
        self.store.lock().expect("the store").commit_all(writes)
    }

    fn scan(&mut self, entity: &str) -> Result<spindle_kernel::Scan, StoreError> {
        self.asked
            .lock()
            .expect("the tally")
            .scans
            .push(entity.to_owned());
        self.store.lock().expect("the store").scan(entity)
    }

    fn counts(&mut self, entities: &[String]) -> Result<BTreeMap<String, usize>, StoreError> {
        self.store.lock().expect("the store").counts(entities)
    }

    fn reading(&mut self) -> Option<Arc<dyn spindle_kernel::Reading>> {
        self.store.lock().expect("the store").reading()
    }
}

/// `story:view-read-without-log-scan`: an aggregate view costs what any view read costs, one scan
/// of its source's resting rows and no load. On SQLite, neither a read on the host's thread nor
/// one through its reader reads one event of the log.
#[test]
fn an_aggregate_read_scans_the_resting_rows_once_and_reads_no_event() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let counted = Counted {
        store: Arc::new(Mutex::new(
            SqliteStore::open(&state.path().join("desk.sqlite")).expect("the SQLite file opens"),
        )),
        asked: Arc::new(Mutex::new(Asked::default())),
    };
    let stores = Stores::from([(
        "desk".to_owned(),
        Box::new(counted.clone()) as Box<dyn Store>,
    )]);
    let mut host = Host::supplied(
        ir(),
        &supplied(),
        Arc::new(ManualClock::new()),
        None,
        stores,
    )
    .unwrap_or_else(|error| panic!("the host opens on the counted store: {error}"));
    for (command, input) in script() {
        host.submit(command, object(input.clone()))
            .unwrap_or_else(|error| panic!("{command} {input} runs: {error}"));
    }
    let reader = host.reader();
    let events = counted.store.lock().expect("the store").events_read();
    *counted.asked.lock().expect("the tally") = Asked::default();

    host.list(TOTALS)
        .unwrap_or_else(|error| panic!("{TOTALS} answers: {error}"));
    {
        let asked = counted.asked.lock().expect("the tally");
        assert_eq!(asked.scans, [SESSION], "one scan of the source");
        assert!(asked.loads.is_empty(), "no load: {:?}", asked.loads);
    }
    reader
        .list(OPEN_PER_ACCOUNT)
        .unwrap_or_else(|error| panic!("{OPEN_PER_ACCOUNT} is read beside: {error}"));
    assert_eq!(
        counted.store.lock().expect("the store").events_read(),
        events,
        "no event of the log is read for either read"
    );
}
