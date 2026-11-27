//! A view filter quantifying over a `Map` field of the row (`exists v in stamps: v > …`, and
//! `forall`), answered as ESS's interpreter answers it (`story:view-filter-map-value-order`).
//!
//! ESS's semantics (`ess-conformance` at ESS 0.99.2): a source row binds a map as its count and
//! its values at an ordinal in key order (`input::bind`), and a value at an ordinal is typed by the
//! map's value type (`declared_primitive`, `map_value`), so a `Timestamp` value, a newtype over one
//! among them, is ordered by the instant it names, not by its bytes.
//!
//! `tests/fixtures/view-filter-map` declares one component, `ledger`, keeping entries, each with
//! stamps by name.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
use spindle_host::{Host, ManualClock, Stores};
use spindle_kernel::{MemoryStore, SqliteStore, Store};

/// `exists v in stamps: v > "2026-01-01T09:00:00Z"`.
const LATE: &str = "org.ledger.Late";
/// `forall v in stamps: v > "2026-01-01T09:00:00Z"`.
const ALL_LATE: &str = "org.ledger.AllLate";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/view-filter-map")
}

fn ir() -> ess_compiler::EssIr {
    spindle_spec::load(&fixture())
        .expect("the view-filter-map fixture compiles")
        .ir
}

/// The entries every case adds. `e1`'s stamp is after 09:00 by its bytes and before it as an
/// instant (08:00Z); `e4`'s `b` is before it by its bytes and after it as an instant (09:30Z);
/// `e3` holds no stamp; `e5`'s stamp is 09:00Z itself.
fn script() -> Vec<Value> {
    vec![
        json!({"entry_id": "e1", "stamps": {"a": "2026-01-01T10:00:00+02:00"}}),
        json!({"entry_id": "e2", "stamps": {"a": "2026-01-01T10:00:00Z"}}),
        json!({"entry_id": "e3", "stamps": {}}),
        json!({"entry_id": "e4", "stamps": {"a": "2026-01-01T10:00:00Z",
                                            "b": "2026-01-01T08:30:00-01:00"}}),
        json!({"entry_id": "e5", "stamps": {"a": "2026-01-01T09:00:00Z"}}),
    ]
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

fn supplied() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"view-filter-map\"\n",
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

/// A host whose component `ledger` keeps its state in `store`, with [`script`] run on it.
fn host(store: Box<dyn Store>) -> Host {
    let mut host = Host::supplied(
        ir(),
        &supplied(),
        Arc::new(ManualClock::new()),
        None,
        Stores::from([("ledger".to_owned(), store)]),
    )
    .unwrap_or_else(|error| panic!("the host opens on the supplied store: {error}"));
    for input in script() {
        host.submit("org.ledger.AddEntry", object(input.clone()))
            .unwrap_or_else(|error| panic!("AddEntry {input} runs on the host: {error}"));
    }
    host
}

/// ESS's interpreter with [`script`] run on it.
fn interpreter() -> Interpreted {
    let interpreter = Interpreted::for_model(ir());
    let correlation = CorrelationId::new("view-filter-map").expect("a correlation");
    for input in script() {
        let fields: BTreeMap<String, Node> =
            serde_json::from_value(input.clone()).expect("the input as ESS values");
        interpreter
            .execute_command(SemanticCommandRequest {
                command: "org.ledger.AddEntry".parse().expect("a command name"),
                actor: None,
                caller: None,
                input: fields,
                correlation: correlation.clone(),
            })
            .unwrap_or_else(|error| panic!("AddEntry {input} runs on the interpreter: {error:?}"));
    }
    interpreter
}

/// The entry ids of the rows the interpreter answers for `view`, or why it refuses the read.
fn interpreted(interpreter: &Interpreted, view: &str) -> Result<Vec<String>, String> {
    let answer = interpreter
        .query_view(SemanticViewRequest {
            view: view.parse().expect("a view name"),
            params: BTreeMap::new(),
            consistency: QueryConsistency::Current,
            correlation: CorrelationId::new("view-filter-map").expect("a correlation"),
            deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
        })
        .map_err(|error| format!("{error:?}"))?;
    Ok(answer
        .rows
        .iter()
        .map(|row| match row.get("entry_id") {
            Some(Node::Text(id)) => id.clone(),
            other => panic!("a row of {view} without an entry id: {other:?}"),
        })
        .collect())
}

/// The entry ids of the rows `host` answers for `view`, or why it refuses the read.
fn served(host: &mut Host, view: &str) -> Result<Vec<String>, String> {
    host.list_with(view, &Map::new())
        .map(|rows| {
            rows.iter()
                .map(|row| row["entry_id"].as_str().expect("an entry id").to_owned())
                .collect()
        })
        .map_err(|error| error.to_string())
}

/// A filter quantifying over a `Map<String, Stamp>` field, `Stamp` a newtype over `Timestamp`,
/// answers on the memory and the SQLite store, on the host and through its reader, the rows ESS's
/// interpreter answers: each stamp ordered by the instant it names, so a stamp written with an
/// offset that sorts after the literal by its bytes and names an earlier instant is not late, and
/// one that sorts before it and names a later instant is.
#[test]
fn a_view_filter_orders_a_map_value_by_its_instant() {
    let ir = ir();
    let interpreter = interpreter();
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let sqlite =
        SqliteStore::open(&state.path().join("ledger.sqlite")).expect("the SQLite file opens");
    let expected = [(LATE, vec!["e2", "e4"]), (ALL_LATE, vec!["e2", "e3", "e4"])];
    for (view, ids) in &expected {
        spindle_host::reads(&ir, view).unwrap_or_else(|error| panic!("{view} is served: {error}"));
        assert_eq!(
            interpreted(&interpreter, view).as_deref(),
            Ok(&ids.iter().map(|id| (*id).to_owned()).collect::<Vec<_>>()[..]),
            "the interpreter orders each stamp of {view} by its instant"
        );
    }
    let stores: [(&str, Box<dyn Store>); 2] = [
        ("memory", Box::new(MemoryStore::default())),
        ("SQLite", Box::new(sqlite)),
    ];
    for (name, store) in stores {
        let mut host = host(store);
        let reader = host.reader();
        for (view, _) in &expected {
            let interpreted = interpreted(&interpreter, view);
            assert_eq!(
                served(&mut host, view),
                interpreted,
                "{view} on the {name} store, on the host"
            );
            let beside = reader
                .list_with(view, &Map::new())
                .map(|rows| {
                    rows.iter()
                        .map(|row| row["entry_id"].as_str().expect("an entry id").to_owned())
                        .collect::<Vec<_>>()
                })
                .map_err(|error| error.to_string());
            assert_eq!(
                beside, interpreted,
                "{view} on the {name} store, through the reader"
            );
        }
    }
}
