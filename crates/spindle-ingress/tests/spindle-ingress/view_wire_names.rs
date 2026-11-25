//! A view whose fields declare wire names, served over HTTP: each row answered under its fields'
//! wire names, as the `OpenAPI` document ESS projects declares the row and as ESS's generated
//! servers answer it (`story:view-field-wire-names`). ESS's interpreter answers semantic rows
//! under the model names and never applies a wire name, so its rows are compared here with each
//! field renamed to the wire name the fixture declares.
//!
//! The fixture `tests/fixtures/wire-names` declares one component, `entries`, reached over the
//! network, and the view `Grouped`: the fields `entry_kind` (wire `type`), `group_label` (wire
//! `groupLabel`) and the `Optional` `about_text` (wire `aboutText`), `entry_id` without a wire
//! name, and the parameter `group_label` (wire `groupLabel`).

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
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

const COMPONENT: &str = "entries";
const ADD: &str = "catalog.entries.AddEntry";
const GROUPED: &str = "catalog.entries.Grouped";

/// Each field of `Grouped` and the name its rows answer it under.
const WIRE: [(&str, &str); 6] = [
    ("shade", "shade"),
    ("size", "size"),
    ("entry_id", "entry_id"),
    ("entry_kind", "type"),
    ("group_label", "groupLabel"),
    ("about_text", "aboutText"),
];

/// How many exchanges one request may wait for before the test gives up.
const STEPS: usize = 50;
/// How long one wait for an exchange lasts.
const WAIT: Duration = Duration::from_millis(200);

