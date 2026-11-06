//! Adversary cases for `story:refusal-payload-complete`.
//!
//! The fixture `tests/fixtures/adversary-refusal` declares `ReleaseDesk`, whose `wrong_state:`
//! refusal `NotTaken` sets `desk_id` from the input and leaves `state` (of the entity's state
//! type) to the row the refusal is answered for. For an identity no record carries, ESS answers
//! that same branch, and its published schema says the `payload` is absent: an instance that does
//! not exist has nothing for the error to describe (`ess-gen` `openapi::outcome_schema`). Where a
//! `payload` is present it is the error's schema, which requires every declared field.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ess_compiler::EssIr;
use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{Listener, Surface};

const COMPONENT: &str = "desk";
const OPEN: &str = "study.desk.OpenDesk";
const RELEASE: &str = "study.desk.ReleaseDesk";
const STEPS: usize = 50;
const WAIT: Duration = Duration::from_millis(200);

fn ir() -> EssIr {
    spindle_spec::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary-refusal"),
    )
    .expect("the adversary-refusal fixture compiles")
    .ir
}

struct Contract(Value);

impl Contract {
    fn of(ir: &EssIr) -> Self {
        let component = ir
            .components()
            .values()
            .find(|component| component.name.as_str() == COMPONENT)
            .expect("the fixture declares the component");
        Self(serde_json::from_str(&ess_gen::openapi::json(ir, component)).expect("JSON"))
    }

    fn operation(&self, id: &str) -> (String, String) {
        for (path, item) in self.0["paths"].as_object().expect("paths") {
            for (method, operation) in item.as_object().expect("a path item") {
                if operation["operationId"] == id {
                    return (method.to_uppercase(), path.clone());
                }
            }
        }
        panic!("no operation {id}")
    }

    fn errors(&self, id: &str, status: u16, body: &Value) -> Vec<String> {
        let (method, path) = self.operation(id);
        let schema = &self.0["paths"][&path][method.to_lowercase()]["responses"]
            [status.to_string()]["content"]["application/json"]["schema"];
        assert!(!schema.is_null(), "{id} declares no JSON {status} answer");
        let mut document = self.0.clone();
        document["allOf"] = json!([schema]);
        let validator = jsonschema::draft202012::new(&document).expect("a schema");
        validator
            .iter_errors(body)
            .map(|error| format!("{error} at {}", error.instance_path()))
            .collect()
    }
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"adversary-refusal\"\n",
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
    .expect("the environment parses")
}

struct Room {
    host: Host,
    listener: Listener,
    contract: Contract,
    _state: tempfile::TempDir,
}

impl Room {
    fn open() -> Self {
        let ir = ir();
        let contract = Contract::of(&ir);
        let surface = Surface::new(&ir, &[COMPONENT.to_owned()], None)
            .unwrap_or_else(|refused| panic!("served: {refused}"));
        let listener = Listener::bind(0, surface).expect("the listener binds");
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
            .unwrap_or_else(|error| panic!("the host admits the fixture: {error}"));
        Self {
            host,
            listener,
            contract,
            _state: state,
        }
    }

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
}

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
    stream.read_to_string(&mut response).expect("the response");
    let (head, body) = response.split_once("\r\n\r\n").expect("a body");
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .expect("a status");
    (status, serde_json::from_str(body).expect("JSON"))
}

#[test]
fn adversary_a_wrong_state_refusal_of_a_held_row_answers_its_full_payload() {
    let mut room = Room::open();
    let (status, _) = room.send(OPEN, &json!({"desk_id": "d1", "holder": "ada"}));
    assert!(status < 300, "the desk opens");
    let (status, _) = room.send(RELEASE, &json!({"desk_id": "d1"}));
    assert!(status < 300, "the desk is released");
    let (status, body) = room.send(RELEASE, &json!({"desk_id": "d1"}));
    assert_eq!(body["error"], "study.desk.NotTaken", "{status} {body}");
    assert_eq!(
        body["payload"],
        json!({"desk_id": "d1", "state": "Free"}),
        "the state is read from the row"
    );
    let errors = room.contract.errors(RELEASE, status, &body);
    assert!(errors.is_empty(), "{status} {body}: {errors:?}");
}

#[test]
fn adversary_an_unknown_instance_wrong_state_refusal_answers_what_its_schema_admits() {
    let mut room = Room::open();
    let (status, body) = room.send(RELEASE, &json!({"desk_id": "nobody"}));
    assert_eq!(body["error"], "study.desk.NotTaken", "{status} {body}");
    let errors = room.contract.errors(RELEASE, status, &body);
    assert!(
        errors.is_empty(),
        "the unknown-instance arm answered {status} {body}, which the published schema refuses \
         (ESS answers it with no `payload`): {errors:?}"
    );
}
