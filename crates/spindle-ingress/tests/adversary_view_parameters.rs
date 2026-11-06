//! Adversary cases for views with parameters, served over HTTP: each read where the `OpenAPI`
//! document ESS publishes for the binding places it, and compared with the rows ESS's
//! interpreter answers, or the refusal ESS's served application answers, for the same values.
//!
//! The fixture `tests/fixtures/adversary-view` declares one component, `drawer`, with views over
//! its notebooks by id (a newtype with a prefix), page count (`Integer`, and a newtype of one with
//! an invariant), instant (`Timestamp`), binding (`Boolean`), colour (an enum), drawer (an
//! `Optional<String>`) and both page count (in the path, `tests/fixtures/adversary-view-http.yaml`)
//! and drawer (in the query). `tests/fixtures/adversary-view-grants` declares a read-granted view.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ess_compiler::EssIr;
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
use spindle_host::{Host, ManualClock};
use spindle_ingress::{Caller, Listener, Surface};

const COMPONENT: &str = "drawer";
const ADD: &str = "desk.notebook.AddNotebook";
const BY_ID: &str = "desk.notebook.NotebookById";
const WITH_PAGES: &str = "desk.notebook.NotebooksWithPages";
const WITH_COUNT: &str = "desk.notebook.NotebooksWithCount";
const OPENED_AT: &str = "desk.notebook.NotebooksOpenedAt";
const BOUND: &str = "desk.notebook.NotebooksBound";
const OF_COLOUR: &str = "desk.notebook.NotebooksOfColour";
const IN_DRAWER: &str = "desk.notebook.NotebooksInDrawer";
const PLACED: &str = "desk.notebook.NotebooksPlaced";

const STEPS: usize = 50;
const WAIT: Duration = Duration::from_millis(200);

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn ir(name: &str) -> EssIr {
    spindle_spec::load(&fixtures().join(name))
        .unwrap_or_else(|error| panic!("the {name} fixture compiles: {error:?}"))
        .ir
}

fn binding(ir: &EssIr) -> ess_gen::http::Binding {
    let text =
        std::fs::read_to_string(fixtures().join("adversary-view-http.yaml")).expect("the binding");
    let spec = ess_http::HttpSpec::from_yaml(&text).expect("the binding parses");
    let http = ess_http::compile(&spec, ir, &ess_gen::http::derived(ir))
        .unwrap_or_else(|refused| panic!("ESS refuses the binding: {refused}"));
    ess_gen::http::Binding::new(http).unwrap_or_else(|refused| panic!("not servable: {refused}"))
}

/// The `OpenAPI` document ESS publishes for a component, with or without a binding.
struct Contract(Value);

impl Contract {
    fn of(ir: &EssIr, component: &str, binding: Option<&ess_gen::http::Binding>) -> Self {
        let component = ir
            .components()
            .values()
            .find(|declared| declared.name.as_str() == component)
            .expect("the fixture declares the component");
        let document = ess_gen::openapi::bound_json(ir, component, binding);
        Self(serde_json::from_str(&document).expect("the projection is JSON"))
    }

    fn operation(&self, id: &str) -> (String, String, &Value) {
        let paths = self.0["paths"].as_object().expect("the document has paths");
        for (path, item) in paths {
            for (method, operation) in item.as_object().expect("a path item is an object") {
                if operation["operationId"] == id {
                    return (method.to_uppercase(), path.clone(), operation);
                }
            }
        }
        panic!("the contract declares no operation {id}")
    }

    fn location(&self, id: &str, name: &str) -> String {
        let (_, _, operation) = self.operation(id);
        operation["parameters"]
            .as_array()
            .and_then(|parameters| {
                parameters
                    .iter()
                    .find(|parameter| parameter["name"] == name)
            })
            .and_then(|parameter| parameter["in"].as_str())
            .unwrap_or_else(|| panic!("{id} declares no parameter {name}"))
            .to_owned()
    }

