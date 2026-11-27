//! Adversary cases for `story:structured-json-logs` (wave 28, unit U2): `run --log-format json`
//! read against its own help text, which says that a line carries `component`, `command`, ...
//! "where the line names them", that `time` is the instant the line was written, and that the
//! error a run exits with is a line too; and the text a refused binding keeps without the flag.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant, SystemTime};

use serde_json::{Map, Value, json};

/// Writes notes over the network; each written note drops one bound command after 5 attempts.
const RETRIED: &str = "crates/spindle-cli/tests/fixtures/retried";
/// One of its commands the kernel does not run: the run reports it before listening.
const PARTIAL: &str = "crates/spindle-cli/tests/fixtures/partial";
/// A fixture; `REFUSED_HTTP` binds a command it does not declare, so ESS refuses the binding.
const RESERVED: &str = "crates/spindle-cli/tests/fixtures/reserved";
const REFUSED_HTTP: &str = "crates/spindle-cli/tests/fixtures/adversary-w28-u2-refused-http.yaml";
const LAPTOP: &str = "examples/environments/laptop.toml";
const INDEX_NOTE: &str = "notes.index.IndexNote";
const WRITE_NOTE: &str = "notes.pad.WriteNote";

const STARTUP: Duration = Duration::from_secs(30);
const ANSWER: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(20);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory")
}

/// `line` as one JSON object, or a failed case naming it.
fn object(line: &str) -> Map<String, Value> {
    match serde_json::from_str::<Value>(line) {
        Ok(Value::Object(object)) => object,
        _ => panic!("a stderr line that is not one JSON object: {line:?}"),
    }
}

/// `b10x-spindle run <spec> <arguments>` run to its exit, stdin empty, its `TMPDIR` a scratch
/// directory; at most [`STARTUP`].
fn exited(spec: &str, arguments: &[&str]) -> Output {
    let directory = scratch();
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .env("TMPDIR", directory.path())
        .arg("run")
        .arg(spec)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let deadline = Instant::now() + STARTUP;
    while child.try_wait().expect("waited on").is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("run did not exit within {STARTUP:?}");
        }
        std::thread::sleep(POLL);
    }
    child.wait_with_output().expect("the output is read")
}

/// Every stderr line written so far, read on a thread of its own.
#[derive(Clone, Default)]
struct Stderr(Arc<Mutex<Vec<String>>>);

impl Stderr {
    fn lines(&self) -> Vec<String> {
        self.0.lock().expect("the stderr lines").clone()
    }

    fn until(&self, needle: &str) -> String {
        let deadline = Instant::now() + ANSWER;
        loop {
            if let Some(line) = self.lines().into_iter().find(|line| line.contains(needle)) {
                return line;
            }
            assert!(
                Instant::now() < deadline,
                "no stderr line holds {needle:?} within {ANSWER:?}: {:#?}",
                self.lines()
            );
            std::thread::sleep(POLL);
        }
    }
}

/// A served run, killed when dropped.
struct Served {
    child: Child,
    address: SocketAddr,
    stderr: Stderr,
    _scratch: tempfile::TempDir,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `b10x-spindle run <spec> --environment laptop <arguments>` once it prints where it listens.
fn serve(spec: &str, arguments: &[&str]) -> Served {
    let directory = scratch();
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .env("TMPDIR", directory.path())
        .args(["run", spec, "--environment", LAPTOP])
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let stderr = Stderr::default();
    let filled = stderr.clone();
    let pipe = child.stderr.take().expect("stderr is piped");
    std::thread::spawn(move || {
        for line in BufReader::new(pipe).lines() {
            let Ok(line) = line else { break };
            filled.0.lock().expect("the stderr lines").push(line);
        }
    });
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
        panic!("run's first line {line:?} is not JSON ({error})")
    });
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("run's first line names no address: {ready}"));
    Served {
        child,
        address,
        stderr,
        _scratch: directory,
    }
}

/// Writes the note `id` on the retried fixture: the status.
fn write_note(address: SocketAddr, id: &str) -> u16 {
    let loaded = spindle_spec::load(&root().join(RETRIED)).expect("the fixture compiles");
    let component = loaded
        .ir
        .components()
        .values()
        .find(|component| component.name.as_str() == "pad")
        .expect("the fixture declares pad");
    let document: Value = serde_json::from_str(&ess_gen::openapi::json(&loaded.ir, component))
        .expect("the projection is JSON");
    let (method, path) = document["paths"]
        .as_object()
        .expect("paths")
        .iter()
        .find_map(|(path, item)| {
            item.as_object()?.iter().find_map(|(method, operation)| {
                (operation["operationId"] == WRITE_NOTE)
                    .then(|| (method.to_uppercase(), path.clone()))
            })
        })
        .expect("the contract declares WriteNote");
    let body = json!({"note_id": id, "text": "kept"}).to_string();
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream.set_read_timeout(Some(ANSWER)).expect("a timeout");
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .expect("the request is written");
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).expect("the answer is read");
    String::from_utf8_lossy(&answer)
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .expect("a status line")
}

