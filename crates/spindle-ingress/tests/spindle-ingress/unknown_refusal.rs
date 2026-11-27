//! An `unknown_instance:` refusal is served as ESS's served application serves it
//! (`ess-synth` `determined.rs`): a required field of its error that the outcome gives no source
//! has no row to be read from, so the outcome is not served, `501` naming it and the field, and
//! nothing is written; the published schema requires that field (`ess-gen` `openapi.rs`). The
//! command's other outcomes are served as before, a refusal that sources every field answers the
//! full payload, and an `Optional` field without a source is left out.
//!
//! The fixture `tests/fixtures/unknown-refusal` declares one component, `bin`, reached over the
//! network. `FillBin` refuses an unknown bin with `NoSuchBin`, whose `bin_id` its outcome gives no
//! source; `SealBin` with `UnknownBin`, whose `bin_id` it reads from the input; `LabelBin` with
//! `MissingBin`, whose `bin_id` it reads from the input and whose `Optional` `hint` nothing gives.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
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

const COMPONENT: &str = "bin";
const OPEN: &str = "study.bin.OpenBin";
const FILL: &str = "study.bin.FillBin";
const SEAL: &str = "study.bin.SealBin";
const LABEL: &str = "study.bin.LabelBin";

/// How many exchanges one request may wait for before the test gives up.
const STEPS: usize = 50;
/// How long one wait for an exchange lasts.
const WAIT: Duration = Duration::from_millis(200);

fn ir() -> EssIr {
    spindle_spec::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/unknown-refusal"),
    )
    .expect("the unknown-refusal fixture compiles")
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
        "name = \"unknown-refusal\"\n",
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

/// The host serving the fixture with bin `b1` opened.
struct Store {
    host: Host,
    listener: Listener,
    contract: Contract,
    report: Vec<String>,
    _state: tempfile::TempDir,
}

impl Store {
    fn open() -> Self {
        let ir = ir();
        let contract = Contract::of(&ir);
        let surface = Surface::new(&ir, &[COMPONENT.to_owned()], None)
            .unwrap_or_else(|refused| panic!("the bin surface is served: {refused}"));
        let report = surface.report();
        let listener = Listener::bind(0, surface).expect("the listener binds");
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let mut host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
            .unwrap_or_else(|error| panic!("the host admits the bins: {error}"));
        host.submit(OPEN, object(json!({"bin_id": "b1", "content": "nails"})))
            .unwrap_or_else(|error| panic!("{OPEN} runs on the host: {error}"));
        Self {
            host,
            listener,
            contract,
            report,
            _state: state,
        }
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

    /// What the host holds: every event published and the rows of each entity.
    fn written(&mut self) -> (usize, Vec<(String, usize)>) {
        let events = self
            .host
            .published()
            .expect("the host reads its events")
            .len();
        let rows = self.host.rows().expect("the host counts its rows");
        (events, rows)
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

/// The error ESS's interpreter reports for `command` with `input` on a model with no bin: its
/// name and the fields it carries, as JSON.
fn interpreted(command: &str, input: Value) -> (String, Map<String, Value>) {
    let input: BTreeMap<String, Node> =
        serde_json::from_value(input).expect("the input as ESS values");
    let result = Interpreted::for_model(ir())
        .execute_command(SemanticCommandRequest {
            command: command.parse().expect("a command name"),
            actor: None,
            caller: None,
            input,
            correlation: CorrelationId::new("unknown-refusal").expect("a correlation"),
        })
        .unwrap_or_else(|error| panic!("{command} runs on the interpreter: {error:?}"));
    let error = result
        .error
        .unwrap_or_else(|| panic!("the interpreter refuses {command}"));
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
}

#[test]
fn an_unknown_instance_refusal_with_an_unsourced_field_is_not_served() {
    let mut store = Store::open();
    assert!(
        store
            .report
            .iter()
            .any(|line| line.contains(&format!("`{FILL}/no-such-bin`"))
                && line.contains("study.bin.NoSuchBin.bin_id")),
        "the outcome is listed as answered 501, naming the field: {:?}",
        store.report
    );
    let before = store.written();
    let (status, body) = store.send(FILL, &json!({"bin_id": "b9", "item": "screws"}));
    assert_eq!(status, 501, "{body}");
    let refused = body["refused"]
        .as_str()
        .unwrap_or_else(|| panic!("the 501 names what is not served: {body}"));
    assert!(
        refused.contains(&format!("{FILL}/no-such-bin")),
        "the 501 names the outcome: {refused}"
    );
    assert!(
        refused.contains("study.bin.NoSuchBin.bin_id"),
        "the 501 names the field: {refused}"
    );
    assert_eq!(body["committed"], false, "{body}");
    assert_eq!(store.written(), before, "nothing is written");
}

#[test]
fn the_other_outcomes_of_that_route_are_still_served() {
    let mut store = Store::open();
    let (status, body) = store.send(FILL, &json!({"bin_id": "b1", "item": "screws"}));
    assert_eq!(body["outcome"], "filled", "{status} {body}");
    store.contract.validate(FILL, status, &body);
    assert_eq!(
        body["published"],
        json!([{"event": "study.bin.BinFilled", "payload": {"bin_id": "b1", "item": "screws"}}]),
        "{body}"
    );
}

#[test]
fn an_unknown_instance_refusal_whose_fields_all_have_sources_is_served() {
    let mut store = Store::open();
    let input = json!({"bin_id": "b9"});
    let before = store.written();
    let (status, body) = store.send(SEAL, &input);
    assert_eq!(body["error"], "study.bin.UnknownBin", "{status} {body}");
    assert_eq!(body["payload"], json!({"bin_id": "b9"}), "{body}");
    store.contract.validate(SEAL, status, &body);
    let (error, carried) = interpreted(SEAL, input);
    assert_eq!(error, "study.bin.UnknownBin");
    assert_eq!(
        Value::Object(carried),
        body["payload"],
        "the payload the interpreter carries"
    );
    assert_eq!(store.written(), before, "a refusal writes nothing");
}

#[test]
fn an_optional_unsourced_field_of_an_unknown_instance_refusal_is_left_out() {
    let mut store = Store::open();
    let input = json!({"bin_id": "b9", "label": "spare"});
    let (status, body) = store.send(LABEL, &input);
    assert_eq!(body["error"], "study.bin.MissingBin", "{status} {body}");
    let payload = body["payload"]
        .as_object()
        .unwrap_or_else(|| panic!("the refusal answers a payload: {body}"));
    assert_eq!(
        payload.get("hint"),
        None,
        "the hint nothing gives has no key: {body}"
    );
    assert_eq!(payload.get("bin_id"), Some(&json!("b9")), "{body}");
    store.contract.validate(LABEL, status, &body);
    let (error, carried) = interpreted(LABEL, input);
    assert_eq!(error, "study.bin.MissingBin");
    assert_eq!(&carried, payload, "the payload the interpreter carries");
}
