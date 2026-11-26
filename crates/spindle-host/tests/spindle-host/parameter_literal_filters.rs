//! A view filter that reads a parameter in the forms a consumer's model declares
//! (`story:view-filter-parameter-literal`): a parameter compared with a literal of its type, a
//! text parameter (or an element of a list parameter) under `equals_ignore_case`,
//! `in_ignore_case` and `trimmed`, a parameter compared `as timestamp`, the count of a list
//! parameter, a member of an element of a list field, and a field of the row under a case fold,
//! combined with the leaves read before by `any`, `all`, `not` and `exists`. Each view answers
//! the rows ESS's interpreter answers, with each parameter left out and given, and refuses where
//! it refuses.
//!
//! ESS's semantics (`ess-primitives` `Predicate::evaluate` at ESS 0.99.1): a comparison with a
//! parameter the read left out is unknown, so a filter left unknown for a row refuses the read
//! unless a guard (`not defined(param.x)`) decides it; a fold is ASCII-only; `trimmed` trims
//! only the folded subject.
//!
//! `tests/fixtures/parameter-literal` declares one component, `desk`, keeping items with a label,
//! a direction, an optional agent, the time each came in and the groups each is in, which close.

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

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/parameter-literal")
}

fn ir() -> ess_compiler::EssIr {
    spindle_spec::load(&fixture())
        .expect("the parameter-literal fixture compiles")
        .ir
}

/// The commands every case sends, in order. `i2` and `i4` have no agent; `i4` and `i5` close.
fn script() -> Vec<(&'static str, Value)> {
    vec![
        (
            "org.desk.AddItem",
            json!({"item_id": "i1", "label": "alpha", "direction": "inbound", "agent": "ann",
                   "created": "2026-01-01T00:00:00Z", "groups": [{"id": "g1", "name": "One"}]}),
        ),
        (
            "org.desk.AddItem",
            json!({"item_id": "i2", "label": "Beta", "direction": "outbound",
                   "created": "2026-02-01T00:00:00Z", "groups": [{"id": "g2"}]}),
        ),
        (
            "org.desk.AddItem",
            json!({"item_id": "i3", "label": "", "direction": "all", "agent": "bo",
                   "created": "2026-03-01T00:00:00Z", "groups": []}),
        ),
        (
            "org.desk.AddItem",
            json!({"item_id": "i4", "label": " ALPHA ", "direction": "inbound",
                   "created": "2026-04-01T00:00:00Z",
                   "groups": [{"id": "g1"}, {"id": "g2", "name": "Two"}]}),
        ),
        (
            "org.desk.AddItem",
            json!({"item_id": "i5", "label": "gamma", "direction": "outbound", "agent": "cy",
                   "created": "2026-05-01T00:00:00Z", "groups": [{"id": "g3", "name": ""}]}),
        ),
        ("org.desk.CloseItem", json!({"item_id": "i4"})),
        ("org.desk.CloseItem", json!({"item_id": "i5"})),
    ]
}

/// The values each parameter is given: the empty text, case and white space around a word the
/// filters fold, a value no row holds, and `null`.
fn values(param: &str) -> Vec<Value> {
    match param {
        "label" => vec![
            json!(""),
            json!("alpha"),
            json!("ALPHA"),
            json!("Beta"),
            json!(null),
        ],
        "assigned" => vec![json!(true), json!(false), json!(null)],
        "direction" => vec![json!("all"), json!("inbound"), json!("outbound")],
        "open" => vec![
            json!("on"),
            json!("ON"),
            json!("True"),
            json!(" on "),
            json!("off"),
            json!(""),
        ],
        "wanted" => vec![
            json!("every"),
            json!("EVERY"),
            json!(" every\t"),
            json!("Beta"),
            json!("beta"),
        ],
        "kinds" => vec![
            json!([]),
            json!(["inbound"]),
            json!(["INBOUND"]),
            json!([" inbound "]),
            json!(["Out"]),
            json!(["outbound", "inbound"]),
            json!(["alpha"]),
            json!(["Beta", "gamma"]),
        ],
        "since" | "until" => vec![
            json!("2026-02-01T00:00:00Z"),
            json!("2026-03-01T01:00:00+02:00"),
            json!("2027-01-01T00:00:00Z"),
        ],
        "group" => vec![json!("g1"), json!("g2"), json!("g3"), json!("g9")],
        "groups" => vec![json!([]), json!(["g1"]), json!(["g2", "g3"]), json!(["g9"])],
        other => panic!("no values for the parameter {other}"),
    }
}

