//! `run --observe` serving HTTP: the request log at `GET /__spindle/requests`
//! (`story:ingress-request-log`), and the reserved prefix `/__spindle/`, with and without the flag.
//!
//! The scripted sequence runs on the fixture `tests/fixtures/ingress` with its `ess-http/1`
//! binding, and its log is compared with `ess/fixtures/observe/requests/scripted.json`, the
//! fixture the observe model's schema validates (`task check`), times and durations aside. The
//! fixture `tests/fixtures/reserved` has a view routed at `GET /{a}/metrics/{b}`, which a path
//! under the prefix would match, and a command a binding routes under the prefix. (`GET /{a}/{b}`
//! would match the console's `GET /api/model` too, which `http-loopback` refuses at start.)

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const INGRESS: &str = "crates/spindle-cli/tests/fixtures/ingress";
const INGRESS_HTTP: &str = "crates/spindle-cli/tests/fixtures/ingress-http.yaml";
const RESERVED: &str = "crates/spindle-cli/tests/fixtures/reserved";
const RESERVED_PREFIX_HTTP: &str = "crates/spindle-cli/tests/fixtures/reserved-prefix-http.yaml";
const RESERVED_VARIABLES_HTTP: &str =
    "crates/spindle-cli/tests/fixtures/reserved-variables-http.yaml";
const FIXED_CALLER: &str = "crates/spindle-cli/tests/fixtures/fixed-caller";
/// A fixture with no component reached over the network: `run` reads stdin and serves no HTTP.
const REPLAY: &str = "crates/spindle-cli/tests/fixtures/replay";
const SCRIPTED: &str = "ess/fixtures/observe/requests/scripted.json";
const SCHEMA: &str = "ess/generated/jsonschema/schema/types/spindle.observe.RequestLog.schema.json";
const LAPTOP: &str = "examples/environments/laptop.toml";

const READER: &str = "quire.notebook.Reader";
const CREATE: &str = "quire.notebook.CreateNotebook";
const SHARE: &str = "quire.notebook.ShareNotebook";
const CARD: &str = "quire.notebook.NotebookCard";
const OWNER: &str = "0f1e2d3c-4b5a-4968-8776-a5b4c3d2e1f0";
const OTHER: &str = "9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d";
const NOTEBOOK: &str = "5b2d8f14-3c6a-4e7b-9d1f-2a3b4c5d6e7f";

/// Every route of the reserved prefix the epic lays out.
const RESERVED_ROUTES: [&str; 4] = [
    "/__spindle/requests",
    "/__spindle/metrics",
    "/__spindle/ui",
    "/__spindle/topology",
];

/// How long `run` may take to admit, open its state and print where it listens.
const STARTUP: Duration = Duration::from_secs(60);
/// How long one request may take to be answered.
const ANSWER: Duration = Duration::from_secs(10);
/// How long a refused `run` may take to exit on its own.
const EXIT: Duration = Duration::from_secs(60);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
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

/// `run` on the fixture `spec` and laptop, with `arguments` added.
fn serve(spec: &str, arguments: &[&str]) -> Served {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", spec, "--environment", LAPTOP, "--state"])
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
        ready,
        _state: state,
    }
}

