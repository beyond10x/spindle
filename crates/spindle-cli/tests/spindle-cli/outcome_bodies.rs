//! A command outcome whose `ess-http/1` binding lists a success body, `response`,
//! `response.<field>` or `none`, is answered with that body and the bound status; one bound with
//! headers answers them beside it (`story:outcome-response-headers`); one bound with a redirect or
//! a non-default encoding still answers `501`, naming the outcome and what this ingress does not
//! write (`story:http-outcome-bodies`).
//!
//! `tests/fixtures/outcome-bodies` declares one component reached over the network and six
//! commands, each creating one parcel. `tests/fixtures/outcome-bodies-http.yaml` binds the one
//! outcome of `RegisterParcel` to `body: response`, of `TagParcel` to `body: response.id` and of
//! `StoreParcel` to `204` with `body: none`. `AnnounceParcel`, `ForwardParcel` and `PrintParcel`
//! each have a second, unlisted outcome; the one a request selects with a flag is bound to a
//! header, a redirect and the `text` encoding. Every answer is validated against the `OpenAPI`
//! document ESS projects for the component with the binding.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

const FIXTURE: &str = "crates/spindle-cli/tests/fixtures/outcome-bodies";
const BINDING: &str = "crates/spindle-cli/tests/fixtures/outcome-bodies-http.yaml";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "depot";

const REGISTER: &str = "depot.parcel.RegisterParcel";
const TAG: &str = "depot.parcel.TagParcel";
const STORE: &str = "depot.parcel.StoreParcel";
const ANNOUNCE: &str = "depot.parcel.AnnounceParcel";
const FORWARD: &str = "depot.parcel.ForwardParcel";
const PRINT: &str = "depot.parcel.PrintParcel";
const ALL_PARCELS: &str = "depot.parcel.AllParcels";

/// The seed every run is reset with, so one request mints the same response in each.
const SEED: u64 = 7;

/// How long `run` may take to admit, open its state and print where it listens.
const STARTUP: Duration = Duration::from_secs(60);
/// How long one request may take to be answered.
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The `OpenAPI` document ESS projects for the fixture's component, with the binding where given.
struct Contract(Value);

impl Contract {
    fn load(binding: Option<&str>) -> Self {
        let loaded = spindle_spec::load(&root().join(FIXTURE)).expect("the fixture compiles");
        let component = loaded
            .ir
            .components()
            .values()
            .find(|component| component.name.as_str() == COMPONENT)
            .expect("the fixture declares the component");
        let binding = binding.map(|binding| {
            let text = std::fs::read_to_string(root().join(binding)).expect("the binding is read");
            let parsed = ess_http::HttpSpec::from_yaml(&text).expect("the binding parses");
            let http = ess_http::compile(&parsed, &loaded.ir, &ess_gen::http::derived(&loaded.ir))
                .unwrap_or_else(|refused| panic!("ESS refuses the binding: {refused}"));
            ess_gen::http::Binding::new(http)
                .unwrap_or_else(|refused| panic!("the binding cannot be served: {refused}"))
        });
        let document = ess_gen::openapi::bound_json(&loaded.ir, component, binding.as_ref());
        Self(serde_json::from_str(&document).expect("the projection is JSON"))
    }

    /// The method and path template of the operation `id`.
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

    /// The response the contract declares for `id` under `status`.
    fn response(&self, id: &str, status: u16) -> &Value {
        let (method, path) = self.route(id);
        let response =
            &self.0["paths"][&path][method.to_lowercase()]["responses"][status.to_string()];
        assert!(
            !response.is_null(),
            "the contract declares no answer of {id} under {status}"
        );
        response
    }

