//! `run --observe` serving HTTP: the request log at `GET /__spindle/requests`
//! (`story:ingress-request-log`), the metrics at `GET /__spindle/metrics`
//! (`story:ingress-prometheus-metrics`), read with a Prometheus text parser and held to
//! `ess/fixtures/observe/metrics/families.json`, and the reserved prefix `/__spindle/`, with and
//! without the flag.
//!
//! The scripted sequence runs on the fixture `tests/fixtures/ingress` with its `ess-http/1`
//! binding, and its log is compared with `ess/fixtures/observe/requests/scripted.json`, the
//! fixture the observe model's schema validates (`task check`), times and durations aside. The
//! fixture `tests/fixtures/reserved` has a view routed at `GET /{a}/metrics/{b}`, which a path
//! under the prefix would match, and a command a binding routes under the prefix. (`GET /{a}/{b}`
//! would match the console's `GET /api/model` too, which `http-loopback` refuses at start.)

use std::collections::BTreeMap;
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
    // Under `--observe` the prefix answers the metrics (`story:ingress-prometheus-metrics`),
    // never the view.
    let (status, media, body) = text(observed.address, "GET", "/__spindle/metrics");
    assert_eq!(status, 200, "{body}");
    assert_eq!(media.as_deref(), Some(METRICS_MEDIA), "{body}");
    assert!(
        body.starts_with("# HELP ") && serde_json::from_str::<Value>(&body).is_err(),
        "{body}"
    );
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
        // `/__spindle/ui` answers an HTML page under `--observe`: read without parsing it as JSON.
        crate::observe_panel::get(served.address, path);
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

/// `spindle-channel`'s library, with `lending` reached over the network: `CloseLoan` publishes
/// `LoanClosed`, which `thank-on-close` maps into `SendThanks` across a declared conversion.
const LIBRARY: &str = "crates/spindle-ingress/tests/fixtures/library";

/// `story:binding-delivery-specified`: `member-0042` is no `ReaderRef`, so `thank-on-close`'s
/// input is unmet and `SendThanks` is not invoked. The request that closed the loan lists it under
/// `unmet`; one whose binding mapped lists none. The log reads as its specified type and the
/// schema admits it.
#[test]
fn an_unmet_binding_input_is_listed_in_the_served_request() {
    let served = serve(LIBRARY, &["--observe"]);
    let (method, path) = route(LIBRARY, "lending", "library.lending.CloseLoan");
    for (loan, member) in [("loan-1", "m-7"), ("loan-2", "member-0042")] {
        let body = json!({ "loan_id": loan, "member_id": member });
        let (status, answer) = exchange(served.address, &method, &path, Some(&body));
        assert!((200..300).contains(&status), "{status} {answer}");
    }
    let answered = log(&served, "");
    let typed: spindle_observe_api::RequestLog = serde_json::from_value(answered.clone())
        .unwrap_or_else(|error| panic!("{error}: {answered}"));
    assert_eq!(typed.requests.len(), 2, "{answered}");
    assert!(typed.requests[1].unmet.is_empty(), "{answered}");
    assert_eq!(typed.requests[0].unmet.len(), 1, "{answered}");
    assert_eq!(
        answered["requests"][0]["unmet"][0]["binding"], "thank-on-close",
        "{answered}"
    );
    assert_eq!(
        answered["requests"][0]["unmet"][0]["component"], "notices",
        "{answered}"
    );
    assert_eq!(
        answered["requests"][0]["unmet"][0]["command"], "library.notices.SendThanks",
        "{answered}"
    );
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join(SCHEMA)).expect("the schema is read"),
    )
    .expect("the schema is JSON");
    let validator = jsonschema::draft202012::new(&schema).expect("the schema compiles");
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

// Adversary cases for `story:ingress-request-log`.

/// A binding routing the reserved fixture's view at `GET /{a}/{b}/x`: a route led by variables.
const RESERVED_LEADING_HTTP: &str = "crates/spindle-cli/tests/fixtures/reserved-leading-http.yaml";

