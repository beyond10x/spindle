//! Adversary cases for `run` serving `ingress.network` with `--decisions` and `--answer-default`.
//!
//! `tests/fixtures/served-review` is the unit's own fixture. `tests/fixtures/adversary-served-cascade`
//! adds a network-reached `desk` whose `relay.desk.Take` emits an event that a binding turns into
//! `relay.audit.Audit` on an in-process `audit` component, whose one external outcome sits beside a
//! default: an external cause reached by a binding inside a served request.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const REVIEW: &str = "crates/spindle-cli/tests/fixtures/served-review";
const CASCADE: &str = "crates/spindle-cli/tests/fixtures/adversary-served-cascade";
const LAPTOP: &str = "examples/environments/laptop.toml";

const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(10);
const EXIT: Duration = Duration::from_secs(60);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn contract(spec: &str, component: &str) -> Value {
    let loaded = spindle_spec::load(&root().join(spec)).expect("the fixture compiles");
    let component = loaded
        .ir
        .components()
        .values()
        .find(|candidate| candidate.name.as_str() == component)
        .expect("the fixture declares the component");
    serde_json::from_str(&ess_gen::openapi::json(&loaded.ir, component))
        .expect("the projection is JSON")
}

fn route(spec: &str, component: &str, id: &str) -> (String, String) {
    let document = contract(spec, component);
    for (path, item) in document["paths"].as_object().expect("paths") {
        for (method, operation) in item.as_object().expect("a path item") {
            if operation["operationId"] == id {
                return (method.to_uppercase(), path.clone());
            }
        }
    }
    panic!("the contract declares no operation {id}")
}

fn declares(spec: &str, component: &str, id: &str, status: u16) -> bool {
    let (method, path) = route(spec, component, id);
    !contract(spec, component)["paths"][&path][method.to_lowercase()]["responses"]
        [status.to_string()]
    .is_null()
}

fn decisions(directory: &tempfile::TempDir, text: &str) -> String {
    let path = directory.path().join("decisions.toml");
    std::fs::write(&path, text).expect("the decisions file is written");
    path.to_str().expect("a UTF-8 path").to_owned()
}

struct Served {
    child: Child,
    address: SocketAddr,
}

impl Served {
    /// Stops the run and returns everything it wrote on stderr.
    fn stop(mut self) -> String {
        let _ = self.child.kill();
        let mut stderr = String::new();
        if let Some(mut pipe) = self.child.stderr.take() {
            let _ = pipe.read_to_string(&mut stderr);
        }
        let _ = self.child.wait();
        stderr
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn command(spec: &str, state: &tempfile::TempDir, arguments: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"));
    command
        .current_dir(root())
        .args(["run", spec, "--environment", LAPTOP, "--state"])
        .arg(state.path().join("state"))
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn serve(spec: &str, state: &tempfile::TempDir, arguments: &[&str]) -> Served {
    let mut child = command(spec, state, arguments)
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
    let address = serde_json::from_str::<Value>(&line)
        .ok()
        .and_then(|ready| ready["listening"].as_str()?.parse().ok());
    let Some(address) = address else {
        let _ = child.kill();
        let output = child.wait_with_output();
        panic!("run's first line {line:?} names no address: {output:?}");
    };
    Served { child, address }
}

fn exited(spec: &str, state: &tempfile::TempDir, arguments: &[&str]) -> Output {
    let mut child = command(spec, state, arguments)
        .spawn()
        .expect("b10x-spindle starts");
    let started = Instant::now();
    loop {
        if child.try_wait().expect("the child is waited on").is_some() {
            break;
        }
        if started.elapsed() > EXIT {
            let _ = child.kill();
            let _ = child.wait();
            panic!("run did not exit within {EXIT:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    child.wait_with_output().expect("the output is read")
}

fn exchange(served: &Served, method: &str, path: &str, body: &Value) -> (u16, Value) {
    let mut stream =
        TcpStream::connect_timeout(&served.address, ANSWER).expect("the listener accepts");
    stream.set_read_timeout(Some(ANSWER)).expect("timeout");
    stream.set_write_timeout(Some(ANSWER)).expect("timeout");
    let body = if body.is_null() {
        String::new()
    } else {
        body.to_string()
    };
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        served.address,
        body.len()
    )
    .expect("the request is written");
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).expect("the answer is read");
    let answer = String::from_utf8(answer).expect("UTF-8");
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

fn call(served: &Served, spec: &str, component: &str, id: &str, body: &Value) -> (u16, Value) {
    let (method, path) = route(spec, component, id);
    exchange(served, &method, &path, body)
}

fn events(served: &Served) -> Vec<String> {
    let (status, body) = exchange(served, "GET", "/api/events", &Value::Null);
    assert_eq!(status, 200, "{body}");
    body["events"]
        .as_array()
        .expect("events")
        .iter()
        .map(|event| event["event"].as_str().unwrap_or_default().to_owned())
        .collect()
}

#[test]
fn served_scoped_answer_wins_and_the_default_answers_what_the_file_does_not() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let file = decisions(
        &state,
        "flagged = true\n\n[\"probe.review.Submit\"]\nflagged = false\n",
    );
    let served = serve(REVIEW, &state, &["--decisions", &file, "--answer-default"]);
    let (status, answer) = call(
        &served,
        REVIEW,
        "probe",
        "probe.review.Submit",
        &json!({"submission_id": "s1", "amount": 5}),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    assert_eq!(answer["outcome"], "submitted", "{answer}");
}

#[test]
fn served_unanswered_request_is_a_declared_501_and_writes_nothing() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let file = decisions(
        &state,
        "[\"probe.review.Submit\"]\nflagged = false\nescalated = false\n",
    );
    let served = serve(REVIEW, &state, &["--decisions", &file]);
    let (status, answer) = call(
        &served,
        REVIEW,
        "probe",
        "probe.review.Decide",
        &json!({"decision_id": "d1", "approve": true}),
    );
    assert_eq!(status, 501, "{answer}");
    assert!(
        declares(REVIEW, "probe", "probe.review.Decide", 501),
        "the contract declares the 501"
    );
    assert_eq!(answer["committed"], false, "{answer}");
    assert!(
        answer["refused"]
            .as_str()
            .unwrap_or_default()
            .contains("external.probe.review.Decide.vetoed"),
        "{answer}"
    );
    assert!(events(&served).is_empty(), "nothing was published");
}

#[test]
fn served_request_body_cannot_carry_its_own_external_answer() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let served = serve(REVIEW, &state, &["--answer-default"]);
    let (status, answer) = call(
        &served,
        REVIEW,
        "probe",
        "probe.review.Decide",
        &json!({"decision_id": "d1", "approve": true, "external": {"vetoed": false}}),
    );
    assert_eq!(status, 400, "{answer}");
}

#[test]
fn served_decider_survives_a_console_reset() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let file = decisions(&state, "[\"probe.review.Submit\"]\nflagged = true\n");
    let served = serve(REVIEW, &state, &["--decisions", &file]);
    let (status, answer) = exchange(&served, "POST", "/api/reset", &Value::Null);
    assert_eq!(status, 200, "{answer}");
    let (_, answer) = call(
        &served,
        REVIEW,
        "probe",
        "probe.review.Submit",
        &json!({"submission_id": "s1", "amount": 5}),
    );
    assert_eq!(answer["outcome"], "flagged", "{answer}");
}

#[test]
fn served_decisions_file_naming_an_unknown_command_exits_before_listening() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let file = decisions(&state, "[\"probe.review.Nothing\"]\nflagged = true\n");
    let output = exited(REVIEW, &state, &["--decisions", &file]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("listening"));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("probe.review.Nothing"),
        "{output:?}"
    );
}

