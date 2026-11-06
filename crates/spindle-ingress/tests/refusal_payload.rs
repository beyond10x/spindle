//! A refusal whose error declares fields answers over HTTP with a `payload` holding every field
//! the published `OpenAPI` schema requires, the fields `payload:` sets as ESS's interpreter sets
//! them and the others as ESS leaves them: the implementation's to choose.
//!
//! The fixture `tests/fixtures/desk` declares one component, `desk`, reached over the network.
//! `TakeDesk` refuses a taken desk with `AlreadyTaken`, whose `desk_id` the outcome sets from the
//! input and whose `holder` it leaves unset: the row the refusal is answered for holds a `holder`,
//! and ESS's served application reads it from there. `BookDesk` refuses a weekend booking with
//! `ClosedOnWeekends`, before any row is read, whose `desk_id` the outcome sets and whose `note`
//! nothing determines: ESS's interpreter leaves it to the implementation
//! (`interpret::execute::declared_error`, a generated value of its type), so it is minted.
//! Every answer is validated against the schema the document `ess_gen::openapi::json` declares
//! for its status.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ess_compiler::EssIr;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{ConformanceTarget, SemanticCommandRequest};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{Listener, Surface};

const COMPONENT: &str = "desk";
const OPEN: &str = "study.desk.OpenDesk";
const TAKE: &str = "study.desk.TakeDesk";
const BOOK: &str = "study.desk.BookDesk";

/// How many exchanges one request may wait for before the test gives up.
const STEPS: usize = 50;
/// How long one wait for an exchange lasts.
const WAIT: Duration = Duration::from_millis(200);

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn ir() -> EssIr {
    spindle_spec::load(&fixtures().join("desk"))
        .expect("the desk fixture compiles")
        .ir
}

/// The `OpenAPI` document ESS publishes for the component.
struct Contract(Value);

impl Contract {
    fn of(ir: &EssIr) -> Self {
        let component = ir
            .components()
            .values()
            .find(|component| component.name.as_str() == COMPONENT)
            .expect("the fixture declares the component");
        let document = ess_gen::openapi::json(ir, component);
        Self(serde_json::from_str(&document).expect("the projection is JSON"))
    }

    /// The method and path of the operation `id`.
    fn operation(&self, id: &str) -> (String, String) {
        let paths = self.0["paths"].as_object().expect("the document has paths");
        for (path, item) in paths {
            for (method, operation) in item.as_object().expect("a path item is an object") {
                if operation["operationId"] == id {
                    return (method.to_uppercase(), path.clone());
                }
            }
        }
        panic!("the contract declares no operation {id}")
    }

    /// Fails unless `body` validates against the schema the contract declares for `id`'s answer
    /// at `status`.
    fn validate(&self, id: &str, status: u16, body: &Value) {
        let (method, path) = self.operation(id);
        let schema = &self.0["paths"][&path][method.to_lowercase()]["responses"]
            [status.to_string()]["content"]["application/json"]["schema"];
        assert!(!schema.is_null(), "{id} declares no JSON {status} answer");
        let mut document = self.0.clone();
        document["allOf"] = json!([schema]);
        let validator =
            jsonschema::draft202012::new(&document).expect("the contract compiles as a schema");
        let errors = validator
            .iter_errors(body)
            .map(|error| format!("{error} at {}", error.instance_path()))
            .collect::<Vec<_>>();
        assert!(
            errors.is_empty(),
            "{id} answered {status} {body}: {errors:?}"
        );
    }
}

/// An environment that binds every store to `memory`.
fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"refusal-payload\"\n",
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
        "\"identity.mint\" = [\"uuid-v4\"]\n",
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

/// The host serving the fixture with desk `d1` opened for `ada` and taken, and ESS's interpreter
/// with the same commands run on it.
struct Room {
    host: Host,
    interpreter: Interpreted,
    listener: Listener,
    contract: Contract,
    _state: tempfile::TempDir,
}

impl Room {
    fn open() -> Self {
        let ir = ir();
        let contract = Contract::of(&ir);
        let surface = Surface::new(&ir, &[COMPONENT.to_owned()], None)
            .unwrap_or_else(|refused| panic!("the desk surface is served: {refused}"));
        let listener = Listener::bind(0, surface).expect("the listener binds");
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let interpreter = Interpreted::for_model(ir.clone());
        let host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
            .unwrap_or_else(|error| panic!("the host admits the desk: {error}"));
        let mut room = Self {
            host,
            interpreter,
            listener,
            contract,
            _state: state,
        };
        for (command, input) in [
            (OPEN, json!({"desk_id": "d1", "holder": "ada"})),
            (TAKE, json!({"desk_id": "d1"})),
        ] {
            room.host
                .submit(command, object(input.clone()))
                .unwrap_or_else(|error| panic!("{command} runs on the host: {error}"));
            let interpreted = room.interpreted(command, input);
            assert!(
                interpreted.is_none(),
                "{command} is refused: {interpreted:?}"
            );
        }
        room
    }

