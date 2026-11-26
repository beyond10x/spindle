//! Adversary cases of wave 26 unit U2 (`story:view-filter-parameter-literal`): the forms the
//! unit newly admits, read where its own tests do not reach, each compared with ESS's interpreter
//! (`ess_conformance::interpret`) row by row: an element of a list parameter compared with a
//! literal and ordered against one, an `Integer`, `Decimal` and `Timestamp` parameter ordered
//! against a literal (two spellings of one instant among them), a list parameter's count ordered
//! and read by `defined`, an `Optional` member of a list field's element, a member of a struct
//! member of one, a case fold over non-ASCII text, `trimmed` over every whitespace ESS trims and
//! one it does not, `forall` over an empty list, a paged and ordered read, and a quantifier
//! nested over an element's own list.
//!
//! `tests/fixtures/adversary-w26-u2` declares one component, `bench`, keeping cards.

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
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary-w26-u2")
}

fn ir() -> ess_compiler::EssIr {
    spindle_spec::load(&fixture())
        .expect("the adversary-w26-u2 fixture compiles")
        .ir
}

fn script() -> Vec<Value> {
    vec![
        json!({"card_id": "c1", "title": "Éclair", "rank": 1, "amount": 1.0,
               "slots": [{"code": "a", "note": "n1", "meta": {"tag": "t1"}, "words": ["x"]}]}),
        json!({"card_id": "c2", "title": "éclair", "rank": 2, "amount": 1.5,
               "slots": [{"code": "b", "meta": {"tag": "t2"}, "words": []}]}),
        json!({"card_id": "c3", "title": "Strasse", "rank": 3, "amount": 2.25, "slots": []}),
        json!({"card_id": "c4", "title": "straße", "rank": 0, "amount": 0.5,
               "slots": [{"code": "c", "note": "N1", "meta": {"tag": "t1"}, "words": ["y", "x"]},
                         {"code": "d", "note": "", "meta": {"tag": "t3"}, "words": []}]}),
        json!({"card_id": "c5", "title": " eclair\t", "rank": 4, "amount": 3,
               "slots": [{"code": "e", "note": "n2", "meta": {"tag": "T1"}, "words": ["X"]}]}),
    ]
}

fn values(param: &str) -> Vec<Value> {
    match param {
        "kinds" => vec![
            json!([]),
            json!(["high"]),
            json!(["HIGH"]),
            json!(["low"]),
            json!(["high", "low"]),
        ],
        "ranks" => vec![json!([]), json!([1]), json!([2, 3]), json!([4, 0])],
        "limit" => vec![json!(2), json!(3), json!(-1), json!(null)],
        "amount" => vec![
            json!(1.5),
            json!(1.50),
            json!(2),
            json!(0.25),
            json!(1.4999),
        ],
        "at" => vec![
            json!("2026-03-01T00:00:00Z"),
            json!("2026-03-01T01:00:00+01:00"),
            json!("2026-03-01T00:00:00.000Z"),
            json!("2026-02-28T23:30:00-01:00"),
            json!("2026-02-28T23:59:59Z"),
        ],
        "tag" => vec![json!("t1"), json!("T1"), json!("t9"), json!("")],
        "word" => vec![
            json!("go"),
            json!("\u{a0}go"),
            json!("\u{b}GO\u{c}"),
            json!("\r\n go\t"),
            json!("g o"),
            json!(""),
        ],
        "wanted" => vec![json!("every"), json!("EVERY"), json!("x")],
        "page" => vec![json!(0), json!(1)],
        "size" => vec![json!(2)],
        other => panic!("no values for the parameter {other}"),
    }
}