    /// Fails unless `body` validates against the JSON schema the contract declares for `id` under
    /// `status`.
    fn validate(&self, id: &str, status: u16, body: &Value) {
        let schema = &self.response(id, status)["content"]["application/json"]["schema"];
        assert!(
            !schema.is_null(),
            "the contract declares no JSON answer of {id} under {status}"
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

/// `b10x-spindle run <fixture> --environment laptop <arguments>`, its state and `TMPDIR` in a
/// scratch directory, once it prints where it listens, reset with [`SEED`].
fn serve(arguments: &[&str]) -> Served {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let file = state.path().join("state");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .env("TMPDIR", state.path())
        .args(["run", FIXTURE, "--environment", LAPTOP, "--state"])
        .arg(&file)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
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
    let ready: Value = serde_json::from_str(&line).unwrap_or_else(|error| {
        let _ = child.kill();
        let _ = child.wait();
        panic!("run's first line {line:?} is not JSON ({error})")
    });
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("run's first line names no address: {ready}"));
    let served = Served {
        child,
        address,
        _state: state,
    };
    let reset = served.exchange("POST", "/api/reset", Some(&json!({"mint": SEED})));
    assert_eq!(reset.status, 200, "{reset:?}");
    served
}

/// One answer as written: its status, its header lines and its body bytes.
#[derive(Debug)]
struct Reply {
    status: u16,
    head: Vec<String>,
    body: String,
}

impl Reply {
    /// The value of the header `name`, compared ignoring case, where the answer has one.
    fn header(&self, name: &str) -> Option<&str> {
        self.head.iter().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim()
                .eq_ignore_ascii_case(name)
                .then_some(value.trim())
        })
    }

    /// The body read as JSON.
    fn json(&self) -> Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|error| panic!("{error}: the body is not JSON: {self:?}"))
    }
}

impl Served {
    /// One HTTP/1.1 exchange on its own connection.
    fn exchange(&self, method: &str, path: &str, body: Option<&Value>) -> Reply {
        let mut stream =
            TcpStream::connect_timeout(&self.address, ANSWER).expect("the listener accepts");
        stream
            .set_read_timeout(Some(ANSWER))
            .expect("a read timeout");
        let body = body.map(Value::to_string).unwrap_or_default();
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            self.address,
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
        let mut lines = head.lines();
        let status = lines
            .next()
            .and_then(|line| line.split(' ').nth(1))
            .and_then(|status| status.parse().ok())
            .unwrap_or_else(|| panic!("a status line: {head:?}"));
        Reply {
            status,
            head: lines.map(str::to_owned).collect(),
            body: body.to_owned(),
        }
    }

    /// The command `id` sent at its route with `input`.
    fn send(&self, contract: &Contract, id: &str, input: &Value) -> Reply {
        let (method, path) = contract.route(id);
        self.exchange(&method, &path, Some(input))
    }

    fn rows(&self, contract: &Contract) -> Value {
        let (method, path) = contract.route(ALL_PARCELS);
        let reply = self.exchange(&method, &path, None);
        assert_eq!(reply.status, 200, "{reply:?}");
        let rows = reply.json();
        contract.validate(ALL_PARCELS, reply.status, &rows);
        rows
    }
}

/// The `response` member of the envelope `id` answers for `input` without the binding, on a run
/// reset with [`SEED`]: what the bare body must equal.
fn enveloped_response(id: &str, input: &Value) -> Value {
    let contract = Contract::load(None);
    let served = serve(&[]);
    let reply = served.send(&contract, id, input);
    assert!((200..300).contains(&reply.status), "{reply:?}");
    let envelope = reply.json();
    contract.validate(id, reply.status, &envelope);
    let response = envelope["response"].clone();
    assert!(
        response.is_object(),
        "the envelope carries the response: {envelope}"
    );
    response
}

#[test]
fn a_response_body_is_served_bare() {
    let input = json!({"parcel_id": "p1", "label": "fragile"});
    let expected = enveloped_response(REGISTER, &input);

    let contract = Contract::load(Some(BINDING));
    let served = serve(&["--http", BINDING]);
    let reply = served.send(&contract, REGISTER, &input);
    assert_eq!(reply.status, 201, "the bound status: {reply:?}");
    assert_eq!(
        reply.header("content-type"),
        Some("application/json"),
        "{reply:?}"
    );
    let body = reply.json();
    assert_eq!(
        body, expected,
        "the bare body is the response the envelope would carry"
    );
    contract.validate(REGISTER, reply.status, &body);
    assert_eq!(
        served.rows(&contract),
        json!({"rows": [{"parcel_id": "p1", "label": "fragile"}]})
    );
}

