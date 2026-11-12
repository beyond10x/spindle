//! `run` serving a command whose outcome reads its own answer (`{response: id}`, ESS
//! `ResolvedPayloadValue::ResponseField`), over HTTP on 127.0.0.1 (`story:response-field-value-source`).
//!
//! The fixture `tests/fixtures/response-source`: `RegisterParcel` answers a `Uuid` `id`, publishes
//! it in `ParcelRegistered` and creates the parcel under it, the row's `id`. Its sibling
//! `tests/fixtures/response-source-baseline` answers the same response and reads it in neither
//! the payload nor the row, which is created under a minted id. Every path is read from the
//! `OpenAPI` document ESS projects for the component, and every answer is validated against it.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use ess_conformance::direct_response::Observation;
use ess_primitives::node::Node;
use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/response-source";
const BASELINE: &str = "crates/spindle-cli/tests/fixtures/response-source-baseline";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "desk";

const REGISTER: &str = "parcels.desk.RegisterParcel";
const PARCELS: &str = "parcels.desk.Parcels";

/// How long `run` may take to admit, open its state and print where it listens.
const STARTUP: Duration = Duration::from_secs(60);
/// How long one request may take to be answered.
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(&root().join(SPEC))
        .expect("the fixture compiles")
        .ir
}

/// The `OpenAPI` document ESS projects for the served component.
struct Contract(Value);

impl Contract {
    fn load() -> Self {
        let ir = ir();
        let component = ir
            .components()
            .values()
            .find(|component| component.name.as_str() == COMPONENT)
            .expect("the fixture declares the component");
        let document = ess_gen::openapi::json(&ir, component);
        Self(serde_json::from_str(&document).expect("the projection is JSON"))
    }

    /// The method and path of the operation `id`.
    fn route(&self, id: &str) -> (String, String) {
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

    /// Fails unless `body` validates against the schema the contract declares for operation `id`
    /// answering `status`.
    fn validate(&self, id: &str, status: u16, body: &Value) {
        let (method, path) = self.route(id);
        let schema = &self.0["paths"][&path][method.to_lowercase()]["responses"]
            [status.to_string()]["content"]["application/json"]["schema"];
        assert!(
            !schema.is_null(),
            "the contract declares no JSON answer of {id} under {status}: {body}"
        );
        let mut document = self.0.clone();
        document["allOf"] = json!([schema]);
        let validator =
            jsonschema::draft202012::new(&document).expect("the contract compiles as a schema");
        let errors = validator
            .iter_errors(body)
            .map(|error| format!("{} at {}", error, error.instance_path()))
            .collect::<Vec<_>>();
        assert!(
            errors.is_empty(),
            "{id} answered {status} with {body}, which its projected schema refuses: {errors:?}"
        );
    }
}

/// A running `b10x-spindle run` serving HTTP; stopped when dropped.
struct Served {
    child: Child,
    address: SocketAddr,
    _state: tempfile::TempDir,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn serve() -> Served {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--environment", LAPTOP, "--state"])
        .arg(state.path().join("state"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("b10x-spindle starts");
    let stdout = child.stdout.take().expect("stdout is piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = sender.send(line);
    });
    let line = match receiver.recv_timeout(STARTUP) {
        Ok(line) => line,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("run printed no line within {STARTUP:?}: {error}");
        }
    };
    let ready: Value = serde_json::from_str(&line)
        .unwrap_or_else(|error| panic!("run's first line {line:?} is not JSON ({error})"));
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("run's first line names no address: {ready}"));
    Served {
        child,
        address,
        _state: state,
    }
}

/// One HTTP/1.1 exchange on its own connection: the status and the JSON body.
fn exchange(address: SocketAddr, method: &str, path: &str, body: Option<&Value>) -> (u16, Value) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    stream
        .set_write_timeout(Some(ANSWER))
        .expect("a write timeout");
    let body = body.map(Value::to_string).unwrap_or_default();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .expect("the request is written");
    let mut answer = Vec::new();
    stream
        .read_to_end(&mut answer)
        .expect("the answer is read before the timeout");
    let answer = String::from_utf8(answer).expect("the answer is UTF-8");
    let (head, body) = answer
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("an answer with a head: {answer:?}"));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {head:?}"));
    let body = serde_json::from_str(body).unwrap_or_else(|error| panic!("{error}: {body:?}"));
    (status, body)
}

/// Calls the operation `id` at the method and path the contract publishes for it.
fn call(served: &Served, contract: &Contract, id: &str, body: Option<&Value>) -> (u16, Value) {
    let (method, path) = contract.route(id);
    exchange(served.address, &method, &path, body)
}