/// `run` on the fixture `spec` and laptop, with `arguments` added, expected to exit on its own:
/// its exit code, stdout and stderr.
fn refused(spec: &str, arguments: &[&str]) -> (Option<i32>, String, String) {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", spec, "--environment", LAPTOP, "--state"])
        .arg(state.path().join("state"))
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let deadline = Instant::now() + EXIT;
    loop {
        if child.try_wait().expect("the child is waited on").is_some() {
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("run did not exit within {EXIT:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let output = child.wait_with_output().expect("the output is read");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
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
    let body = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(body).unwrap_or_else(|error| panic!("{error}: {body:?}"))
    };
    (status, body)
}

/// The request log, `GET /__spindle/requests` with `query` (with its `?`, or empty).
fn log(served: &Served, query: &str) -> Value {
    let (status, body) = exchange(
        served.address,
        "GET",
        &format!("/__spindle/requests{query}"),
        None,
    );
    assert_eq!(status, 200, "the request log is answered: {body}");
    body
}

/// The method and path the served surface of `component` in `spec` puts `operation` at.
fn route(spec: &str, component: &str, operation: &str) -> (String, String) {
    let ir = spindle_spec::load(&root().join(spec))
        .expect("the fixture compiles")
        .ir;
    let surface = spindle_ingress::Surface::new(&ir, &[component.to_owned()], None)
        .expect("the fixture's surface is routed");
    let route = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == operation)
        .unwrap_or_else(|| panic!("a route for {operation}"));
    (route.method.as_str().to_uppercase(), route.path.clone())
}

/// Calls `operation` of the fixed-caller fixture's `desk` at its route.
fn call_desk(served: &Served, operation: &str, body: Option<&Value>) -> (u16, Value) {
    let (method, path) = route(FIXED_CALLER, "desk", operation);
    exchange(served.address, &method, &path, body)
}

/// The scripted sequence of `story:ingress-request-log` on the ingress fixture.
fn scripted(served: &Served) {
    let steps: [(&str, &str, Option<Value>, u16); 7] = [
        (
            "PUT",
            "/tasks/t1",
            Some(json!({"title": "Water the plants"})),
            201,
        ),
        ("POST", "/tasks/t1/close", None, 200),
        ("POST", "/tasks/t1/close", None, 410),
        ("GET", "/tasks", None, 200),
        (
            "PUT",
            "/tasks/t2",
            Some(json!({"title": "Sweep", "priority": 1})),
            400,
        ),
        ("GET", "/nowhere", None, 404),
        ("DELETE", "/tasks", None, 405),
    ];
    for (method, path, body, expected) in steps {
        let (status, answer) = exchange(served.address, method, path, body.as_ref());
        assert_eq!(status, expected, "{method} {path}: {answer}");
    }
}

/// `log` without what a run cannot reproduce: each record's `at` and `duration_us`.
fn timeless(mut log: Value) -> Value {
    for record in log["requests"].as_array_mut().expect("requests") {
        let record = record.as_object_mut().expect("a record is an object");
        assert!(
            record.remove("at").is_some_and(|at| at.is_string()),
            "{record:?}"
        );
        assert!(
            record
                .remove("duration_us")
                .is_some_and(|duration| duration.is_u64()),
            "{record:?}"
        );
    }
    log
}

#[test]
fn without_observe_the_reserved_routes_answer_404() {
    let served = serve(INGRESS, &["--http", INGRESS_HTTP]);
    for path in RESERVED_ROUTES {
        let (status, body) = exchange(served.address, "GET", path, None);
        assert_eq!(status, 404, "{path}: {body}");
        assert!(body["refused"].is_string(), "{path}: {body}");
    }
}

#[test]
fn the_listening_line_is_unchanged_by_observe() {
    let keys = |ready: &Value| {
        ready
            .as_object()
            .expect("the listening line is an object")
            .keys()
            .cloned()
            .collect::<Vec<_>>()
    };
    let plain = serve(INGRESS, &[]);
    let observed = serve(INGRESS, &["--observe"]);
    assert_eq!(keys(&plain.ready), keys(&observed.ready));
    assert_eq!(plain.ready["components"], observed.ready["components"]);
}

#[test]
fn a_specification_route_under_the_reserved_prefix_is_refused_at_start() {
    for observe in [false, true] {
        let mut arguments = vec!["--http", RESERVED_PREFIX_HTTP];
        if observe {
            arguments.push("--observe");
        }
        let (code, stdout, stderr) = refused(RESERVED, &arguments);
        assert_eq!(code, Some(1), "observe {observe}: {stdout}{stderr}");
        assert!(
            !stdout.contains("listening"),
            "refused before listening: {stdout}"
        );
        for named in [
            "notes.shelf.PinNote",
            "PUT",
            "/__spindle/x",
            "under /__spindle/",
        ] {
            assert!(
                stderr.contains(named),
                "observe {observe}: the refusal names {named:?}: {stderr}"
            );
        }
    }
}

#[test]
fn a_variable_only_route_never_answers_under_the_reserved_prefix() {
    let plain = serve(RESERVED, &["--http", RESERVED_VARIABLES_HTTP]);
    for path in [
        "/__spindle/metrics",
        "/__spindle/metrics/y",
        "/%5F%5Fspindle/metrics/y",
        "/__spindle/x",
    ] {
        let (status, body) = exchange(plain.address, "GET", path, None);
        assert_eq!(status, 404, "{path} reaches no view: {body}");
        assert!(body.get("rows").is_none(), "{path}: {body}");
    }
    let (status, body) = exchange(plain.address, "GET", "/x/metrics/y", None);
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, json!({"rows": []}));
    drop(plain);

    let observed = serve(RESERVED, &["--http", RESERVED_VARIABLES_HTTP, "--observe"]);
    let (status, body) = exchange(observed.address, "GET", "/x/metrics/y", None);
    assert_eq!(status, 200, "{body}");
    let (status, body) = exchange(observed.address, "GET", "/__spindle/metrics", None);
    assert_eq!(status, 404, "{body}");
    let answered = log(&observed, "");
    assert_eq!(answered["format"], "spindle-requests/1", "{answered}");
    assert_eq!(answered["recorded"], 1, "{answered}");
    assert_eq!(
        answered["requests"][0]["path"], "/x/metrics/y",
        "{answered}"
    );
    assert_eq!(
        answered["requests"][0]["route"], "/{a}/metrics/{b}",
        "{answered}"
    );
    assert_eq!(
        answered["requests"][0]["operation"], "notes.shelf.NotesOn",
        "{answered}"
    );
}

