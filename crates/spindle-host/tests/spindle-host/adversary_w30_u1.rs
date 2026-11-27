//! Adversary cases of wave 30 unit U1 (`story:view-filter-nested-element-quantifier`,
//! `story:view-filter-map-value-order`): quantifiers nested three deep over list members of list
//! elements, `exists` inside `forall` and the reverse over empty inner lists, a shadowed binder,
//! `Optional` lists, a map inside a list element, a map of structs, an `Optional` map, maps of
//! `Decimal`, `Integer`, `String` and `Duration` values, and instants spelled with offsets and
//! fractional seconds, each compared with ESS's interpreter (`ess_conformance::interpret`) on the
//! memory and the SQLite store, on the host and through its reader.
//!
//! `tests/fixtures/adversary-w30-u1` declares one component, `probe`, keeping items.

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
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary-w30-u1")
}

fn ir() -> ess_compiler::EssIr {
    spindle_spec::load(&fixture())
        .expect("the adversary-w30-u1 fixture compiles")
        .ir
}

/// The items every case adds. Instants equal to 09:00Z: `i1`'s stamp `a` (10:00+01:00), `i2`'s
/// `a` (09:00:00.000Z), `i4`'s `a` (04:00-05:00) and `i4`'s `at` (09:00-00:00).
fn script() -> Vec<Value> {
    vec![
        json!({"item_id": "i1", "at": "2026-01-01T09:00:00Z",
               "groups": [{"name": "x", "words": ["x"], "codes": [1],
                           "slots": [{"words": ["x"], "codes": ["x"]}],
                           "maybe": ["x"], "stamps": {"a": "2026-01-01T10:00:00+02:00"}}],
               "stamps": {"a": "2026-01-01T10:00:00+01:00"},
               "opt_stamps": {"a": "2026-01-01T09:30:00Z"},
               "amounts": {"a": 1.50}, "counts": {"a": 10}, "texts": {"a": "10"},
               "spans": {"a": "PT60M"},
               "info": {"k": {"at": "2026-01-01T10:00:00+02:00", "words": ["x"]}},
               "tags": ["x"]}),
        json!({"item_id": "i2", "at": "2026-01-01T09:00:00.5Z",
               "groups": [{"name": "n", "words": [], "codes": [], "slots": [],
                           "stamps": {}}],
               "stamps": {"a": "2026-01-01T09:00:00.000Z", "b": "2026-01-01T09:00:00.75Z"},
               "amounts": {"a": 2, "b": 1}, "counts": {"a": 9}, "texts": {"a": "9"},
               "spans": {"a": "PT1H"}, "info": {}, "tags": []}),
        json!({"item_id": "i3", "at": "2026-01-01T08:00:00Z", "groups": [],
               "stamps": {}, "opt_stamps": {}, "amounts": {}, "counts": {}, "texts": {},
               "spans": {}, "info": {}}),
        json!({"item_id": "i4", "at": "2026-01-01T09:00:00-00:00",
               "groups": [{"name": "y", "words": ["y", "x"], "codes": [],
                           "slots": [{"words": [], "codes": []},
                                     {"words": ["y"], "codes": ["y"]}],
                           "maybe": [], "stamps": {"a": "2026-01-01T08:30:00-01:00"}},
                          {"name": "z", "words": ["x"], "codes": [],
                           "slots": [{"words": ["x"], "codes": ["x"]}], "stamps": {}}],
               "stamps": {"a": "2026-01-01T04:00:00-05:00", "z": "2026-01-01T09:00:00.25Z"},
               "opt_stamps": {"a": "2026-01-01T08:00:00Z"},
               "amounts": {"a": 1.4999}, "counts": {"a": -1, "b": 100},
               "texts": {"a": "100"}, "spans": {},
               "info": {"a": {"at": "2026-01-01T08:00:00Z", "words": []},
                        "b": {"at": "2026-01-01T09:00:00.001Z", "words": ["X"]}},
               "tags": ["y"]}),
        json!({"item_id": "i5", "at": "2026-01-01T09:00:00.5Z",
               "groups": [{"name": "x", "words": ["x", "x"], "codes": [],
                           "slots": [{"words": ["x", "x"], "codes": []}],
                           "stamps": {"b": "2026-01-01T09:00:00Z"}}],
               "stamps": {"a": "2026-01-01T09:00:00.5001Z"},
               "opt_stamps": {"a": "2026-01-01T09:00:00.001Z"},
               "amounts": {"a": 1.5}, "counts": {}, "texts": {"a": "9a"},
               "spans": {"a": "PT3600S"},
               "info": {"x": {"at": "2026-01-01T09:00:00Z", "words": ["x", "y"]}}}),
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
        "name = \"adversary-w30-u1\"\n",
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
        Stores::from([("probe".to_owned(), store)]),
    )
    .unwrap_or_else(|error| panic!("the host opens on the supplied store: {error}"));
    for input in script() {
        host.submit("org.probe.AddItem", object(input.clone()))
            .unwrap_or_else(|error| panic!("AddItem {input} runs on the host: {error}"));
    }
    host
}