/// A path variable that does not decode is refused `400` after the path matched a route; the
/// model keeps `route` absent only for an unrouted request, so the record names the route and the
/// command it was bound to, as the scripted `400` does.
#[test]
fn a_malformed_path_variable_is_recorded_with_its_route() {
    let served = serve(INGRESS, &["--http", INGRESS_HTTP, "--observe"]);
    let (status, body) = exchange(
        served.address,
        "PUT",
        "/tasks/%FF",
        Some(&json!({"title": "Water the plants"})),
    );
    assert_eq!(status, 400, "{body}");
    let answered = log(&served, "");
    let record = &answered["requests"][0];
    assert_eq!(record["class"], "refused", "{answered}");
    assert_eq!(record["reason"], body["refused"], "{answered}");
    assert_eq!(record["route"], "/tasks/{task_id}", "{answered}");
    assert_eq!(record["operation"], "tasks.board.OpenTask", "{answered}");
    assert_eq!(record["kind"], "command", "{answered}");
}

/// `//__spindle/x` is not matched as reserved (its first segment is empty), yet the route
/// `/{a}/{b}/x` matches it carrying `a = "__spindle"`: the reserved segment reaches a
/// specification route through a variable.
#[test]
fn a_double_slash_never_carries_the_reserved_segment_into_a_route() {
    let plain = serve(RESERVED, &["--http", RESERVED_LEADING_HTTP]);
    let (status, body) = exchange(plain.address, "GET", "/y/z/x", None);
    assert_eq!(status, 200, "the route answers an ordinary path: {body}");
    let (status, body) = exchange(plain.address, "GET", "//__spindle/x", None);
    assert_eq!(status, 404, "//__spindle/x reaches no view: {body}");
    assert!(body.get("rows").is_none(), "{body}");
}

// The metrics a served run exposes at `GET /__spindle/metrics` (`story:ingress-prometheus-metrics`).

/// The metric families the observe model specifies, in order.
const FAMILIES: &str = "ess/fixtures/observe/metrics/families.json";
/// The fixture whose binding retries (`on_failure: retry`).
const RETRIED: &str = "crates/spindle-cli/tests/fixtures/retried";
/// The rally example: event bindings between three components and a periodic binding.
const RALLY: &str = "examples/rally/ess";
/// The media type of the Prometheus text exposition.
const METRICS_MEDIA: &str = "text/plain; version=0.0.4";

/// `method path` on its own connection, sent without a body: the status, the `Content-Type` and
/// the body as text.
fn text(address: SocketAddr, method: &str, path: &str) -> (u16, Option<String>, String) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
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
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {head:?}"));
    let media = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-type"))
        .map(|(_, value)| value.trim().to_owned());
    (status, media, body.to_owned())
}

/// `GET /__spindle/metrics`, answered `200` as the Prometheus text exposition.
fn scrape(served: &Served) -> String {
    let (status, media, body) = text(served.address, "GET", "/__spindle/metrics");
    assert_eq!(status, 200, "the metrics are answered: {body}");
    assert_eq!(media.as_deref(), Some(METRICS_MEDIA), "{body}");
    body
}

/// The specified families: name, kind and label names, in order.
fn families() -> Vec<(String, String, Vec<String>)> {
    let families: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join(FAMILIES)).expect("the families are read"),
    )
    .expect("the families are JSON");
    families["families"]
        .as_array()
        .expect("families")
        .iter()
        .map(|family| {
            let labels = family["labels"]
                .as_array()
                .expect("labels")
                .iter()
                .map(|label| label.as_str().expect("a label name").to_owned())
                .collect();
            (
                family["name"].as_str().expect("a name").to_owned(),
                family["kind"].as_str().expect("a kind").to_owned(),
                labels,
            )
        })
        .collect()
}

/// `name{label="value",...}` with the labels in the order given; `name` without any.
fn series(name: &str, labels: &[(&str, &str)]) -> String {
    if labels.is_empty() {
        return name.to_owned();
    }
    let labels = labels
        .iter()
        .map(|(label, value)| format!("{label}=\"{value}\""))
        .collect::<Vec<_>>()
        .join(",");
    format!("{name}{{{labels}}}")
}

