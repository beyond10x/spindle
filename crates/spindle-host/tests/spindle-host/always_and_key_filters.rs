//! A view filter holding an `(always)` leaf inside a quantifier, or `never` beside other leaves,
//! and one reading a member of the row's struct identity (`key.integration == param.id`), each
//! answered as ESS's interpreter answers it (`story:view-filter-always-and-key`).
//!
//! ESS's semantics (`ess-primitives` `Predicate::evaluate` at ESS 0.99.1): `always` is true and
//! `never` false for every row; a quantifier over an `Optional` list parameter the read left out
//! is unknown, and `not defined(param.tags)` true, so a disjunction led by it holds. A path
//! `<field>.<member>` whose field is a struct (an identity among them) reads that member of the
//! row, never a row a reference names (`ResolvedView::navigated`), as `ess-conformance` binds a
//! source row (`input::bind`).
//!
//! `tests/fixtures/always-and-key` declares one component, `links`, keeping links keyed by an
//! integration and a name, each with tags.

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

const TAGGED: &str = "desk.links.Tagged";
const OF_INTEGRATION: &str = "desk.links.OfIntegration";
const OF_INTEGRATION_TAGGED: &str = "desk.links.OfIntegrationTagged";
const NAMED: &str = "desk.links.Named";
const LISTED: &str = "desk.links.Listed";
const COMPARED: &str = "desk.links.Compared";
/// The view whose filter reads `tags.count`, a two-segment path that names no struct member and
/// navigates no reference: still refused whole.
const COUNTED: &str = "desk.links.Counted";
const REGIONAL: &str = "desk.links.Regional";
/// The view whose filter holds `always` beside a leaf still unread (`label.count`, a text's
/// length): still refused whole.
const FOLDED: &str = "desk.links.Folded";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/always-and-key")
}

fn ir() -> ess_compiler::EssIr {
    spindle_spec::load(&fixture())
        .expect("the always-and-key fixture compiles")
        .ir
}

/// The links every case adds. `mail/beta` holds no tag.
fn script() -> Vec<Value> {
    vec![
        json!({"key": {"integration": "mail", "name": "alpha"}, "label": "one", "origin": {"place": {"region": "eu"}}, "tags": ["red"]}),
        json!({"key": {"integration": "mail", "name": "beta"}, "label": "two", "origin": {"place": {"region": "us"}}, "tags": []}),
        json!({"key": {"integration": "chat", "name": "alpha"}, "label": "three", "origin": {"place": {"region": "eu"}},
               "tags": ["red", "blue"]}),
        json!({"key": {"integration": "chat", "name": "gamma"}, "label": "gamma", "origin": {"place": {"region": "us"}},
               "tags": ["blue"]}),
    ]
}

/// The values of `tags` each view reading it is read with: left out, `null`, empty, naming one tag
/// some links hold, naming two, and naming one no link holds.
fn tag_sets() -> Vec<Option<Value>> {
    vec![
        None,
        Some(Value::Null),
        Some(json!([])),
        Some(json!(["red"])),
        Some(json!(["blue", "red"])),
        Some(json!(["green"])),
    ]
}

/// Every read the cases make: each view of the fixture with each value of each parameter it reads.
fn reads() -> Vec<(&'static str, Value)> {
    let mut reads = vec![
        (NAMED, json!({})),
        (LISTED, json!({})),
        (COMPARED, json!({})),
        (REGIONAL, json!({})),
    ];
    for tags in tag_sets() {
        let mut params = Map::new();
        if let Some(tags) = &tags {
            params.insert("tags".to_owned(), tags.clone());
        }
        reads.push((TAGGED, Value::Object(params.clone())));
        for id in ["mail", "chat", "none"] {
            let mut params = params.clone();
            params.insert("id".to_owned(), json!(id));
            reads.push((OF_INTEGRATION_TAGGED, Value::Object(params)));
        }
    }
    for id in ["mail", "chat", "none"] {
        reads.push((OF_INTEGRATION, json!({"id": id})));
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
        "name = \"always-and-key\"\n",
        "os = \"linux\"\n",
        "arch = \"x86_64\"\n",
        "\n",
        "[provides]\n",
        "\"store.durable\" = [\"supplied\"]\n",
        "\"store.ephemeral\" = [\"supplied\"]\n",
        "clock = [\"system\"]\n",
        "channel = [\"in-process\"]\n",
        "projection = [\"in-process\"]\n",
        "\"ingress.network\" = [\"http-loopback\"]\n",
    ))
    .expect("the supplied environment parses")
}

