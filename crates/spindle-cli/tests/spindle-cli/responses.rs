//! `run` serving a command whose outcome returns the command's response (`returns: true`), over
//! HTTP on 127.0.0.1.
//!
//! The fixture is the kernel's `tests/fixtures/responses`: one component, `desk`, reached over the
//! network, whose `ShareNotebook` returns `{link, share_code}` with `one_time_response:
//! [share_code]`. Every path is read from the `OpenAPI` document ESS projects for that component,
//! and every answer is validated, exactly as served, against the schema that document declares for
//! it under its status.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-kernel/tests/fixtures/responses";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "desk";

const OPEN: &str = "notes.book.OpenNotebook";
const SHARE: &str = "notes.book.ShareNotebook";
const NOTEBOOKS: &str = "notes.book.Notebooks";

/// How long `run` may take to admit, open its state and print where it listens.
const STARTUP: Duration = Duration::from_secs(60);
/// How long one request may take to be answered.
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The `OpenAPI` document ESS projects for the served component.
struct Contract(Value);

impl Contract {
    fn load() -> Self {
        let loaded = spindle_spec::load(&root().join(SPEC)).expect("the fixture compiles");
        let component = loaded
            .ir
            .components()
            .values()
            .find(|component| component.name.as_str() == COMPONENT)
            .expect("the fixture declares the component");
        let document = ess_gen::openapi::json(&loaded.ir, component);
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
    state: tempfile::TempDir,
}

impl Served {
    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        self.stop();
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
        state,
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

/// Calls the operation `id` at the method and path the contract publishes for it, and validates
/// the answer against the contract.
fn call(served: &Served, contract: &Contract, id: &str, body: Option<&Value>) -> (u16, Value) {
    let (method, path) = contract.route(id);
    let (status, answer) = exchange(served.address, &method, &path, body);
    contract.validate(id, status, &answer);
    (status, answer)
}

fn open(served: &Served, contract: &Contract, notebook_id: &str) {
    let (status, answer) = call(
        served,
        contract,
        OPEN,
        Some(&json!({"notebook_id": notebook_id, "title": "Field notes"})),
    );
    assert_eq!(status, 202, "{answer}");
}

/// Shares `notebook_id` with `audience`; the answer, which must be `200` with a response.
fn share(served: &Served, contract: &Contract, notebook_id: &str, audience: &str) -> Value {
    let (status, answer) = call(
        served,
        contract,
        SHARE,
        Some(&json!({"notebook_id": notebook_id, "audience": audience})),
    );
    assert_eq!(status, 200, "a returning outcome answers 200: {answer}");
    answer
}

fn share_code(answer: &Value) -> String {
    answer["response"]["share_code"]
        .as_str()
        .unwrap_or_else(|| panic!("the answer carries the share code: {answer}"))
        .to_owned()
}

#[test]
fn the_ingress_answers_the_response_in_the_envelope() {
    let contract = Contract::load();
    let served = serve();
    open(&served, &contract, "nb-1");

    let answer = share(&served, &contract, "nb-1", "readers");
    let members: Vec<&str> = answer
        .as_object()
        .expect("an envelope is an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(members, ["outcome", "published", "response"], "{answer}");
    assert_eq!(answer["outcome"], "shared");
    assert_eq!(
        answer["published"],
        json!([{
            "event": "notes.book.NotebookShared",
            "payload": {"notebook_id": "nb-1", "audience": "readers"},
        }])
    );
    let response = answer["response"]
        .as_object()
        .unwrap_or_else(|| panic!("the response is an object: {answer}"));
    assert_eq!(
        response.keys().map(String::as_str).collect::<Vec<_>>(),
        ["link", "share_code"],
        "{answer}"
    );
    assert!(response.values().all(Value::is_string), "{answer}");

    let (status, refused) = call(
        &served,
        &contract,
        SHARE,
        Some(&json!({"notebook_id": "nb-none", "audience": "readers"})),
    );
    assert_eq!(status, 404, "{refused}");
    assert_eq!(refused["error"], "notes.book.NoSuchNotebook");
    assert!(
        refused.get("response").is_none(),
        "a refusal carries no response: {refused}"
    );
}

/// Every file under `directory`, read whole.
fn files(directory: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).expect("the state directory is readable") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let bytes = std::fs::read(&path).expect("a state file is readable");
                found.push((path, bytes));
            }
        }
    }
    found
}

fn holds(bytes: &[u8], text: &str) -> bool {
    bytes
        .windows(text.len())
        .any(|window| window == text.as_bytes())
}

#[test]
fn a_one_time_field_appears_nowhere_else() {
    let contract = Contract::load();
    let mut served = serve();
    open(&served, &contract, "nb-1");
    let first = share(&served, &contract, "nb-1", "first-audience");
    let code = share_code(&first);
    assert!(!code.is_empty());

    let later = share(&served, &contract, "nb-1", "second-audience");
    assert!(
        !later.to_string().contains(&code),
        "a later answer holds the earlier share code {code:?}: {later}"
    );
    open(&served, &contract, "nb-2");
    let other = share(&served, &contract, "nb-2", "third-audience");
    assert!(
        !other.to_string().contains(&code),
        "another notebook's answer holds the share code {code:?}: {other}"
    );
    let (status, refused) = call(
        &served,
        &contract,
        SHARE,
        Some(&json!({"notebook_id": "nb-none", "audience": "readers"})),
    );
    assert_eq!(status, 404, "{refused}");
    assert!(!refused.to_string().contains(&code), "{refused}");

    let (status, rows) = call(&served, &contract, NOTEBOOKS, None);
    assert_eq!(status, 200, "{rows}");
    assert!(
        rows.to_string().contains("second-audience"),
        "the view holds what the shares stored: {rows}"
    );
    assert!(
        !rows.to_string().contains(&code),
        "the view holds the share code {code:?}: {rows}"
    );
    let (status, events) = exchange(served.address, "GET", "/api/events", None);
    assert_eq!(status, 200, "{events}");
    assert!(
        events.to_string().contains("first-audience"),
        "the published events hold what the first share published: {events}"
    );
    assert!(
        !events.to_string().contains(&code),
        "a published event holds the share code {code:?}: {events}"
    );

    served.stop();
    let stored = files(served.state.path());
    assert!(
        stored
            .iter()
            .any(|(_, bytes)| holds(bytes, "first-audience")),
        "the state directory holds what the first share stored: {:?}",
        stored.iter().map(|(path, _)| path).collect::<Vec<_>>()
    );
    for (path, bytes) in &stored {
        assert!(
            !holds(bytes, &code),
            "{} holds the share code {code:?}",
            path.display()
        );
    }
}
