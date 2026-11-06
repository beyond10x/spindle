//! Views that declare parameters, served over HTTP: each parameter read where the `OpenAPI`
//! document ESS publishes for the binding places it (the path or the query string), and each
//! answer the rows ESS's interpreter answers for those values.
//!
//! The fixture `tests/fixtures/shelf` declares one component, `shelf`, reached over the network,
//! with three views over its books: `BookById` (an `id` parameter the binding carries in the
//! path), `BooksIn` (a `state` parameter, in the query string) and `BooksOfYear` (an `Integer`
//! `year`, at ESS's derived route). Every method, path and parameter location is read from the
//! document `ess_gen::openapi::bound_json` projects with the binding `tests/fixtures/shelf-http.yaml`,
//! and every `200` answer is validated against the schema that document declares for it.

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
use spindle_ingress::{Listener, Surface};

const COMPONENT: &str = "shelf";
const SHELVE: &str = "library.shelf.ShelveBook";
const LEND: &str = "library.shelf.LendBook";
const BY_ID: &str = "library.shelf.BookById";
const IN_STATE: &str = "library.shelf.BooksIn";
const OF_YEAR: &str = "library.shelf.BooksOfYear";

/// How many exchanges one request may wait for before the test gives up.
const STEPS: usize = 50;
/// How long one wait for an exchange lasts.
const WAIT: Duration = Duration::from_millis(200);

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn ir() -> EssIr {
    spindle_spec::load(&fixtures().join("shelf"))
        .expect("the shelf fixture compiles")
        .ir
}

fn binding(ir: &EssIr) -> ess_gen::http::Binding {
    let text = std::fs::read_to_string(fixtures().join("shelf-http.yaml")).expect("the binding");
    let spec = ess_http::HttpSpec::from_yaml(&text).expect("the binding parses");
    let http = ess_http::compile(&spec, ir, &ess_gen::http::derived(ir))
        .unwrap_or_else(|refused| panic!("ESS refuses the binding: {refused}"));
    ess_gen::http::Binding::new(http).unwrap_or_else(|refused| panic!("not servable: {refused}"))
}

/// The `OpenAPI` document ESS publishes for the component with the binding.
struct Contract(Value);

impl Contract {
    fn of(ir: &EssIr, binding: &ess_gen::http::Binding) -> Self {
        let component = ir
            .components()
            .values()
            .find(|component| component.name.as_str() == COMPONENT)
            .expect("the fixture declares the component");
        let document = ess_gen::openapi::bound_json(ir, component, Some(binding));
        Self(serde_json::from_str(&document).expect("the projection is JSON"))
    }

    /// The method, path template and operation object of `id`.
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

    /// Where the contract places the parameter `name` of `id`: `path` or `query`, and whether it
    /// is required.
    fn parameter(&self, id: &str, name: &str) -> (String, bool) {
        let (_, _, operation) = self.operation(id);
        let parameter = operation["parameters"]
            .as_array()
            .and_then(|parameters| {
                parameters
                    .iter()
                    .find(|parameter| parameter["name"] == name)
            })
            .unwrap_or_else(|| panic!("{id} declares no parameter {name}"));
        (
            parameter["in"].as_str().expect("a location").to_owned(),
            parameter["required"] == true,
        )
    }