/// Every sample of `exposition` as a Prometheus text parser reads it, by its series: a counter or
/// gauge by its name, a histogram as its `_bucket` (with `le`), `_count` and `_sum` series, each
/// with its labels in the order its specified family lists them (the parser hands them back
/// sorted by name).
fn samples(exposition: &str) -> BTreeMap<String, f64> {
    use openmetrics_parser::PrometheusValue;
    let parsed = openmetrics_parser::prometheus::parse_prometheus(exposition)
        .unwrap_or_else(|error| panic!("{error:?}: {exposition}"));
    let declared = families();
    let mut samples = BTreeMap::new();
    for (name, family) in &parsed.families {
        let order = declared
            .iter()
            .find(|(declared, ..)| declared == name)
            .map(|(_, _, labels)| labels.clone())
            .unwrap_or_else(|| panic!("{name} is not a specified family"));
        for sample in family.iter_samples() {
            let labelset = sample.get_labelset().expect("a sample of a family");
            let mut labels = labelset
                .iter()
                .map(|(label, value)| (label.as_str(), value.as_str()))
                .collect::<Vec<_>>();
            labels.sort_by_key(|(label, _)| order.iter().position(|named| named == label));
            match &sample.value {
                PrometheusValue::Counter(counter) => {
                    samples.insert(series(name, &labels), counter.value.as_f64());
                }
                PrometheusValue::Gauge(value) | PrometheusValue::Unknown(value) => {
                    samples.insert(series(name, &labels), value.as_f64());
                }
                PrometheusValue::Histogram(histogram) => {
                    for bucket in &histogram.buckets {
                        let bound = if bucket.upper_bound.is_infinite() {
                            "+Inf".to_owned()
                        } else {
                            bucket.upper_bound.to_string()
                        };
                        let mut labels = labels.clone();
                        labels.push(("le", &bound));
                        samples.insert(
                            series(&format!("{name}_bucket"), &labels),
                            bucket.count.as_f64(),
                        );
                    }
                    if let Some(count) = histogram.count {
                        samples.insert(series(&format!("{name}_count"), &labels), count as f64);
                    }
                    if let Some(sum) = &histogram.sum {
                        samples.insert(series(&format!("{name}_sum"), &labels), sum.as_f64());
                    }
                }
                PrometheusValue::Summary(_) => panic!("{name} is a summary: {exposition}"),
            }
        }
    }
    samples
}

/// The value of `key` among `samples`.
fn sample(samples: &BTreeMap<String, f64>, key: &str) -> f64 {
    *samples
        .get(key)
        .unwrap_or_else(|| panic!("no sample {key} among {samples:#?}"))
}

/// The `spindle_http_requests_total` series of one request.
fn requests(
    method: &str,
    route: &str,
    operation: &str,
    kind: &str,
    class: &str,
    status_class: &str,
) -> String {
    series(
        "spindle_http_requests_total",
        &[
            ("method", method),
            ("route", route),
            ("operation", operation),
            ("kind", kind),
            ("class", class),
            ("status_class", status_class),
        ],
    )
}

/// `POST /api/reset` with no body, answered `200`.
fn reset(served: &Served) {
    let (status, _, body) = text(served.address, "POST", "/api/reset");
    assert_eq!(status, 200, "the reset is answered: {body}");
}

#[test]
fn the_metrics_parse_as_prometheus_text() {
    let served = serve(INGRESS, &["--http", INGRESS_HTTP, "--observe"]);
    scripted(&served);
    let exposition = scrape(&served);
    assert!(
        exposition.ends_with('\n'),
        "every line ends: {exposition:?}"
    );
    openmetrics_parser::prometheus::parse_prometheus(&exposition)
        .unwrap_or_else(|error| panic!("{error:?}: {exposition}"));
    let declared = families();
    let sampled = |name: &str| {
        declared.iter().any(|(family, kind, _)| {
            name == family
                || (kind == "histogram"
                    && ["_bucket", "_count", "_sum"]
                        .iter()
                        .any(|suffix| name.strip_suffix(suffix) == Some(family.as_str())))
        })
    };
    for line in exposition.lines() {
        let described = ["# HELP ", "# TYPE "].iter().find_map(|prefix| {
            line.strip_prefix(prefix)
                .and_then(|rest| rest.split(' ').next())
        });
        if let Some(name) = described {
            assert!(
                declared.iter().any(|(family, ..)| family == name),
                "{line:?} describes a declared family"
            );
            continue;
        }
        let name = line
            .split(['{', ' '])
            .next()
            .expect("a line has a first token");
        assert!(
            sampled(name),
            "{line:?} is a sample of a declared family:\n{exposition}"
        );
    }
    let (status, _, body) = text(served.address, "POST", "/__spindle/metrics");
    assert_eq!(status, 405, "{body}");
}

