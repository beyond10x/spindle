//! A view with computed fields (`expr:`, `story:view-computed-fields`): each row carries what ESS's
//! evaluator computes over the row's own fields, as ESS's interpreter answers it, on the host's
//! thread and through its reader, on the memory and the SQLite store; read by identity too.
//!
//! `tests/fixtures/computed` declares one component, `table`, keeping teams and entries: each entry
//! with a score, an optional previous score, games won and lost, a `Decimal` rate and an optional
//! bonus, on a team or on none.

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
use spindle_host::{Host, HostError, ManualClock, Stores};
use spindle_kernel::{MemoryStore, SqliteStore, Store};

const SCORE_CHANGES: &str = "league.entries.ScoreChanges";
const FIGURES: &str = "league.entries.Figures";
const RECORDS: &str = "league.entries.Records";

const STORES: [&str; 2] = ["memory", "sqlite"];

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/computed")
}

fn ir() -> ess_compiler::EssIr {
    spindle_spec::load(&fixture())
        .expect("the computed fixture compiles")
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

const ENTER: &str = "league.entries.Enter";

/// The commands every case sends, in order. `e1` is on `t1`; `e2` on no team, with no previous
/// score, nothing won or lost and no bonus; `e3` on `t2`, dropped after; `e4` on `t9`, which no
/// row holds.
fn script() -> Vec<(&'static str, Value)> {
    vec![
        (
            "league.teams.AddTeam",
            json!({"team_id": "t1", "name": "Rovers"}),
        ),
        (
            "league.teams.AddTeam",
            json!({"team_id": "t2", "name": "Comets"}),
        ),
        (
            ENTER,
            json!({"entry_id": "e1", "team_id": "t1", "score": 10, "previous_score": 7,
                   "won": 3, "lost": 1, "rate": 1.5, "bonus": 0.25}),
        ),
        (
            ENTER,
            json!({"entry_id": "e2", "score": 4, "won": 0, "lost": 0, "rate": 0.1}),
        ),
        (
            ENTER,
            json!({"entry_id": "e3", "team_id": "t2", "score": 5, "previous_score": 9,
                   "won": 2, "lost": 1, "rate": 2.25}),
        ),
        (
            ENTER,
            json!({"entry_id": "e4", "team_id": "t9", "score": 0, "previous_score": 0,
                   "won": 1, "lost": 2, "rate": 0, "bonus": 1.1}),
        ),
        ("league.teams.DropTeam", json!({"team_id": "t2"})),
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
            Box::new(SqliteStore::open(&state.join("table.sqlite")).expect("the SQLite file opens"))
        }
    }
}

