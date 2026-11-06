//! Adversary cases for `run` serving `reached_by: network` components over HTTP.
//!
//! Each case states what ESS publishes or does for the same request (the `OpenAPI` document
//! `ess_gen::openapi` projects, or the server ESS synthesises) and asserts that `run` does it too.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/ingress";
const LAPTOP: &str = "examples/environments/laptop.toml";
const BINDING: &str = "crates/spindle-cli/tests/fixtures/ingress-http.yaml";
const COMPONENT: &str = "board";

const OPEN_TASK: &str = "tasks.board.OpenTask";
const CLOSE_TASK: &str = "tasks.board.CloseTask";
const OPEN_TASKS: &str = "tasks.board.OpenTasks";

const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

struct Contract(Value);

impl Contract {
    fn component(ir: &spindle_spec::EssIr) -> &ess_compiler::ir::ResolvedComponent {
        ir.components()
            .values()
            .find(|component| component.name.as_str() == COMPONENT)
            .expect("the fixture declares the component")
    }

    fn load() -> Self {
        let loaded = spindle_spec::load(&root().join(SPEC)).expect("the fixture compiles");
        let document = ess_gen::openapi::json(&loaded.ir, Self::component(&loaded.ir));
        Self(serde_json::from_str(&document).expect("the projection is JSON"))
    }

    fn bound() -> Self {
        let loaded = spindle_spec::load(&root().join(SPEC)).expect("the fixture compiles");
        let text = std::fs::read_to_string(root().join(BINDING)).expect("the binding is read");
        let spec = ess_http::HttpSpec::from_yaml(&text).expect("the binding parses");
        let http = ess_http::compile(&spec, &loaded.ir, &ess_gen::http::derived(&loaded.ir))
            .unwrap_or_else(|refused| panic!("ESS refuses the binding: {refused}"));
        let binding = ess_gen::http::Binding::new(http)
            .unwrap_or_else(|refused| panic!("the binding cannot be served: {refused}"));
        let document =
            ess_gen::openapi::bound_json(&loaded.ir, Self::component(&loaded.ir), Some(&binding));
        Self(serde_json::from_str(&document).expect("the projection is JSON"))
    }

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

    fn statuses(&self, id: &str) -> Vec<u16> {
        let (method, path) = self.route(id);
        self.0["paths"][&path][method.to_lowercase()]["responses"]
            .as_object()
            .expect("the operation declares responses")
            .keys()
            .map(|status| status.parse().expect("a numeric status"))
            .collect()
    }

    /// The answer exactly as served, validated against the schema the contract publishes for
    /// `id` under `status`: no projection of the body first.
    fn refuses(&self, id: &str, status: u16, body: &Value) -> Vec<String> {
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
        validator
            .iter_errors(body)
            .map(|error| format!("{} at {}", error, error.instance_path()))
            .collect()
    }
}

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

