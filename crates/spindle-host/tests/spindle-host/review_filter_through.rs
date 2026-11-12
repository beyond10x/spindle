//! Conformance checks of a view filter read through a reference
//! (`story:view-filter-through-reference`) against ESS's interpreter on states the unit's own
//! fixture does not build: Kleene logic under `not` and `any` with a reference that names no row,
//! `defined` through a missing reference, a `Decimal` ordered against an `Integer` read through a
//! reference, `Timestamp`s spelled with offsets whose text order is not their instant order, a
//! reference from a task to another task (to itself among them), and a read the interpreter
//! refuses.
//!
//! `tests/fixtures/filter-through-review` is `tests/fixtures/filter-through` with a person's
//! `Integer` level, a task's `Decimal` score and a task's optional parent task.

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

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/filter-through-review")
}

fn ir() -> ess_compiler::EssIr {
    spindle_spec::load(&fixture())
        .expect("the filter-through-review fixture compiles")
        .ir
}

fn person(id: &str, name: &str, nickname: Option<&str>, level: i64, joined_at: &str) -> Value {
    let mut person = json!({"person_id": id, "name": name, "team": "red", "level": level,
                            "joined_at": joined_at});
    if let Some(nickname) = nickname {
        person["nickname"] = json!(nickname);
    }
    person
}

fn task(
    id: &str,
    holder: &str,
    person: &str,
    reviewer: Option<&str>,
    parent: Option<&str>,
    score: f64,
) -> Value {
    let mut task = json!({"task_id": id, "holder": holder, "person_id": person, "score": score,
                          "started_at": "2026-02-01T00:00:00Z"});
    if let Some(reviewer) = reviewer {
        task["reviewer_id"] = json!(reviewer);
    }
    if let Some(parent) = parent {
        task["parent_id"] = json!(parent);
    }
    task
}

/// `p3` joined at `2026-02-01T00:00:00Z`, spelled `+01:00`, so its text sorts after `p5`'s
/// `2026-02-01T00:30:00Z` though its instant is earlier. `p4` is removed; `p9` and `t9` were never
/// added. `t10` is its own parent. `t8`'s person names no row and its reviewer `p2` has no
/// nickname.
fn script(with_t8: bool) -> Vec<(&'static str, Value)> {
    let mut script = vec![
        (
            "org.people.AddPerson",
            person("p1", "ada", Some("A"), 2, "2026-01-01T00:00:00Z"),
        ),
        (
            "org.people.AddPerson",
            person("p2", "bob", None, 5, "2026-03-01T00:00:00Z"),
        ),
        (
            "org.people.AddPerson",
            person("p3", "cy", Some("C"), 1, "2026-02-01T01:00:00+01:00"),
        ),
        (
            "org.people.AddPerson",
            person("p4", "dee", None, 1, "2026-01-01T00:00:00Z"),
        ),
        (
            "org.people.AddPerson",
            person("p5", "fay", None, 3, "2026-02-01T00:30:00Z"),
        ),
        (
            "org.tasks.OpenTask",
            task("t1", "ada", "p1", Some("p2"), None, 2.5),
        ),
        (
            "org.tasks.OpenTask",
            task("t2", "bob", "p1", Some("p1"), None, 2.0),
        ),
        (
            "org.tasks.OpenTask",
            task("t3", "bob", "p2", None, None, 5.0),
        ),
        (
            "org.tasks.OpenTask",
            task("t4", "dee", "p4", Some("p3"), Some("t1"), 9.0),
        ),
        (
            "org.tasks.OpenTask",
            task("t5", "eve", "p9", Some("p9"), Some("t9"), 9.0),
        ),
        (
            "org.tasks.OpenTask",
            task("t7", "ada", "p1", Some("p3"), None, 0.5),
        ),
        (
            "org.tasks.OpenTask",
            task("t6", "cy", "p3", Some("p3"), Some("t7"), 1.25),
        ),
        (
            "org.tasks.OpenTask",
            task("t9b", "fay", "p5", Some("p3"), None, 3.0),
        ),
        (
            "org.tasks.OpenTask",
            task("t10", "ada", "p1", None, Some("t10"), 1.0),
        ),
    ];
    if with_t8 {
        script.push((
            "org.tasks.OpenTask",
            task("t8", "zed", "p9", Some("p2"), None, 1.0),
        ));
    }
    script.extend([
        ("org.tasks.CloseTask", json!({"task_id": "t7"})),
        ("org.people.LeavePerson", json!({"person_id": "p2"})),
        ("org.people.RemovePerson", json!({"person_id": "p4"})),
    ]);
    script
}