    /// The request target for `id` with `values`, raw text each, placed where the contract
    /// places it.
    fn target(&self, id: &str, values: &[(&str, &str)]) -> (String, String) {
        let (method, mut path, _) = self.operation(id);
        let mut query = Vec::new();
        for (name, value) in values {
            match self.location(id, name).as_str() {
                "path" => path = path.replace(&format!("{{{name}}}"), value),
                "query" => query.push(format!("{name}={value}")),
                other => panic!("{id}: {name} is in {other}"),
            }
        }
        if !query.is_empty() {
            path = format!("{path}?{}", query.join("&"));
        }
        (method, path)
    }
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"adversary-view-parameters\"\n",
        "os = \"linux\"\n",
        "arch = \"x86_64\"\n",
        "\n",
        "[provides]\n",
        "\"store.durable\" = [\"memory\"]\n",
        "\"store.ephemeral\" = [\"memory\"]\n",
        "clock = [\"system\"]\n",
        "timer = [\"tokio\"]\n",
        "channel = [\"in-process\"]\n",
        "projection = [\"in-process\"]\n",
        "\"ingress.network\" = [\"http-loopback\"]\n",
    ))
    .expect("the in-memory environment parses")
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

/// `(id, pages, opened_at, bound, colour, drawer)`.
const NOTEBOOKS: [(&str, i64, &str, bool, &str, &str); 3] = [
    ("N-1", 42, "2026-01-01T00:00:00Z", true, "Red", "top shelf"),
    ("N-2", 0, "2026-02-01T00:00:00Z", false, "Blue", "a+b"),
    ("N-3", 7, "2026-03-01T00:00:00Z", true, "Blue", "top shelf"),
];

struct Desk {
    host: Host,
    interpreter: Interpreted,
    listener: Listener,
    contract: Contract,
    _state: tempfile::TempDir,
}

impl Desk {
    fn open() -> Self {
        let ir = ir("adversary-view");
        let binding = binding(&ir);
        let contract = Contract::of(&ir, COMPONENT, Some(&binding));
        let surface = Surface::new(&ir, &[COMPONENT.to_owned()], Some(&binding))
            .unwrap_or_else(|refused| panic!("the desk surface is served: {refused}"));
        let listener = Listener::bind(0, surface).expect("the listener binds");
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let interpreter = Interpreted::for_model(ir.clone());
        let mut host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
            .unwrap_or_else(|error| panic!("the host admits the desk: {error}"));
        let correlation = CorrelationId::new("adversary-view").expect("a correlation");
        for (id, pages, opened_at, bound, colour, drawer) in NOTEBOOKS {
            let input = json!({
                "notebook_id": id,
                "pages": pages,
                "opened_at": opened_at,
                "bound": bound,
                "colour": colour,
                "drawer": drawer,
            });
            host.submit(ADD, object(input.clone()))
                .unwrap_or_else(|error| panic!("{id} is added on the host: {error}"));
            let input: BTreeMap<String, Node> =
                serde_json::from_value(input).expect("the input as ESS values");
            interpreter
                .execute_command(SemanticCommandRequest {
                    command: ADD.parse().expect("a command name"),
                    actor: None,
                    caller: None,
                    input,
                    correlation: correlation.clone(),
                })
                .unwrap_or_else(|error| panic!("{id} is added on the interpreter: {error:?}"));
        }
        Self {
            host,
            interpreter,
            listener,
            contract,
            _state: state,
        }
    }

    fn get(&mut self, target: (String, String)) -> (u16, Value) {
        serve(&self.listener, &mut self.host, target)
    }

    /// The rows ESS's interpreter answers for `view` with `params`, or its refusal.
    fn interpreted(&self, view: &str, params: Value) -> Result<Value, String> {
        let params: BTreeMap<String, Node> =
            serde_json::from_value(params).expect("the parameters as ESS values");
        self.interpreter
            .query_view(SemanticViewRequest {
                view: view.parse().expect("a view name"),
                params,
                consistency: QueryConsistency::Current,
                correlation: CorrelationId::new("adversary-view").expect("a correlation"),
                deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
            })
            .map(|answer| {
                json!({"rows": integral(serde_json::to_value(answer.rows).expect("rows as JSON"))})
            })
            .map_err(|error| format!("{error:?}"))
    }

    fn rows(&self, view: &str, params: Value) -> Value {
        self.interpreted(view, params.clone())
            .unwrap_or_else(|error| panic!("the interpreter reads {view} {params}: {error}"))
    }
}

fn integral(value: Value) -> Value {
    match value {
        Value::Number(number) => match number.as_f64() {
            Some(float) if number.is_f64() && float.fract() == 0.0 && float.abs() < 9.0e15 => {
                json!(float as i64)
            }
            _ => Value::Number(number),
        },
        Value::Array(items) => Value::Array(items.into_iter().map(integral).collect()),
        Value::Object(members) => Value::Object(
            members
                .into_iter()
                .map(|(name, member)| (name, integral(member)))
                .collect(),
        ),
        other => other,
    }
}