fn serve_with(arguments: &[&str], stderr: Stdio) -> Served {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--environment", LAPTOP, "--state"])
        .arg(state.path().join("state"))
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(stderr)
        .spawn()
        .expect("b10x-spindle starts");
    let stdout = child.stdout.take().expect("stdout is piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = sender.send(line);
    });
    let Ok(line) = receiver.recv_timeout(STARTUP) else {
        let _ = child.kill();
        let _ = child.wait();
        panic!("run printed no line within {STARTUP:?}");
    };
    let ready: Value = serde_json::from_str(&line).unwrap_or_else(|error| {
        let _ = child.kill();
        let _ = child.wait();
        panic!("run's first line {line:?} is not JSON: {error}")
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

fn serve() -> Served {
    serve_with(&[], Stdio::inherit())
}

fn serve_bound() -> Served {
    serve_with(&["--http", BINDING], Stdio::inherit())
}

/// Writes `request` verbatim on a new connection and reads the whole answer: its status and its
/// body as text. `Err` when nothing is answered within [`ANSWER`].
fn raw(address: SocketAddr, request: &[u8]) -> Result<(u16, String), String> {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER)
        .map_err(|error| format!("connect: {error}"))?;
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    stream
        .set_write_timeout(Some(ANSWER))
        .expect("a write timeout");
    let _ = stream.write_all(request);
    let mut answer = Vec::new();
    stream
        .read_to_end(&mut answer)
        .map_err(|error| format!("no answer within {ANSWER:?}: {error}"))?;
    let answer = String::from_utf8_lossy(&answer).into_owned();
    let (head, body) = answer
        .split_once("\r\n\r\n")
        .ok_or_else(|| format!("an answer without a head: {answer:?}"))?;
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .ok_or_else(|| format!("no status line: {head:?}"))?;
    Ok((status, body.to_owned()))
}

fn request(address: SocketAddr, method: &str, path: &str, body: &str) -> Vec<u8> {
    format!(
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

fn exchange(address: SocketAddr, method: &str, path: &str, body: Option<&Value>) -> (u16, Value) {
    let body = body.map(Value::to_string).unwrap_or_default();
    let (status, text) = raw(address, &request(address, method, path, &body))
        .unwrap_or_else(|error| panic!("{method} {path}: {error}"));
    let value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{error}: {text:?}"));
    (status, value)
}

/// Where `ess generate --kind openapi` publishes `id` without a binding: with no `/api/`
/// fallback, that is where `run` serves it. `command` is kept from the fallback this replaced.
fn fallback(id: &str, command: bool) -> String {
    let (method, path) = Contract::load().route(id);
    assert_eq!(
        method == "POST",
        command,
        "{id} is published under {method}"
    );
    path
}

fn all_tasks(address: SocketAddr, path: &str) -> Value {
    let (status, body) = exchange(address, "GET", path, None);
    assert_eq!(status, 200, "{body}");
    body["rows"].clone()
}

/// The story's acceptance `an_answer_validates_against_its_projected_schema`, applied to the body
/// `run` writes rather than to a reshaped copy of it: ESS publishes `{outcome, published}` with
/// `additionalProperties: false` for `opened` under 202.
#[test]
fn a_served_command_answer_is_the_body_its_published_schema_declares() {
    let contract = Contract::load();
    let served = serve();
    let (status, answer) = exchange(
        served.address,
        "POST",
        &fallback(OPEN_TASK, true),
        Some(&json!({"task_id": "t1", "title": "Checked"})),
    );
    assert_eq!(status, 202, "{answer}");
    let refused = contract.refuses(OPEN_TASK, status, &answer);
    assert!(
        refused.is_empty(),
        "{OPEN_TASK} answered {status} with {answer}, which the schema ESS publishes refuses: {refused:?}"
    );
}

/// Under the binding, ESS publishes `already-done` at 410 as `{outcome, published, error}` with
/// `error` the qualified error name as a string.
#[test]
fn a_bound_refusal_is_the_body_its_published_schema_declares() {
    let contract = Contract::bound();
    let served = serve_bound();
    let address = served.address;
    let (status, _) = exchange(address, "PUT", "/tasks/t1", Some(&json!({"title": "x"})));
    assert_eq!(status, 201);
    let (status, _) = exchange(address, "POST", "/tasks/t1/close", None);
    assert_eq!(status, 200);
    let (status, answer) = exchange(address, "POST", "/tasks/t1/close", None);
    assert_eq!(status, 410, "{answer}");
    let refused = contract.refuses(CLOSE_TASK, status, &answer);
    assert!(
        refused.is_empty(),
        "{CLOSE_TASK} answered {status} with {answer}, which the schema ESS publishes refuses: {refused:?}"
    );
}

/// Without `--http`, `ess generate --kind openapi` publishes each operation at ESS's derived
/// route (`/board/views/OpenTasks`); with a binding that names other operations, `run` serves
/// exactly that route for this view. Without one it does not.
#[test]
fn without_a_binding_the_routes_ess_publishes_are_served() {
    let contract = Contract::load();
    let (method, path) = contract.route(OPEN_TASKS);
    let served = serve();
    let (status, answer) = exchange(served.address, &method, &path, None);
    assert_eq!(
        status, 200,
        "{method} {path}, which ESS publishes for {OPEN_TASKS}, answered {answer}"
    );
}

/// ESS's served application reads `%XX` as two hexadecimal digits and refuses anything else
/// (`percent_decoded`, `to_digit(16)`); `%+1` is not an escape.
#[test]
fn a_broken_percent_escape_in_a_path_variable_is_refused() {
    let served = serve_bound();
    let address = served.address;
    let (status, answer) = exchange(address, "PUT", "/tasks/%+1", Some(&json!({"title": "x"})));
    let rows = all_tasks(address, "/tasks");
    assert_eq!(
        (status, rows.clone()),
        (400, json!([])),
        "PUT /tasks/%+1 answered {status} {answer}, and the store now holds {rows}"
    );
}

/// The input schema ESS publishes is closed (`additionalProperties: false`), and ESS's server
/// refuses a field its input does not declare with 400.
#[test]
fn an_input_field_the_command_does_not_declare_is_refused() {
    let served = serve();
    let address = served.address;
    let (status, answer) = exchange(
        address,
        "POST",
        &fallback(OPEN_TASK, true),
        Some(&json!({"task_id": "t1", "title": "x", "owner": "someone else"})),
    );
    let rows = all_tasks(address, &fallback("tasks.board.AllTasks", false));
    assert_eq!(
        (status, rows.clone()),
        (400, json!([])),
        "an undeclared field answered {status} {answer}, and the store holds {rows}"
    );
}

/// HTTP methods are case-sensitive (RFC 9110 §9.1), and ESS's served application compares the
/// method verbatim (`request.method != "PUT"` answers 405).
#[test]
fn a_method_is_matched_exactly() {
    let served = serve_bound();
    let address = served.address;
    let (status, text) = raw(
        address,
        &request(
            address,
            "put",
            "/tasks/t1",
            &json!({"title": "x"}).to_string(),
        ),
    )
    .expect("an answer");
    let rows = all_tasks(address, "/tasks");
    assert_eq!(
        (status, rows.clone()),
        (405, json!([])),
        "`put` answered {status} {text}, and the store holds {rows}"
    );
}

/// A second `OpenTask` for an identity that exists answers a status the contract declares for
/// `OpenTask`, and if that is the `501` its `committed` is `false` and nothing moved.
#[test]
fn opening_an_existing_task_answers_a_declared_status() {
    let contract = Contract::load();
    let declared = contract.statuses(OPEN_TASK);
    let served = serve();
    let address = served.address;
    let path = fallback(OPEN_TASK, true);
    let (status, _) = exchange(
        address,
        "POST",
        &path,
        Some(&json!({"task_id": "t1", "title": "first"})),
    );
    assert_eq!(status, 202);
    let (status, answer) = exchange(
        address,
        "POST",
        &path,
        Some(&json!({"task_id": "t1", "title": "second"})),
    );
    assert!(
        declared.contains(&status),
        "a second open answered {status} {answer}; the contract declares {declared:?}"
    );
    if status == 501 {
        assert_eq!(answer["committed"], json!(false), "{answer}");
    }
    assert_eq!(
        all_tasks(address, &fallback("tasks.board.AllTasks", false)),
        json!([{"task_id": "t1", "state": "Open", "title": "first"}])
    );
}

/// A body that cannot be read for a reason other than its size (a broken chunk) is a malformed
/// request (400), not one too large (413).
#[test]
fn a_malformed_chunked_body_is_not_answered_as_too_large() {
    let served = serve();
    let address = served.address;
    let head = format!(
        "POST {} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n{{}}\r\n0\r\n\r\n",
        fallback(OPEN_TASK, true)
    );
    let (status, body) = raw(address, head.as_bytes()).expect("an answer");
    assert_ne!(
        status, 413,
        "a broken chunk was answered as a body over the size limit: {body}"
    );
}

/// The listener keeps serving after accepting a connection failed once: a run of connections that
/// exhausts the process's descriptors is answered again once they close.
#[test]
fn the_listener_survives_running_out_of_descriptors() {
    use rustix::process::{Pid, Resource, Rlimit, getrlimit, prlimit};

    let served = serve_with(&[], Stdio::piped());
    let address = served.address;
    let pid = Pid::from_child(&served.child);
    let open = std::fs::read_dir(format!("/proc/{}/fd", served.child.id()))
        .expect("the child's descriptors are listed")
        .count();
    let limit = u64::try_from(open + 4).expect("a small number");
    prlimit(
        Some(pid),
        Resource::Nofile,
        Rlimit {
            current: Some(limit),
            maximum: getrlimit(Resource::Nofile).maximum,
        },
    )
    .expect("the child's descriptor limit is lowered");
    let mut held = Vec::new();
    for _ in 0..32 {
        if let Ok(stream) = TcpStream::connect_timeout(&address, ANSWER) {
            held.push(stream);
        }
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    drop(held);
    prlimit(
        Some(pid),
        Resource::Nofile,
        Rlimit {
            current: Some(4096),
            maximum: getrlimit(Resource::Nofile).maximum,
        },
    )
    .expect("the child's descriptor limit is restored");
    let answered = raw(
        address,
        &request(address, "GET", &fallback("tasks.board.AllTasks", false), ""),
    );
    let mut served = served;
    let _ = served.child.kill();
    let mut stderr = String::new();
    if let Some(mut pipe) = served.child.stderr.take() {
        let _ = pipe.read_to_string(&mut stderr);
    }
    let answered = answered.unwrap_or_else(|error| {
        panic!("after the descriptors were freed the listener answered nothing ({error}); stderr: {stderr}")
    });
    assert_eq!(answered.0, 200, "{answered:?}; stderr: {stderr}");
}

/// SIGTERM while a request's body is still arriving: `run` exits 143 promptly.
#[test]
fn sigterm_during_a_slow_request_exits_promptly() {
    let mut served = serve();
    let address = served.address;
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("connect");
    write!(
        stream,
        "POST {} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: 100\r\n\r\n{{",
        fallback(OPEN_TASK, true)
    )
    .expect("a partial request");
    std::thread::sleep(Duration::from_millis(300));
    rustix::process::kill_process(
        rustix::process::Pid::from_child(&served.child),
        rustix::process::Signal::TERM,
    )
    .expect("SIGTERM is sent");
    let deadline = Instant::now() + ANSWER;
    let status = loop {
        if let Some(status) = served.child.try_wait().expect("waited on") {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "run did not exit within {ANSWER:?} of SIGTERM"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(status.code(), Some(143), "{status:?}");
    drop(stream);
}

/// Concurrent commands each land once.
#[test]
fn concurrent_commands_each_land_once() {
    let served = serve();
    let address = served.address;
    let threads: Vec<_> = (0..16)
        .map(|index| {
            std::thread::spawn(move || {
                exchange(
                    address,
                    "POST",
                    &fallback(OPEN_TASK, true),
                    Some(&json!({"task_id": format!("t{index:02}"), "title": "c"})),
                )
                .0
            })
        })
        .collect();
    for thread in threads {
        assert_eq!(thread.join().expect("a thread"), 202);
    }
    let rows = all_tasks(address, &fallback("tasks.board.AllTasks", false));
    assert_eq!(rows.as_array().map(Vec::len), Some(16), "{rows}");
}

/// A body announced over the limit is refused 413 without waiting for it.
#[test]
fn an_announced_oversized_body_is_refused_at_once() {
    let served = serve();
    let address = served.address;
    let head = format!(
        "POST {} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        fallback(OPEN_TASK, true),
        2 << 20
    );
    let (status, body) = raw(address, head.as_bytes()).expect("an answer");
    assert_eq!(status, 413, "{body}");
}

/// A body sent over the limit is refused 413, and nothing is opened.
#[test]
fn a_sent_oversized_body_is_refused() {
    let served = serve();
    let address = served.address;
    let filler = "x".repeat(2 << 20);
    let body = format!("{{\"task_id\":\"big\",\"title\":\"{filler}\"}}");
    let answered = raw(
        address,
        &request(address, "POST", &fallback(OPEN_TASK, true), &body),
    );
    let rows = all_tasks(address, &fallback("tasks.board.AllTasks", false));
    let (status, text) = answered.expect("an answer");
    assert_eq!((status, rows), (413, json!([])), "{text}");
}
