//! `run` serving a component `reached_by: network` over HTTP on 127.0.0.1.
//!
//! The fixture `tests/fixtures/ingress` declares one component, `board`, reached over the network.
//! A command is `POST /api/commands/<qualified name>` and a view `GET /api/views/<qualified name>`;
//! which operations exist, their methods and statuses are read from the `OpenAPI` document ESS
//! projects for that component (`ess_gen::openapi::json`). A view answers the body ESS projects for
//! it, validated as is; a command answers `{outcome, error?, events, invocations}`, whose outcome,
//! events and error are validated against the outcome envelope the same document declares.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/ingress";
const LAPTOP: &str = "examples/environments/laptop.toml";
const MICROVM: &str = "examples/environments/microvm.toml";
const BINDING: &str = "crates/spindle-cli/tests/fixtures/ingress-http.yaml";
const COMPONENT: &str = "board";

const OPEN_TASK: &str = "tasks.board.OpenTask";
const CLOSE_TASK: &str = "tasks.board.CloseTask";
const ALL_TASKS: &str = "tasks.board.AllTasks";
const OPEN_TASKS: &str = "tasks.board.OpenTasks";

/// How long `run` may take to admit, open its state and print where it listens.
const STARTUP: Duration = Duration::from_secs(60);
/// How long one request may take to be answered.
const ANSWER: Duration = Duration::from_secs(10);
/// How long a refused `run` may take to exit on its own.
const EXIT: Duration = Duration::from_secs(60);

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

    /// The document ESS projects with the `ess-http/1` binding at [`BINDING`], compiled as
    /// `ess specify http validate` compiles it.
    fn bound() -> Self {
        let loaded = spindle_spec::load(&root().join(SPEC)).expect("the fixture compiles");
        let text = std::fs::read_to_string(root().join(BINDING)).expect("the binding is read");
        let spec = ess_http::HttpSpec::from_yaml(&text).expect("the binding parses");
        let http = ess_http::compile(&spec, &loaded.ir, &ess_gen::http::derived(&loaded.ir))
            .unwrap_or_else(|refused| panic!("ESS refuses the binding: {refused}"));
        let binding = ess_gen::http::Binding::new(http)
            .unwrap_or_else(|refused| panic!("the binding cannot be served: {refused}"));
        let component = loaded
            .ir
            .components()
            .values()
            .find(|component| component.name.as_str() == COMPONENT)
            .expect("the fixture declares the component");
        let document = ess_gen::openapi::bound_json(&loaded.ir, component, Some(&binding));
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
    fn validate(&self, id: &str, status: u16, answer: &Value) {
        let body = if answer.get("outcome").is_some() {
            projected(answer)
        } else {
            answer.clone()
        };
        let body = &body;
        let validator = self.validator(id, status);
        assert!(
            !validator.is_valid(&json!({"outcome": "undeclared", "published": []})),
            "the schema of {id} under {status} admits an undeclared outcome, so it checks nothing"
        );
        let errors = validator
            .iter_errors(body)
            .map(|error| format!("{} at {}", error, error.instance_path()))
            .collect::<Vec<_>>();
        assert!(
            errors.is_empty(),
            "{id} answered {status} with {body}, which its projected schema refuses: {errors:?}"
        );
    }

    /// The contract as a schema rooted at the answer of `id` under `status`.
    fn validator(&self, id: &str, status: u16) -> jsonschema::Validator {
        let (method, path) = self.route(id);
        let schema = &self.0["paths"][&path][method.to_lowercase()]["responses"]
            [status.to_string()]["content"]["application/json"]["schema"];
        assert!(
            !schema.is_null(),
            "the contract declares no JSON answer of {id} under {status}"
        );
        let mut document = self.0.clone();
        document["allOf"] = json!([schema]);
        jsonschema::draft202012::new(&document).expect("the contract compiles as a schema")
    }
}