/// A host whose component `links` keeps its state in `store`, with [`script`] run on it.
fn host(store: Box<dyn Store>) -> Host {
    let mut host = Host::supplied(
        ir(),
        &supplied(),
        Arc::new(ManualClock::new()),
        None,
        Stores::from([("links".to_owned(), store)]),
    )
    .unwrap_or_else(|error| panic!("the host opens on the supplied store: {error}"));
    for input in script() {
        host.submit("desk.links.AddLink", object(input.clone()))
            .unwrap_or_else(|error| panic!("AddLink {input} runs on the host: {error}"));
    }
    host
}

/// ESS's interpreter with [`script`] run on it.
fn interpreter() -> Interpreted {
    let interpreter = Interpreted::for_model(ir());
    let correlation = CorrelationId::new("always-and-key").expect("a correlation");
    for input in script() {
        let fields: BTreeMap<String, Node> =
            serde_json::from_value(input.clone()).expect("the input as ESS values");
        interpreter
            .execute_command(SemanticCommandRequest {
                command: "desk.links.AddLink".parse().expect("a command name"),
                actor: None,
                caller: None,
                input: fields,
                correlation: correlation.clone(),
            })
            .unwrap_or_else(|error| panic!("AddLink {input} runs on the interpreter: {error:?}"));
    }
    interpreter
}

/// The rows the interpreter answers for `view` with `params`, in its order, or `None` where it
/// refuses the read.
fn interpreted(interpreter: &Interpreted, view: &str, params: &Value) -> Option<Vec<Value>> {
    let params: BTreeMap<String, Node> =
        serde_json::from_value(params.clone()).expect("the parameters as ESS values");
    let answer = interpreter
        .query_view(SemanticViewRequest {
            view: view.parse().expect("a view name"),
            params: params
                .into_iter()
                .filter(|(_, value)| *value != Node::Null)
                .collect(),
            consistency: QueryConsistency::Current,
            correlation: CorrelationId::new("always-and-key").expect("a correlation"),
            deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
        })
        .ok()?;
    Some(
        answer
            .rows
            .iter()
            .map(|row| serde_json::to_value(row).expect("a row as JSON"))
            .collect(),
    )
}

fn rows(answer: Vec<Map<String, Value>>) -> Vec<Value> {
    answer.into_iter().map(Value::Object).collect()
}

/// The keys of `rows`, each `<integration>/<name>`, in their order.
fn keys(rows: &[Value]) -> Vec<String> {
    rows.iter()
        .map(|row| {
            format!(
                "{}/{}",
                row["key"]["integration"].as_str().expect("an integration"),
                row["key"]["name"].as_str().expect("a name")
            )
        })
        .collect()
}

/// The keys `view` answers for `params` on `host`, after failing unless the view is served and
/// the rows are those ESS's interpreter answers.
fn agreed(host: &mut Host, interpreter: &Interpreted, view: &str, params: &Value) -> Vec<String> {
    spindle_host::reads(&ir(), view).unwrap_or_else(|error| panic!("{view} is served: {error}"));
    let served = rows(
        host.list_with(view, &object(params.clone()))
            .unwrap_or_else(|error| panic!("{view} {params} answers: {error}")),
    );
    assert_eq!(
        Some(&served),
        interpreted(interpreter, view, params).as_ref(),
        "{view} {params}: the rows ESS's interpreter answers"
    );
    keys(&served)
}

