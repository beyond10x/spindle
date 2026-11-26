//! `run` reading JSON lines on stdin fires the event bindings of the specification, as a served
//! run routes them (story:stdin-run-fires-bindings): a line whose command emits an event a binding
//! maps prints its own answer, then the bound command's answer on a line of its own carrying
//! `"caused_by": {"line": <n>, "binding": "<id>"}`, on the run's one store. A bound command that
//! is refused or rejected fails the run after the last line; a binding the router refuses stops
//! the run before any line is read.
//!
//! The fixtures `tests/fixtures/stdin-bindings/*.yaml` declare `desk.tickets`: `OpenTicket`
//! emits `TicketOpened`, which the binding `file-ticket` (`on_failure: retry`) maps to
//! `FileTicket`; that declines a ticket with no copies with a declared error, and breaks the
//! filing's invariant (`copies <= 9`) beyond nine copies, which the kernel rejects.
//! `ArchiveFiling` answers `archived` only where a filing is stored. `in-process.yaml` and
//! `network.yaml` differ only in the component's `reached_by`; `implied.yaml` declares no
//! component and `escalate.yaml` a binding that escalates a failure.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};
use spindle_kernel::Store;

const IN_PROCESS: &str = "crates/spindle-cli/tests/fixtures/stdin-bindings/in-process.yaml";
const NETWORK: &str = "crates/spindle-cli/tests/fixtures/stdin-bindings/network.yaml";
const IMPLIED: &str = "crates/spindle-cli/tests/fixtures/stdin-bindings/implied.yaml";
const ESCALATE: &str = "crates/spindle-cli/tests/fixtures/stdin-bindings/escalate.yaml";
const MEMORY: &str = "crates/spindle-cli/tests/fixtures/memory.toml";
const LAPTOP: &str = "examples/environments/laptop.toml";

const OPEN_TICKET: &str = "desk.tickets.OpenTicket";
const FILE_TICKET: &str = "desk.tickets.FileTicket";
const ARCHIVE_FILING: &str = "desk.tickets.ArchiveFiling";
const BINDING: &str = "file-ticket";
const ENTITIES: [&str; 2] = ["desk.tickets.Ticket", "desk.tickets.Filing"];

/// How long a served `run` may take to admit, open its state and print where it listens.
const STARTUP: Duration = Duration::from_secs(30);
/// How long one request may take to be answered.
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(arguments: &[&str], lines: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    // A run that stops before it reads stdin closes the pipe; the write may then fail.
    let _ = child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(lines.as_bytes());
    child.wait_with_output().expect("b10x-spindle runs")
}

fn results(output: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("each result is one JSON line"))
        .collect()
}

fn open(ticket: &str, copies: i64) -> String {
    json!({"command": OPEN_TICKET, "input": {"ticket_id": ticket, "copies": copies}}).to_string()
}

fn archive(ticket: &str) -> String {
    json!({"command": ARCHIVE_FILING, "input": {"ticket_id": ticket}}).to_string()
}