fn reads(view: &str) -> Vec<Value> {
    let ir = ir();
    let declared = ir
        .views()
        .iter()
        .find_map(|(name, declared)| (name.to_string() == view).then_some(declared))
        .unwrap_or_else(|| panic!("the fixture declares {view}"));
    let mut reads = vec![json!({})];
    for param in &declared.params {
        for value in values(&param.name) {
            reads.push(Value::Object(Map::from_iter([(param.name.clone(), value)])));
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
        "name = \"adversary-w26-u2\"\n",
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

fn host(store: Box<dyn Store>) -> Host {
    let mut host = Host::supplied(
        ir(),
        &supplied(),
        Arc::new(ManualClock::new()),
        None,
        Stores::from([("bench".to_owned(), store)]),
    )
    .unwrap_or_else(|error| panic!("the host opens on the supplied store: {error}"));
    for input in script() {
        host.submit("org.bench.AddCard", object(input.clone()))
            .unwrap_or_else(|error| panic!("AddCard {input} runs on the host: {error}"));
    }
    host
}

fn interpreter() -> Interpreted {
    let interpreter = Interpreted::for_model(ir());
    let correlation = CorrelationId::new("adversary-w26-u2").expect("a correlation");
    for input in script() {
        let fields: BTreeMap<String, Node> =
            serde_json::from_value(input.clone()).expect("the input as ESS values");
        interpreter
            .execute_command(SemanticCommandRequest {
                command: "org.bench.AddCard".parse().expect("a command name"),
                actor: None,
                caller: None,
                input: fields,
                correlation: correlation.clone(),
            })
            .unwrap_or_else(|error| panic!("AddCard {input} runs on the interpreter: {error:?}"));
    }
    interpreter
}

/// The rows and total the interpreter answers, or `None` where it refuses the read.
fn interpreted(
    interpreter: &Interpreted,
    view: &str,
    params: &Value,
) -> Option<(Vec<Value>, Option<u64>)> {
    let answer = interpreter
        .query_view(SemanticViewRequest {
            view: view.parse().expect("a view name"),
            params: serde_json::from_value(params.clone()).expect("the parameters as ESS values"),
            consistency: QueryConsistency::Current,
            correlation: CorrelationId::new("adversary-w26-u2").expect("a correlation"),
            deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
        })
        .ok()?;
    Some((
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
        answer.total,
    ))
}

/// Every read of each of `views` ([`reads`], and `extra`), on the host and through its reader,
/// against the interpreter. Returns the reads that differ and, per view, how many reads the
/// interpreter answered with rows.
fn compare(
    store: Box<dyn Store>,
    views: &[&str],
    extra: &[(&str, Value)],
) -> (Vec<String>, BTreeMap<String, usize>) {
    let interpreter = interpreter();
    let ir = ir();
    let mut host = host(store);
    let reader = host.reader();
    let mut differences = Vec::new();
    let mut every = Vec::new();
    for view in views {
        if let Err(error) = spindle_host::reads(&ir, view) {
            differences.push(format!("{view}: not served: {error}"));
        }
        every.extend(reads(view).into_iter().map(|params| (*view, params)));
    }
    every.extend(extra.iter().cloned());
    let mut answered = BTreeMap::<String, usize>::new();
    for (view, params) in every {
        let expected = interpreted(&interpreter, view, &params);
        if expected.as_ref().is_some_and(|(rows, _)| !rows.is_empty()) {
            *answered.entry(view.to_owned()).or_default() += 1;
        }
        let served = host
            .listed_with(view, &object(params.clone()))
            .map(|listed| {
                (
                    listed
                        .rows
                        .into_iter()
                        .map(Value::Object)
                        .collect::<Vec<_>>(),
                    listed.total,
                )
            })
            .map_err(|error| error.to_string());
        if served.as_ref().ok() != expected.as_ref() {
            differences.push(format!(
                "{view} {params}: host {served:?}, interpreter {expected:?}"
            ));
        }
        let beside = reader
            .listed_with(view, &object(params.clone()))
            .map(|listed| {
                (
                    listed
                        .rows
                        .into_iter()
                        .map(Value::Object)
                        .collect::<Vec<_>>(),
                    listed.total,
                )
            })
            .map_err(|error| error.to_string());
        if beside.as_ref().ok() != expected.as_ref() {
            differences.push(format!(
                "{view} {params}: reader {beside:?}, interpreter {expected:?}"
            ));
        }
    }
    (differences, answered)
}

/// Each of `views` agrees with the interpreter on every read, and the interpreter answers each
/// with rows at least once, so no view agrees only by being refused.
fn agree(views: &[&str], extra: &[(&str, Value)]) {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let sqlite =
        SqliteStore::open(&state.path().join("bench.sqlite")).expect("the SQLite file opens");
    let (mut differences, answered) = compare(Box::new(MemoryStore::default()), views, extra);
    differences.extend(compare(Box::new(sqlite), views, extra).0);
    for view in views {
        if !answered.contains_key(*view) {
            differences.push(format!("{view}: the interpreter answers no read with rows"));
        }
    }
    assert!(
        differences.is_empty(),
        "reads that differ from ESS's interpreter: {differences:#?}"
    );
}

fn ids(host: &mut Host, view: &str, params: &Value) -> Vec<String> {
    host.list_with(view, &object(params.clone()))
        .unwrap_or_else(|error| panic!("{view} {params} answers: {error}"))
        .iter()
        .map(|row| row["card_id"].as_str().expect("a card id").to_owned())
        .collect()
}

/// An element of a list parameter compared with a text literal (`p == "high"`, `p != "high"`)
/// and an element of a list of integers ordered against a literal (`n >= 2`): the row-only
/// requirement the unit dropped now admits both.
#[test]
fn a_list_parameter_element_against_a_literal_agrees() {
    agree(&["org.bench.KindIs", "org.bench.RankList"], &[]);
    let mut host = host(Box::new(MemoryStore::default()));
    assert_eq!(
        ids(&mut host, "org.bench.KindIs", &json!({"kinds": ["HIGH"]})),
        ["c1", "c4"],
        "`==` against a literal is byte for byte, so `HIGH` is no `high`"
    );
    assert_eq!(
        ids(&mut host, "org.bench.RankList", &json!({"ranks": [4, 0]})),
        ["c5"]
    );
}

/// An `Integer` and a `Decimal` parameter ordered against a literal, and a list parameter's count
/// ordered against one and read by `defined`.
#[test]
fn a_number_parameter_ordered_against_a_literal_agrees() {
    agree(
        &[
            "org.bench.RankLimit",
            "org.bench.ManyKinds",
            "org.bench.DefinedCount",
            "org.bench.AmountFloor",
        ],
        &[],
    );
    let mut host = host(Box::new(MemoryStore::default()));
    assert_eq!(
        ids(
            &mut host,
            "org.bench.AmountFloor",
            &json!({"amount": 1.4999})
        ),
        ["c1", "c4"]
    );
    assert_eq!(
        ids(&mut host, "org.bench.DefinedCount", &json!({})),
        ["c1", "c4"],
        "an absent list parameter has no count"
    );
}

/// A `Timestamp` parameter compared with and ordered against a text literal: by the instant
/// each names, so another spelling of the literal's instant is equal to it and a spelling that
/// sorts before it by its bytes can name a later instant.
#[test]
fn a_timestamp_parameter_ordered_against_a_literal_agrees() {
    agree(&["org.bench.AtInstant"], &[]);
    let mut host = host(Box::new(MemoryStore::default()));
    assert_eq!(
        ids(
            &mut host,
            "org.bench.AtInstant",
            &json!({"at": "2026-03-01T01:00:00+01:00"})
        ),
        ["c3", "c5"]
    );
    assert_eq!(
        ids(
            &mut host,
            "org.bench.AtInstant",
            &json!({"at": "2026-02-28T23:30:00-01:00"})
        ),
        ["c2"]
    );
}

/// A member of a list field's element: `Optional` (`g.note == "n1"`, unknown where an element
/// holds no note, so the read is refused as the interpreter refuses it), guarded by `defined`
/// and folded, a member of a struct member (`g.meta.tag`), and `forall` over an empty list.
#[test]
fn a_member_of_a_list_field_element_agrees() {
    let (differences, _) = compare(Box::new(MemoryStore::default()), &["org.bench.NoteIs"], &[]);
    assert!(differences.is_empty(), "{differences:#?}");
    agree(
        &[
            "org.bench.NoteFolded",
            "org.bench.MetaTag",
            "org.bench.AllNoted",
        ],
        &[],
    );
    let mut host = host(Box::new(MemoryStore::default()));
    assert_eq!(
        ids(&mut host, "org.bench.NoteFolded", &json!({})),
        ["c1", "c4"]
    );
    assert_eq!(
        ids(&mut host, "org.bench.AllNoted", &json!({})),
        ["c1", "c3", "c4", "c5"]
    );
}

/// A case fold over non-ASCII text folds ASCII only (`É` is no `é`, `ß` no `SS`), and `trimmed`
/// trims space, `\t`, `\n`, `\v`, `\f` and `\r` and no U+00A0.
#[test]
fn a_fold_over_non_ascii_and_trimmed_whitespace_agrees() {
    agree(
        &[
            "org.bench.FoldNonAscii",
            "org.bench.TrimmedWord",
            "org.bench.TrimmedTitle",
        ],
        &[],
    );
    let mut host = host(Box::new(MemoryStore::default()));
    assert_eq!(
        ids(&mut host, "org.bench.FoldNonAscii", &json!({})),
        ["c2", "c3"]
    );
    assert_eq!(
        ids(
            &mut host,
            "org.bench.TrimmedWord",
            &json!({"word": "\u{a0}go"})
        ),
        ["c1", "c4"]
    );
    assert_eq!(
        ids(
            &mut host,
            "org.bench.TrimmedWord",
            &json!({"word": "\u{b}GO\u{c}"})
        ),
        ["c2", "c3", "c5"]
    );
}

/// A paged read ordered by `order_by` under a fold of a parameter and a trimmed fold of a field:
/// the same page, order and total as the interpreter's.
#[test]
fn a_paged_ordered_read_under_the_new_filters_agrees() {
    let extra = [
        ("org.bench.Paged", json!({"page": 0, "size": 2})),
        ("org.bench.Paged", json!({"page": 1, "size": 2})),
        ("org.bench.Paged", json!({"page": 2, "size": 2})),
        (
            "org.bench.Paged",
            json!({"page": 0, "size": 2, "wanted": "EVERY"}),
        ),
        (
            "org.bench.Paged",
            json!({"page": 0, "size": 2, "wanted": "x"}),
        ),
        (
            "org.bench.Paged",
            json!({"page": 1, "size": 1, "wanted": "x"}),
        ),
    ];
    agree(&["org.bench.Paged"], &extra);
}

/// A quantifier over a list member of a list field's element (`exists w in g.words`), which ESS
/// reads through the outer element: served with the interpreter's rows, or refused whole naming
/// its filter, never answered differently.
#[test]
fn a_quantifier_nested_over_an_element_member_is_read_or_refused_whole() {
    match spindle_host::reads(&ir(), "org.bench.SlotWords") {
        Ok(()) => agree(&["org.bench.SlotWords"], &[]),
        Err(error) => {
            let text = error.to_string();
            assert!(
                text.contains("org.bench.SlotWords") && text.contains("g.words"),
                "the refusal names the view and its filter: {text}"
            );
        }
    }
}