#[test]
fn the_metrics_count_a_scripted_request_sequence() {
    let served = serve(INGRESS, &["--http", INGRESS_HTTP, "--observe"]);
    scripted(&served);
    let samples = samples(&scrape(&served));
    let close = "/tasks/{task_id}/close";
    let close_command = "tasks.board.CloseTask";
    for status_class in ["2xx", "4xx"] {
        let key = requests(
            "POST",
            close,
            close_command,
            "command",
            "served",
            status_class,
        );
        assert_eq!(sample(&samples, &key), 1.0, "{key}");
    }
    let unrouted = samples
        .iter()
        .filter(|(key, _)| {
            key.starts_with("spindle_http_requests_total{") && key.contains("class=\"unrouted\"")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        unrouted.iter().map(|(_, value)| **value).sum::<f64>(),
        2.0,
        "{unrouted:?}"
    );
    for (key, _) in &unrouted {
        assert!(
            key.contains("route=\"\",operation=\"\",kind=\"\""),
            "an unrouted request has an empty route, operation and kind: {key}"
        );
    }
    let all = requests(
        "GET",
        "/tasks",
        "tasks.board.AllTasks",
        "view",
        "served",
        "2xx",
    );
    assert_eq!(sample(&samples, &all), 1.0, "{all}");
    let closed = series(
        "spindle_commands_total",
        &[("command", close_command), ("outcome", "closed")],
    );
    assert_eq!(sample(&samples, &closed), 1.0, "{closed}");
    // story:metrics-observed-series: the durations are counted by method and route template.
    let durations = |suffix: &str| {
        samples
            .iter()
            .filter(|(key, _)| {
                key.starts_with(&format!("spindle_http_request_duration_seconds{suffix}{{"))
                    && (suffix != "_bucket" || key.ends_with("le=\"+Inf\"}"))
            })
            .map(|(_, value)| *value)
            .sum::<f64>()
    };
    assert_eq!(durations("_count"), 7.0, "{samples:#?}");
    assert_eq!(durations("_bucket"), 7.0, "{samples:#?}");
    let closing = series(
        "spindle_http_request_duration_seconds_count",
        &[("method", "POST"), ("route", close)],
    );
    assert_eq!(sample(&samples, &closing), 2.0, "{closing}");
    let total = samples
        .iter()
        .filter(|(key, _)| key.starts_with("spindle_http_requests_total{"))
        .map(|(_, value)| *value)
        .sum::<f64>();
    assert_eq!(total, 7.0, "{samples:#?}");
}

#[test]
fn the_exposed_families_are_the_specified_families() {
    let served = serve(INGRESS, &["--http", INGRESS_HTTP, "--observe"]);
    scripted(&served);
    let exposition = scrape(&served);
    let typed = exposition
        .lines()
        .filter_map(|line| line.strip_prefix("# TYPE "))
        .map(|rest| {
            let (name, kind) = rest.split_once(' ').expect("a TYPE line names a kind");
            (name.to_owned(), kind.to_owned())
        })
        .collect::<Vec<_>>();
    let declared = families();
    let specified = declared
        .iter()
        .map(|(name, kind, _)| (name.clone(), kind.clone()))
        .collect::<Vec<_>>();
    assert_eq!(typed, specified);
    // Every sample line carries its family's labels in the specified order (`le` last).
    let mut sampled = std::collections::BTreeSet::new();
    for line in exposition.lines().filter(|line| !line.starts_with('#')) {
        let name = line
            .split(['{', ' '])
            .next()
            .expect("a sample names its series");
        let (family, _, labels) = declared
            .iter()
            .find(|(family, kind, _)| {
                name == family
                    || (kind == "histogram"
                        && ["_bucket", "_count", "_sum"]
                            .iter()
                            .any(|suffix| name.strip_suffix(suffix) == Some(family.as_str())))
            })
            .unwrap_or_else(|| panic!("{line:?} is a sample of a specified family"));
        let named = line
            .split_once('{')
            .and_then(|(_, rest)| rest.rsplit_once('}'))
            .map(|(pairs, _)| {
                pairs
                    .split("\",")
                    .map(|pair| pair.split_once('=').expect("a label pair").0.to_owned())
                    .filter(|label| label != "le")
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        assert_eq!(&named, labels, "{line}");
        sampled.insert(family.clone());
    }
    assert!(
        sampled.contains("spindle_http_requests_total"),
        "the label order is checked on the request family: {sampled:?}"
    );
}

/// story:metrics-observed-series: a catalogue counter is exposed once it counted anything, and
/// every counter sample exposed is one catalogue counter, never 0.
#[test]
fn every_counted_catalogue_counter_is_exposed() {
    use spindle_telemetry::Key;
    let mut kinds = std::collections::HashSet::new();
    let mut counted_any = false;
    for spec in [INGRESS, RETRIED, RALLY] {
        let served = if spec == INGRESS {
            serve(spec, &["--http", INGRESS_HTTP, "--observe"])
        } else {
            serve(spec, &["--observe"])
        };
        let unexecuted = samples(&scrape(&served));
        if spec == INGRESS {
            scripted(&served);
        }
        let samples = samples(&scrape(&served));
        let ir = spindle_spec::load(&root().join(spec))
            .expect("the specification compiles")
            .ir;
        let mut expected = std::collections::BTreeSet::new();
        for component in ir.components().values() {
            let component = component.name.as_str();
            let catalogue = spindle_telemetry::catalogue(&ir, component).expect("a catalogue");
            for counter in &catalogue.counters {
                let key = match &counter.key {
                    Key::Outcome { command, outcome } => series(
                        "spindle_commands_total",
                        &[("command", command), ("outcome", outcome)],
                    ),
                    Key::Failed { command, failure } => series(
                        "spindle_command_failures_total",
                        &[("command", command), ("failure", failure.as_str())],
                    ),
                    Key::Duration { command } => series(
                        "spindle_command_duration_seconds_bucket",
                        &[("command", command), ("le", "+Inf")],
                    ),
                    Key::Appended { event } => {
                        series("spindle_events_appended_total", &[("event", event)])
                    }
                    Key::Carried { binding, carried } => series(
                        "spindle_bindings_total",
                        &[
                            ("component", component),
                            ("binding", binding),
                            ("carried", carried.as_str()),
                        ],
                    ),
                    Key::Retried { binding, retried } => series(
                        "spindle_binding_retries_total",
                        &[("binding", binding), ("retried", retried.as_str())],
                    ),
                    Key::Timed { binding, timed } => series(
                        "spindle_timer_ticks_total",
                        &[("binding", binding), ("timed", timed.as_str())],
                    ),
                    Key::RowsWritten => series(
                        "spindle_store_rows_written_total",
                        &[("component", component)],
                    ),
                    Key::BytesWritten => series(
                        "spindle_store_bytes_written_total",
                        &[("component", component)],
                    ),
                };
                kinds.insert(std::mem::discriminant(&counter.key));
                assert!(
                    !unexecuted.contains_key(&key),
                    "{spec}: {} of {component} is exposed as {key} before it counted anything",
                    counter.name
                );
                expected.insert(key);
            }
        }
        let counted = [
            "spindle_commands_total{",
            "spindle_command_failures_total{",
            "spindle_command_duration_seconds_bucket{",
            "spindle_events_appended_total{",
            "spindle_bindings_total{",
            "spindle_binding_retries_total{",
            "spindle_timer_ticks_total{",
            "spindle_store_rows_written_total{",
            "spindle_store_bytes_written_total{",
        ];
        let exposed = samples
            .keys()
            .filter(|key| counted.iter().any(|prefix| key.starts_with(prefix)))
            .filter(|key| {
                !key.starts_with("spindle_command_duration_seconds_bucket{")
                    || key.ends_with(",le=\"+Inf\"}")
            })
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        let uncatalogued = exposed.difference(&expected).collect::<Vec<_>>();
        assert!(
            uncatalogued.is_empty(),
            "{spec}: every counter sample is one catalogue counter: {uncatalogued:?}"
        );
        for key in &exposed {
            assert!(sample(&samples, key) > 0.0, "{spec}: {key} is exposed at 0");
        }
        counted_any |= !exposed.is_empty();
    }
    assert!(counted_any, "the scripted requests are counted");
    assert_eq!(kinds.len(), 9, "the fixtures hold every kind of counter");
}

#[test]
fn entity_rows_follow_writes_and_resets() {
    let served = serve(INGRESS, &["--http", INGRESS_HTTP, "--observe"]);
    let rows = series("spindle_entity_rows", &[("entity", "tasks.board.Task")]);
    let before = samples(&scrape(&served));
    // story:metrics-observed-series: an entity that never held a row is not written.
    assert!(!before.contains_key(&rows), "{before:#?}");
    assert_eq!(sample(&before, "spindle_resets_total"), 0.0);
    for (task, title) in [("t1", "Water the plants"), ("t2", "Sweep")] {
        let (status, body) = exchange(
            served.address,
            "PUT",
            &format!("/tasks/{task}"),
            Some(&json!({"title": title})),
        );
        assert_eq!(status, 201, "{body}");
    }
    let opened = samples(&scrape(&served));
    assert_eq!(sample(&opened, &rows), 2.0);
    reset(&served);
    let emptied = samples(&scrape(&served));
    assert_eq!(sample(&emptied, &rows), 0.0);
    assert_eq!(sample(&emptied, "spindle_resets_total"), 1.0);
}

#[test]
fn a_reset_leaves_counters_and_the_log_intact() {
    let served = serve(INGRESS, &["--http", INGRESS_HTTP, "--observe"]);
    scripted(&served);
    let before = samples(&scrape(&served));
    let logged = log(&served, "");
    reset(&served);
    let after = samples(&scrape(&served));

    // The reset is itself a request the run answered: it adds its own series and one duration.
    let own = requests(
        "POST",
        "/api/reset",
        "POST /api/reset",
        "console",
        "served",
        "2xx",
    );
    assert!(!before.contains_key(&own), "{before:#?}");
    assert_eq!(sample(&after, &own), 1.0, "{own}");
    // story:metrics-observed-series: its duration is counted under its own route, and the
    // second scrape writes how long the first took.
    let duration = "spindle_http_request_duration_seconds";
    let own_duration =
        |key: &str| key.starts_with(duration) && key.contains("route=\"/api/reset\"");
    let scraped = "spindle_metrics_scrape_duration_seconds";
    for (key, value) in &before {
        if key.starts_with("spindle_entity_rows") || key == "spindle_resets_total" || key == scraped
        {
            continue;
        }
        assert_eq!(sample(&after, key), *value, "{key} is kept by the reset");
    }
    let reset_count = series(
        &format!("{duration}_count"),
        &[("method", "POST"), ("route", "/api/reset")],
    );
    assert_eq!(sample(&after, &reset_count), 1.0, "{reset_count}");
    assert!(after.contains_key(scraped), "{after:#?}");
    let added = after
        .keys()
        .filter(|key| !before.contains_key(*key) && !own_duration(key) && *key != scraped)
        .collect::<Vec<_>>();
    assert_eq!(added, [&own], "the reset adds only its own series");
    assert!(sample(&before, "spindle_entity_rows{entity=\"tasks.board.Task\"}") > 0.0);
    assert_eq!(
        sample(&after, "spindle_entity_rows{entity=\"tasks.board.Task\"}"),
        0.0
    );
    assert_eq!(
        sample(&after, "spindle_resets_total"),
        sample(&before, "spindle_resets_total") + 1.0
    );

    let kept = log(&served, "");
    assert_eq!(kept["recorded"], 8, "{kept}");
    let mut totals = logged["totals"].clone();
    totals["served"] = json!(logged["totals"]["served"].as_u64().expect("a total") + 1);
    assert_eq!(kept["totals"], totals, "{kept}");
    let records = kept["requests"].as_array().expect("requests");
    assert_eq!(records[0]["path"], "/api/reset", "{kept}");
    assert_eq!(
        records[1..],
        logged["requests"].as_array().expect("requests")[..],
        "the earlier records are kept"
    );

    let (status, body) = exchange(
        served.address,
        "PUT",
        "/tasks/t3",
        Some(&json!({"title": "Dust"})),
    );
    assert_eq!(status, 201, "{body}");
    let later = samples(&scrape(&served));
    let opened = series(
        "spindle_commands_total",
        &[("command", "tasks.board.OpenTask"), ("outcome", "opened")],
    );
    assert_eq!(sample(&later, &opened), sample(&before, &opened) + 1.0);
    let open = requests(
        "PUT",
        "/tasks/{task_id}",
        "tasks.board.OpenTask",
        "command",
        "served",
        "2xx",
    );
    assert_eq!(sample(&later, &open), sample(&before, &open) + 1.0);
}

#[test]
fn reading_the_metrics_changes_no_count() {
    let served = serve(INGRESS, &["--http", INGRESS_HTTP, "--observe"]);
    scripted(&served);
    // story:metrics-observed-series: only the gauge of the last scrape's duration moves.
    let counted = |exposition: String| {
        exposition
            .lines()
            .filter(|line| !line.starts_with("spindle_metrics_scrape_duration_seconds "))
            .map(|line| format!("{line}\n"))
            .collect::<String>()
    };
    let first = counted(scrape(&served));
    let second = counted(scrape(&served));
    assert_eq!(first, second);
    assert_eq!(log(&served, "")["recorded"], 7, "a scrape is not recorded");
}

// story:external-answers-observed: what answered each external cause a served command asked
// about, in its request's `externals` and in `spindle_external_causes_total`. The fixture
// `tests/fixtures/served-review` serves `probe.review.Submit`, which asks `flagged` and then
// `escalated` beside a default branch, and `probe.review.Decide`, which asks `vetoed` of an
// approving decision with no default branch beside it.

const REVIEW: &str = "crates/spindle-cli/tests/fixtures/served-review";
const SUBMIT: &str = "probe.review.Submit";
const DECIDE: &str = "probe.review.Decide";

/// Writes `text` as a decisions file in `directory` and returns its path.
fn decisions_file(directory: &tempfile::TempDir, text: &str) -> String {
    let path = directory.path().join("decisions.toml");
    std::fs::write(&path, text).expect("the decisions file is written");
    path.to_str().expect("a UTF-8 path").to_owned()
}

/// Calls `operation` of the review fixture's `probe` at its route with `body`.
fn call_review(served: &Served, operation: &str, body: &Value) -> (u16, Value) {
    let (method, path) = route(REVIEW, "probe", operation);
    exchange(served.address, &method, &path, Some(body))
}

/// `POST /api/reset` carrying `world`, answered `200`.
fn reset_to(served: &Served, world: &Value) {
    let (status, body) = exchange(served.address, "POST", "/api/reset", Some(world));
    assert_eq!(status, 200, "the reset is answered: {body}");
}

/// The newest record of the request log.
fn newest(served: &Served) -> Value {
    log(served, "")["requests"][0].clone()
}

#[test]
fn an_external_cause_answered_by_the_decisions_file_is_recorded_as_file() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let file = decisions_file(&directory, "[\"probe.review.Submit\"]\nflagged = true\n");
    let served = serve(REVIEW, &["--decisions", &file, "--observe"]);
    let (_, answer) = call_review(
        &served,
        SUBMIT,
        &json!({"submission_id": "s1", "amount": 5}),
    );
    assert_eq!(answer["outcome"], "flagged", "{answer}");
    let record = newest(&served);
    assert_eq!(record["operation"], SUBMIT, "{record}");
    assert_eq!(
        record["externals"],
        json!([{"command": SUBMIT, "outcome": "flagged", "holds": true, "source": "file"}]),
        "{record}"
    );
}

#[test]
fn an_external_cause_answered_by_the_reset_world_is_recorded_as_world() {
    let served = serve(REVIEW, &["--answer-default", "--observe"]);
    reset_to(
        &served,
        &json!({"decisions": {"probe.review.Submit": {"flagged": false}}}),
    );
    let (status, answer) = call_review(
        &served,
        SUBMIT,
        &json!({"submission_id": "s1", "amount": 5}),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    assert_eq!(answer["outcome"], "submitted", "{answer}");
    let record = newest(&served);
    assert_eq!(record["operation"], SUBMIT, "{record}");
    assert_eq!(
        record["externals"],
        json!([
            {"command": SUBMIT, "outcome": "flagged", "holds": false, "source": "world"},
            {"command": SUBMIT, "outcome": "escalated", "holds": false, "source": "default"},
        ]),
        "the world's table answers first, the run's answers what it does not: {record}"
    );
}

#[test]
fn an_external_cause_answered_by_answer_default_is_recorded_as_default() {
    let served = serve(REVIEW, &["--answer-default", "--observe"]);
    let (status, answer) = call_review(
        &served,
        SUBMIT,
        &json!({"submission_id": "s1", "amount": 5}),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    let record = newest(&served);
    assert_eq!(record["class"], "served", "{record}");
    assert_eq!(
        record["externals"],
        json!([
            {"command": SUBMIT, "outcome": "flagged", "holds": false, "source": "default"},
            {"command": SUBMIT, "outcome": "escalated", "holds": false, "source": "default"},
        ]),
        "{record}"
    );
}

#[test]
fn an_unanswered_cause_is_recorded_as_unanswered() {
    let served = serve(REVIEW, &["--answer-default", "--observe"]);
    let (status, answer) = call_review(
        &served,
        DECIDE,
        &json!({"decision_id": "d1", "approve": true}),
    );
    assert_eq!(status, 501, "{answer}");
    let record = newest(&served);
    assert_eq!(record["operation"], DECIDE, "{record}");
    assert_eq!(record["class"], "not-served", "{record}");
    assert_eq!(record["reason"], answer["refused"], "{record}");
    assert_eq!(
        record["externals"],
        json!([{"command": DECIDE, "outcome": "vetoed", "source": "unanswered"}]),
        "{record}"
    );
    let answered = log(&served, "");
    let typed: spindle_observe_api::RequestLog = serde_json::from_value(answered.clone())
        .unwrap_or_else(|error| panic!("{error}: {answered}"));
    assert_eq!(typed.requests[0].externals.len(), 1, "{answered}");
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join(SCHEMA)).expect("the schema is read"),
    )
    .expect("the schema is JSON");
    let validator = jsonschema::draft202012::new(&schema).expect("the schema compiles");
    let errors = validator
        .iter_errors(&answered)
        .map(|error| format!("{error} at {}", error.instance_path()))
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "{answered}: {errors:?}");
}

#[test]
fn external_causes_are_counted_per_command_outcome_and_source() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let file = decisions_file(&directory, "[\"probe.review.Submit\"]\nflagged = false\n");
    let served = serve(
        REVIEW,
        &["--decisions", &file, "--answer-default", "--observe"],
    );
    let submit = |id: &str| {
        let (status, answer) =
            call_review(&served, SUBMIT, &json!({"submission_id": id, "amount": 5}));
        assert_eq!(answer["outcome"], "submitted", "{status} {answer}");
    };
    let decide = |id: &str, approve: bool| {
        call_review(
            &served,
            DECIDE,
            &json!({"decision_id": id, "approve": approve}),
        )
    };
    submit("s1");
    submit("s2");
    let (status, answer) = decide("d1", true);
    assert_eq!(status, 501, "nothing answers `vetoed`: {answer}");
    let (_, answer) = decide("d2", false);
    assert_eq!(
        answer["outcome"], "declined",
        "no external cause is asked: {answer}"
    );
    reset_to(
        &served,
        &json!({"decisions": {"probe.review.Decide": {"vetoed": false}}}),
    );
    let (_, answer) = decide("d3", true);
    assert_eq!(answer["outcome"], "approved", "{answer}");
    submit("s3");

    let mut recorded = BTreeMap::<String, f64>::new();
    let logged = log(&served, "");
    for record in logged["requests"].as_array().expect("requests") {
        for external in record["externals"].as_array().expect("externals") {
            let key = series(
                "spindle_external_causes_total",
                &[
                    ("command", external["command"].as_str().expect("a command")),
                    ("outcome", external["outcome"].as_str().expect("an outcome")),
                    ("source", external["source"].as_str().expect("a source")),
                ],
            );
            *recorded.entry(key).or_default() += 1.0;
        }
    }
    let counted = samples(&scrape(&served))
        .into_iter()
        .filter(|(key, _)| key.starts_with("spindle_external_causes_total"))
        .collect::<BTreeMap<_, _>>();
    let expected = [
        (SUBMIT, "flagged", "file", 3.0),
        (SUBMIT, "escalated", "default", 3.0),
        (DECIDE, "vetoed", "unanswered", 1.0),
        (DECIDE, "vetoed", "world", 1.0),
    ]
    .into_iter()
    .map(|(command, outcome, source, count)| {
        let labels = [
            ("command", command),
            ("outcome", outcome),
            ("source", source),
        ];
        (series("spindle_external_causes_total", &labels), count)
    })
    .collect::<BTreeMap<_, _>>();
    assert_eq!(counted, expected, "{logged}");
    assert_eq!(
        recorded, counted,
        "the metric matches the records: {logged}"
    );
}
