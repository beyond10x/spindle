//! A refused outcome whose `ess-http/1` binding answers it with a status and an error object,
//! `body: {<member>: error}`, is answered with that status and an object whose `<member>` is the
//! error's wire name, beside the error's own fields under their wire names, and nothing is written
//! (`story:ingress-error-object-bodies`).
//!
//! `tests/fixtures/error-object-bodies` declares one component reached over the network and one
//! command, `SubmitForm`, which refuses an incomplete form with `InvalidField` (wire name
//! `invalid_field`, its `field` written `fieldName`) and otherwise takes it in.
//! `tests/fixtures/error-object-bodies-http.yaml` binds the refusal to `400` with
//! `body: {problem: error}`. Every answer is validated against the `OpenAPI` document ESS projects
//! for the component with the binding.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

const FIXTURE: &str = "crates/spindle-cli/tests/fixtures/error-object-bodies";
const BINDING: &str = "crates/spindle-cli/tests/fixtures/error-object-bodies-http.yaml";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "intake";

const SUBMIT: &str = "intake.form.SubmitForm";
const ALL_FORMS: &str = "intake.form.AllForms";

/// How long `run` may take to admit, open its state and print where it listens.
const STARTUP: Duration = Duration::from_secs(60);
/// How long one request may take to be answered.
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The `OpenAPI` document ESS projects for the fixture's component with the binding.
struct Contract(Value);

impl Contract {
    fn load() -> Self {
        let loaded = spindle_spec::load(&root().join(FIXTURE)).expect("the fixture compiles");
        let component = loaded
            .ir
            .components()
            .values()
            .find(|component| component.name.as_str() == COMPONENT)
            .expect("the fixture declares the component");
        let text = std::fs::read_to_string(root().join(BINDING)).expect("the binding is read");
        let parsed = ess_http::HttpSpec::from_yaml(&text).expect("the binding parses");
        let http = ess_http::compile(&parsed, &loaded.ir, &ess_gen::http::derived(&loaded.ir))
            .unwrap_or_else(|refused| panic!("ESS refuses the binding: {refused}"));
        let binding = ess_gen::http::Binding::new(http)
            .unwrap_or_else(|refused| panic!("the binding cannot be served: {refused}"));
        let document = ess_gen::openapi::bound_json(&loaded.ir, component, Some(&binding));
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

    /// Fails unless `body` validates against the JSON schema the contract declares for `id` under
    /// `status`.
    fn validate(&self, id: &str, status: u16, body: &Value) {
        let (method, path) = self.route(id);
        let schema = &self.0["paths"][&path][method.to_lowercase()]["responses"]
            [status.to_string()]["content"]["application/json"]["schema"];
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

/// A running `b10x-spindle run --http <binding>` serving HTTP; stopped when dropped.
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

/// `b10x-spindle run <fixture> --environment laptop --http <binding>`, its state and `TMPDIR` in a
/// scratch directory, once it prints where it listens.
fn serve() -> Served {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let file = state.path().join("state");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .env("TMPDIR", state.path())
        .args(["run", FIXTURE, "--environment", LAPTOP, "--state"])
        .arg(&file)
        .args(["--http", BINDING])
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
    Served {
        child,
        address,
        _state: state,
    }
}

/// One answer as written: its status, its header lines and its body.
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
        let (method, path) = contract.route(ALL_FORMS);
        let reply = self.exchange(&method, &path, None);
        assert_eq!(reply.status, 200, "{reply:?}");
        let rows = reply.json();
        contract.validate(ALL_FORMS, reply.status, &rows);
        rows
    }
}

#[test]
fn a_bound_error_outcome_answers_its_status_and_error_object() {
    let contract = Contract::load();
    let served = serve();

    let reply = served.send(
        &contract,
        SUBMIT,
        &json!({"form_id": "f1", "contact": "someone", "complete": false}),
    );
    assert_eq!(reply.status, 400, "the bound status: {reply:?}");
    assert_eq!(
        reply.header("content-type"),
        Some("application/json"),
        "{reply:?}"
    );
    let body = reply.json();
    assert_eq!(
        body,
        json!({
            "problem": "invalid_field",
            "fieldName": "Contact",
            "reason": "is not complete",
            "given": "someone",
        }),
        "the error's wire name under the bound member, beside its fields under their wire names"
    );
    contract.validate(SUBMIT, reply.status, &body);
    assert_eq!(
        served.rows(&contract),
        json!({"rows": []}),
        "the refusal writes nothing"
    );

    let taken = served.send(
        &contract,
        SUBMIT,
        &json!({"form_id": "f2", "contact": "someone", "complete": true}),
    );
    assert!((200..300).contains(&taken.status), "{taken:?}");
    assert_eq!(
        served.rows(&contract),
        json!({"rows": [{"form_id": "f2", "contact": "someone"}]}),
        "the route still takes in a complete form"
    );
}