fn reads() -> Vec<(String, Value)> {
    let mut reads = Vec::new();
    for (name, view) in ir().views() {
        let name = name.to_string();
        match view.params.first().map(|param| param.name.as_str()) {
            None => reads.push((name, json!({}))),
            Some("before") => {
                for before in [
                    "2026-02-01T00:15:00Z",
                    "2026-02-01T01:15:00+01:00",
                    "2026-01-31T23:00:00-02:00",
                ] {
                    reads.push((name.clone(), json!({"before": before})));
                }
            }
            Some(other) => panic!("{name} reads a parameter no case gives: {other}"),
        }
    }
    reads
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
        "name = \"filter-through-review\"\n",
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

fn host(stores: Stores, with_t8: bool) -> Host {
    let mut host = Host::supplied(
        ir(),
        &supplied(),
        Arc::new(ManualClock::new()),
        None,
        stores,
    )
    .unwrap_or_else(|error| panic!("the host opens on the supplied stores: {error}"));
    for (command, input) in script(with_t8) {
        host.submit(command, object(input.clone()))
            .unwrap_or_else(|error| panic!("{command} {input} runs on the host: {error}"));
    }
    host
}

fn stores(store: &str, state: &Path) -> Stores {
    let open = |component: &str| -> Box<dyn Store> {
        match store {
            "memory" => Box::new(MemoryStore::default()),
            _ => Box::new(
                SqliteStore::open(&state.join(format!("{component}.sqlite")))
                    .expect("the SQLite file opens"),
            ),
        }
    };
    Stores::from([
        ("directory".to_owned(), open("directory")),
        ("work".to_owned(), open("work")),
    ])
}

fn interpreter(with_t8: bool) -> Interpreted {
    let interpreter = Interpreted::for_model(ir());
    let correlation = CorrelationId::new("filter-through-review").expect("a correlation");
    for (command, input) in script(with_t8) {
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

/// The task ids the interpreter answers for `view`, in its order, or `None` where it refuses.
fn interpreted(interpreter: &Interpreted, view: &str, params: &Value) -> Option<Vec<String>> {
    let answer = interpreter
        .query_view(SemanticViewRequest {
            view: view.parse().expect("a view name"),
            params: serde_json::from_value(params.clone()).expect("the parameters as ESS values"),
            consistency: QueryConsistency::Current,
            correlation: CorrelationId::new("filter-through-review").expect("a correlation"),
            deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
        })
        .ok()?;
    Some(
        answer
            .rows
            .iter()
            .map(|row| match row.get("task_id") {
                Some(Node::Text(id)) => id.clone(),
                other => panic!("{view}: a task id, not {other:?}"),
            })
            .collect(),
    )
}

fn ids(rows: &[Map<String, Value>]) -> Vec<String> {
    rows.iter()
        .map(|row| row["task_id"].as_str().expect("a task id").to_owned())
        .collect()
}

fn differences(with_t8: bool) -> Vec<String> {
    let interpreter = interpreter(with_t8);
    let mut differences = Vec::new();
    for store in ["memory", "sqlite"] {
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let mut host = host(stores(store, state.path()), with_t8);
        let reader = host.reader();
        for (view, params) in reads() {
            let expected = interpreted(&interpreter, &view, &params);
            let served = host
                .list_with(&view, &object(params.clone()))
                .map(|rows| ids(&rows))
                .map_err(|error| error.to_string());
            if served.as_ref().ok() != expected.as_ref() {
                differences.push(format!(
                    "{store} {view} {params}: host {served:?}, interpreter {expected:?}"
                ));
            }
            let beside = reader
                .list_with(&view, &object(params.clone()))
                .map(|rows| ids(&rows))
                .map_err(|error| error.to_string());
            if beside.as_ref().ok() != expected.as_ref() {
                differences.push(format!(
                    "{store} {view} {params}: reader {beside:?}, interpreter {expected:?}"
                ));
            }
        }
    }
    differences
}

/// Every view of the review fixture answers, on the memory and SQLite stores, on the host's thread
/// and through its reader, what ESS's interpreter answers, and refuses where it refuses.
#[test]
fn review_filters_through_a_reference_agree_with_the_interpreter() {
    let differences = differences(false);
    assert!(
        differences.is_empty(),
        "reads whose rows differ from ESS's interpreter: {differences:#?}"
    );
}

/// With `t8`, whose person names no row and whose reviewer holds no nickname, a filter left
/// unknown by an absent field of a row that exists is refused by both, and every other read
/// agrees.
#[test]
fn review_filters_through_a_reference_agree_with_the_interpreter_when_it_refuses() {
    let differences = differences(true);
    assert!(
        differences.is_empty(),
        "reads whose rows differ from ESS's interpreter: {differences:#?}"
    );
}

/// The interpreter's answers the two cases above compare with, spelled out, so neither compares
/// two refusals where it means to compare rows: without `t8` it answers every read, and with `t8`
/// it refuses `HeldByNameOrNickA` alone. `NeitherLeft` hides `t10`, which names no reviewer, and
/// `t4`, whose person is removed: `not (any [unknown, false])` is unknown and the interpreter
/// hides the row (`absent_navigation`).
#[test]
fn review_the_interpreter_answers_what_the_cases_compare() {
    let answered = |with_t8: bool| {
        let interpreter = interpreter(with_t8);
        reads()
            .into_iter()
            .map(|(view, params)| {
                let answer = interpreted(&interpreter, &view, &params);
                ((view, params.to_string()), answer)
            })
            .collect::<BTreeMap<_, _>>()
    };
    let without = answered(false);
    assert!(
        without.values().all(Option::is_some),
        "the interpreter answers every read without t8: {without:#?}"
    );
    let at = |view: &str| (format!("org.tasks.{view}"), "{}".to_owned());
    assert_eq!(
        without[&at("NeitherLeft")],
        Some(vec!["t2".into(), "t6".into(), "t7".into(), "t9b".into()])
    );
    assert_eq!(
        without[&at("ScoredAboveLevel")],
        Some(vec!["t1".into(), "t6".into()])
    );
    assert_eq!(
        without[&at("ReviewerJoinedFirst")],
        Some(vec!["t9b".into()])
    );
    assert_eq!(
        without[&at("UnderOpenParent")],
        Some(vec!["t10".into(), "t4".into()])
    );
    let with = answered(true);
    let refused = with
        .iter()
        .filter(|(_, answer)| answer.is_none())
        .map(|((view, _), _)| view.as_str())
        .collect::<Vec<_>>();
    assert_eq!(refused, ["org.tasks.HeldByNameOrNickA"]);
}

/// A view read by identity holds a task as the list holds it, for every task of the script.
#[test]
fn review_a_row_read_by_identity_is_held_as_the_list_holds_it() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = host(stores("memory", state.path()), false);
    for (view, params) in reads() {
        if params != json!({}) {
            continue;
        }
        let listed = host
            .list(&view)
            .unwrap_or_else(|error| panic!("{view} answers: {error}"));
        for task in ["t1", "t2", "t3", "t4", "t5", "t6", "t7", "t9b", "t10"] {
            let one = host
                .view(&view, &json!(task))
                .unwrap_or_else(|error| panic!("{view} answers {task}: {error}"));
            assert_eq!(
                one.as_ref(),
                listed.iter().find(|row| row["task_id"] == task),
                "{view}: {task} by identity"
            );
        }
    }
}
