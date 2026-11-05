//! "Nothing that ran before changes its answer without a flag": the same requests sent to a
//! `b10x-spindle` built from the unit's base commit (`SPINDLE_BASE_BIN`) and to this build, on
//! every fixture that ran before, compared answer by answer.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::Value;

const INGRESS: &str = "crates/spindle-cli/tests/fixtures/ingress";
const INGRESS_HTTP: &str = "crates/spindle-cli/tests/fixtures/ingress-http.yaml";
const RETRIED: &str = "crates/spindle-cli/tests/fixtures/retried";
const LAPTOP: &str = "examples/environments/laptop.toml";
const MEMORY: &str = "crates/spindle-cli/tests/fixtures/memory.toml";

const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(10);
const EXIT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(20);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn binaries() -> [(String, PathBuf); 2] {
    let base = std::env::var_os("SPINDLE_BASE_BIN").expect("SPINDLE_BASE_BIN names the base build");
    [
        ("base".to_owned(), PathBuf::from(base)),
        (
            "head".to_owned(),
            PathBuf::from(env!("CARGO_BIN_EXE_b10x-spindle")),
        ),
    ]
}

struct Killed(Child);

impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn exchange(address: SocketAddr, method: &str, path: &str, body: &str) -> (u16, Value) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .expect("the request is written");
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).expect("the answer is read");
    let answer = String::from_utf8_lossy(&answer).into_owned();
    let (head, body) = answer.split_once("\r\n\r\n").unwrap_or((&answer, ""));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or(0);
    let body = serde_json::from_str(body).unwrap_or(Value::String(body.to_owned()));
    (status, body)
}

/// Every answer of `requests` from `binary` serving `arguments`, or its exit status and stdout
/// when it does not listen.
fn answers(binary: &Path, arguments: &[&str], requests: &[(&str, &str, &str)]) -> Vec<String> {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let file = state.path().join("state");
    let mut all = arguments.to_vec();
    let file = file.to_str().expect("a UTF-8 path").to_owned();
    if !arguments.contains(&"--no-state") {
        all.extend(["--state", file.as_str()]);
    }
    all.retain(|argument| *argument != "--no-state");
    let mut child = Killed(
        Command::new(binary)
            .current_dir(root())
            .env("TMPDIR", state.path())
            .args(&all)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("b10x-spindle starts"),
    );
    let stdout = child.0.stdout.take().expect("stdout is piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let mut reader = BufReader::new(stdout);
        let _ = reader.read_line(&mut text);
        let _ = sender.send(text.clone());
        let mut rest = String::new();
        let _ = reader.read_to_string(&mut rest);
        let _ = sender.send(rest);
    });
    let line = receiver.recv_timeout(STARTUP).expect("a first line or EOF");
    let ready: Option<SocketAddr> = serde_json::from_str::<Value>(&line)
        .ok()
        .and_then(|ready| ready["listening"].as_str().and_then(|a| a.parse().ok()));
    let Some(address) = ready else {
        let deadline = Instant::now() + EXIT;
        let status = loop {
            if let Some(status) = child.0.try_wait().expect("waited on") {
                break status;
            }
            assert!(Instant::now() < deadline, "run neither listened nor exited");
            std::thread::sleep(POLL);
        };
        let rest = receiver.recv_timeout(EXIT).unwrap_or_default();
        return vec![format!("exit {:?}: {line}{rest}", status.code())];
    };
    requests
        .iter()
        .map(|(method, path, body)| {
            let (status, answer) = exchange(address, method, path, body);
            format!("{method} {path} {body} -> {status} {answer}")
        })
        .collect()
}

fn compare(arguments: &[&str], requests: &[(&str, &str, &str)]) {
    let [(_, base), (_, head)] = binaries();
    let before = answers(&base, arguments, requests);
    let after = answers(&head, arguments, requests);
    for (index, (before, after)) in before.iter().zip(&after).enumerate() {
        assert_eq!(before, after, "answer {index} of {arguments:?} changed");
    }
    assert_eq!(before.len(), after.len(), "{before:#?} {after:#?}");
}

