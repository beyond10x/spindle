//! Adversary cases for `story:external-answers-observed`: `run --observe` lists in each request
//! record's `externals` what answered every external cause its command asked about, and counts
//! them as `spindle_external_causes_total{command,outcome,source}`.
//!
//! `tests/fixtures/served-review` is the unit's own fixture. `tests/fixtures/adversary-served-cascade`
//! has a network-reached `relay.desk.Take` whose event a binding turns into `relay.audit.Audit`,
//! which asks `rejected`: a cause asked inside a served request by a command the request did not
//! name. `tests/fixtures/adversary-external-relay` has one network-reached command,
//! `chain.desk.Forward`, which asks `held` and whose event a binding turns into the same command
//! for the next link: a cause asked inside a served request by a command of the same name.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

const REVIEW: &str = "crates/spindle-cli/tests/fixtures/served-review";
const CASCADE: &str = "crates/spindle-cli/tests/fixtures/adversary-served-cascade";
const RELAY: &str = "crates/spindle-cli/tests/fixtures/adversary-external-relay";
const LAPTOP: &str = "examples/environments/laptop.toml";
const SCHEMA: &str = "ess/generated/jsonschema/schema/types/spindle.observe.RequestLog.schema.json";
const SUBMIT: &str = "probe.review.Submit";
const DECIDE: &str = "probe.review.Decide";
const FAMILY: &str = "spindle_external_causes_total";

const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
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

fn serve(spec: &str, arguments: &[&str]) -> Served {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", spec, "--environment", LAPTOP, "--state"])
        .arg(state.path().join("state"))
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

/// One exchange on its own connection: status and raw body.
fn raw(address: SocketAddr, method: &str, path: &str, body: Option<&Value>) -> (u16, String) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream.set_read_timeout(Some(ANSWER)).expect("a timeout");
    let body = body.map(Value::to_string).unwrap_or_default();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .expect("the request is written");
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).expect("the answer is read");
    let answer = String::from_utf8(answer).expect("UTF-8");
    let (head, body) = answer.split_once("\r\n\r\n").expect("a head");
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .expect("a status");
    (status, body.to_owned())
}

fn exchange(address: SocketAddr, method: &str, path: &str, body: Option<&Value>) -> (u16, Value) {
    let (status, body) = raw(address, method, path, body);
    let body = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&body).unwrap_or_else(|error| panic!("{error}: {body:?}"))
    };
    (status, body)
}

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

fn call(
    served: &Served,
    spec: &str,
    component: &str,
    operation: &str,
    body: &Value,
) -> (u16, Value) {
    let (method, path) = route(spec, component, operation);
    let path = body
        .as_object()
        .expect("an object")
        .iter()
        .fold(path, |path, (field, value)| {
            path.replace(
                &format!("{{{field}}}"),
                value.as_str().unwrap_or(&value.to_string()),
            )
        });
    let carried = body
        .as_object()
        .expect("an object")
        .iter()
        .filter(|(field, _)| {
            !route(spec, component, operation)
                .1
                .contains(&format!("{{{field}}}"))
        })
        .map(|(field, value)| (field.clone(), value.clone()))
        .collect::<serde_json::Map<_, _>>();
    exchange(
        served.address,
        &method,
        &path,
        Some(&Value::Object(carried)),
    )
}

fn log(served: &Served) -> Value {
    let (status, body) = exchange(served.address, "GET", "/__spindle/requests", None);
    assert_eq!(status, 200, "{body}");
    body
}

/// Every `spindle_external_causes_total` sample, by its label text.
fn counted(served: &Served) -> BTreeMap<String, f64> {
    let (status, body) = raw(served.address, "GET", "/__spindle/metrics", None);
    assert_eq!(status, 200, "{body}");
    body.lines()
        .filter(|line| line.starts_with(&format!("{FAMILY}{{")))
        .map(|line| {
            let (series, value) = line.rsplit_once(' ').expect("a sample line");
            (series.to_owned(), value.parse::<f64>().expect("a number"))
        })
        .collect()
}

fn decisions(directory: &tempfile::TempDir, text: &str) -> String {
    let path = directory.path().join("decisions.toml");
    std::fs::write(&path, text).expect("the decisions file is written");
    path.to_str().expect("a UTF-8 path").to_owned()
}

fn directory() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory")
}

/// The causes the request log lists, by command, outcome and source.
fn listed(log: &Value) -> BTreeMap<String, f64> {
    let mut listed = BTreeMap::new();
    for record in log["requests"].as_array().expect("requests") {
        for external in record["externals"].as_array().expect("externals") {
            let key = format!(
                "{FAMILY}{{command=\"{}\",outcome=\"{}\",source=\"{}\"}}",
                record["operation"].as_str().expect("an operation"),
                external["outcome"].as_str().expect("an outcome"),
                external["source"].as_str().expect("a source"),
            );
            *listed.entry(key).or_insert(0.0) += 1.0;
        }
    }
    listed
}

