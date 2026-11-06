//! `run` serving `ingress.network` with `--decisions` and `--answer-default`: a request's external
//! causes are answered from the decisions file, then by each command's default branch, as a stdin
//! line's are when the line carries no `external` object.
//!
//! The fixture `tests/fixtures/served-review` declares one component, `probe`, reached over the
//! network: `probe.review.Submit`, whose two external outcomes sit beside a default branch, and
//! `probe.review.Decide`, whose external outcome has no default beside it. Every path and method is
//! read from the `OpenAPI` document ESS projects for that component.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/served-review";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "probe";

const SUBMIT: &str = "probe.review.Submit";
const DECIDE: &str = "probe.review.Decide";

/// How long `run` may take to admit, open its state and print where it listens.
const STARTUP: Duration = Duration::from_secs(60);
/// How long one request may take to be answered.
const ANSWER: Duration = Duration::from_secs(10);
/// How long a refused `run` may take to exit on its own.
const EXIT: Duration = Duration::from_secs(60);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// ESS's `OpenAPI` projection of the served component.
fn contract() -> Value {
    let loaded = spindle_spec::load(&root().join(SPEC)).expect("the fixture compiles");
    let component = loaded
        .ir
        .components()
        .values()
        .find(|component| component.name.as_str() == COMPONENT)
        .expect("the fixture declares the component");
    serde_json::from_str(&ess_gen::openapi::json(&loaded.ir, component))
        .expect("the projection is JSON")
}

/// Whether the contract declares an answer of operation `id` under `status`.
fn declares(id: &str, status: u16) -> bool {
    let (method, path) = route(id);
    !contract()["paths"][&path][method.to_lowercase()]["responses"][status.to_string()].is_null()
}

/// The method and path the contract publishes for `id`.
fn route(id: &str) -> (String, String) {
    let document = contract();
    let paths = document["paths"]
        .as_object()
        .expect("the document has paths");
    for (path, item) in paths {
        for (method, operation) in item.as_object().expect("a path item is an object") {
            if operation["operationId"] == id {
                return (method.to_uppercase(), path.clone());
            }
        }
    }
    panic!("the contract declares no operation {id}")
}

/// Writes `text` as a decisions file in `directory` and returns its path.
fn decisions(directory: &tempfile::TempDir, text: &str) -> String {
    let path = directory.path().join("decisions.toml");
    std::fs::write(&path, text).expect("the decisions file is written");
    path.to_str().expect("a UTF-8 path").to_owned()
}

/// A running `b10x-spindle run` serving HTTP; stopped when dropped.
struct Served {
    child: Child,
    address: SocketAddr,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn command(state: &tempfile::TempDir, arguments: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"));
    command
        .current_dir(root())
        .args(["run", SPEC, "--environment", LAPTOP, "--state"])
        .arg(state.path().join("state"))
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped());
    command
}

/// `run` on the fixture and laptop with `arguments` added, once it prints where it listens.
fn serve(state: &tempfile::TempDir, arguments: &[&str]) -> Served {
    let mut child = command(state, arguments)
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

/// `run` on the fixture and laptop with `arguments` added, which is expected to exit on its own.
fn refused(state: &tempfile::TempDir, arguments: &[&str]) -> Output {
    let mut child = command(state, arguments)
        .stderr(Stdio::piped())
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

/// Calls the operation `id` with `body`, on its own connection: the status and the JSON body.
fn call(served: &Served, id: &str, body: &Value) -> (u16, Value) {
    let (method, path) = route(id);
    let mut stream =
        TcpStream::connect_timeout(&served.address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    stream
        .set_write_timeout(Some(ANSWER))
        .expect("a write timeout");
    let body = body.to_string();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        served.address,
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

#[test]
fn a_served_command_is_answered_from_the_decisions_file() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let file = decisions(
        &state,
        "escalated = false\nvetoed = false\n\n[\"probe.review.Submit\"]\nflagged = true\n",
    );
    let served = serve(&state, &["--decisions", &file]);

    let (status, answer) = call(
        &served,
        SUBMIT,
        &json!({"submission_id": "s1", "amount": 5}),
    );
    assert!(!(200..300).contains(&status), "{status} {answer}");
    assert!(
        declares(SUBMIT, status),
        "the contract declares {status} for {SUBMIT}"
    );
    assert_eq!(answer["outcome"], "flagged", "{answer}");
    assert_eq!(answer["error"], "probe.review.Flagged", "{answer}");
    assert_eq!(answer["published"], json!([]), "{answer}");

    let (status, answer) = call(
        &served,
        DECIDE,
        &json!({"decision_id": "d1", "approve": true}),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    assert_eq!(answer["outcome"], "approved", "{answer}");
    assert_eq!(
        answer["published"],
        json!([{"event": "probe.review.Approved", "payload": {"decision_id": "d1"}}])
    );
}

#[test]
fn a_served_command_takes_the_default_with_answer_default() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let served = serve(&state, &["--answer-default"]);

    let (status, answer) = call(
        &served,
        SUBMIT,
        &json!({"submission_id": "s1", "amount": 5}),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    assert_eq!(answer["outcome"], "submitted", "{answer}");
    assert_eq!(
        answer["published"],
        json!([{"event": "probe.review.Submitted", "payload": {"submission_id": "s1"}}])
    );

    let (status, answer) = call(
        &served,
        DECIDE,
        &json!({"decision_id": "d1", "approve": true}),
    );
    assert_eq!(status, 501, "{answer}");
    assert_eq!(answer["committed"], false, "{answer}");
    let refused = answer["refused"].as_str().unwrap_or_default();
    assert!(
        refused.contains("external.probe.review.Decide.vetoed")
            && refused.contains("no default branch"),
        "a command with no default branch is not answered, naming the outcome: {answer}"
    );
}

#[test]
fn a_served_run_without_decisions_answers_as_today() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let output = refused(&state, &[]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("listening"), "nothing listened: {stdout}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("external.probe.review.") && stderr.contains("answers no external cause"),
        "without --decisions or --answer-default the host answers no external cause: {stderr}"
    );
}