#[test]
fn a_response_field_body_is_served_bare() {
    let input = json!({"parcel_id": "p2", "label": "boxed"});
    let expected = enveloped_response(TAG, &input);

    let contract = Contract::load(Some(BINDING));
    let served = serve(&["--http", BINDING]);
    let reply = served.send(&contract, TAG, &input);
    assert_eq!(reply.status, 201, "the bound status: {reply:?}");
    assert_eq!(
        reply.header("content-type"),
        Some("application/json"),
        "{reply:?}"
    );
    let body = reply.json();
    assert!(body.is_string(), "the value of `id` alone: {body}");
    assert_eq!(
        body, expected["id"],
        "the bare body is the `id` of the response the envelope would carry"
    );
    contract.validate(TAG, reply.status, &body);
    assert_eq!(
        served.rows(&contract),
        json!({"rows": [{"parcel_id": "p2", "label": "boxed"}]})
    );
}

#[test]
fn a_none_body_answers_no_content() {
    let contract = Contract::load(Some(BINDING));
    let served = serve(&["--http", BINDING]);
    let reply = served.send(
        &contract,
        STORE,
        &json!({"parcel_id": "p3", "label": "flat"}),
    );
    assert_eq!(reply.status, 204, "the bound status: {reply:?}");
    assert_eq!(reply.body, "", "no body: {reply:?}");
    assert_eq!(reply.header("content-type"), None, "{reply:?}");
    assert!(
        contract.response(STORE, 204).get("content").is_none(),
        "the contract declares no content under 204: {}",
        contract.response(STORE, 204)
    );
    assert_eq!(
        served.rows(&contract),
        json!({"rows": [{"parcel_id": "p3", "label": "flat"}]})
    );
}

#[test]
fn a_header_outcome_answers_its_header_beside_the_envelope() {
    let contract = Contract::load(Some(BINDING));
    let served = serve(&["--http", BINDING]);
    let reply = served.send(
        &contract,
        ANNOUNCE,
        &json!({"parcel_id": "p4", "label": "loud", "announce": true}),
    );
    assert_eq!(reply.status, 201, "the bound status: {reply:?}");
    assert_eq!(reply.header("x-announced"), Some("yes"), "{reply:?}");
    let declared = &contract.response(ANNOUNCE, 201)["headers"]["X-Announced"];
    assert_eq!(
        declared["schema"]["const"], "yes",
        "the value the projection declares: {declared}"
    );
    let answer = reply.json();
    assert_eq!(answer["outcome"], "announced", "{answer}");
    contract.validate(ANNOUNCE, reply.status, &answer);
    assert_eq!(
        served.rows(&contract),
        json!({"rows": [{"parcel_id": "p4", "label": "loud"}]})
    );
}

#[test]
fn an_unserved_outcome_presentation_is_refused_by_name() {
    let contract = Contract::load(Some(BINDING));
    let served = serve(&["--http", BINDING]);
    for (id, outcome, missing, input) in [
        (
            FORWARD,
            "depot.parcel.ForwardParcel/forwarded",
            "redirect",
            json!({"parcel_id": "p5", "label": "away", "return_to": "https://example.test/back", "forward": true}),
        ),
        (
            PRINT,
            "depot.parcel.PrintParcel/printed",
            "encoding",
            json!({"parcel_id": "p6", "label": "inked", "print": true}),
        ),
    ] {
        let reply = served.send(&contract, id, &input);
        assert_eq!(reply.status, 501, "{reply:?}");
        let answer = reply.json();
        contract.validate(id, reply.status, &answer);
        assert_eq!(answer["committed"], false, "{answer}");
        let refused = answer["refused"].as_str().expect("a refusal in words");
        assert!(
            refused.contains(outcome),
            "the 501 names the outcome {outcome}: {refused}"
        );
        assert!(
            refused.contains(missing),
            "the 501 names what this ingress does not write, {missing}: {refused}"
        );
    }
    assert_eq!(
        served.rows(&contract),
        json!({"rows": []}),
        "nothing was written"
    );
}
