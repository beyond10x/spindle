//! The console routes a loopback `run` serves beside the routes ESS publishes: `GET /api/model`,
//! `GET /api/events` and `POST /api/reset`.
//!
//! The fixture `tests/fixtures/ingress` declares `board`, reached over the network, and `ledger`,
//! which only a binding reaches. The paths of commands and views are read from the `OpenAPI`
//! document ESS projects for `board`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use spindle_ingress::{Console, HTTP_LOOPBACK, Resolved, Surface};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/ingress";
const LAPTOP: &str = "examples/environments/laptop.toml";
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

/// The method and path ESS publishes for the operation `id` of the served component.
fn route(id: &str) -> (String, String) {
    let loaded = spindle_spec::load(&root().join(SPEC)).expect("the fixture compiles");
    let component = loaded
        .ir
        .components()
        .values()
        .find(|component| component.name.as_str() == COMPONENT)
        .expect("the fixture declares the component");
    let document: Value = serde_json::from_str(&ess_gen::openapi::json(&loaded.ir, component))
        .expect("the projection is JSON");
    for (path, item) in document["paths"].as_object().expect("paths") {
        for (method, operation) in item.as_object().expect("a path item") {
            if operation["operationId"] == id {
                return (method.to_uppercase(), path.clone());
            }
        }
    }
    panic!("the contract declares no operation {id}")
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

/// `run` on the fixture and laptop, its state in SQLite.
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
        .unwrap_or_else(|error| panic!("run's first line {line:?} is not JSON: {error}"));
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

/// One HTTP/1.1 exchange on its own connection: the status and the body as sent.
fn raw(address: SocketAddr, method: &str, path: &str, body: Option<&Value>) -> (u16, String) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
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
    assert!(
        head.to_ascii_lowercase()
            .contains("content-type: application/json"),
        "{head}"
    );
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {head:?}"));
    (status, body.to_owned())
}

fn exchange(address: SocketAddr, method: &str, path: &str, body: Option<&Value>) -> (u16, Value) {
    let (status, body) = raw(address, method, path, body);
    let body = serde_json::from_str(&body).unwrap_or_else(|error| panic!("{error}: {body:?}"));
    (status, body)
}

fn call(served: &Served, id: &str, body: Option<&Value>) -> (u16, Value) {
    let (method, path) = route(id);
    exchange(served.address, &method, &path, body)
}

fn rows(served: &Served, view: &str) -> Vec<Value> {
    let (status, body) = call(served, view, None);
    assert_eq!(status, 200, "{body}");
    body["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("{view} answers rows: {body}"))
        .clone()
}

fn events(served: &Served) -> Vec<Value> {
    let (status, body) = exchange(served.address, "GET", "/api/events", None);
    assert_eq!(status, 200, "{body}");
    body["events"]
        .as_array()
        .unwrap_or_else(|| panic!("the events route answers events: {body}"))
        .clone()
}

/// What `ess specify compile --format json` prints for the fixture.
fn compiled_by_ess() -> String {
    let output = Command::new("ess")
        .current_dir(root())
        .args(["specify", "compile", "--path", SPEC, "--format", "json"])
        .output()
        .expect("the ess CLI runs");
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).expect("the IR is UTF-8")
}

#[derive(serde::Deserialize)]
struct Model<'a> {
    #[serde(borrow)]
    ir: &'a serde_json::value::RawValue,
    subjects: Value,
}

#[test]
fn the_model_route_answers_the_compiled_ir() {
    let served = serve();
    let (status, body) = raw(served.address, "GET", "/api/model", None);
    assert_eq!(status, 200, "{body}");
    let model: Model<'_> = serde_json::from_str(&body).expect("the model answer is JSON");
    let expected = compiled_by_ess();
    assert_eq!(
        model.ir.get(),
        expected.trim_end_matches('\n'),
        "the IR is answered byte for byte as `ess specify compile --format json` prints it"
    );

    let ir: Value = serde_json::from_str(&expected).expect("the IR is JSON");
    let mut subjects = Vec::new();
    for (command, declared) in ir["commands"].as_object().expect("commands by name") {
        for outcome in declared["outcomes"].as_array().expect("outcomes") {
            subjects.push(json!({
                "command": command,
                "outcome": outcome["name"],
                "subject": outcome.get("subject").cloned().unwrap_or(Value::Null),
            }));
        }
    }
    assert_eq!(model.subjects, Value::Array(subjects));
    let subject = |command: &str, outcome: &str| {
        model
            .subjects
            .as_array()
            .expect("subjects")
            .iter()
            .find(|subject| subject["command"] == command && subject["outcome"] == outcome)
            .unwrap_or_else(|| panic!("no subject for {command} {outcome}"))["subject"]
            .clone()
    };
    assert_eq!(subject(OPEN_TASK, "opened")["entity"], "tasks.board.Task");
    assert_eq!(subject(OPEN_TASK, "opened")["effect"], "creates");
    assert_eq!(subject(CLOSE_TASK, "no-such-task"), Value::Null);
    assert_eq!(
        subject("tasks.ledger.RecordClosure", "recorded")["entity"],
        "tasks.ledger.Closure"
    );
}