fn serve(listener: &Listener, host: &mut Host, (method, target): (String, String)) -> (u16, Value) {
    let address = listener.local_addr();
    let client = thread::spawn(move || request(address, &method, &target));
    for _ in 0..STEPS {
        if client.is_finished() {
            break;
        }
        if let Some(exchange) = listener.next(WAIT) {
            exchange.serve(host);
        }
    }
    assert!(client.is_finished(), "the request was not answered in time");
    client.join().expect("the client thread")
}

fn request(address: SocketAddr, method: &str, target: &str) -> (u16, Value) {
    let mut stream = TcpStream::connect(address).expect("the listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a read timeout");
    write!(
        stream,
        "{method} {target} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    )
    .expect("the request is written");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("the response is read");
    let (head, body) = response
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("a response without a body: {response}"));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a response without a status: {head}"));
    let body = serde_json::from_str(body).unwrap_or_else(|_| panic!("not JSON: {body}"));
    (status, body)
}

/// ESS's served application reads exact decimal text as a JSON number and decodes it with
/// `str::parse::<i64>` (`ess-synth` `json.rs` `integer_at`), so `0042` is 42 and `-0` is 0, in the
/// query string and in a path segment alike.
#[test]
fn an_integer_parameter_spelt_with_leading_zeros_reads_as_its_value() {
    let mut desk = Desk::open();
    let expected = desk.rows(WITH_PAGES, json!({"pages": 42}));
    assert_eq!(expected["rows"].as_array().map(Vec::len), Some(1));
    let (status, body) = desk.get(desk.contract.target(WITH_PAGES, &[("pages", "0042")]));
    assert_eq!(
        (status, &body),
        (200, &expected),
        "query `pages=0042` is 42"
    );

    assert_eq!(desk.contract.location(PLACED, "pages"), "path");
    let expected = desk.rows(PLACED, json!({"pages": 7, "drawer": "top shelf"}));
    assert_eq!(expected, json!({"rows": [{"notebook_id": "N-3"}]}));
    let (status, body) = desk.get(
        desk.contract
            .target(PLACED, &[("pages", "007"), ("drawer", "top+shelf")]),
    );
    assert_eq!((status, &body), (200, &expected), "path segment `007` is 7");
}

#[test]
fn an_integer_parameter_spelt_as_negative_zero_reads_as_zero() {
    let mut desk = Desk::open();
    let expected = desk.rows(WITH_PAGES, json!({"pages": 0}));
    assert_eq!(
        expected,
        json!({"rows": [{"notebook_id": "N-2", "pages": 0}]})
    );
    let (status, body) = desk.get(desk.contract.target(WITH_PAGES, &[("pages", "-0")]));
    assert_eq!((status, &body), (200, &expected), "query `pages=-0` is 0");
}

/// A whole number no `Integer` holds is refused by ESS's decoder, never rounded through a float.
#[test]
fn an_integer_parameter_past_64_bits_is_refused_400() {
    let mut desk = Desk::open();
    for text in [
        "9223372036854775808",
        "18446744073709551616",
        "-9223372036854775809",
    ] {
        let (status, body) = desk.get(desk.contract.target(WITH_PAGES, &[("pages", text)]));
        assert_eq!(status, 400, "`pages={text}` is no Integer: {body}");
    }
}

/// ESS's decoder holds a newtype's prefix and invariants (`ess-synth` `wire.rs`
/// `newtype_body`, beyond10x/ess#579), and the interpreter refuses a read whose parameter breaks
/// them (`validate_typed_value`): `count=0` breaks `value > 0`, `id=X-1` the prefix `N-`.
#[test]
fn a_parameter_breaking_its_newtype_constraint_is_refused_400() {
    let mut desk = Desk::open();
    assert!(
        desk.interpreted(WITH_COUNT, json!({"count": 0})).is_err(),
        "the interpreter refuses count 0"
    );
    assert!(
        desk.interpreted(BY_ID, json!({"id": "X-1"})).is_err(),
        "the interpreter refuses id X-1"
    );
    for (view, name, text) in [(WITH_COUNT, "count", "0"), (BY_ID, "id", "X-1")] {
        let (status, body) = desk.get(desk.contract.target(view, &[(name, text)]));
        assert_eq!(status, 400, "{view} `{name}={text}`: {body}");
        assert!(
            body["refused"].as_str().unwrap_or_default().contains(name),
            "the refusal names {name}: {body}"
        );
    }
}