fn interpreter() -> Interpreted {
    let interpreter = Interpreted::for_model(ir());
    let correlation = CorrelationId::new("adversary-w30-u1").expect("a correlation");
    for input in script() {
        let fields: BTreeMap<String, Node> =
            serde_json::from_value(input.clone()).expect("the input as ESS values");
        interpreter
            .execute_command(SemanticCommandRequest {
                command: "org.probe.AddItem".parse().expect("a command name"),
                actor: None,
                caller: None,
                input: fields,
                correlation: correlation.clone(),
            })
            .unwrap_or_else(|error| panic!("AddItem {input} runs on the interpreter: {error:?}"));
    }
    interpreter
}

/// The item ids the interpreter answers for `view` read with `params`, or why it refuses.
fn interpreted(
    interpreter: &Interpreted,
    view: &str,
    params: &Value,
) -> Result<Vec<String>, String> {
    let answer = interpreter
        .query_view(SemanticViewRequest {
            view: view.parse().expect("a view name"),
            params: serde_json::from_value(params.clone()).expect("the parameters as ESS values"),
            consistency: QueryConsistency::Current,
            correlation: CorrelationId::new("adversary-w30-u1").expect("a correlation"),
            deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
        })
        .map_err(|error| format!("{error:?}"))?;
    Ok(answer
        .rows
        .iter()
        .map(|row| match row.get("item_id") {
            Some(Node::Text(id)) => id.clone(),
            other => panic!("a row of {view} without an item id: {other:?}"),
        })
        .collect())
}

fn ids(rows: Result<Vec<Map<String, Value>>, String>) -> Result<Vec<String>, String> {
    rows.map(|rows| {
        rows.iter()
            .map(|row| row["item_id"].as_str().expect("an item id").to_owned())
            .collect()
    })
}

/// What each answer of `view` read with `params` is, by where it was read: the interpreter, and
/// the host and its reader on the memory and the SQLite store.
fn answers(view: &str, params: &Value) -> Vec<(String, Result<Vec<String>, String>)> {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let sqlite =
        SqliteStore::open(&state.path().join("probe.sqlite")).expect("the SQLite file opens");
    let mut answers = vec![(
        "interpreter".to_owned(),
        interpreted(&interpreter(), view, params),
    )];
    let stores: [(&str, Box<dyn Store>); 2] = [
        ("memory", Box::new(MemoryStore::default())),
        ("SQLite", Box::new(sqlite)),
    ];
    for (name, store) in stores {
        let mut host = host(store);
        let reader = host.reader();
        let given = object(params.clone());
        answers.push((
            format!("host on {name}"),
            ids(host
                .list_with(view, &given)
                .map_err(|error| error.to_string())),
        ));
        answers.push((
            format!("reader on {name}"),
            ids(reader
                .list_with(view, &given)
                .map_err(|error| error.to_string())),
        ));
    }
    answers
}

/// Where each of `views` is answered by Spindle, its rows are the interpreter's: no different
/// answer, and no answer where the interpreter refuses the read. A refusal where the interpreter
/// answers is not checked here ([`served`]).
fn no_different_answer(views: &[&str]) {
    let mut differences = Vec::new();
    for view in views {
        let answers = answers(view, &json!({}));
        let expected = &answers[0].1;
        for (place, answer) in &answers[1..] {
            if let Ok(rows) = answer
                && expected.as_ref() != Ok(rows)
            {
                differences.push(format!(
                    "{view} {place}: {rows:?}, interpreter {expected:?}"
                ));
            }
        }
    }
    assert!(
        differences.is_empty(),
        "answers that differ from ESS's interpreter: {differences:#?}"
    );
}