#[test]
fn the_request_log_lists_a_scripted_sequence_newest_first() {
    let served = serve(INGRESS, &["--http", INGRESS_HTTP, "--observe"]);
    scripted(&served);
    let expected: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join(SCRIPTED)).expect("the fixture is read"),
    )
    .expect("the fixture is JSON");
    assert_eq!(timeless(log(&served, "")), timeless(expected));
}

#[test]
fn a_request_log_class_filter_keeps_only_those_classes() {
    let served = serve(INGRESS, &["--http", INGRESS_HTTP, "--observe"]);
    scripted(&served);
    let unrouted = log(&served, "?class=unrouted");
    let statuses = unrouted["requests"]
        .as_array()
        .expect("requests")
        .iter()
        .map(|record| record["status"].clone())
        .collect::<Vec<_>>();
    assert_eq!(statuses, [json!(405), json!(404)], "{unrouted}");
    assert_eq!(unrouted["recorded"], 7, "the totals are not filtered");
    assert_eq!(unrouted["totals"]["served"], 4, "{unrouted}");
    let two = log(&served, "?class=refused&class=unrouted");
    assert_eq!(two["requests"].as_array().map(Vec::len), Some(3), "{two}");
    let (status, body) = exchange(
        served.address,
        "GET",
        "/__spindle/requests?class=nothing",
        None,
    );
    assert_eq!(status, 400, "an unknown class is refused: {body}");
    assert!(
        body["refused"]
            .as_str()
            .is_some_and(|why| why.contains("nothing")),
        "{body}"
    );
}

#[test]
fn the_request_log_keeps_the_refusal_text() {
    let caller = json!({"owner_id": OWNER}).to_string();
    let reader = serve(
        FIXED_CALLER,
        &[
            "--fixed-actor",
            READER,
            "--fixed-caller",
            &caller,
            "--observe",
        ],
    );
    let (bad, bad_body) = call_desk(
        &reader,
        CREATE,
        Some(&json!({"notebook_id": NOTEBOOK, "title": "notes", "colour": "red"})),
    );
    assert_eq!(bad, 400, "{bad_body}");
    let (denied, denied_body) = call_desk(
        &reader,
        CREATE,
        Some(&json!({"notebook_id": NOTEBOOK, "title": "notes"})),
    );
    assert_eq!(denied, 403, "{denied_body}");
    let answered = log(&reader, "");
    let records = answered["requests"].as_array().expect("requests");
    assert_eq!(records.len(), 2, "{answered}");
    assert_eq!(records[0]["class"], "not-granted", "{answered}");
    assert_eq!(records[0]["reason"], denied_body["refused"], "{answered}");
    assert_eq!(records[1]["class"], "refused", "{answered}");
    assert_eq!(records[1]["reason"], bad_body["refused"], "{answered}");
    drop(reader);

    let plain = serve(FIXED_CALLER, &["--observe"]);
    let (unserved, unserved_body) = call_desk(
        &plain,
        SHARE,
        Some(&json!({"notebook_id": NOTEBOOK, "with": OTHER})),
    );
    assert_eq!(unserved, 501, "{unserved_body}");
    let answered = log(&plain, "");
    assert_eq!(answered["requests"][0]["class"], "not-served", "{answered}");
    assert_eq!(
        answered["requests"][0]["reason"], unserved_body["refused"],
        "{answered}"
    );
}