    /// The request target for `id` with `values`, each placed where the contract places it:
    /// substituted into the path, or appended to the query string.
    fn target(&self, id: &str, values: &[(&str, &str)]) -> (String, String) {
        let (method, mut path, _) = self.operation(id);
        let mut query = Vec::new();
        for (name, value) in values {
            match self.parameter(id, name).0.as_str() {
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

    /// Fails unless `body` validates against the schema the contract declares for `id`'s `200`.
    fn validate(&self, id: &str, body: &Value) {
        let (method, path, _) = self.operation(id);
        let schema = &self.0["paths"][&path][method.to_lowercase()]["responses"]["200"]["content"]
            ["application/json"]["schema"];
        assert!(!schema.is_null(), "{id} declares no JSON 200 answer");
        let mut document = self.0.clone();
        document["allOf"] = json!([schema]);
        let validator =
            jsonschema::draft202012::new(&document).expect("the contract compiles as a schema");
        let errors = validator
            .iter_errors(body)
            .map(|error| format!("{error} at {}", error.instance_path()))
            .collect::<Vec<_>>();
        assert!(errors.is_empty(), "{id} answered {body}: {errors:?}");
    }
}

/// An environment that binds every store to `memory`.
fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"view-parameters\"\n",
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

/// The books every test shelves, `(id, title, year)`; `b2` and `b3` are then lent.
const BOOKS: [(&str, &str, i64); 3] = [
    ("b1", "Atlas", 1990),
    ("b2", "Bestiary", 2001),
    ("b3", "Codex", 1990),
];
const LENT: [&str; 2] = ["b2", "b3"];

/// The host serving the fixture with [`BOOKS`] shelved and [`LENT`] lent, and ESS's interpreter
/// with the same commands run on it.
struct Shelf {
    host: Host,
    interpreter: Interpreted,
    listener: Listener,
    contract: Contract,
    _state: tempfile::TempDir,
}

impl Shelf {
    fn open() -> Self {
        let ir = ir();
        let binding = binding(&ir);
        let contract = Contract::of(&ir, &binding);
        let surface = Surface::new(&ir, &[COMPONENT.to_owned()], Some(&binding))
            .unwrap_or_else(|refused| panic!("the shelf surface is served: {refused}"));
        let listener = Listener::bind(0, surface).expect("the listener binds");
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let interpreter = Interpreted::for_model(ir.clone());
        let mut host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
            .unwrap_or_else(|error| panic!("the host admits the shelf: {error}"));
        let correlation = CorrelationId::new("view-parameters").expect("a correlation");
        let mut run = |command: &str, input: Value| {
            host.submit(command, object(input.clone()))
                .unwrap_or_else(|error| panic!("{command} runs on the host: {error}"));
            let input: BTreeMap<String, Node> =
                serde_json::from_value(input).expect("the input as ESS values");
            interpreter
                .execute_command(SemanticCommandRequest {
                    command: command.parse().expect("a command name"),
                    actor: None,
                    caller: None,
                    input,
                    correlation: correlation.clone(),
                })
                .unwrap_or_else(|error| panic!("{command} runs on the interpreter: {error:?}"));
        };
        for (id, title, year) in BOOKS {
            run(SHELVE, json!({"book_id": id, "title": title, "year": year}));
        }
        for id in LENT {
            run(LEND, json!({"book_id": id}));
        }
        Self {
            host,
            interpreter,
            listener,
            contract,
            _state: state,
        }
    }

    /// Sends `method target` to the listener and serves it on the host: the status and the body.
    fn get(&mut self, (method, target): (String, String)) -> (u16, Value) {
        let address = self.listener.local_addr();
        let client = thread::spawn(move || request(address, &method, &target));
        for _ in 0..STEPS {
            if client.is_finished() {
                break;
            }
            if let Some(exchange) = self.listener.next(WAIT) {
                exchange.serve(&mut self.host);
            }
        }
        assert!(client.is_finished(), "the request was not answered in time");
        client.join().expect("the client thread")
    }

    /// The rows ESS's interpreter answers for `view` read with `params`, as JSON.
    fn interpreted(&self, view: &str, params: Value) -> Value {
        let params: BTreeMap<String, Node> =
            serde_json::from_value(params).expect("the parameters as ESS values");
        let answer = self
            .interpreter
            .query_view(SemanticViewRequest {
                view: view.parse().expect("a view name"),
                params,
                consistency: QueryConsistency::Current,
                correlation: CorrelationId::new("view-parameters").expect("a correlation"),
                deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
            })
            .unwrap_or_else(|error| panic!("the interpreter reads {view}: {error:?}"));
        json!({"rows": integral(serde_json::to_value(answer.rows).expect("rows as JSON"))})
    }
}

/// `value` with every integral float written as the integer it is: ESS's values reach JSON here
/// through `f64`, so the interpreter's `1990` arrives as `1990.0`, which JSON compares unequal to
/// the `1990` ESS's served application writes.
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

/// One HTTP/1.1 request with no body, `Connection: close`: the status and the JSON body.
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

#[test]
fn a_view_parameter_in_the_path_selects_the_row() {
    let mut shelf = Shelf::open();
    assert_eq!(
        shelf.contract.parameter(BY_ID, "id"),
        ("path".to_owned(), true),
        "the contract carries id in the path"
    );
    let target = shelf.contract.target(BY_ID, &[("id", "b2")]);
    assert_eq!(target.1, "/books/b2");
    let (status, body) = shelf.get(target);
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body,
        json!({"rows": [{"book_id": "b2", "title": "Bestiary", "state": "Lent"}]})
    );
    assert_eq!(body, shelf.interpreted(BY_ID, json!({"id": "b2"})));
    shelf.contract.validate(BY_ID, &body);

    let (status, body) = shelf.get(shelf.contract.target(BY_ID, &[("id", "b9")]));
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, json!({"rows": []}));
    assert_eq!(body, shelf.interpreted(BY_ID, json!({"id": "b9"})));
    shelf.contract.validate(BY_ID, &body);
}

#[test]
fn a_view_parameter_in_the_query_filters_the_rows() {
    let mut shelf = Shelf::open();
    assert_eq!(
        shelf.contract.parameter(IN_STATE, "state"),
        ("query".to_owned(), true),
        "the contract carries state in the query string"
    );
    let target = shelf.contract.target(IN_STATE, &[("state", "Lent")]);
    assert_eq!(target.1, "/books?state=Lent");
    let (status, body) = shelf.get(target);
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body,
        json!({"rows": [
            {"book_id": "b2", "title": "Bestiary"},
            {"book_id": "b3", "title": "Codex"},
        ]})
    );
    assert_eq!(body, shelf.interpreted(IN_STATE, json!({"state": "Lent"})));
    shelf.contract.validate(IN_STATE, &body);