/// Each of `views` is admitted (`spindle_host::reads`) and answers the interpreter's rows, which
/// are `expected`, everywhere it is read.
fn served(views: &[(&str, &[&str])]) {
    let ir = ir();
    let mut differences = Vec::new();
    for (view, expected) in views {
        if let Err(error) = spindle_host::reads(&ir, view) {
            differences.push(format!("{view}: not served: {error}"));
        }
        let expected = Ok(expected
            .iter()
            .map(|id| (*id).to_owned())
            .collect::<Vec<_>>());
        for (place, answer) in answers(view, &json!({})) {
            if answer != expected {
                differences.push(format!("{view} {place}: {answer:?}, expected {expected:?}"));
            }
        }
    }
    assert!(
        differences.is_empty(),
        "reads that are not the interpreter's rows: {differences:#?}"
    );
}

/// Quantifiers three deep over list members of list elements, `forall` inside `exists` and the
/// reverse, empty inner lists and empty outer lists among them, and an inner binder compared with
/// a member of the outer one, served with the interpreter's rows
/// (`story:view-filter-nested-element-quantifier`).
#[test]
fn nested_element_quantifiers_three_deep_and_mixed_are_served_as_the_interpreter() {
    served(&[
        ("org.probe.ThreeDeep", &["i1", "i4", "i5"]),
        ("org.probe.ForallInExists", &["i1", "i2", "i4", "i5"]),
        ("org.probe.ExistsInForall", &["i1", "i3", "i4", "i5"]),
        ("org.probe.ForallThree", &["i1", "i2", "i3", "i5"]),
        ("org.probe.WordIsName", &["i1", "i4", "i5"]),
    ]);
}

/// A binder shadowed by an inner quantifier of the same name (`exists g in g.slots` inside
/// `exists g in groups`): the innermost `g` is read, as ESS's interpreter reads it.
#[test]
fn a_shadowed_binder_reads_the_innermost_element() {
    no_different_answer(&["org.probe.Shadowed"]);
    served(&[("org.probe.Shadowed", &["i1", "i4"])]);
}

/// `Optional` lists, as a field of the row and as a member of a list field's element, and an
/// `Optional` map, each absent on a row: no answer differs from the interpreter's, which refuses
/// the read as unknown for that row.
#[test]
fn optional_lists_and_maps_never_answer_differently() {
    no_different_answer(&[
        "org.probe.TagsOpt",
        "org.probe.MaybeWords",
        "org.probe.OptStamps",
    ]);
    for view in [
        "org.probe.TagsOpt",
        "org.probe.MaybeWords",
        "org.probe.OptStamps",
    ] {
        for (place, answer) in answers(view, &json!({})) {
            assert!(answer.is_err(), "{view} {place} refuses: {answer:?}");
        }
    }
}

/// An `Optional` map of plain `Timestamp` values, read where the row holds it and guarded by
/// `not defined` where it does not, is served with the interpreter's rows, each value ordered by
/// its instant: `i2` holds none, `i3` an empty map, `i4`'s 08:00Z is not after 09:00Z.
#[test]
fn an_optional_map_held_is_served_as_the_interpreter() {
    served(&[("org.probe.OptStampsGuarded", &["i1", "i2", "i5"])]);
}

/// A `Timestamp` map value compared by `==`, `!=`, `>`, `>=` and `<` (under `not`) with a literal
/// written in UTC, with fractional seconds and with an offset, and with a `Timestamp` field of the
/// row, each value ordered by its instant: equal instants spelled differently are equal
/// (`story:view-filter-map-value-order`).
#[test]
fn map_value_instants_spelled_differently_are_served_as_the_interpreter() {
    served(&[
        ("org.probe.StampEq", &["i1", "i2", "i4"]),
        ("org.probe.StampNe", &["i3", "i5"]),
        ("org.probe.StampFraction", &["i2", "i5"]),
        ("org.probe.StampOffsetLiteral", &["i1", "i2", "i4", "i5"]),
        ("org.probe.StampVsField", &["i2", "i4", "i5"]),
        ("org.probe.NoneEarly", &["i1", "i2", "i3", "i4", "i5"]),
    ]);
}