/// Every read of the fixture on a host whose `links` keeps its state in `store`, on the host's
/// thread and through its reader, against the interpreter: the same rows where it answers, a
/// refusal where it refuses. Panics naming every read that differs.
fn agree_on(name: &str, store: Box<dyn Store>) {
    let interpreter = interpreter();
    let ir = ir();
    let mut host = host(store);
    let reader = host.reader();
    let mut differences = Vec::new();
    for view in ir.views().keys() {
        let view = view.to_string();
        if view == COUNTED || view == FOLDED {
            continue;
        }
        if let Err(error) = spindle_host::reads(&ir, &view) {
            differences.push(format!("{view}: not served: {error}"));
        }
    }
    for (view, params) in reads() {
        let expected = interpreted(&interpreter, view, &params);
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
    assert!(
        differences.is_empty(),
        "reads that differ from ESS's interpreter: {differences:#?}"
    );
}

/// The consumer's filter a): with `tags` left out every link, with it empty every link (no
/// element, so `not exists … (always)` holds), and with tags named the links holding one of them.
#[test]
fn an_always_leaf_in_a_quantifier_is_read() {
    let interpreter = interpreter();
    let mut host = host(Box::new(MemoryStore::default()));
    let every = ["chat/alpha", "chat/gamma", "mail/alpha", "mail/beta"];
    assert_eq!(agreed(&mut host, &interpreter, TAGGED, &json!({})), every);
    assert_eq!(
        agreed(&mut host, &interpreter, TAGGED, &json!({"tags": []})),
        every
    );
    assert_eq!(
        agreed(&mut host, &interpreter, TAGGED, &json!({"tags": ["red"]})),
        ["chat/alpha", "mail/alpha"]
    );
    assert_eq!(
        agreed(&mut host, &interpreter, TAGGED, &json!({"tags": ["green"]})),
        Vec::<String>::new()
    );
}

/// The consumer's filter b): the links whose key's `integration` equals the parameter.
#[test]
fn a_key_path_is_read() {
    let interpreter = interpreter();
    let mut host = host(Box::new(MemoryStore::default()));
    assert_eq!(
        agreed(
            &mut host,
            &interpreter,
            OF_INTEGRATION,
            &json!({"id": "mail"})
        ),
        ["mail/alpha", "mail/beta"]
    );
    assert_eq!(
        agreed(
            &mut host,
            &interpreter,
            OF_INTEGRATION,
            &json!({"id": "none"})
        ),
        Vec::<String>::new()
    );
    assert_eq!(
        agreed(&mut host, &interpreter, NAMED, &json!({})),
        ["chat/alpha", "mail/alpha"],
        "`never` beside a key member compared with a literal"
    );
    assert_eq!(
        agreed(&mut host, &interpreter, LISTED, &json!({})),
        ["chat/alpha", "chat/gamma"],
        "a key member listed `in:` literals"
    );
    assert_eq!(
        agreed(&mut host, &interpreter, COMPARED, &json!({})),
        ["chat/gamma"],
        "a key member `defined` and compared with another field"
    );
    assert_eq!(
        agreed(&mut host, &interpreter, REGIONAL, &json!({})),
        ["chat/alpha", "mail/alpha"],
        "a member of a struct field that is no identity, two levels deep through an `Optional`"
    );
}

/// A two-segment path that names no struct member and navigates no reference (`tags.count`) is
/// still refused whole, naming the filter.
#[test]
fn a_path_no_struct_holds_is_still_refused() {
    let refused = spindle_host::reads(&ir(), COUNTED)
        .expect_err("a list's count is no struct member")
        .to_string();
    assert!(
        refused.contains(COUNTED) && refused.contains("tags.count"),
        "the refusal names the view and its filter: {refused}"
    );
}

/// `always` beside a leaf still unread (`label.count`) does not make the filter read: it is
/// still refused whole, naming the unread leaf.
#[test]
fn an_unread_leaf_beside_always_is_still_refused() {
    let refused = spindle_host::reads(&ir(), FOLDED)
        .expect_err("a text's length is not read")
        .to_string();
    assert!(
        refused.contains(FOLDED) && refused.contains("label.count > 2"),
        "the refusal names the view and its whole filter: {refused}"
    );
}

/// Both constructs in one filter are served and answer the interpreter's rows.
#[test]
fn a_view_with_both_is_served() {
    let interpreter = interpreter();
    let mut host = host(Box::new(MemoryStore::default()));
    assert_eq!(
        agreed(
            &mut host,
            &interpreter,
            OF_INTEGRATION_TAGGED,
            &json!({"id": "chat", "tags": ["red"]})
        ),
        ["chat/alpha"]
    );
    assert_eq!(
        agreed(
            &mut host,
            &interpreter,
            OF_INTEGRATION_TAGGED,
            &json!({"id": "mail"})
        ),
        ["mail/alpha", "mail/beta"]
    );
}

/// Every read of the fixture answers, on the memory and on the SQLite store, what ESS's
/// interpreter answers, and is refused where it refuses.
#[test]
fn always_and_key_filters_agree_with_the_interpreter() {
    agree_on("memory", Box::new(MemoryStore::default()));
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    agree_on(
        "sqlite",
        Box::new(
            SqliteStore::open(&state.path().join("links.sqlite")).expect("the SQLite file opens"),
        ),
    );
}