/// The help says `time` is "the instant it was written": every line's `time` lies between the
/// start of the run and the moment the test read it, not at a fixed instant such as the epoch
/// the writer falls back to.
#[test]
fn time_is_the_instant_each_line_was_written() {
    let before = SystemTime::now() - Duration::from_secs(1);
    let served = serve(RETRIED, &["--log-format", "json"]);
    assert_eq!(write_note(served.address, "n1"), 202);
    served.stderr.until(INDEX_NOTE);
    let after = SystemTime::now() + Duration::from_secs(1);
    let lines = served.stderr.lines();
    assert!(
        lines.len() >= 2,
        "the state line and the drop line: {lines:#?}"
    );
    for line in &lines {
        let object = object(line);
        let time = object["time"].as_str().expect("time is a string");
        let at = spindle_cli::cli::instant(time).expect("time is RFC 3339");
        assert!(
            before <= at && at <= after,
            "time {time} is not the instant the line was written: {line}"
        );
    }
}

/// The help says a line carries `command` "where the line names" it. The lines a run prints
/// before listening for an operation it does not serve, or an outcome it refuses, name the
/// operation (`not served: <operation> at ...`, ``unsupported outcome `<operation>/<outcome>` ``):
/// each carries it as `command`, so a collector need not parse the message for it.
#[test]
fn a_report_line_names_its_command_in_a_field() {
    let served = serve(PARTIAL, &["--log-format", "json"]);
    // Every report line is written before the line naming where the run listens; give the
    // stderr reader a moment to catch up with it.
    served.stderr.until("\"message\":\"");
    std::thread::sleep(Duration::from_millis(200));
    let lines = served.stderr.lines();
    let reported: Vec<Map<String, Value>> = lines
        .iter()
        .map(|line| object(line))
        .filter(|object| {
            object["message"].as_str().is_some_and(|message| {
                message.starts_with("not served: ") || message.starts_with("unsupported outcome ")
            })
        })
        .collect();
    assert!(
        !reported.is_empty(),
        "the partial fixture reports an operation it does not run: {lines:#?}"
    );
    for object in &reported {
        let message = object["message"].as_str().unwrap_or_default();
        let command = object.get("command").and_then(Value::as_str);
        assert!(
            command.is_some_and(|command| message.contains(command)),
            "the line names an operation and carries no `command` field naming it: {object:?}"
        );
    }
}

/// The help says the error a run exits 1 with is a line too: in JSON it is one object at
/// `error`, its message without the text prefix.
#[test]
fn the_error_a_served_run_exits_with_is_one_json_line() {
    let directory = scratch();
    let secret = directory.path().join("secret");
    std::fs::write(&secret, "s3cret-value-long-enough\n").expect("the secret is written");
    let output = exited(
        RETRIED,
        &[
            "--environment",
            LAPTOP,
            "--trusted-caller-secret",
            secret.to_str().expect("a UTF-8 path"),
            "--log-format",
            "json",
        ],
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(lines.len(), 1, "one line: {stderr}");
    let line = object(lines[0]);
    assert_eq!(line["level"], "error", "{line:?}");
    let message = line["message"].as_str().expect("a message");
    assert!(
        message.starts_with("--trusted-caller-secret ")
            && message.contains("without --trusted-caller-header"),
        "{message}"
    );
}

/// The refusal of a non-loopback address without a proxy, exit 2, is one JSON object at `error`.
#[test]
fn the_non_loopback_refusal_is_one_json_line() {
    let output = exited(
        RETRIED,
        &[
            "--environment",
            LAPTOP,
            "--address",
            "0.0.0.0",
            "--log-format",
            "json",
        ],
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(lines.len(), 1, "one line: {stderr}");
    let line = object(lines[0]);
    assert_eq!(line["level"], "error", "{line:?}");
    assert!(
        line["message"]
            .as_str()
            .is_some_and(|message| message.starts_with("--address 0.0.0.0 is not loopback")),
        "{line:?}"
    );
}

/// A binding ESS or the ingress refuses is printed, in text, as it always was: its first line
/// starts with the binding's path, never with `b10x-spindle: `. In JSON its several lines are
/// one object whose message keeps them.
#[test]
fn a_refused_binding_keeps_its_text_and_is_one_json_object() {
    let text = exited(RESERVED, &["--environment", LAPTOP, "--http", REFUSED_HTTP]);
    let stderr = String::from_utf8_lossy(&text.stderr);
    assert_eq!(text.status.code(), Some(1), "{text:?}");
    assert!(
        stderr.lines().any(|line| line.starts_with(REFUSED_HTTP)),
        "the refusal starts with the binding's path: {stderr}"
    );
    assert!(
        !stderr.contains(&format!("b10x-spindle: {REFUSED_HTTP}")),
        "the refusal never carried the prefix: {stderr}"
    );

    let json = exited(
        RESERVED,
        &[
            "--environment",
            LAPTOP,
            "--http",
            REFUSED_HTTP,
            "--log-format",
            "json",
        ],
    );
    let stderr = String::from_utf8_lossy(&json.stderr);
    assert_eq!(json.status.code(), Some(1), "{json:?}");
    let objects: Vec<Map<String, Value>> = stderr.lines().map(object).collect();
    let refusal = objects
        .iter()
        .find(|object| {
            object["message"]
                .as_str()
                .is_some_and(|message| message.starts_with(REFUSED_HTTP))
        })
        .unwrap_or_else(|| panic!("no object holds the refusal: {stderr}"));
    assert_eq!(refusal["level"], "error", "{refusal:?}");
    let message = refusal["message"].as_str().unwrap_or_default();
    assert!(
        message.contains('\n') && message.contains("NoSuchCommand"),
        "the refusal's lines are kept in one message: {message:?}"
    );
}