/// Map values of `Decimal`, `Integer`, `String` (integers as text, ordered by bytes) and
/// `Duration` (compared by `==` only) are served with the interpreter's rows.
#[test]
fn map_values_of_other_types_are_served_as_the_interpreter() {
    served(&[
        ("org.probe.Amounts", &["i2"]),
        ("org.probe.Counts", &["i1", "i4"]),
        ("org.probe.Texts", &["i5"]),
        ("org.probe.Spans", &["i2"]),
    ]);
}

/// A map inside a list field's element and a map of structs (a `Timestamp` member read, and a
/// list member walked), each value ordered by its instant: `i1`'s `10:00+02:00` is not after
/// 09:00Z, `i4`'s `08:30-01:00` and `09:00:00.001Z` are.
#[test]
fn maps_inside_elements_and_of_structs_are_served_as_the_interpreter() {
    served(&[
        ("org.probe.GroupStamps", &["i4"]),
        ("org.probe.InfoAt", &["i4"]),
        ("org.probe.InfoWords", &["i1", "i5"]),
    ]);
}

/// An aggregate measure's `where:` quantifying over a `Timestamp` map and over a list member of
/// a list field's element counts the rows the interpreter counts: 3 late (`i2`, `i4`, `i5`, by
/// instant), 3 nested (`i1`, `i4`, `i5`).
#[test]
fn an_aggregate_where_over_a_map_and_a_nested_list_counts_as_the_interpreter() {
    let view = "org.probe.LateCount";
    spindle_host::reads(&ir(), view).unwrap_or_else(|error| panic!("{view} is served: {error}"));
    let answer = interpreter()
        .query_view(SemanticViewRequest {
            view: view.parse().expect("a view name"),
            params: BTreeMap::new(),
            consistency: QueryConsistency::Current,
            correlation: CorrelationId::new("adversary-w30-u1").expect("a correlation"),
            deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
        })
        .unwrap_or_else(|error| panic!("the interpreter answers {view}: {error:?}"));
    // Counts compared as numbers: the interpreter's `Node` writes `3` as `3.0`.
    let counts = |row: &Value| {
        ["late", "nested"].map(|name| row[name].as_f64().unwrap_or_else(|| panic!("{row}")))
    };
    let expected = vec![[3.0, 3.0]];
    let interpreted = answer
        .rows
        .iter()
        .map(|row| counts(&serde_json::to_value(row).expect("a row as JSON")))
        .collect::<Vec<_>>();
    assert_eq!(interpreted, expected, "the interpreter counts {view}");
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let sqlite =
        SqliteStore::open(&state.path().join("probe.sqlite")).expect("the SQLite file opens");
    let stores: [(&str, Box<dyn Store>); 2] = [
        ("memory", Box::new(MemoryStore::default())),
        ("SQLite", Box::new(sqlite)),
    ];
    for (name, store) in stores {
        let mut host = host(store);
        let rows = host
            .list_with(view, &Map::new())
            .unwrap_or_else(|error| panic!("{view} answers on {name}: {error}"))
            .into_iter()
            .map(|row| counts(&Value::Object(row)))
            .collect::<Vec<_>>();
        assert_eq!(rows, expected, "{view} on the {name} store");
    }
}

/// A quantifier over a `Map` parameter is refused whole, read with or without the parameter
/// (decision D2 of `story:view-filter-map-value-order`).
#[test]
fn a_quantifier_over_a_map_parameter_is_refused_whole() {
    let ir = ir();
    assert!(
        spindle_host::reads(&ir, "org.probe.MapParam").is_err(),
        "a quantifier over a map parameter is not read"
    );
    let mut host = host(Box::new(MemoryStore::default()));
    for params in [json!({}), json!({"ps": {"a": "2026-01-01T10:00:00Z"}})] {
        assert!(
            host.list_with("org.probe.MapParam", &object(params.clone()))
                .is_err(),
            "MapParam {params} is refused"
        );
    }
}