#[test]
fn the_not_served_filter_lists_only_501_answers() {
    let served = serve(FIXED_CALLER, &["--observe"]);
    let (status, body) = call_desk(
        &served,
        SHARE,
        Some(&json!({"notebook_id": NOTEBOOK, "with": OTHER})),
    );
    assert_eq!(status, 501, "{body}");
    let (status, body) = call_desk(&served, CARD, None);
    assert_eq!(status, 200, "{body}");
    let (status, body) = exchange(served.address, "GET", "/nowhere", None);
    assert_eq!(status, 404, "{body}");
    let (status, body) = call_desk(
        &served,
        SHARE,
        Some(&json!({"notebook_id": NOTEBOOK, "with": OTHER})),
    );
    assert_eq!(status, 501, "{body}");

    let filtered = log(&served, "?class=not-served");
    let records = filtered["requests"].as_array().expect("requests");
    assert_eq!(records.len(), 2, "{filtered}");
    for record in records {
        assert_eq!(record["status"], 501, "{filtered}");
        assert_eq!(record["class"], "not-served", "{filtered}");
    }
    assert_eq!(filtered["recorded"], 4, "{filtered}");
    assert_eq!(filtered["totals"]["not_served"], 2, "{filtered}");
}

#[test]
fn the_ring_drops_the_oldest_beyond_its_capacity() {
    let served = serve(INGRESS, &["--observe", "--observe-requests", "2"]);
    for _ in 0..3 {
        let (status, body) = exchange(served.address, "GET", "/nowhere", None);
        assert_eq!(status, 404, "{body}");
    }
    let answered = log(&served, "");
    assert_eq!(answered["capacity"], 2, "{answered}");
    assert_eq!(answered["recorded"], 3, "{answered}");
    let sequences = answered["requests"]
        .as_array()
        .expect("requests")
        .iter()
        .map(|record| record["sequence"].clone())
        .collect::<Vec<_>>();
    assert_eq!(sequences, [json!(2), json!(1)], "{answered}");
    assert_eq!(answered["totals"]["unrouted"], 3, "{answered}");
}

#[test]
fn the_ring_capacity_is_bounded() {
    for capacity in ["0", "10001"] {
        let (code, stdout, stderr) =
            refused(INGRESS, &["--observe", "--observe-requests", capacity]);
        assert_eq!(code, Some(2), "{capacity}: {stdout}{stderr}");
    }
}

#[test]
fn reserved_requests_are_neither_recorded_nor_counted() {
    let served = serve(INGRESS, &["--observe"]);
    for path in RESERVED_ROUTES {
        exchange(served.address, "GET", path, None);
    }
    exchange(served.address, "GET", "/__spindle/elsewhere", None);
    let (status, body) = exchange(served.address, "POST", "/__spindle/requests", None);
    assert_eq!(status, 405, "{body}");
    let answered = log(&served, "");
    assert_eq!(answered["recorded"], 0, "{answered}");
    assert_eq!(answered["requests"], json!([]), "{answered}");
    assert_eq!(
        answered["totals"],
        json!({
            "served": 0,
            "refused": 0,
            "not_granted": 0,
            "not_served": 0,
            "failed": 0,
            "unrouted": 0,
        }),
        "{answered}"
    );
}

#[test]
fn the_request_log_reads_as_its_specified_type() {
    let served = serve(INGRESS, &["--http", INGRESS_HTTP, "--observe"]);
    scripted(&served);
    let answered = log(&served, "");
    let typed: spindle_observe_api::RequestLog = serde_json::from_value(answered.clone())
        .unwrap_or_else(|error| panic!("{error}: {answered}"));
    assert_eq!(typed.requests.len(), 7);
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join(SCHEMA)).expect("the schema is read"),
    )
    .expect("the schema is JSON");
    let validator = jsonschema::draft202012::new(&schema).expect("the schema compiles");
    assert!(
        !validator.is_valid(&json!({"format": "spindle-requests/1"})),
        "the schema refuses a log without its fields, so it checks something"
    );
    let errors = validator
        .iter_errors(&answered)
        .map(|error| format!("{error} at {}", error.instance_path()))
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "{answered}: {errors:?}");
}

#[test]
fn observe_where_run_serves_no_http_is_refused() {
    let (code, stdout, stderr) = refused(REPLAY, &["--observe"]);
    assert_eq!(code, Some(1), "{stdout}{stderr}");
    assert!(stderr.contains("--observe"), "{stderr}");
}