fn lines(lines: &[String]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

/// The bound command's answer follows the line's own, names the line and the binding, and its
/// write is read by the next line.
#[test]
fn a_stdin_line_fires_the_binding_its_event_matches() {
    let output = run(
        &["run", IN_PROCESS, "--environment", MEMORY],
        &lines(&[open("t-1", 2), archive("t-1")]),
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 3, "{results:?}");
    assert_eq!(results[0]["command"], OPEN_TICKET);
    assert_eq!(results[0]["outcome"], "opened");
    assert!(results[0].get("caused_by").is_none(), "{}", results[0]);
    assert_eq!(results[1]["command"], FILE_TICKET);
    assert_eq!(results[1]["outcome"], "filed", "{}", results[1]);
    assert_eq!(
        results[1]["caused_by"],
        json!({"line": 1, "binding": BINDING})
    );
    assert_eq!(
        results[1]["events"],
        json!([{"event": "desk.tickets.TicketFiled", "payload": {"ticket_id": "t-1"}}])
    );
    assert_eq!(
        results[2]["outcome"], "archived",
        "the filing the binding wrote is read by the next line: {}",
        results[2]
    );
}

/// A bound command answered by a refusing outcome prints that outcome on its line, every line is
/// read, and the run exits 1, naming the binding and the line on stderr.
#[test]
fn a_refused_bound_command_is_printed_and_fails_the_run() {
    let output = run(
        &["run", IN_PROCESS, "--environment", MEMORY],
        &lines(&[open("t-2", 0), open("t-3", 1)]),
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 4, "every line is read: {results:?}");
    assert_eq!(results[0]["outcome"], "opened");
    assert_eq!(results[1]["command"], FILE_TICKET);
    assert_eq!(results[1]["outcome"], "declined", "{}", results[1]);
    assert_eq!(results[1]["error"]["error"], "desk.tickets.NothingToFile");
    assert_eq!(
        results[1]["caused_by"],
        json!({"line": 1, "binding": BINDING})
    );
    assert_eq!(results[2]["outcome"], "opened");
    assert_eq!(results[3]["outcome"], "filed");
    assert_eq!(results[3]["caused_by"]["line"], 2);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr
            .lines()
            .any(|line| line.contains("line 1") && line.contains(BINDING)),
        "stderr names line 1 and the binding: {stderr}"
    );
    assert!(
        !stderr.lines().any(|line| line.contains("line 2")),
        "line 2 and what it caused ran: {stderr}"
    );
}

/// A bound command the kernel rejects (no declared outcome) prints that failure on its line, and
/// the run exits 1 after reading every line.
#[test]
fn a_rejected_bound_command_is_printed_and_fails_the_run() {
    let output = run(
        &["run", IN_PROCESS, "--environment", MEMORY],
        &lines(&[open("t-4", 12), archive("t-4")]),
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 3, "every line is read: {results:?}");
    assert_eq!(results[0]["outcome"], "opened");
    assert_eq!(results[1]["command"], FILE_TICKET);
    assert!(results[1].get("outcome").is_none(), "{}", results[1]);
    assert_eq!(
        results[1]["failure"]["kind"], "broken_invariant",
        "{}",
        results[1]
    );
    assert_eq!(
        results[1]["caused_by"],
        json!({"line": 1, "binding": BINDING})
    );
    assert_eq!(
        results[2]["outcome"], "no-such-filing",
        "the rejected bound command wrote nothing: {}",
        results[2]
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr
            .lines()
            .any(|line| line.contains("line 1") && line.contains(BINDING)),
        "stderr names line 1 and the binding: {stderr}"
    );
}

/// A binding the served run's router refuses (`on_failure: escalate`) stops the run before any
/// line is read, naming the binding, as a served run refuses it.
#[test]
fn a_binding_the_router_refuses_stops_the_run_before_stdin() {
    let output = run(
        &["run", ESCALATE, "--environment", MEMORY],
        &lines(&[open("t-5", 1)]),
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        output.stdout.is_empty(),
        "no line was answered: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(BINDING),
        "stderr names the binding: {stderr}"
    );
}

/// `--drop-retries` is accepted on stdin: the retried binding's failing command runs once and is
/// not retried, where without the flag it runs five times.
#[test]
fn drop_retries_is_accepted_on_stdin() {
    let dropped = run(
        &["run", IN_PROCESS, "--environment", MEMORY, "--drop-retries"],
        &lines(&[open("t-6", 12)]),
    );
    assert_eq!(dropped.status.code(), Some(1), "{dropped:?}");
    let stderr = String::from_utf8_lossy(&dropped.stderr);
    assert!(
        stderr.contains(&format!(
            "binding {BINDING} retries on failure; --drop-retries"
        )),
        "stderr names the binding whose retries are dropped: {stderr}"
    );
    let results = results(&dropped);
    assert_eq!(results.len(), 2, "{results:?}");
    assert_eq!(results[1]["caused_by"]["binding"], BINDING);
    assert_eq!(results[1]["attempts"], 1, "{}", results[1]);

    let retried = run(
        &["run", IN_PROCESS, "--environment", MEMORY],
        &lines(&[open("t-6", 12)]),
    );
    let results = self::results(&retried);
    assert_eq!(results[1]["attempts"], 5, "{}", results[1]);
}