fn ir() -> EssIr {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/wire-names");
    spindle_spec::load(&fixture)
        .expect("the wire-names fixture compiles")
        .ir
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"wire-names\"\n",
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

/// The entries added before every read, each under the model names the host and the interpreter
/// take. Every entry holds a description, so no row holds an absent `Optional`, whose answer
/// `story:view-rows-absent-optional-fields` decides.
fn entries() -> Vec<Value> {
    vec![
        json!({"entry_id": "e1", "entry_kind": "tool", "group_label": "tools",
               "about_text": "a hammer", "shade": "pale_blue", "size": {"width_cm": 30}}),
        json!({"entry_id": "e2", "entry_kind": "tool", "group_label": "tools",
               "about_text": "a saw", "shade": "dark", "size": {"width_cm": 12}}),
        json!({"entry_id": "e3", "entry_kind": "book", "group_label": "books",
               "about_text": "a novel", "shade": "pale_blue", "size": {"width_cm": 20}}),
    ]
}

/// `value`, the interpreter's value of the field `name`, as the fixture declares it on the wire:
/// the variant `pale_blue` of `shade` spelled `PALE_BLUE`, the member `width_cm` of `size` named
/// `widthCm`, an integral number written as an integer.
fn written(name: &str, value: Value) -> Value {
    match (name, value) {
        ("shade", Value::String(variant)) if variant == "pale_blue" => json!("PALE_BLUE"),
        ("size", Value::Object(members)) => Value::Object(
            members
                .into_iter()
                .map(|(member, held)| {
                    let held = held
                        .as_f64()
                        .map_or(held.clone(), |width| json!(width as i64));
                    (
                        if member == "width_cm" {
                            "widthCm".to_owned()
                        } else {
                            member
                        },
                        held,
                    )
                })
                .collect(),
        ),
        (_, value) => value,
    }
}

/// The `OpenAPI` document ESS projects for `entries`.
fn contract(ir: &EssIr) -> Value {
    let component = ir
        .components()
        .values()
        .find(|component| component.name.as_str() == COMPONENT)
        .expect("the fixture declares the component");
    serde_json::from_str(&ess_gen::openapi::json(ir, component)).expect("the projection is JSON")
}

/// The method and path of `id` in `contract`.
fn operation(contract: &Value, id: &str) -> (String, String) {
    for (path, item) in contract["paths"]
        .as_object()
        .expect("the document has paths")
    {
        for (method, operation) in item.as_object().expect("a path item is an object") {
            if operation["operationId"] == id {
                return (method.to_uppercase(), path.clone());
            }
        }
    }
    panic!("the contract declares no operation {id}")
}

/// The errors of `body` against the schema `contract` declares for `id`'s `200`.
fn invalid(contract: &Value, id: &str, body: &Value) -> Vec<String> {
    let (method, path) = operation(contract, id);
    let schema = &contract["paths"][&path][method.to_lowercase()]["responses"]["200"]["content"]["application/json"]
        ["schema"];
    assert!(!schema.is_null(), "{id} declares no JSON 200 answer");
    let mut document = contract.clone();
    document["allOf"] = json!([schema]);
    let validator =
        jsonschema::draft202012::new(&document).expect("the contract compiles as a schema");
    validator
        .iter_errors(body)
        .map(|error| format!("{error} at {}", error.instance_path()))
        .collect()
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

/// The host serving the fixture with [`entries`] added, and ESS's interpreter with the same
/// commands run on it.
struct Catalog {
    host: Host,
    interpreter: Interpreted,
    listener: Listener,
    contract: Value,
    _state: tempfile::TempDir,
}

impl Catalog {
    fn open() -> Self {
        let ir = ir();
        let contract = contract(&ir);
        let surface = Surface::new(&ir, &[COMPONENT.to_owned()], None)
            .unwrap_or_else(|refused| panic!("the entries surface is served: {refused}"));
        let listener = Listener::bind(0, surface).expect("the listener binds");
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let interpreter = Interpreted::for_model(ir.clone());
        let mut host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
            .unwrap_or_else(|error| panic!("the host admits the entries: {error}"));
        let correlation = CorrelationId::new("wire-names").expect("a correlation");
        for input in entries() {
            let Value::Object(fields) = input.clone() else {
                panic!("not an object: {input}");
            };
            host.submit(ADD, fields)
                .unwrap_or_else(|error| panic!("{ADD} runs on the host: {error}"));
            interpreter
                .execute_command(SemanticCommandRequest {
                    command: ADD.parse().expect("a command name"),
                    actor: None,
                    caller: None,
                    input: serde_json::from_value(input).expect("the input as ESS values"),
                    correlation: correlation.clone(),
                })
                .unwrap_or_else(|error| panic!("{ADD} runs on the interpreter: {error:?}"));
        }
        Self {
            host,
            interpreter,
            listener,
            contract,
            _state: state,
        }
    }

    /// `GET` of `Grouped` with the query string `query`: the status and the body.
    fn get(&mut self, query: &str) -> (u16, Value) {
        let (method, path) = operation(&self.contract, GROUPED);
        let target = format!("{path}?{query}");
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
        assert!(client.is_finished(), "{query} was not answered in time");
        client.join().expect("the client thread")
    }

    /// The rows ESS's interpreter answers for `Grouped` in `group`, each field renamed to the
    /// name [`WIRE`] gives it and its value [`written`] as the fixture declares it.
    fn expected(&self, group: &str) -> Value {
        let params = BTreeMap::from([("group_label".to_owned(), Node::Text(group.to_owned()))]);
        let answer = self
            .interpreter
            .query_view(SemanticViewRequest {
                view: GROUPED.parse().expect("a view name"),
                params,
                consistency: QueryConsistency::Current,
                correlation: CorrelationId::new("wire-names").expect("a correlation"),
                deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
            })
            .unwrap_or_else(|error| panic!("the interpreter reads {GROUPED}: {error:?}"));
        let rows = answer
            .rows
            .iter()
            .map(|row| {
                Value::Object(
                    row.iter()
                        .map(|(name, value)| {
                            let wire = WIRE
                                .iter()
                                .find(|(model, _)| model == name)
                                .map_or(name.as_str(), |(_, wire)| wire);
                            (
                                wire.to_owned(),
                                written(
                                    name,
                                    serde_json::to_value(value).expect("a value as JSON"),
                                ),
                            )
                        })
                        .collect::<Map<String, Value>>(),
                )
            })
            .collect::<Vec<_>>();
        json!({"rows": rows})
    }
}

/// Each answer of `Grouped` is a body ESS's projected schema admits: its rows under the wire
/// names the generated server writes.
#[test]
fn a_view_row_validates_against_the_projected_schema() {
    let mut catalog = Catalog::open();
    for group in ["tools", "books", "none"] {
        let (status, body) = catalog.get(&format!("groupLabel={group}"));
        assert_eq!(status, 200, "{group}: {body}");
        let errors = invalid(&catalog.contract, GROUPED, &body);
        assert!(errors.is_empty(), "{group}: {body}: {errors:?}");
    }
}

/// Each row of `Grouped` is the interpreter's row with each field under its wire name, an enum
/// variant in its wire spelling and a struct's members under their wire names.
#[test]
fn a_view_row_is_answered_under_wire_names() {
    let mut catalog = Catalog::open();
    for group in ["tools", "books", "none"] {
        let (status, body) = catalog.get(&format!("groupLabel={group}"));
        assert_eq!(status, 200, "{group}: {body}");
        assert_eq!(body, catalog.expected(group), "{group}: the rows, renamed");
    }
}

/// A field that declares no wire name keeps its model name, and no row holds a model name a wire
/// name replaces.
#[test]
fn a_field_without_a_wire_name_keeps_its_name() {
    let mut catalog = Catalog::open();
    let (status, body) = catalog.get("groupLabel=tools");
    assert_eq!(status, 200, "{body}");
    let rows = body["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 2, "{body}");
    for row in rows {
        assert!(row.get("entry_id").is_some(), "{row}");
        for model in ["entry_kind", "group_label", "about_text"] {
            assert!(
                row.get(model).is_none(),
                "{model} is answered as a wire name: {row}"
            );
        }
    }
}

/// A view parameter is read under its wire name, as the projected document places it; under its
/// model name it is missing.
#[test]
fn a_view_parameter_is_read_under_its_wire_name() {
    let mut catalog = Catalog::open();
    let (status, body) = catalog.get("group_label=tools");
    assert_eq!(status, 400, "the model name is not the parameter: {body}");
    let (status, body) = catalog.get("groupLabel=books");
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["rows"].as_array().map(Vec::len), Some(1), "{body}");
}