/// A running `b10x-spindle run` serving HTTP; stopped when dropped.
struct Served {
    child: Child,
    address: SocketAddr,
    ready: Value,
    _state: tempfile::TempDir,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn serve() -> Served {
    serve_with(&[])
}

/// `run` on the fixture and laptop, with `arguments` added.
fn serve_with(arguments: &[&str]) -> Served {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--environment", LAPTOP, "--state"])
        .arg(state.path().join("state"))
        .args(arguments)
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
    let ready: Value = match serde_json::from_str(&line) {
        Ok(ready) => ready,
        Err(error) => {
            let _ = child.kill();
            let output = child.wait_with_output();
            panic!("run's first line {line:?} is not JSON ({error}): {output:?}");
        }
    };
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("run's first line names no address: {ready}"));
    Served {
        child,
        address,
        ready,
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
    assert!(
        head.to_ascii_lowercase()
            .contains("content-type: application/json"),
        "{head}"
    );
    let body = serde_json::from_str(body).unwrap_or_else(|error| panic!("{error}: {body:?}"));
    (status, body)
}

/// A command answer as the outcome envelope ESS projects: `outcome`, its events as `published`,
/// and a refusal's error name and payload as `error` and `payload`. Fails unless the answer holds
/// exactly `outcome`, `events`, `invocations` and, where refused, `error`.
fn projected(answer: &Value) -> Value {
    let mut keys = answer
        .as_object()
        .unwrap_or_else(|| panic!("a command answers an object: {answer}"))
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    keys.sort_unstable();
    let expected: &[&str] = if answer.get("error").is_some() {
        &["error", "events", "invocations", "outcome"]
    } else {
        &["events", "invocations", "outcome"]
    };
    assert_eq!(keys, expected, "{answer}");
    assert!(answer["invocations"].is_array(), "{answer}");
    let mut envelope = json!({"outcome": answer["outcome"], "published": answer["events"]});
    if let Some(error) = answer.get("error") {
        let mut error_keys = error
            .as_object()
            .unwrap_or_else(|| panic!("an error is an object: {answer}"))
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        error_keys.retain(|key| *key != "payload");
        assert_eq!(error_keys, ["error"], "{answer}");
        envelope["error"] = error["error"].clone();
        if let Some(payload) = error.get("payload") {
            envelope["payload"] = payload.clone();
        }
    }
    envelope
}

/// Calls the operation `id` under the method the contract declares for it, at
/// `/api/commands/<id>` for a command and `/api/views/<id>` for a view.
fn call(served: &Served, contract: &Contract, id: &str, body: Option<&Value>) -> (u16, Value) {
    let (method, _) = contract.route(id);
    let path = match method.as_str() {
        "POST" => format!("/api/commands/{id}"),
        "GET" => format!("/api/views/{id}"),
        other => panic!("the contract serves {id} under {other}"),
    };
    exchange(served.address, &method, &path, body)
}

fn rows(served: &Served, contract: &Contract, view: &str) -> Vec<Value> {
    let (status, body) = call(served, contract, view, None);
    assert_eq!(status, 200, "{body}");
    body["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("{view} answers rows: {body}"))
        .clone()
}

#[test]
fn a_command_posted_over_http_executes_and_answers() {
    let contract = Contract::load();
    let served = serve();
    assert!(rows(&served, &contract, ALL_TASKS).is_empty());

    let (status, answer) = call(
        &served,
        &contract,
        OPEN_TASK,
        Some(&json!({"task_id": "t1", "title": "Water the plants"})),
    );
    assert_eq!(status, 202, "{answer}");
    assert_eq!(
        answer,
        json!({
            "outcome": "opened",
            "events": [{
                "event": "tasks.board.TaskOpened",
                "payload": {"task_id": "t1", "title": "Water the plants"},
            }],
            "invocations": [],
        })
    );
    assert_eq!(
        rows(&served, &contract, ALL_TASKS),
        [json!({"task_id": "t1", "state": "Open", "title": "Water the plants"})]
    );

    let (status, answer) = call(
        &served,
        &contract,
        CLOSE_TASK,
        Some(&json!({"task_id": "t1"})),
    );
    assert_eq!(status, 202, "{answer}");
    assert_eq!(answer["outcome"], "closed");
    assert_eq!(answer["events"][0]["event"], "tasks.board.TaskClosed");
    assert_eq!(
        answer["invocations"],
        json!([{
            "binding": "record-closure",
            "component": "ledger",
            "command": "tasks.ledger.RecordClosure",
            "outcome": "recorded",
            "events": [{
                "event": "tasks.ledger.ClosureRecorded",
                "payload": {"task_id": "t1"},
            }],
        }])
    );
    assert_eq!(
        rows(&served, &contract, ALL_TASKS),
        [json!({"task_id": "t1", "state": "Done", "title": "Water the plants"})]
    );
    assert!(rows(&served, &contract, OPEN_TASKS).is_empty());
}

#[test]
fn a_refused_command_answers_with_its_declared_error() {
    let contract = Contract::load();
    let served = serve();

    let (status, answer) = call(
        &served,
        &contract,
        CLOSE_TASK,
        Some(&json!({"task_id": "nobody"})),
    );
    assert_eq!(status, 404, "{answer}");
    assert_eq!(answer["outcome"], "no-such-task");
    assert_eq!(
        answer["error"],
        json!({"error": "tasks.board.TaskNotFound"})
    );
    assert_eq!(answer["events"], json!([]));
    assert!(rows(&served, &contract, ALL_TASKS).is_empty());

    call(
        &served,
        &contract,
        OPEN_TASK,
        Some(&json!({"task_id": "t1", "title": "Once"})),
    );
    call(
        &served,
        &contract,
        CLOSE_TASK,
        Some(&json!({"task_id": "t1"})),
    );
    let before = rows(&served, &contract, ALL_TASKS);
    let (status, answer) = call(
        &served,
        &contract,
        CLOSE_TASK,
        Some(&json!({"task_id": "t1"})),
    );
    assert_eq!(status, 409, "{answer}");
    assert_eq!(answer["outcome"], "already-done");
    assert_eq!(answer["error"], json!({"error": "tasks.board.AlreadyDone"}));
    assert_eq!(answer["events"], json!([]));
    assert_eq!(rows(&served, &contract, ALL_TASKS), before);
}

#[test]
fn a_view_is_answered_over_http() {
    let contract = Contract::load();
    let served = serve();
    for (id, title) in [("t3", "Third"), ("t1", "First"), ("t2", "Second")] {
        let (status, answer) = call(
            &served,
            &contract,
            OPEN_TASK,
            Some(&json!({"task_id": id, "title": title})),
        );
        assert_eq!(status, 202, "{answer}");
    }
    call(
        &served,
        &contract,
        CLOSE_TASK,
        Some(&json!({"task_id": "t2"})),
    );
    assert_eq!(
        rows(&served, &contract, ALL_TASKS),
        [
            json!({"task_id": "t1", "state": "Open", "title": "First"}),
            json!({"task_id": "t2", "state": "Done", "title": "Second"}),
            json!({"task_id": "t3", "state": "Open", "title": "Third"}),
        ]
    );
    assert_eq!(
        rows(&served, &contract, OPEN_TASKS),
        [
            json!({"task_id": "t1", "title": "First"}),
            json!({"task_id": "t3", "title": "Third"}),
        ]
    );
}

#[test]
fn the_listener_binds_loopback_only() {
    let contract = Contract::load();
    let served = serve();
    assert_eq!(
        served.address.ip(),
        std::net::IpAddr::from([127, 0, 0, 1]),
        "{}",
        served.ready
    );
    assert_ne!(served.address.port(), 0, "{}", served.ready);
    assert_eq!(served.ready["components"], json!([COMPONENT]));
    assert_eq!(rows(&served, &contract, ALL_TASKS), Vec::<Value>::new());
}

#[test]
fn an_ingress_binding_other_than_http_loopback_is_refused() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--environment", MICROVM, "--state"])
        .arg(state.path().join("state"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let started = Instant::now();
    let exited = loop {
        if let Some(status) = child.try_wait().expect("the child is waited on") {
            break status;
        }
        if started.elapsed() > EXIT {
            let _ = child.kill();
            let _ = child.wait();
            panic!("run did not exit within {EXIT:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let output = child.wait_with_output().expect("the output is read");
    assert!(!exited.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("listening"), "nothing listened: {stdout}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ingress.network") && stderr.contains("`http`"),
        "the refusal names the implementation it does not serve: {stderr}"
    );
    assert!(stderr.contains("http-loopback"), "{stderr}");
}

#[test]
fn an_answer_validates_against_its_projected_schema() {
    let contract = Contract::load();
    let served = serve();

    let (status, answer) = call(
        &served,
        &contract,
        OPEN_TASK,
        Some(&json!({"task_id": "t1", "title": "Checked"})),
    );
    contract.validate(OPEN_TASK, status, &answer);

    let (status, answer) = call(
        &served,
        &contract,
        CLOSE_TASK,
        Some(&json!({"task_id": "t1"})),
    );
    contract.validate(CLOSE_TASK, status, &answer);

    let (status, answer) = call(
        &served,
        &contract,
        CLOSE_TASK,
        Some(&json!({"task_id": "t1"})),
    );
    assert_eq!(status, 409, "{answer}");
    contract.validate(CLOSE_TASK, status, &answer);

    let (status, answer) = call(
        &served,
        &contract,
        CLOSE_TASK,
        Some(&json!({"task_id": "nobody"})),
    );
    assert_eq!(status, 404, "{answer}");
    contract.validate(CLOSE_TASK, status, &answer);

    for view in [ALL_TASKS, OPEN_TASKS] {
        let (status, answer) = call(&served, &contract, view, None);
        assert_eq!(status, 200, "{answer}");
        contract.validate(view, status, &answer);
    }
}

#[test]
fn a_bound_route_answers_with_the_bindings_status_per_outcome() {
    let contract = Contract::bound();
    assert_eq!(
        contract.route(OPEN_TASK),
        ("PUT".to_owned(), "/tasks/{task_id}".to_owned())
    );
    let served = serve_with(&["--http", BINDING]);
    let address = served.address;

    let (status, answer) = exchange(
        address,
        "PUT",
        "/tasks/t1",
        Some(&json!({"title": "Bound"})),
    );
    assert_eq!(status, 201, "the binding answers opened 201: {answer}");
    assert_eq!(answer["outcome"], "opened");
    assert_eq!(
        answer["events"],
        json!([{"event": "tasks.board.TaskOpened", "payload": {"task_id": "t1", "title": "Bound"}}])
    );
    contract.validate(OPEN_TASK, status, &answer);

    let (status, answer) = exchange(address, "POST", "/tasks/t1/close", None);
    assert_eq!(status, 200, "the binding answers closed 200: {answer}");
    assert_eq!(answer["outcome"], "closed");
    contract.validate(CLOSE_TASK, status, &answer);

    let (status, answer) = exchange(address, "POST", "/tasks/t1/close", None);
    assert_eq!(
        status, 410,
        "the binding answers already-done 410: {answer}"
    );
    assert_eq!(answer["error"], json!({"error": "tasks.board.AlreadyDone"}));
    contract.validate(CLOSE_TASK, status, &answer);

    let (status, answer) = exchange(address, "POST", "/tasks/nobody/close", None);
    assert_eq!(
        status, 404,
        "an outcome the binding does not list keeps ESS's: {answer}"
    );
    contract.validate(CLOSE_TASK, status, &answer);

    let (status, answer) = exchange(address, "GET", "/tasks", None);
    assert_eq!(status, 200, "{answer}");
    assert_eq!(
        answer,
        json!({"rows": [{"task_id": "t1", "state": "Done", "title": "Bound"}]})
    );
    contract.validate(ALL_TASKS, status, &answer);

    let (method, path) = contract.route(OPEN_TASKS);
    let (status, answer) = exchange(address, &method, &path, None);
    assert_eq!(
        status, 200,
        "a view the binding does not name keeps ESS's route: {answer}"
    );
    contract.validate(OPEN_TASKS, status, &answer);

    let (status, answer) = exchange(address, "POST", "/api/commands/tasks.board.OpenTask", None);
    assert_eq!(
        status, 404,
        "with a binding the fallback is not served: {answer}"
    );
}