// A cause asked by a command an event binding invoked, inside the served request that emitted the
// event: the metric counts it, and no record of the request log lists it, so the two disagree for
// one request. The story's outcome: "every external cause a served command asks about is
// recorded", and its acceptance holds the metric to the records.
#[test]
fn a_cause_asked_by_a_bound_command_is_listed_where_it_is_counted() {
    let directory = directory();
    let file = decisions(&directory, "[\"relay.audit.Audit\"]\nrejected = false\n");
    let served = serve(CASCADE, &["--decisions", &file, "--observe"]);
    let (status, answer) = call(
        &served,
        CASCADE,
        "desk",
        "relay.desk.Take",
        &json!({"request_id": "r1"}),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    let counted = counted(&served);
    assert_eq!(
        counted.get(&format!(
            "{FAMILY}{{command=\"relay.audit.Audit\",outcome=\"rejected\",source=\"file\"}}"
        )),
        Some(&1.0),
        "the bound command asked once: {counted:?}"
    );
    let logged = log(&served);
    assert_eq!(
        listed(&logged),
        counted,
        "the one served request caused the one cause counted, and its record lists none: {logged}"
    );
}

// A command an event binding invokes inside the served request, of the same name as the routed
// command: its cause lands in the request's own `externals`, which then lists the routed command
// asking the same outcome more than once. The kernel asks a cause once per run, so the routed
// command asked `held` once.
#[test]
fn a_reinvoked_command_does_not_add_to_the_externals_of_the_request() {
    let directory = directory();
    let file = decisions(&directory, "held = false\n");
    let served = serve(RELAY, &["--decisions", &file, "--observe"]);
    let (status, answer) = call(
        &served,
        RELAY,
        "desk",
        "chain.desk.Forward",
        &json!({"link_id": "a", "next": "b"}),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    let record = log(&served)["requests"][0].clone();
    assert_eq!(record["operation"], "chain.desk.Forward", "{record}");
    assert_eq!(
        record["externals"],
        json!([{"outcome": "held", "holds": false, "source": "file"}]),
        "chain.desk.Forward asked `held` once in this run: {record}"
    );
}

// Probe: a command refused by a cause that holds lists what it asked.
#[test]
fn a_command_refused_by_its_cause_lists_it() {
    let directory = directory();
    let file = decisions(&directory, "[\"probe.review.Decide\"]\nvetoed = true\n");
    let served = serve(REVIEW, &["--decisions", &file, "--observe"]);
    let (status, answer) = call(
        &served,
        REVIEW,
        "probe",
        DECIDE,
        &json!({"decision_id": "d1", "approve": true}),
    );
    assert_eq!(answer["outcome"], "vetoed", "{status} {answer}");
    assert!(status >= 400, "{status} {answer}");
    let record = log(&served)["requests"][0].clone();
    assert_eq!(
        record["externals"],
        json!([{"outcome": "vetoed", "holds": true, "source": "file"}]),
        "{record}"
    );
}

// Probe: a reset without a decision table hands causes back to the run's own answers.
#[test]
fn a_reset_without_a_table_returns_causes_to_the_run() {
    let served = serve(REVIEW, &["--answer-default", "--observe"]);
    let (status, body) = exchange(
        served.address,
        "POST",
        "/api/reset",
        Some(&json!({"decisions": {"flagged": false}})),
    );
    assert_eq!(status, 200, "{body}");
    let (status, body) = exchange(served.address, "POST", "/api/reset", Some(&json!({})));
    assert_eq!(status, 200, "{body}");
    let (status, answer) = call(
        &served,
        REVIEW,
        "probe",
        SUBMIT,
        &json!({"submission_id": "s1", "amount": 5}),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    let record = log(&served)["requests"][0].clone();
    assert_eq!(
        record["externals"],
        json!([
            {"outcome": "flagged", "holds": false, "source": "default"},
            {"outcome": "escalated", "holds": false, "source": "default"},
        ]),
        "{record}"
    );
}

// Probe: requests sent at once each list only their own causes.
#[test]
fn concurrent_requests_keep_their_own_externals() {
    let directory = directory();
    let file = decisions(&directory, "[\"probe.review.Submit\"]\nflagged = false\n");
    let served = serve(
        REVIEW,
        &["--decisions", &file, "--answer-default", "--observe"],
    );
    let address = served.address;
    let (submit_method, submit_path) = route(REVIEW, "probe", SUBMIT);
    let (decide_method, decide_path) = route(REVIEW, "probe", DECIDE);
    let threads = (0..24)
        .map(|index| {
            let (method, path, body) = if index % 2 == 0 {
                (
                    submit_method.clone(),
                    submit_path.clone(),
                    json!({"submission_id": format!("s{index}"), "amount": 5}),
                )
            } else {
                (
                    decide_method.clone(),
                    decide_path.clone(),
                    json!({"decision_id": format!("d{index}"), "approve": true}),
                )
            };
            std::thread::spawn(move || exchange(address, &method, &path, Some(&body)))
        })
        .collect::<Vec<_>>();
    for thread in threads {
        thread.join().expect("the request thread ends");
    }
    let logged = log(&served);
    for record in logged["requests"].as_array().expect("requests") {
        let expected = if record["operation"] == SUBMIT {
            json!([
                {"outcome": "flagged", "holds": false, "source": "file"},
                {"outcome": "escalated", "holds": false, "source": "default"},
            ])
        } else {
            json!([{"outcome": "vetoed", "source": "unanswered"}])
        };
        assert_eq!(record["externals"], expected, "{record}");
    }
    assert_eq!(listed(&logged), counted(&served), "{logged}");
}

// Probe: every source in one log, read as the generated type and validated by the schema.
#[test]
fn every_source_validates_against_the_request_log_schema() {
    let directory = directory();
    let file = decisions(&directory, "[\"probe.review.Submit\"]\nflagged = false\n");
    let served = serve(
        REVIEW,
        &["--decisions", &file, "--answer-default", "--observe"],
    );
    call(
        &served,
        REVIEW,
        "probe",
        SUBMIT,
        &json!({"submission_id": "s1", "amount": 5}),
    );
    call(
        &served,
        REVIEW,
        "probe",
        DECIDE,
        &json!({"decision_id": "d1", "approve": true}),
    );
    let (status, body) = exchange(
        served.address,
        "POST",
        "/api/reset",
        Some(&json!({"decisions": {"probe.review.Decide": {"vetoed": true}}})),
    );
    assert_eq!(status, 200, "{body}");
    call(
        &served,
        REVIEW,
        "probe",
        DECIDE,
        &json!({"decision_id": "d2", "approve": true}),
    );
    let logged = log(&served);
    let sources = logged["requests"]
        .as_array()
        .expect("requests")
        .iter()
        .flat_map(|record| record["externals"].as_array().expect("externals").clone())
        .map(|external| external["source"].as_str().expect("a source").to_owned())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        sources.into_iter().collect::<Vec<_>>(),
        ["default", "file", "unanswered", "world"],
        "{logged}"
    );
    let _: spindle_observe_api::RequestLog =
        serde_json::from_value(logged.clone()).unwrap_or_else(|error| panic!("{error}: {logged}"));
    let schema: Value =
        serde_json::from_str(&std::fs::read_to_string(root().join(SCHEMA)).expect("the schema"))
            .expect("JSON");
    let validator = jsonschema::draft202012::new(&schema).expect("the schema compiles");
    let errors = validator
        .iter_errors(&logged)
        .map(|error| format!("{error} at {}", error.instance_path()))
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "{logged}: {errors:?}");
}

// Probe: without `--observe` the same requests are answered the same way.
#[test]
fn without_observe_the_answers_are_unchanged() {
    let directory = directory();
    let file = decisions(&directory, "[\"probe.review.Submit\"]\nflagged = true\n");
    let sequence = |served: &Served| {
        let mut answers = Vec::new();
        answers.push(call(
            served,
            REVIEW,
            "probe",
            SUBMIT,
            &json!({"submission_id": "s1", "amount": 5}),
        ));
        let (status, _) = exchange(
            served.address,
            "POST",
            "/api/reset",
            Some(&json!({"decisions": {"probe.review.Submit": {"flagged": false}}})),
        );
        assert_eq!(status, 200);
        answers.push(call(
            served,
            REVIEW,
            "probe",
            SUBMIT,
            &json!({"submission_id": "s2", "amount": 5}),
        ));
        answers.push(call(
            served,
            REVIEW,
            "probe",
            DECIDE,
            &json!({"decision_id": "d1", "approve": true}),
        ));
        answers
    };
    let observed = serve(
        REVIEW,
        &["--decisions", &file, "--answer-default", "--observe"],
    );
    let plain = serve(REVIEW, &["--decisions", &file, "--answer-default"]);
    assert_eq!(sequence(&observed), sequence(&plain));
    let (status, _) = raw(plain.address, "GET", "/__spindle/metrics", None);
    assert_eq!(status, 404);
}