#[test]
fn the_events_route_lists_published_events_in_order() {
    let served = serve();
    assert_eq!(events(&served), Vec::<Value>::new());
    for (task, title) in [("t2", "Second"), ("t1", "First")] {
        let (status, answer) = call(
            &served,
            OPEN_TASK,
            Some(&json!({"task_id": task, "title": title})),
        );
        assert_eq!(status, 202, "{answer}");
    }
    let (status, answer) = call(&served, CLOSE_TASK, Some(&json!({"task_id": "t2"})));
    assert_eq!(status, 202, "{answer}");
    let (status, answer) = call(&served, CLOSE_TASK, Some(&json!({"task_id": "t2"})));
    assert_eq!(status, 409, "a refusal publishes nothing: {answer}");
    assert_eq!(
        events(&served),
        [
            json!({"component": "board", "command": OPEN_TASK, "event": "tasks.board.TaskOpened",
                   "payload": {"task_id": "t2", "title": "Second"}}),
            json!({"component": "board", "command": OPEN_TASK, "event": "tasks.board.TaskOpened",
                   "payload": {"task_id": "t1", "title": "First"}}),
            json!({"component": "board", "command": CLOSE_TASK, "event": "tasks.board.TaskClosed",
                   "payload": {"task_id": "t2"}}),
            json!({"component": "ledger", "command": "tasks.ledger.RecordClosure",
                   "event": "tasks.ledger.ClosureRecorded", "payload": {"task_id": "t2"}}),
        ]
    );
}

#[test]
fn reset_empties_every_store_of_the_run() {
    let served = serve();
    for (task, title) in [("t1", "First"), ("t2", "Second")] {
        call(
            &served,
            OPEN_TASK,
            Some(&json!({"task_id": task, "title": title})),
        );
    }
    call(&served, CLOSE_TASK, Some(&json!({"task_id": "t1"})));
    assert_eq!(rows(&served, ALL_TASKS).len(), 2);
    assert_eq!(rows(&served, OPEN_TASKS).len(), 1);
    assert_eq!(events(&served).len(), 4);

    let (status, answer) = exchange(served.address, "POST", "/api/reset", None);
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer, json!({"emptied": ["board", "ledger"]}));
    for view in [ALL_TASKS, OPEN_TASKS] {
        assert_eq!(rows(&served, view), Vec::<Value>::new(), "{view}");
    }
    assert_eq!(events(&served), Vec::<Value>::new());

    let (status, answer) = call(
        &served,
        OPEN_TASK,
        Some(&json!({"task_id": "t1", "title": "Again"})),
    );
    assert_eq!(status, 202, "t1 is gone from board's store: {answer}");
    call(&served, CLOSE_TASK, Some(&json!({"task_id": "t1"})));
    assert_eq!(
        events(&served).last().map(|event| event["event"].clone()),
        Some(json!("tasks.ledger.ClosureRecorded")),
        "t1's closure is gone from ledger's store, which no view reads"
    );
}

#[test]
fn reset_is_refused_outside_loopback() {
    let loaded = spindle_spec::load(&root().join(SPEC)).expect("the fixture compiles");
    let components = [COMPONENT.to_owned()];
    let surface = Surface::new(&loaded.ir, &components, None).expect("the surface");

    let loopback = surface
        .clone()
        .with_console(HTTP_LOOPBACK)
        .expect("nothing ESS publishes is under /api");
    for (method, path, console) in [
        ("GET", "/api/model", Console::Model),
        ("GET", "/api/events", Console::Events),
        ("POST", "/api/reset", Console::Reset),
    ] {
        assert_eq!(
            loopback.resolve(method, path),
            Resolved::Console(console),
            "{method} {path}"
        );
    }
    assert!(matches!(
        loopback.resolve("GET", "/api/reset"),
        Resolved::WrongMethod(_)
    ));

    for elsewhere in ["http", "microvm-ingress"] {
        let surface = surface
            .clone()
            .with_console(elsewhere)
            .expect("no console, so nothing to collide");
        for (method, path) in [
            ("POST", "/api/reset"),
            ("GET", "/api/model"),
            ("GET", "/api/events"),
        ] {
            assert_eq!(
                surface.resolve(method, path),
                Resolved::NotFound,
                "{method} {path} on {elsewhere}"
            );
        }
    }

    let served = serve();
    let (status, answer) = exchange(served.address, "GET", "/api/reset", None);
    assert_eq!(status, 405, "{answer}");
}

/// `run` with the fixture's binding, its line `from` replaced by `to`.
fn refused_with_binding(from: &str, to: &str) -> String {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let text = std::fs::read_to_string(root().join(BINDING)).expect("the binding reads");
    let rewritten = text.replace(from, to);
    assert_ne!(rewritten, text, "the binding has the line {from:?}");
    let binding = state.path().join("binding.yaml");
    std::fs::write(&binding, rewritten).expect("the binding is written");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--environment", LAPTOP, "--state"])
        .arg(state.path().join("state"))
        .arg("--http")
        .arg(&binding)
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
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn a_console_route_the_binding_claims_is_refused_at_startup() {
    let stderr = refused_with_binding("    path: /tasks\n", "    path: /api/model\n");
    assert!(
        stderr.contains("GET /api/model") && stderr.contains(ALL_TASKS),
        "the refusal names the console route and what claims it: {stderr}"
    );
    let stderr = refused_with_binding("    path: /tasks/{task_id}\n", "    path: /api/{task_id}\n");
    assert!(
        stderr.contains("/api/{task_id}") && stderr.contains(OPEN_TASK),
        "a template that matches a console path is refused too: {stderr}"
    );
}