const DERIVED: &[(&str, &str, &str)] = &[
    ("GET", "/api/events", ""),
    ("GET", "/api/model", ""),
    (
        "POST",
        "/board/commands/OpenTask",
        r#"{"task_id": "t1", "title": "a"}"#,
    ),
    (
        "POST",
        "/board/commands/OpenTask",
        r#"{"task_id": "t1", "title": "a"}"#,
    ),
    ("POST", "/board/commands/OpenTask", r#"{"task_id": "t2"}"#),
    (
        "POST",
        "/board/commands/OpenTask",
        r#"{"task_id": "t3", "title": "a", "x": 1}"#,
    ),
    ("POST", "/board/commands/OpenTask", "not json"),
    ("POST", "/board/commands/OpenTask", "[]"),
    ("POST", "/board/commands/OpenTask", ""),
    ("GET", "/board/commands/OpenTask", ""),
    ("POST", "/board/commands/CloseTask", r#"{"task_id": "t1"}"#),
    ("POST", "/board/commands/CloseTask", r#"{"task_id": "t1"}"#),
    (
        "POST",
        "/board/commands/CloseTask",
        r#"{"task_id": "nobody"}"#,
    ),
    ("GET", "/board/views/AllTasks", ""),
    ("GET", "/board/views/OpenTasks", ""),
    ("POST", "/board/views/AllTasks", ""),
    ("GET", "/nowhere", ""),
    ("GET", "/api/events", ""),
    ("POST", "/api/reset", ""),
    ("GET", "/api/reset", ""),
    ("GET", "/board/views/AllTasks", ""),
    ("GET", "/api/events", ""),
];

const BOUND: &[(&str, &str, &str)] = &[
    ("PUT", "/tasks/t1", r#"{"title": "a"}"#),
    ("PUT", "/tasks/t1", r#"{"title": "a"}"#),
    ("PUT", "/tasks/t2", r#"{"task_id": "t2", "title": "a"}"#),
    ("PUT", "/tasks/%+1", r#"{"title": "a"}"#),
    ("PUT", "/tasks/", r#"{"title": "a"}"#),
    ("POST", "/tasks/t1/close", ""),
    ("POST", "/tasks/t1/close", ""),
    ("POST", "/tasks/nobody/close", ""),
    ("GET", "/tasks", ""),
    ("DELETE", "/tasks", ""),
    ("GET", "/api/events", ""),
    ("POST", "/api/reset", ""),
    ("GET", "/tasks", ""),
];

#[test]
#[ignore = "needs SPINDLE_BASE_BIN, a b10x-spindle built from the unit's base commit"]
fn adversary_a_derived_surface_answers_as_the_base_did() {
    compare(&["run", INGRESS, "--environment", LAPTOP], DERIVED);
    compare(
        &["run", INGRESS, "--environment", MEMORY, "--no-state"],
        DERIVED,
    );
}

#[test]
#[ignore = "needs SPINDLE_BASE_BIN, a b10x-spindle built from the unit's base commit"]
fn adversary_a_bound_surface_answers_as_the_base_did() {
    compare(
        &[
            "run",
            INGRESS,
            "--environment",
            LAPTOP,
            "--http",
            INGRESS_HTTP,
        ],
        BOUND,
    );
}

#[test]
#[ignore = "needs SPINDLE_BASE_BIN, a b10x-spindle built from the unit's base commit"]
fn adversary_refusals_without_a_flag_exit_as_the_base_did() {
    compare(&["run", RETRIED, "--environment", LAPTOP], &[]);
    compare(&["run", INGRESS, "--environment", MEMORY], &[]);
}

#[test]
#[ignore = "needs SPINDLE_BASE_BIN, a b10x-spindle built from the unit's base commit"]
fn adversary_stdin_runs_answer_as_the_base_did() {
    let lines = concat!(
        r#"{"command": "spindle.deployment.OfferBundle", "input": {"deployment_id": "d1", "bundle": "sha256:0f", "environment_id": "laptop", "requires": []}}"#,
        "\n",
        r#"{"command": "spindle.deployment.DecideAdmission", "input": {"deployment_id": "d1", "bindings": [], "missing": []}, "external": {"refused": false}}"#,
        "\n",
        r#"{"command": "spindle.deployment.Nothing", "input": {}}"#,
        "\n",
        "not json\n",
    );
    let mut outputs = Vec::new();
    for (_, binary) in binaries() {
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
        let file = state.path().join("state.sqlite");
        let mut child = Killed(
            Command::new(&binary)
                .current_dir(root())
                .args([
                    "run",
                    "ess",
                    "--environment",
                    LAPTOP,
                    "--state",
                    file.to_str().expect("a UTF-8 path"),
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .expect("b10x-spindle starts"),
        );
        let mut stdin = child.0.stdin.take().expect("stdin is piped");
        stdin.write_all(lines.as_bytes()).expect("written");
        drop(stdin);
        let deadline = Instant::now() + EXIT;
        let status = loop {
            if let Some(status) = child.0.try_wait().expect("waited on") {
                break status;
            }
            assert!(Instant::now() < deadline, "run did not exit");
            std::thread::sleep(POLL);
        };
        let mut stdout = String::new();
        child
            .0
            .stdout
            .take()
            .expect("stdout")
            .read_to_string(&mut stdout)
            .expect("read");
        outputs.push(format!("{:?}\n{stdout}", status.code()));
    }
    assert_eq!(outputs[0], outputs[1]);
}
