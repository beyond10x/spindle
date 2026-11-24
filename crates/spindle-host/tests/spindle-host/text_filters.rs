//! A view filter that tests text against a parameter or a literal
//! (`story:view-filter-text-contains`): `{address: {contains: {param: text}}}`, `starts_with`,
//! `ends_with`, under `not`, `all`, `any` and `exists` over a list field, answered as ESS's
//! interpreter answers it on the memory, SQLite and PostgreSQL stores.
//!
//! ESS's semantics (`ess-primitives` `TextOp::holds`, `Predicate::evaluate` at ESS 0.98.0): the
//! match is byte for byte and case-sensitive, with no Unicode folding; the empty text is
//! contained in every text; a field the row holds no value for is unknown, so the read is
//! refused; an `Optional` parameter left out hides every row (`absent_text_operands`).
//!
//! `tests/fixtures/text-filters` declares one component, `directory`, keeping people with an
//! address, an optional nickname and a list of tags, who leave.

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

const SEARCH: &str = "org.people.AddressSearch";
const OPTIONAL: &str = "org.people.OptionalSearch";
const NICKNAME: &str = "org.people.NicknameSearch";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/text-filters")
}

fn ir() -> ess_compiler::EssIr {
    spindle_spec::load(&fixture())
        .expect("the text-filters fixture compiles")
        .ir
}

/// The commands every case sends, in order. `p2` and `p4` have no nickname; `p5` leaves.
fn script() -> Vec<(&'static str, Value)> {
    vec![
        (
            "org.people.AddPerson",
            json!({"person_id": "p1", "address": "ada@example.org", "nickname": "Ada",
                   "tags": ["red", "Blue"]}),
        ),
        (
            "org.people.AddPerson",
            json!({"person_id": "p2", "address": "BOB@Example.ORG", "tags": []}),
        ),
        (
            "org.people.AddPerson",
            json!({"person_id": "p3", "address": "STRASSE@x.example", "nickname": "cy",
                   "tags": ["straße"]}),
        ),
        (
            "org.people.AddPerson",
            json!({"person_id": "p4", "address": "straße@y.test", "tags": ["a b"]}),
        ),
        (
            "org.people.AddPerson",
            json!({"person_id": "p5", "address": "eve@host", "nickname": "e", "tags": ["x"]}),
        ),
        ("org.people.LeavePerson", json!({"person_id": "p5"})),
    ]
}

/// The texts each view with a `text` parameter is read with: a letter in both cases, the empty
/// text, white space, a character outside ASCII and its upper-case spelling, and one no row holds.
const TEXTS: [&str; 12] = [
    "a", "A", "ex", "Ex", "", " ", "example", "ß", "SS", "@", "e@h", "zzz",
];