#[test]
fn served_answer_default_with_garbage_value_is_refused_by_the_parser() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let output = exited(REVIEW, &state, &["--answer-default=maybe"]);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
}

#[test]
fn served_cascaded_external_is_answered_from_the_decisions_file() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let file = decisions(&state, "[\"relay.audit.Audit\"]\nrejected = false\n");
    let served = serve(CASCADE, &state, &["--decisions", &file]);
    let (status, answer) = call(
        &served,
        CASCADE,
        "desk",
        "relay.desk.Take",
        &json!({"request_id": "r1"}),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    assert_eq!(
        events(&served),
        vec![
            "relay.desk.Taken".to_owned(),
            "relay.audit.Audited".to_owned()
        ]
    );
}

#[test]
fn served_unanswered_cascaded_external_is_reported_naming_the_outcome() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let file = decisions(&state, "");
    let served = serve(CASCADE, &state, &["--decisions", &file]);
    let (status, answer) = call(
        &served,
        CASCADE,
        "desk",
        "relay.desk.Take",
        &json!({"request_id": "r1"}),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    assert_eq!(events(&served), vec!["relay.desk.Taken".to_owned()]);
    let (status, answer) = call(
        &served,
        CASCADE,
        "desk",
        "relay.desk.Take",
        &json!({"request_id": "r2"}),
    );
    assert!(
        (200..300).contains(&status),
        "the server still answers after an unanswered cascade: {status} {answer}"
    );
    let stderr = served.stop();
    assert!(
        stderr.contains("external.relay.audit.Audit.rejected"),
        "the dropped invocation names the unanswered outcome: {stderr}"
    );
}

const CASCADE_RETRY: &str = "crates/spindle-cli/tests/fixtures/adversary-served-cascade-retry";

/// An unanswered external in a binding that retries: each attempt asks the decider again, and
/// the drop names all five attempts. Kills the mutant that opens the deciding host without the
/// run's `Retries`.
#[test]
fn served_unanswered_cascade_in_a_retried_binding_is_asked_on_every_attempt() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let file = decisions(&state, "");
    let served = serve(CASCADE_RETRY, &state, &["--decisions", &file]);
    let (status, answer) = call(
        &served,
        CASCADE_RETRY,
        "desk",
        "relay.desk.Take",
        &json!({"request_id": "r1"}),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    let stderr = served.stop();
    assert!(
        stderr.contains("dropped after 5 attempts")
            && stderr.contains("external.relay.audit.Audit.rejected"),
        "{stderr}"
    );
}

#[test]
fn served_drop_retries_holds_with_a_decider() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let file = decisions(&state, "");
    let served = serve(
        CASCADE_RETRY,
        &state,
        &["--decisions", &file, "--drop-retries"],
    );
    let (status, answer) = call(
        &served,
        CASCADE_RETRY,
        "desk",
        "relay.desk.Take",
        &json!({"request_id": "r1"}),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    let stderr = served.stop();
    assert!(
        stderr.contains("external.relay.audit.Audit.rejected") && !stderr.contains("attempts:"),
        "with --drop-retries the invocation is tried once: {stderr}"
    );
}

#[test]
fn served_cascade_without_flags_exits_before_listening() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let output = exited(CASCADE, &state, &[]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("external.relay.audit.Audit.rejected"),
        "{output:?}"
    );
}