/// A declared `Timestamp` compares by the instant it names under every operator, `==` included
/// (`ess-primitives` `predicate.rs`, "so `==` agrees with `<=` and `>=` on two spellings of one
/// instant"): the interpreter reads its facts typed (`TypedFacts::orders_as_instant`).
#[test]
fn a_timestamp_parameter_selects_the_rows_of_its_instant_however_spelt() {
    let mut desk = Desk::open();
    let expected = desk.rows(OPENED_AT, json!({"at": "2026-01-01T01:00:00+01:00"}));
    assert_eq!(
        expected,
        json!({"rows": [{"notebook_id": "N-1", "opened_at": "2026-01-01T00:00:00Z"}]}),
        "the interpreter reads the instant"
    );
    let (status, body) = desk.get(
        desk.contract
            .target(OPENED_AT, &[("at", "2026-01-01T01%3A00%3A00%2B01%3A00")]),
    );
    assert_eq!((status, &body), (200, &expected));
}

/// Controls: the parameter types the implementation reads the interpreter's way.
#[test]
fn boolean_enum_and_encoded_text_parameters_answer_the_interpreters_rows() {
    let mut desk = Desk::open();
    for (view, params, values) in [
        (BOUND, json!({"bound": true}), vec![("bound", "true")]),
        (BOUND, json!({"bound": false}), vec![("bound", "false")]),
        (
            OF_COLOUR,
            json!({"colour": "Blue"}),
            vec![("colour", "Blue")],
        ),
        (
            IN_DRAWER,
            json!({"drawer": "a+b"}),
            vec![("drawer", "a%2Bb")],
        ),
        (
            IN_DRAWER,
            json!({"drawer": "top shelf"}),
            vec![("drawer", "top+shelf")],
        ),
        (WITH_COUNT, json!({"count": 7}), vec![("count", "7")]),
    ] {
        let expected = desk.rows(view, params.clone());
        let (status, body) = desk.get(desk.contract.target(view, &values));
        assert_eq!((status, &body), (200, &expected), "{view} {params}");
    }
    for (view, name, text) in [(BOUND, "bound", "True"), (OF_COLOUR, "colour", "Green")] {
        let (status, body) = desk.get(desk.contract.target(view, &[(name, text)]));
        assert_eq!(status, 400, "{view} `{name}={text}`: {body}");
    }
}

/// `drawer == param.drawer` with the `Optional` `drawer` left out is unknown for every row, and
/// the interpreter refuses the read rather than answer no rows (`interpret/views.rs` `select`:
/// only a string operator's absent operand hides a row). Answering `200 {"rows": []}` is a row
/// set the interpreter never answers.
#[test]
fn an_equality_with_an_omitted_optional_parameter_is_not_answered_as_no_rows() {
    let mut desk = Desk::open();
    let interpreted = desk.interpreted(IN_DRAWER, json!({}));
    assert!(
        interpreted.is_err(),
        "the interpreter refuses the read: {interpreted:?}"
    );
    let (status, body) = desk.get(desk.contract.target(IN_DRAWER, &[]));
    assert_ne!(
        status, 200,
        "the interpreter answers no rows here, and the ingress answered {body}"
    );
}

/// ESS's served surface checks a read-granted view's reader first (`ess-synth` `http.rs`, "A
/// read-granted view checks the reader first", beyond10x/ess#286): a caller the grant does not
/// name is refused `403` whatever its query string holds.
#[test]
fn a_read_granted_view_refuses_an_ungranted_fixed_caller_before_reading_its_query() {
    let ir = ir("adversary-view-grants");
    let caller = Caller {
        actor: "desk.leaf.Guest".to_owned(),
        attributes: Map::new(),
    };
    let surface = Surface::fixing_caller(&ir, &["binder".to_owned()], None, &caller)
        .unwrap_or_else(|refused| panic!("the binder surface is served: {refused}"));
    let contract = Contract::of(&ir, "binder", None);
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .unwrap_or_else(|error| panic!("the host admits the binder: {error}"));
    let view = "desk.leaf.LeavesOfPages";
    let (status, body) = serve(
        &listener,
        &mut host,
        contract.target(view, &[("pages", "1")]),
    );
    assert_eq!(status, 403, "a well-formed read is not granted: {body}");
    for values in [
        vec![("pages", "1"), ("pages", "2")],
        vec![("pages", "")],
        vec![("pages", "%ZZ")],
    ] {
        let target = contract.target(view, &values);
        let (status, body) = serve(&listener, &mut host, target.clone());
        assert_eq!(
            status, 403,
            "{} {}: the reader is checked first: {body}",
            target.0, target.1
        );
    }
}