/// A host on `store` with `script` run on it.
fn host(store: &str, state: &Path, script: &[(&str, Value)]) -> Host {
    let stores = Stores::from([("table".to_owned(), open(store, state))]);
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
    let correlation = CorrelationId::new("computed").expect("a correlation");
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

/// One row as ESS values: numbers compare by value, as ESS compares them (`5` and `5.0` are one).
type Row = BTreeMap<String, Node>;

/// The rows the interpreter answers for `view`, in its order. `None` where it refuses the read.
fn interpreted(interpreter: &Interpreted, view: &str) -> Option<Vec<Row>> {
    let answer = interpreter
        .query_view(SemanticViewRequest {
            view: view.parse().expect("a view name"),
            params: BTreeMap::new(),
            consistency: QueryConsistency::Current,
            correlation: CorrelationId::new("computed").expect("a correlation"),
            deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
        })
        .ok()?;
    Some(answer.rows)
}

fn nodes(rows: &[Map<String, Value>]) -> Vec<Row> {
    rows.iter()
        .map(|row| {
            serde_json::from_value(Value::Object(row.clone())).expect("a served row as ESS values")
        })
        .collect()
}

/// `view` read on the host's thread and through its reader, on another thread: the two answers,
/// which must be one.
fn read(host: &mut Host, store: &str, view: &str) -> Result<Vec<Map<String, Value>>, HostError> {
    let rows = host.list(view);
    let reader = host.reader();
    assert!(
        reader.reads(view),
        "{store}: the reader answers {view} beside the host's thread"
    );
    let owned = view.to_owned();
    let beside = std::thread::spawn(move || reader.list(&owned))
        .join()
        .expect("the reading thread ends");
    match (&rows, &beside) {
        (Ok(rows), Ok(beside)) => assert_eq!(
            beside, rows,
            "{store}: {view}: the reader answers the rows the host's thread answers"
        ),
        (Err(_), Err(_)) => {}
        _ => panic!("{store}: {view}: host {rows:?}, reader {beside:?}"),
    }
    rows
}

fn rows(values: Value) -> Vec<Map<String, Value>> {
    match values {
        Value::Array(rows) => rows.into_iter().map(object).collect(),
        other => panic!("not an array: {other}"),
    }
}

/// The story's acceptance: `score_change: {expr: "score - coalesce(previous_score, score)"}` is
/// computed for each row, `0` for `e2`, whose `previous_score` is absent, each an `Integer`
/// answered as a JSON integer, and the rows are the ones ESS's interpreter answers.
#[test]
fn a_score_change_is_computed_for_each_row() {
    let interpreter = interpreter(&script());
    for store in STORES {
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let mut host = host(store, state.path(), &script());
        let served = read(&mut host, store, SCORE_CHANGES)
            .unwrap_or_else(|error| panic!("{store}: {SCORE_CHANGES} answers: {error}"));
        assert_eq!(
            served,
            rows(json!([
                {"entry_id": "e1", "score": 10, "score_change": 3},
                {"entry_id": "e2", "score": 4, "score_change": 0},
                {"entry_id": "e3", "score": 5, "score_change": -4},
                {"entry_id": "e4", "score": 0, "score_change": 0},
            ])),
            "{store}"
        );
        for row in &served {
            assert!(
                row["score_change"].is_i64(),
                "{store}: an Integer is a JSON integer: {row:?}"
            );
        }
        assert_eq!(
            Some(nodes(&served)),
            interpreted(&interpreter, SCORE_CHANGES),
            "{store}: the rows ESS's interpreter answers"
        );
    }
}

/// Arithmetic over the row's own fields, as ESS computes it: an `Integer` sum, product and
/// negation answered as JSON integers; a `Decimal` exact, answered as ESS writes a `Decimal`
/// (an integral one as `15.0`); an absent `Optional` operand makes the field `null`; a zero
/// divisor too, unless `coalesce` stands in; a quotient rounded as `round(…)` says. The rows come
/// in `order_by` order of a computed field, ties by identity.
#[test]
fn arithmetic_division_and_optional_operands_follow_ess() {
    for store in STORES {
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let mut host = host(store, state.path(), &script());
        let served = read(&mut host, store, FIGURES)
            .unwrap_or_else(|error| panic!("{store}: {FIGURES} answers: {error}"));
        assert_eq!(
            served,
            rows(json!([
                {"entry_id": "e1", "total": 4, "margin": -19, "weighted": 15.0,
                 "adjusted": 1.75, "next_score": 8, "ratio": 3.0, "share": 3.0},
                {"entry_id": "e3", "total": 3, "margin": -9, "weighted": 11.25,
                 "adjusted": null, "next_score": 10, "ratio": 2.0, "share": 2.0},
                {"entry_id": "e4", "total": 3, "margin": 2, "weighted": 0.0,
                 "adjusted": 1.1, "next_score": 1, "ratio": 0.5, "share": 0.5},
                {"entry_id": "e2", "total": 0, "margin": -8, "weighted": 0.4,
                 "adjusted": null, "next_score": null, "ratio": null, "share": 0.0},
            ])),
            "{store}"
        );
    }
}

/// A computed field beside a field read through a reference: `win_pct` is `null` where nothing
/// is won or lost and six places half-even otherwise, and `team_name` the name of the team the
/// entry names, `null` where it names none, a dropped one, or one no row holds.
#[test]
fn a_computed_field_beside_a_field_read_through_a_reference() {
    let interpreter = interpreter(&script());
    for store in STORES {
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let mut host = host(store, state.path(), &script());
        let served = read(&mut host, store, RECORDS)
            .unwrap_or_else(|error| panic!("{store}: {RECORDS} answers: {error}"));
        assert_eq!(
            served,
            rows(json!([
                {"entry_id": "e1", "team_id": "t1", "win_pct": 75.0, "team_name": "Rovers"},
                {"entry_id": "e2", "win_pct": null},
                {"entry_id": "e3", "team_id": "t2", "win_pct": 66.666667},
                {"entry_id": "e4", "team_id": "t9", "win_pct": 33.333333},
            ])),
            "{store}"
        );
        assert_eq!(
            Some(nodes(&served)),
            interpreted(&interpreter, RECORDS).map(|rows| {
                rows.into_iter()
                    .map(|row| {
                        row.into_iter()
                            .filter(|(name, value)| name == "win_pct" || *value != Node::Null)
                            .collect()
                    })
                    .collect::<Vec<Row>>()
            }),
            "{store}: the rows ESS's interpreter answers, which writes `null` for a field read \
             through a reference that reads nothing, where the host leaves it out"
        );
    }
}

/// Every view of the fixture answers, on both stores and both paths, the rows ESS's interpreter
/// answers, in its order; a field the host leaves out is one the interpreter answers `null`.
#[test]
fn every_computed_view_agrees_with_the_interpreter() {
    let views = ir()
        .views()
        .keys()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    assert_eq!(views.len(), 3, "the fixture's views: {views:?}");
    let interpreter = interpreter(&script());
    for store in STORES {
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let mut host = host(store, state.path(), &script());
        let mut differences = Vec::new();
        for view in &views {
            let expected = interpreted(&interpreter, view).map(|rows| {
                rows.into_iter()
                    .map(|row| {
                        row.into_iter()
                            .filter(|(_, value)| *value != Node::Null)
                            .collect::<Row>()
                    })
                    .collect::<Vec<_>>()
            });
            let served = read(&mut host, store, view)
                .map(|rows| {
                    nodes(&rows)
                        .into_iter()
                        .map(|row| {
                            row.into_iter()
                                .filter(|(_, value)| *value != Node::Null)
                                .collect::<Row>()
                        })
                        .collect::<Vec<_>>()
                })
                .ok();
            if expected.is_none() || served != expected {
                differences.push(format!("{view}: host {served:?}, interpreter {expected:?}"));
            }
        }
        assert!(
            differences.is_empty(),
            "{store}: views whose rows differ from ESS's interpreter: {differences:#?}"
        );
    }
}

/// A view with computed fields read by identity answers the row its list answers for that
/// identity, the computed fields included.
#[test]
fn a_computed_view_is_read_by_identity() {
    for store in STORES {
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let mut host = host(store, state.path(), &script());
        for (view, identity) in [(SCORE_CHANGES, "e2"), (FIGURES, "e1"), (RECORDS, "e3")] {
            let listed = host
                .list(view)
                .unwrap_or_else(|error| panic!("{store}: {view} answers: {error}"))
                .into_iter()
                .find(|row| row["entry_id"] == identity)
                .unwrap_or_else(|| panic!("{store}: {view} lists {identity}"));
            assert_eq!(
                host.view(view, &json!(identity))
                    .unwrap_or_else(|error| panic!("{store}: {view} {identity}: {error}")),
                Some(listed),
                "{store}: {view} {identity}"
            );
        }
    }
}

/// A computed value with no spelling of its type refuses the read, as ESS's interpreter refuses
/// it: an `Integer` past `i64`, on the host's thread, through the reader and by identity.
#[test]
fn a_computed_value_past_its_type_refuses_the_read() {
    let mut script = script();
    script.push((
        ENTER,
        json!({"entry_id": "e5", "score": 0, "won": i64::MAX, "lost": 1, "rate": 0}),
    ));
    let interpreter = interpreter(&script);
    assert_eq!(
        interpreted(&interpreter, FIGURES),
        None,
        "ESS's interpreter refuses the read"
    );
    for store in STORES {
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let mut host = host(store, state.path(), &script);
        match read(&mut host, store, FIGURES) {
            Err(HostError::Unsupported(text)) => assert!(
                text.contains(FIGURES) && text.contains("total"),
                "{store}: the refusal names the view and the field: {text}"
            ),
            other => panic!("{store}: the read is refused: {other:?}"),
        }
        assert!(
            matches!(
                host.view(FIGURES, &json!("e5")),
                Err(HostError::Unsupported(_))
            ),
            "{store}: the row by identity is refused too"
        );
        assert!(
            host.view(FIGURES, &json!("e1"))
                .is_ok_and(|row| row.is_some()),
            "{store}: another row by identity is answered"
        );
    }
}