    /// Sends `command` with `input` to the listener and serves it on the host: the status and the
    /// body.
    fn send(&mut self, command: &str, input: &Value) -> (u16, Value) {
        let (method, path) = self.contract.operation(command);
        let address = self.listener.local_addr();
        let body = input.to_string();
        let client = thread::spawn(move || request(address, &method, &path, &body));
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

    /// The error ESS's interpreter reports for `command` with `input`: its name and the fields it
    /// carries, as JSON; `None` where it reports none.
    fn interpreted(&self, command: &str, input: Value) -> Option<(String, Map<String, Value>)> {
        let input: BTreeMap<String, Node> =
            serde_json::from_value(input).expect("the input as ESS values");
        let result = self
            .interpreter
            .execute_command(SemanticCommandRequest {
                command: command.parse().expect("a command name"),
                actor: None,
                caller: None,
                input,
                correlation: CorrelationId::new("refusal-payload").expect("a correlation"),
            })
            .unwrap_or_else(|error| panic!("{command} runs on the interpreter: {error:?}"));
        result.error.map(|error| {
            let fields = error
                .fields
                .iter()
                .map(|(field, value)| {
                    (
                        field.clone(),
                        serde_json::to_value(value).expect("a node is JSON"),
                    )
                })
                .collect();
            (error.error.to_string(), fields)
        })
    }
}

/// One HTTP/1.1 request with a JSON body, `Connection: close`: the status and the JSON body.
fn request(address: SocketAddr, method: &str, target: &str, body: &str) -> (u16, Value) {
    let mut stream = TcpStream::connect(address).expect("the listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a read timeout");
    write!(
        stream,
        "{method} {target} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
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

/// The two refusals the fixture answers: each command, its input, and the error it reports.
const REFUSALS: [(&str, &str, &str); 2] = [
    (TAKE, r#"{"desk_id": "d1"}"#, "study.desk.AlreadyTaken"),
    (
        BOOK,
        r#"{"desk_id": "d1", "weekend": true}"#,
        "study.desk.ClosedOnWeekends",
    ),
];

fn input(text: &str) -> Value {
    serde_json::from_str(text).expect("the input is JSON")
}

#[test]
fn a_refusal_answers_the_payload_its_schema_requires() {
    let mut room = Room::open();
    for (command, text, error) in REFUSALS {
        let (status, body) = room.send(command, &input(text));
        assert_eq!(body["error"], error, "{command} answered {status} {body}");
        room.contract.validate(command, status, &body);
    }
    let (_, taken) = room.send(TAKE, &input(REFUSALS[0].1));
    assert_eq!(
        taken["payload"],
        json!({"desk_id": "d1", "holder": "ada"}),
        "the holder nothing sets is read from the row the refusal is answered for, as ESS's \
         served application reads it"
    );
    let (_, closed) = room.send(BOOK, &input(REFUSALS[1].1));
    assert!(
        closed["payload"]["note"].is_string(),
        "the note nothing determines is minted as a `String`: {closed}"
    );
}

#[test]
fn refusal_payloads_agree_with_the_interpreter() {
    let mut room = Room::open();
    for (command, text, error) in REFUSALS {
        let (status, body) = room.send(command, &input(text));
        let (interpreted, carried) = room
            .interpreted(command, input(text))
            .unwrap_or_else(|| panic!("the interpreter refuses {command}"));
        assert_eq!(interpreted, error, "the interpreter's error for {command}");
        assert_eq!(body["error"], error, "{command} answered {status} {body}");
        let payload = body["payload"]
            .as_object()
            .unwrap_or_else(|| panic!("{command} answered no payload: {body}"));
        for (field, value) in &carried {
            assert_eq!(
                payload.get(field),
                Some(value),
                "{command}: `{field}` as the interpreter sets it, in {body}"
            );
        }
        let ir = ir();
        let declared = ir
            .errors()
            .values()
            .find(|declared| declared.name.to_string() == error)
            .expect("the fixture declares the error");
        let mut answered = payload.keys().cloned().collect::<Vec<_>>();
        answered.sort();
        let mut fields = declared
            .fields
            .iter()
            .map(|field| field.name.clone())
            .collect::<Vec<_>>();
        fields.sort();
        assert_eq!(
            answered, fields,
            "{command} answers every field its error declares, and only those: {body}"
        );
    }
}