/// Registers a parcel labelled `label`; the answer, which must be `200` with a response that
/// validates against the contract.
fn register(served: &Served, contract: &Contract, label: &str) -> Value {
    let (status, answer) = call(served, contract, REGISTER, Some(&json!({"label": label})));
    assert_eq!(status, 200, "a returning outcome answers 200: {answer}");
    contract.validate(REGISTER, status, &answer);
    answer
}

fn answered_id(answer: &Value) -> &Value {
    let id = &answer["response"]["id"];
    assert!(id.is_string(), "the answer carries the id: {answer}");
    id
}

#[test]
fn a_payload_reads_the_returned_response_field() {
    let contract = Contract::load();
    let served = serve();
    let answer = register(&served, &contract, "fragile");
    assert_eq!(answer["outcome"], "registered", "{answer}");
    let id = answered_id(&answer);
    assert_eq!(
        answer["published"],
        json!([{
            "event": "parcels.desk.ParcelRegistered",
            "payload": {"id": id, "label": "fragile"},
        }]),
        "the published event carries the id the response answers"
    );

    let ir = ir();
    let command = ir
        .commands()
        .values()
        .find(|command| command.name.to_string() == REGISTER)
        .expect("the fixture declares RegisterParcel");
    let registered = command
        .outcomes
        .iter()
        .find(|outcome| outcome.name.as_str() == "registered")
        .expect("RegisterParcel declares `registered`");
    let selected = ess_compiler::refs::OutcomeRef::new(
        ess_compiler::refs::CommandRef::new(command.name.clone()),
        registered.name.clone(),
    );
    let check = Observation::of(&ir, command, Some(selected), BTreeMap::new())
        .expect("ESS derives the response check from the command");
    let response: BTreeMap<String, Node> = answer["response"]
        .as_object()
        .expect("the response is an object")
        .iter()
        .map(|(field, value)| {
            (
                field.clone(),
                serde_json::from_value(value.clone()).expect("a JSON value is a node"),
            )
        })
        .collect();
    check.compare(Some(&response)).unwrap_or_else(|refused| {
        panic!("ESS's response check refuses the served answer {answer}: {refused}")
    });
}

/// The rows the view `PARCELS` answers.
fn parcels(served: &Served, contract: &Contract) -> Vec<Value> {
    let (status, answer) = call(served, contract, PARCELS, None);
    assert_eq!(status, 200, "{answer}");
    contract.validate(PARCELS, status, &answer);
    answer["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("the view answers rows: {answer}"))
        .clone()
}

#[test]
fn sets_reads_the_returned_response_field() {
    let contract = Contract::load();
    let served = serve();
    assert_eq!(parcels(&served, &contract), Vec::<Value>::new());

    let first = register(&served, &contract, "fragile");
    let first_id = answered_id(&first).clone();
    assert_eq!(
        parcels(&served, &contract),
        [json!({"id": first_id, "label": "fragile"})],
        "the created row holds the id the request answered"
    );

    let second = register(&served, &contract, "heavy");
    let second_id = answered_id(&second).clone();
    assert_ne!(first_id, second_id, "each request answers its own id");
    let rows = parcels(&served, &contract);
    let second_row = rows
        .iter()
        .find(|row| row["label"] == "heavy")
        .unwrap_or_else(|| panic!("the second parcel is listed: {rows:?}"));
    assert_eq!(second_row["id"], second_id, "{rows:?}");
    assert_eq!(rows.len(), 2, "{rows:?}");
}

/// `b10x-spindle capabilities <spec>`'s text: each capability with the reasons listed under it.
fn capabilities(spec: &str) -> BTreeMap<String, Vec<String>> {
    let output = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["capabilities", spec])
        .output()
        .expect("b10x-spindle runs");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let text = String::from_utf8(output.stdout).expect("UTF-8");
    let mut listed: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut current = None;
    for line in text.lines() {
        if let Some(reason) = line.strip_prefix("  ") {
            let capability: &String = current
                .as_ref()
                .unwrap_or_else(|| panic!("a reason under no capability: {text}"));
            listed
                .get_mut(capability)
                .expect("listed")
                .push(reason.to_owned());
        } else {
            assert!(
                !listed.contains_key(line),
                "{line} is listed twice in:\n{text}"
            );
            listed.insert(line.to_owned(), Vec::new());
            current = Some(line.to_owned());
        }
    }
    listed
}

#[test]
fn capabilities_derive_one_mint_for_the_response_id() {
    let fixture = capabilities(SPEC);
    let baseline = capabilities(BASELINE);
    assert_eq!(
        fixture["identity.mint"],
        [
            "outcome parcels.desk.RegisterParcel/registered: mints the response field \
          `parcels.desk.RegisterParcel.id`"
        ],
        "one mint, for the response id: {fixture:?}"
    );
    assert_eq!(
        fixture.keys().collect::<Vec<_>>(),
        baseline.keys().collect::<Vec<_>>(),
        "the same requirements as the baseline: {fixture:?} against {baseline:?}"
    );
}