    let (status, body) = shelf.get(shelf.contract.target(IN_STATE, &[("state", "Available")]));
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body,
        shelf.interpreted(IN_STATE, json!({"state": "Available"}))
    );
    assert_eq!(body, json!({"rows": [{"book_id": "b1", "title": "Atlas"}]}));

    let (status, body) = shelf.get(shelf.contract.target(OF_YEAR, &[("year", "1990")]));
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, shelf.interpreted(OF_YEAR, json!({"year": 1990})));
    assert_eq!(
        body,
        json!({"rows": [{"book_id": "b1", "year": 1990}, {"book_id": "b3", "year": 1990}]})
    );
    shelf.contract.validate(OF_YEAR, &body);

    let mut ignored = shelf.contract.target(IN_STATE, &[("state", "Lent")]);
    ignored.1.push_str("&shelf=top");
    let (status, body) = shelf.get(ignored);
    assert_eq!(
        status, 200,
        "a key no parameter declares is ignored: {body}"
    );
    assert_eq!(body, shelf.interpreted(IN_STATE, json!({"state": "Lent"})));
}

#[test]
fn a_malformed_view_parameter_is_refused_400() {
    let mut shelf = Shelf::open();
    for (view, values, named) in [
        (OF_YEAR, [("year", "nineteen")], "year"),
        (OF_YEAR, [("year", "")], "year"),
        (IN_STATE, [("state", "Missing")], "state"),
    ] {
        let target = shelf.contract.target(view, &values);
        let (status, body) = shelf.get(target.clone());
        assert_eq!(status, 400, "{} {}: {body}", target.0, target.1);
        let refused = body["refused"].as_str().unwrap_or_default();
        assert!(
            refused.contains(named),
            "{} {}: the refusal names {named}: {body}",
            target.0,
            target.1
        );
    }
    let mut twice = shelf.contract.target(IN_STATE, &[("state", "Lent")]);
    twice.1.push_str("&state=Available");
    let (status, body) = shelf.get(twice);
    assert_eq!(status, 400, "a parameter given twice: {body}");
    assert!(
        body["refused"]
            .as_str()
            .unwrap_or_default()
            .contains("state")
    );
}

#[test]
fn a_missing_required_view_parameter_is_refused_400() {
    let mut shelf = Shelf::open();
    for (view, name) in [(IN_STATE, "state"), (OF_YEAR, "year")] {
        assert_eq!(
            shelf.contract.parameter(view, name),
            ("query".to_owned(), true),
            "the contract requires {name} in the query string"
        );
        let target = shelf.contract.target(view, &[]);
        let (status, body) = shelf.get(target.clone());
        assert_eq!(status, 400, "{} {}: {body}", target.0, target.1);
        assert!(
            body["refused"].as_str().unwrap_or_default().contains(name),
            "the refusal names {name}: {body}"
        );
    }
}