/// Every read the cases make: each view of the fixture with each text, a view without parameters
/// once with none, and the view with an `Optional` parameter also with it left out and `null`.
fn reads() -> Vec<(String, Value)> {
    let mut reads = Vec::new();
    for (name, view) in ir().views() {
        let name = name.to_string();
        if view.params.is_empty() {
            reads.push((name, json!({})));
            continue;
        }
        for text in TEXTS {
            reads.push((name.clone(), json!({"text": text})));
        }
        if name == OPTIONAL {
            reads.push((name.clone(), json!({})));
            reads.push((name, json!({"text": null})));
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

/// An environment whose only store is `supplied`, which the host is given one of per component.
fn supplied() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"text-filters\"\n",
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

/// A host whose component `directory` keeps its state in `store`, with [`script`] run on it.
fn host(store: Box<dyn Store>) -> Host {
    let mut host = Host::supplied(
        ir(),
        &supplied(),
        Arc::new(ManualClock::new()),
        None,
        Stores::from([("directory".to_owned(), store)]),
    )
    .unwrap_or_else(|error| panic!("the host opens on the supplied store: {error}"));
    for (command, input) in script() {
        host.submit(command, object(input.clone()))
            .unwrap_or_else(|error| panic!("{command} {input} runs on the host: {error}"));
    }
    host
}

/// ESS's interpreter with [`script`] run on it.
fn interpreter() -> Interpreted {
    let interpreter = Interpreted::for_model(ir());
    let correlation = CorrelationId::new("text-filters").expect("a correlation");
    for (command, input) in script() {
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

/// The rows the interpreter answers for `view` with `params`, in its order, or `None` where it
/// refuses the read. A member it answers `null` is left out, as the host leaves it out.
fn interpreted(interpreter: &Interpreted, view: &str, params: &Value) -> Option<Vec<Value>> {
    let answer = interpreter
        .query_view(SemanticViewRequest {
            view: view.parse().expect("a view name"),
            params: serde_json::from_value(params.clone()).expect("the parameters as ESS values"),
            consistency: QueryConsistency::Current,
            correlation: CorrelationId::new("text-filters").expect("a correlation"),
            deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
        })
        .ok()?;
    Some(
        answer
            .rows
            .iter()
            .map(|row| {
                Value::Object(
                    row.iter()
                        .filter(|(_, value)| **value != Node::Null)
                        .map(|(name, value)| {
                            (
                                name.clone(),
                                serde_json::to_value(value).expect("a value as JSON"),
                            )
                        })
                        .collect(),
                )
            })
            .collect(),
    )
}

fn rows(answer: Vec<Map<String, Value>>) -> Vec<Value> {
    answer.into_iter().map(Value::Object).collect()
}

/// The person ids `view` answers for `text` on `host`, in its order.
fn people(host: &mut Host, view: &str, params: &Value) -> Vec<String> {
    host.list_with(view, &object(params.clone()))
        .unwrap_or_else(|error| panic!("{view} {params} answers: {error}"))
        .iter()
        .map(|row| row["person_id"].as_str().expect("a person id").to_owned())
        .collect()
}

/// Every read of the fixture on a host whose `directory` keeps its state in `store`, on the
/// host's thread and through its reader, against the interpreter: the same rows where it answers,
/// a refusal where it refuses. Panics naming every read that differs.
pub(crate) fn agree_on(name: &str, store: Box<dyn Store>) {
    let interpreter = interpreter();
    let ir = ir();
    let mut host = host(store);
    let reader = host.reader();
    let mut differences = Vec::new();
    for view in ir.views().keys() {
        let view = view.to_string();
        if let Err(error) = spindle_host::reads(&ir, &view) {
            differences.push(format!("{view}: not served: {error}"));
        }
    }
    for (view, params) in reads() {
        let expected = interpreted(&interpreter, &view, &params);
        let served = host
            .list_with(&view, &object(params.clone()))
            .map(rows)
            .map_err(|error| error.to_string());
        if served.as_ref().ok() != expected.as_ref() {
            differences.push(format!(
                "{name} {view} {params}: host {served:?}, interpreter {expected:?}"
            ));
        }
        let beside = reader
            .list_with(&view, &object(params.clone()))
            .map(rows)
            .map_err(|error| error.to_string());
        if beside.as_ref().ok() != expected.as_ref() {
            differences.push(format!(
                "{name} {view} {params}: reader {beside:?}, interpreter {expected:?}"
            ));
        }
    }
    assert!(
        differences.is_empty(),
        "reads that differ from ESS's interpreter: {differences:#?}"
    );
}

/// Every read of the fixture answers, on the memory and on the SQLite store, what ESS's
/// interpreter answers, and is refused where it refuses.
#[test]
fn text_filters_agree_with_the_interpreter() {
    agree_on("memory", Box::new(MemoryStore::default()));
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    agree_on(
        "sqlite",
        Box::new(
            SqliteStore::open(&state.path().join("directory.sqlite"))
                .expect("the SQLite file opens"),
        ),
    );
}

/// The rows spelled out: the match is byte for byte and case-sensitive, so `ex` does not find
/// `BOB@Example.ORG` and `Ex` finds it alone, `SS` does not find `straße` and `ß` does not find
/// `STRASSE`; a person who left is a row as any other.
#[test]
fn a_contains_filter_matches_bytes_case_sensitively() {
    let mut host = host(Box::new(MemoryStore::default()));
    assert_eq!(
        people(&mut host, SEARCH, &json!({"text": "ex"})),
        ["p1", "p3"]
    );
    assert_eq!(people(&mut host, SEARCH, &json!({"text": "Ex"})), ["p2"]);
    assert_eq!(people(&mut host, SEARCH, &json!({"text": "A"})), ["p3"]);
    assert_eq!(people(&mut host, SEARCH, &json!({"text": "ß"})), ["p4"]);
    assert_eq!(people(&mut host, SEARCH, &json!({"text": "SS"})), ["p3"]);
    assert_eq!(
        people(&mut host, SEARCH, &json!({"text": "e@h"})),
        ["p5"],
        "a person who left is held"
    );
    assert!(people(&mut host, SEARCH, &json!({"text": "zzz"})).is_empty());
}

/// The empty text is contained in every address, so it answers every person.
#[test]
fn an_empty_text_answers_every_row() {
    let mut host = host(Box::new(MemoryStore::default()));
    assert_eq!(
        people(&mut host, SEARCH, &json!({"text": ""})),
        ["p1", "p2", "p3", "p4", "p5"]
    );
}

/// An `Optional` parameter left out, or sent as `null`, holds no row, as ESS's interpreter hides
/// a row whose filter is unknown only for it; given, it reads as a required one does.
#[test]
fn an_optional_text_left_out_holds_no_row() {
    let mut host = host(Box::new(MemoryStore::default()));
    assert!(people(&mut host, OPTIONAL, &json!({})).is_empty());
    assert!(people(&mut host, OPTIONAL, &json!({"text": null})).is_empty());
    assert_eq!(
        people(&mut host, OPTIONAL, &json!({"text": "ex"})),
        ["p1", "p3"]
    );
}

/// A row that holds no value for the field the filter tests makes the filter unknown for it, so
/// the read is refused, as ESS's interpreter refuses it, rather than the row hidden.
#[test]
fn a_field_a_row_does_not_hold_refuses_the_read() {
    let mut host = host(Box::new(MemoryStore::default()));
    let refused = host
        .list_with(NICKNAME, &object(json!({"text": "a"})))
        .expect_err("a person without a nickname leaves the filter unknown");
    assert!(
        refused.to_string().contains("unknown for a row"),
        "the refusal names the unknown filter: {refused}"
    );
    let answered = interpreted(&interpreter(), NICKNAME, &json!({"text": "a"}));
    assert!(answered.is_none(), "interpreter answered {answered:?}");
}

/// A required text left out is refused before anything is read, naming it.
#[test]
fn a_required_text_left_out_is_refused_naming_it() {
    let mut host = host(Box::new(MemoryStore::default()));
    let refused = host
        .list_with(SEARCH, &Map::new())
        .expect_err("the text is required");
    assert!(
        refused.to_string().contains("`text`"),
        "the refusal names the parameter: {refused}"
    );
}