/// Every read of `view`: with every parameter left out, and with each parameter given each of its
/// [`values`] and the others left out.
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

/// An environment whose only store is `supplied`, which the host is given one of per component.
fn supplied() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"parameter-literal\"\n",
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

/// A host whose component `desk` keeps its state in `store`, with [`script`] run on it.
fn host(store: Box<dyn Store>) -> Host {
    let mut host = Host::supplied(
        ir(),
        &supplied(),
        Arc::new(ManualClock::new()),
        None,
        Stores::from([("desk".to_owned(), store)]),
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
    let correlation = CorrelationId::new("parameter-literal").expect("a correlation");
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
            correlation: CorrelationId::new("parameter-literal").expect("a correlation"),
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

/// Every read of each of `views` ([`reads`], and `extra`) on a host whose `desk` keeps its state
/// in `store`, on the host's thread and through its reader, against the interpreter: each view
/// served, the same rows where the interpreter answers, a refusal where it refuses. Each view
/// answers at least one read with rows, so no view agrees only by being refused. Panics naming
/// every read that differs.
fn agree_on(name: &str, store: Box<dyn Store>, views: &[&str], extra: &[(&str, Value)]) {
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
    let mut answered = BTreeMap::<&str, usize>::new();
    for (view, params) in every {
        let expected = interpreted(&interpreter, view, &params);
        if expected.as_ref().is_some_and(|rows| !rows.is_empty()) {
            *answered.entry(view).or_default() += 1;
        }
        let served = host
            .list_with(view, &object(params.clone()))
            .map(rows)
            .map_err(|error| error.to_string());
        if served.as_ref().ok() != expected.as_ref() {
            differences.push(format!(
                "{name} {view} {params}: host {served:?}, interpreter {expected:?}"
            ));
        }
        let beside = reader
            .list_with(view, &object(params.clone()))
            .map(rows)
            .map_err(|error| error.to_string());
        if beside.as_ref().ok() != expected.as_ref() {
            differences.push(format!(
                "{name} {view} {params}: reader {beside:?}, interpreter {expected:?}"
            ));
        }
    }
    for view in views {
        if !answered.contains_key(view) {
            differences.push(format!(
                "{name} {view}: the interpreter answers no read with rows"
            ));
        }
    }
    assert!(
        differences.is_empty(),
        "reads that differ from ESS's interpreter: {differences:#?}"
    );
}

fn agree(views: &[&str]) {
    agree_on("memory", Box::new(MemoryStore::default()), views, &[]);
}

/// The item ids `view` answers for `params` on `host`, in its order.
fn items(host: &mut Host, view: &str, params: &Value) -> Vec<String> {
    host.list_with(view, &object(params.clone()))
        .unwrap_or_else(|error| panic!("{view} {params} answers: {error}"))
        .iter()
        .map(|row| row["item_id"].as_str().expect("an item id").to_owned())
        .collect()
}

/// F1a: a text parameter compared with the empty text, `param.label == ""`.
#[test]
fn f1a_a_text_parameter_equal_to_the_empty_text_agrees() {
    agree(&["org.desk.LabelOrEmpty"]);
    let mut host = host(Box::new(MemoryStore::default()));
    assert_eq!(
        items(&mut host, "org.desk.LabelOrEmpty", &json!({"label": ""})),
        ["i1", "i2", "i3", "i4", "i5"]
    );
    assert_eq!(
        items(
            &mut host,
            "org.desk.LabelOrEmpty",
            &json!({"label": "alpha"})
        ),
        ["i1"]
    );
}

/// F1b: a Boolean parameter compared with a literal, `param.assigned == false`.
#[test]
fn f1b_a_boolean_parameter_equal_to_a_literal_agrees() {
    agree(&["org.desk.ByAssignment"]);
    let mut host = host(Box::new(MemoryStore::default()));
    assert_eq!(
        items(
            &mut host,
            "org.desk.ByAssignment",
            &json!({"assigned": false})
        ),
        ["i2", "i4"]
    );
}

/// F1c: an enum parameter compared with one of its words, `param.direction == all`.
#[test]
fn f1c_an_enum_parameter_equal_to_a_word_agrees() {
    agree(&["org.desk.ByDirection"]);
    let mut host = host(Box::new(MemoryStore::default()));
    assert_eq!(
        items(
            &mut host,
            "org.desk.ByDirection",
            &json!({"direction": "all"})
        ),
        ["i1", "i2", "i3", "i4", "i5"]
    );
}

/// F2a and F2b: a text parameter under `in_ignore_case` and `equals_ignore_case`.
#[test]
fn f2ab_a_text_parameter_under_a_case_fold_agrees() {
    agree(&["org.desk.OpenSwitch", "org.desk.LabelFolded"]);
    let mut host = host(Box::new(MemoryStore::default()));
    assert_eq!(
        items(&mut host, "org.desk.OpenSwitch", &json!({"open": "ON"})),
        ["i1", "i2", "i3"]
    );
    assert_eq!(
        items(&mut host, "org.desk.OpenSwitch", &json!({"open": " on "})),
        ["i4", "i5"],
        "a fold does not trim"
    );
}

/// F2c: an element of a list parameter under `equals_ignore_case` and `in_ignore_case`.
#[test]
fn f2c_an_element_of_a_list_parameter_under_a_case_fold_agrees() {
    agree(&["org.desk.OfKinds"]);
    let mut host = host(Box::new(MemoryStore::default()));
    assert_eq!(
        items(
            &mut host,
            "org.desk.OfKinds",
            &json!({"kinds": ["INBOUND"]})
        ),
        ["i1", "i4"]
    );
}

/// F2d: `trimmed` around a text parameter's fold and around a list element's.
#[test]
fn f2d_trimmed_around_a_parameter_fold_agrees() {
    agree(&["org.desk.TrimmedOpen", "org.desk.TrimmedKinds"]);
    let mut host = host(Box::new(MemoryStore::default()));
    assert_eq!(
        items(&mut host, "org.desk.TrimmedOpen", &json!({"open": " on "})),
        ["i1", "i2", "i3"]
    );
    assert_eq!(
        items(
            &mut host,
            "org.desk.TrimmedKinds",
            &json!({"kinds": [" inbound "]})
        ),
        ["i1", "i4"]
    );
}

/// F3: a field compared with a parameter `as timestamp`, read before this story: a regression
/// test.
#[test]
fn f3_a_parameter_compared_as_timestamp_agrees() {
    agree(&["org.desk.Since"]);
}

/// F4: the count of a list parameter, `param.kinds.count == 0`.
#[test]
fn f4_the_count_of_a_list_parameter_agrees() {
    agree(&["org.desk.KindsOrEvery"]);
    let mut host = host(Box::new(MemoryStore::default()));
    assert_eq!(
        items(&mut host, "org.desk.KindsOrEvery", &json!({"kinds": []})),
        ["i1", "i2", "i3", "i4", "i5"]
    );
    assert_eq!(
        items(
            &mut host,
            "org.desk.KindsOrEvery",
            &json!({"kinds": ["Beta", "gamma"]})
        ),
        ["i2", "i5"]
    );
}

/// F5a, F5b and F5c: a member of an element of a list field compared with a parameter
/// (`g.id == param.group`), with an outer list-parameter binder (`g.id == w`), and read by
/// `defined(g.name)`.
#[test]
fn f5_a_member_of_a_list_field_element_agrees() {
    agree(&[
        "org.desk.InGroup",
        "org.desk.InAnyGroup",
        "org.desk.NamedGroup",
    ]);
    let mut host = host(Box::new(MemoryStore::default()));
    assert_eq!(
        items(&mut host, "org.desk.InGroup", &json!({"group": "g1"})),
        ["i1", "i4"]
    );
    assert_eq!(
        items(
            &mut host,
            "org.desk.InAnyGroup",
            &json!({"groups": ["g2", "g3"]})
        ),
        ["i2", "i4", "i5"]
    );
    assert_eq!(
        items(&mut host, "org.desk.NamedGroup", &json!({"group": "g2"})),
        ["i4"]
    );
}

/// F6: a field of the row under a case fold, `label equals_ignore_case alpha`.
#[test]
fn f6_a_row_field_under_a_case_fold_agrees() {
    agree(&["org.desk.FoldedLabel", "org.desk.FoldedLabels"]);
    let mut host = host(Box::new(MemoryStore::default()));
    assert_eq!(
        items(&mut host, "org.desk.FoldedLabel", &json!({})),
        ["i1"],
        "a fold does not trim ` ALPHA `"
    );
}

/// Three views in the shape of the consumer's filters, each built of the forms together
/// (optional-parameter guards, an `in_ignore_case` switch over the state, a time window cast
/// `as timestamp`, a list-parameter `count == 0` escape), are served and answer the interpreter's
/// rows, given several parameters at once, on the memory and the SQLite store; none is refused.
#[test]
fn the_consumer_filter_shapes_are_served() {
    const SHAPES: [&str; 3] = ["org.desk.Inbox", "org.desk.Queue", "org.desk.Teams"];
    let extra = [
        (
            "org.desk.Inbox",
            json!({"open": "true", "since": "2026-01-15T00:00:00Z",
                   "until": "2026-04-15T00:00:00Z", "kinds": []}),
        ),
        (
            "org.desk.Inbox",
            json!({"label": "", "open": "off", "kinds": [" Inbound", "OUT"]}),
        ),
        (
            "org.desk.Queue",
            json!({"assigned": true, "direction": "outbound", "group": "g3"}),
        ),
        (
            "org.desk.Queue",
            json!({"assigned": false, "direction": "all"}),
        ),
        (
            "org.desk.Teams",
            json!({"groups": ["g1", "g2"], "wanted": " EVERY "}),
        ),
        ("org.desk.Teams", json!({"groups": [], "wanted": "Beta"})),
    ];
    let ir = ir();
    for view in SHAPES {
        spindle_host::reads(&ir, view).unwrap_or_else(|error| panic!("{view} is served: {error}"));
    }
    agree_on("memory", Box::new(MemoryStore::default()), &SHAPES, &extra);
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    agree_on(
        "sqlite",
        Box::new(
            SqliteStore::open(&state.path().join("desk.sqlite")).expect("the SQLite file opens"),
        ),
        &SHAPES,
        &extra,
    );
    let mut host = host(Box::new(MemoryStore::default()));
    for (view, params) in extra {
        if let Err(HostError::Unsupported(text)) = host.list_with(view, &object(params.clone())) {
            panic!("{view} {params} is refused as unsupported: {text}");
        }
    }
}

/// A form outside this story, the length of a text parameter (`param.label.count`), still
/// refuses the view whole, naming its filter.
#[test]
fn a_form_outside_the_list_stays_refused_whole() {
    let refused = spindle_host::reads(&ir(), "org.desk.LabelLonger")
        .expect_err("a text parameter's length is not read")
        .to_string();
    assert!(
        refused.contains("org.desk.LabelLonger") && refused.contains("param.label.count > 3"),
        "the refusal names the view and its whole filter: {refused}"
    );
}