/// A specification that declares no binding prints one answer per line, as before.
#[test]
fn a_specification_without_bindings_prints_as_today() {
    let declare = r#"{"command": "spindle.deployment.DeclareEnvironment", "input": {"environment_id": "scratchpad", "os": "linux", "arch": "x86_64", "provides": []}}"#;
    let output = run(
        &["run", "ess", "--environment", MEMORY],
        &format!("{declare}\n"),
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(results[0]["outcome"], "declared");
    assert!(results[0].get("caused_by").is_none(), "{}", results[0]);
}

/// A specification that declares no component routes its bindings through the one implied
/// component.
#[test]
fn a_specification_without_components_fires_its_bindings_on_stdin() {
    let output = run(
        &["run", IMPLIED, "--environment", MEMORY],
        &lines(&[open("t-7", 3)]),
    );
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 2, "{results:?}");
    assert_eq!(results[1]["command"], FILE_TICKET);
    assert_eq!(results[1]["outcome"], "filed", "{}", results[1]);
    assert_eq!(
        results[1]["caused_by"],
        json!({"line": 1, "binding": BINDING})
    );
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

fn serve(state: &Path) -> Served {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", NETWORK, "--environment", LAPTOP, "--state"])
        .arg(state)
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
    Served { child, address }
}

/// The method and path ESS publishes for the command `id` of the served component.
fn route(id: &str) -> (String, String) {
    let loaded = spindle_spec::load(&root().join(NETWORK)).expect("the fixture compiles");
    let component = loaded
        .ir
        .components()
        .values()
        .next()
        .expect("the fixture declares a component");
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

/// One HTTP/1.1 exchange on its own connection: the status.
fn send(address: SocketAddr, line: &str) -> u16 {
    let line: Value = serde_json::from_str(line).expect("a command line");
    let (method, path) = route(line["command"].as_str().expect("a command"));
    let body = line["input"].to_string();
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
    let mut answer = String::new();
    stream
        .read_to_string(&mut answer)
        .expect("the answer is read before the timeout");
    answer
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {answer:?}"))
}

/// One stored row: its entity, identity, state and fields.
type StoredRow = (String, Value, String, Value);
/// One committed event: its name and payload.
type StoredEvent = (String, Value);

/// Every row of the fixture's entities, by entity, and every event, by name and payload, in the
/// SQLite store at `path`.
fn stored(path: &Path) -> (Vec<StoredRow>, Vec<StoredEvent>) {
    let mut store = spindle_kernel::SqliteStore::open(path).expect("the state opens");
    let mut rows = Vec::new();
    for entity in ENTITIES {
        for (identity, stored) in store.scan(entity).expect("the entity is read").rows {
            rows.push((
                entity.to_owned(),
                identity,
                stored.row.state,
                json!(stored.row.fields),
            ));
        }
    }
    let events = store
        .events_after(0, 1_000)
        .expect("the events are read")
        .into_iter()
        .map(|committed| (committed.event.name, json!(committed.event.payload)))
        .collect();
    (rows, events)
}

/// One sequence of commands, sent on stdin to the fixture run in process and over HTTP to the same
/// fixture reached over the network, leaves the same rows and the same events.
#[test]
fn a_stdin_run_and_a_served_run_fire_the_same_bindings() {
    let sequence = [
        open("t-1", 2),
        open("t-2", 0),
        open("t-3", 12),
        archive("t-1"),
    ];
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");

    let stdin_state = state.path().join("stdin.sqlite");
    let output = run(
        &[
            "run",
            IN_PROCESS,
            "--environment",
            LAPTOP,
            "--state",
            stdin_state.to_str().expect("a UTF-8 path"),
        ],
        &lines(&sequence),
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "two bound commands fail: {output:?}"
    );

    let served_state = state.path().join("served");
    {
        let served = serve(&served_state);
        for line in &sequence {
            let status = send(served.address, line);
            assert!((200..300).contains(&status), "{line}: {status}");
        }
    }

    let on_stdin = stored(&stdin_state);
    let served = stored(&served_state.join("front.sqlite"));
    assert_eq!(on_stdin.0, served.0, "the same rows");
    assert_eq!(on_stdin.1, served.1, "the same events");
    assert_eq!(
        on_stdin.0.len(),
        4,
        "three tickets and one filing: {:?}",
        on_stdin.0
    );
}
