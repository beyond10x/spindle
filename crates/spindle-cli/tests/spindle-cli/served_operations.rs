//! `run` answering `GET /api/served` (`story:served-operations-route`): one row per command and
//! view it routes, whether it serves it and, for one it does not, the reason its start log prints;
//! and `admit` printing, without serving, the same line for each view `run` does not serve.
//!
//! The fixture `crates/spindle-ingress/tests/fixtures/served-operations` declares one command that
//! runs and one the kernel does not run, three views the host reads (one an aggregation) and one
//! view of each construct it does not read: a filter, a computed field, paging and a default row
//! for an absent identity. `ess/fixtures/observe/served/notes.json` is the answer `run` gives for it.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const SERVED: &str = "crates/spindle-ingress/tests/fixtures/served-operations";
const LAPTOP: &str = "examples/environments/laptop.toml";
const ANSWERED: &str = "ess/fixtures/observe/served/notes.json";
const SCHEMA: &str =
    "ess/generated/jsonschema/schema/types/spindle.observe.ServedOperations.schema.json";

/// How long `run` may take to admit, open its state and print where it listens.
const STARTUP: Duration = Duration::from_secs(60);
/// How long one request, or one wait for a stderr line, may take.
const ANSWER: Duration = Duration::from_secs(10);
/// How often a wait looks again.
const POLL: Duration = Duration::from_millis(20);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every line `run` wrote on stderr so far, read on a thread of its own.
#[derive(Clone, Default)]
struct Stderr(Arc<Mutex<Vec<String>>>);

impl Stderr {
    fn read(child: &mut Child) -> Self {
        let lines = Self::default();
        let stderr = child.stderr.take().expect("stderr is piped");
        let filled = lines.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                filled.0.lock().expect("the stderr lines").push(line);
            }
        });
        lines
    }

    /// The lines once one equals `last`, waiting at most [`ANSWER`].
    fn until(&self, last: &str) -> Vec<String> {
        let deadline = Instant::now() + ANSWER;
        loop {
            let lines = self.0.lock().expect("the stderr lines").clone();
            if lines.iter().any(|line| line == last) {
                return lines;
            }
            assert!(
                Instant::now() < deadline,
                "no stderr line is {last:?} within {ANSWER:?}: {lines:#?}"
            );
            std::thread::sleep(POLL);
        }
    }
}

/// A running `b10x-spindle run` serving HTTP; stopped when dropped.
struct Served {
    child: Child,
    address: SocketAddr,
    stderr: Stderr,
    _state: tempfile::TempDir,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn serve(spec: &str) -> Served {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .env("TMPDIR", state.path())
        .args(["run", spec, "--environment", LAPTOP, "--state"])
        .arg(state.path().join("state"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let stderr = Stderr::read(&mut child);
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
        stderr,
        _state: state,
    }
}

/// `GET <path>` on its own connection: the status and the JSON body.
fn get(address: SocketAddr, path: &str) -> (u16, Value) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
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

/// The start-log line of each row of `answered` not served, in order.
fn lines(answered: &Value) -> Vec<String> {
    answered["operations"]
        .as_array()
        .unwrap_or_else(|| panic!("the answer lists operations: {answered}"))
        .iter()
        .filter(|row| row["served"] == false)
        .map(|row| {
            format!(
                "b10x-spindle: not served: {} `{}` at {} {}: {}",
                row["kind"].as_str().expect("a kind"),
                row["name"].as_str().expect("a name"),
                row["method"].as_str().expect("a method"),
                row["path"].as_str().expect("a path"),
                row["reason"].as_str().expect("a reason where not served"),
            )
        })
        .collect()
}

/// The lines of `stderr` that name an operation as not served, in order.
fn not_served(stderr: &[String]) -> Vec<String> {
    stderr
        .iter()
        .filter(|line| line.starts_with("b10x-spindle: not served: "))
        .cloned()
        .collect()
}

#[test]
fn served_rows_equal_the_start_logs_not_served_lines() {
    let served = serve(SERVED);
    let (status, answered) = get(served.address, "/api/served");
    assert_eq!(status, 200, "{answered}");
    let listed = lines(&answered);
    assert_eq!(listed.len(), 5, "one command and four views: {listed:#?}");
    let logged = not_served(&served.stderr.until(&listed[listed.len() - 1]));
    assert_eq!(listed, logged, "{answered:#}");
    let rows = answered["operations"].as_array().expect("rows");
    assert_eq!(rows.len(), 9, "two commands and seven views: {answered:#}");
}

#[test]
fn the_served_answer_reads_as_its_specified_type() {
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join(SCHEMA)).expect("the schema is read"),
    )
    .expect("the schema is JSON");
    let validator = jsonschema::draft202012::new(&schema).expect("the schema compiles");
    assert!(
        !validator.is_valid(&json!({"operations": [{"name": "notes.pad.AllNotes"}]})),
        "the schema refuses a row without its fields, so it checks something"
    );
    let served = serve(SERVED);
    let (status, answered) = get(served.address, "/api/served");
    assert_eq!(status, 200, "{answered}");
    let errors = validator
        .iter_errors(&answered)
        .map(|error| format!("{error} at {}", error.instance_path()))
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "{answered}: {errors:?}");
    let typed: spindle_observe_api::ServedOperations = serde_json::from_value(answered.clone())
        .unwrap_or_else(|error| panic!("{error}: {answered}"));
    assert_eq!(typed.operations.len(), 9, "{answered}");
    let fixture: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join(ANSWERED)).expect("the fixture is read"),
    )
    .expect("the fixture is JSON");
    assert_eq!(
        answered, fixture,
        "the observe fixture is the answer run gives"
    );
}

#[test]
fn admit_names_each_view_run_does_not_serve() {
    let admitted = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["admit", SERVED, "--environment", LAPTOP])
        .output()
        .expect("b10x-spindle runs");
    assert_eq!(admitted.status.code(), Some(0), "{admitted:?}");
    let printed: Vec<String> = String::from_utf8_lossy(&admitted.stderr)
        .lines()
        .map(str::to_owned)
        .collect();
    let views: Vec<String> = not_served(&printed)
        .into_iter()
        .filter(|line| line.starts_with("b10x-spindle: not served: view "))
        .collect();

    let served = serve(SERVED);
    let (status, answered) = get(served.address, "/api/served");
    assert_eq!(status, 200, "{answered}");
    let listed = lines(&answered);
    let logged = not_served(&served.stderr.until(&listed[listed.len() - 1]));
    let run_views: Vec<String> = logged
        .into_iter()
        .filter(|line| line.starts_with("b10x-spindle: not served: view "))
        .collect();
    assert_eq!(run_views.len(), 4, "{run_views:#?}");
    assert_eq!(
        views, run_views,
        "admit prints each view run does not serve, in the same words: {printed:#?}"
    );
}
