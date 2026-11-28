//! A served run keeps each command a binding invoked whose last attempt was refused or did not
//! execute, and that its binding did not escalate, as a dead letter: `GET /api/dead-letters`
//! lists it without a field value, a restart on the same `--state` keeps it, and
//! `POST /api/dead-letters/replay` runs it once (story:dead-letters-kept-and-replayed).
//!
//! The fixture `crates/spindle-host/tests/fixtures/escalation`, its `orders` reached over the
//! network, ships each placed order through `ship-on-order`, which drops `cancelled`, escalates
//! `no-address` and `oversize` and retries `busy`; a decisions file picks the outcome per order.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const DEPOT: &str = "crates/spindle-host/tests/fixtures/escalation";
const LAPTOP: &str = "examples/environments/laptop.toml";
const SHIP: &str = "depot.shipping.Ship";
const SHIPPING: &str = "ship-on-order";

/// An order id no answer about a dead letter may hold: it is a value of the command's input.
const BUSY: &str = "order-busy-4711";
const CANCELLED: &str = "order-cancelled-4712";
const STUCK: &str = "order-stuck-4713";

/// How long a served `run` may take to admit, open its state and print where it listens.
const STARTUP: Duration = Duration::from_secs(30);
/// How long one request may take to be answered.
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn utf8(path: &Path) -> &str {
    path.to_str().expect("a UTF-8 path")
}

/// A copy of the fixture in `directory` whose `orders` is reached over the network.
fn depot(directory: &Path) -> PathBuf {
    let fixture = root().join(DEPOT);
    let copy = directory.join("depot");
    std::fs::create_dir_all(copy.join("domains")).expect("the copy's directory");
    for file in [
        "system.yaml",
        "domains/orders.yaml",
        "domains/shipping.yaml",
    ] {
        let mut text = std::fs::read_to_string(fixture.join(file)).expect("the fixture reads");
        if file == "system.yaml" {
            let (from, to) = (
                "        - depot.orders.OrderFlagged\n    reached_by: in_process",
                "        - depot.orders.OrderFlagged\n    reached_by: network",
            );
            assert!(text.contains(from), "{file} holds {from:?}");
            text = text.replace(from, to);
        }
        std::fs::write(copy.join(file), text).expect("the copy is written");
    }
    copy
}

/// A decisions file in `directory`: `Ship` is busy for [`BUSY`], finds [`CANCELLED`] cancelled
/// and [`STUCK`] without an address, and ships every other order.
fn decisions(directory: &Path) -> PathBuf {
    let mut text = String::from(
        "[\"depot.shipping.Ship\"]\ncancelled = false\nno-address = false\noversize = false\nbusy = false\n",
    );
    for (order, outcome) in [
        (BUSY, "busy"),
        (CANCELLED, "cancelled"),
        (STUCK, "no-address"),
    ] {
        text.push_str(&format!(
            "\n[[rules]]\ncommand = \"{SHIP}\"\noutcome = \"{outcome}\"\nanswer = true\nwhen = {{ \"input.order_id\" = \"{order}\" }}\n"
        ));
    }
    let path = directory.join("decisions.toml");
    std::fs::write(&path, text).expect("the decisions file is written");
    path
}

/// A running `b10x-spindle run` serving HTTP; stopped when dropped. Its stderr lines arrive on
/// `stderr`.
struct Served {
    child: Child,
    address: SocketAddr,
    stderr: mpsc::Receiver<String>,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Served {
    /// The first stderr line holding `text`, waiting at most [`ANSWER`] for it.
    fn line_holding(&self, text: &str) -> Option<String> {
        let deadline = Instant::now() + ANSWER;
        loop {
            let left = deadline.checked_duration_since(Instant::now())?;
            let line = self.stderr.recv_timeout(left).ok()?;
            if line.contains(text) {
                return Some(line);
            }
        }
    }
}

/// The scratch directory, the specification, its decisions file and a state directory in it.
struct Run {
    directory: tempfile::TempDir,
    spec: PathBuf,
    decisions: Option<PathBuf>,
}

impl Run {
    /// The fixture's copy, with [`decisions`].
    fn new() -> Self {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let spec = depot(directory.path());
        let decisions = Some(decisions(directory.path()));
        Self {
            directory,
            spec,
            decisions,
        }
    }

    /// The fixture `retried`: `pad`, served, and `index`, which the binding `index-note`
    /// (`on_failure: retry`) reaches with a command the kernel does not run.
    fn retried() -> Self {
        Self {
            directory: tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory"),
            spec: root().join("crates/spindle-cli/tests/fixtures/retried"),
            decisions: None,
        }
    }

    fn serve(&self) -> Served {
        let state = self.directory.path().join("state");
        let mut command = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"));
        command
            .current_dir(root())
            .args(["run", utf8(&self.spec), "--environment", LAPTOP]);
        if let Some(decisions) = &self.decisions {
            command.args(["--decisions", utf8(decisions)]);
        }
        let mut child = command
            .args(["--state", utf8(&state)])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("b10x-spindle starts");
        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");
        let (lines, received) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                if lines.send(line).is_err() {
                    break;
                }
            }
        });
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
            stderr: received,
        }
    }
}

/// One HTTP/1.1 exchange on its own connection: the status and the body as text.
fn exchange(address: SocketAddr, method: &str, path: &str, body: Option<&Value>) -> (u16, String) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    let body = body.map(Value::to_string).unwrap_or_default();
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
    let status = answer
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {answer:?}"));
    let body = answer
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_owned())
        .unwrap_or_default();
    (status, body)
}

fn json(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or_else(|error| panic!("{body:?} is not JSON: {error}"))
}

fn place(served: &Served, order: &str) {
    let (status, answer) = exchange(
        served.address,
        "POST",
        "/orders/commands/PlaceOrder",
        Some(&json!({"order_id": order})),
    );
    assert_eq!(status, 202, "{answer}");
}

/// The answer of `GET /api/dead-letters`: its body as text, and its list.
fn listed(served: &Served) -> (String, Vec<Value>) {
    let (status, body) = exchange(served.address, "GET", "/api/dead-letters", None);
    assert_eq!(status, 200, "{body}");
    let letters = json(&body)["dead_letters"]
        .as_array()
        .unwrap_or_else(|| panic!("a list of dead letters: {body}"))
        .clone();
    (body, letters)
}

fn replay(served: &Served, letter: &Value) -> (u16, String) {
    exchange(
        served.address,
        "POST",
        "/api/dead-letters/replay",
        Some(&json!({"component": letter["component"], "id": letter["id"]})),
    )
}

#[test]
fn an_exhausted_command_is_kept_as_a_dead_letter() {
    let run = Run::new();
    let served = run.serve();
    place(&served, BUSY);
    place(&served, CANCELLED);
    let (_, letters) = listed(&served);
    assert_eq!(letters.len(), 2, "{letters:?}");
    let busy = &letters[0];
    assert_eq!(busy["component"], "shipping", "{busy}");
    assert_eq!(busy["binding"], SHIPPING, "{busy}");
    assert_eq!(busy["event"], "depot.orders.OrderPlaced", "{busy}");
    assert_eq!(busy["command"], SHIP, "{busy}");
    assert_eq!(busy["outcome"], "busy", "{busy}");
    assert_eq!(busy["error"], "depot.shipping.Busy", "{busy}");
    assert_eq!(busy["attempts"], 5, "{busy}");
    assert!(busy["id"].is_u64(), "{busy}");
    assert_eq!(letters[1]["outcome"], "cancelled", "{}", letters[1]);
    assert_eq!(letters[1]["attempts"], 1, "{}", letters[1]);
}

#[test]
fn a_command_that_did_not_run_is_logged_as_kept() {
    let run = Run::retried();
    let served = run.serve();
    let (status, answer) = exchange(
        served.address,
        "POST",
        "/pad/commands/WriteNote",
        Some(&json!({"note_id": "n1", "text": "kept"})),
    );
    assert_eq!(status, 202, "{answer}");
    let line = served.line_holding("did not run").unwrap_or_default();
    let (_, letters) = listed(&served);
    assert_eq!(letters.len(), 1, "{letters:?}");
    assert_eq!(letters[0]["binding"], "index-note", "{letters:?}");
    assert!(letters[0].get("outcome").is_none(), "{letters:?}");
    assert!(
        line.contains(&format!("kept as dead letter {}", letters[0]["id"]))
            && !line.contains("dropped"),
        "the run says where the command went: {line:?}"
    );
}

#[test]
fn a_dead_letter_lists_no_field_value() {
    let run = Run::new();
    let served = run.serve();
    place(&served, BUSY);
    let (body, letters) = listed(&served);
    assert_eq!(letters.len(), 1, "{body}");
    assert!(
        !body.contains(BUSY),
        "the list names no value of the command's input: {body}"
    );
    assert!(letters[0].get("input").is_none(), "{body}");
}

#[test]
fn a_dead_letter_survives_a_restart() {
    let run = Run::new();
    let before = {
        let served = run.serve();
        place(&served, BUSY);
        listed(&served).1
    };
    assert_eq!(before.len(), 1, "{before:?}");
    let served = run.serve();
    assert_eq!(listed(&served).1, before);
}

#[test]
fn a_replay_runs_the_command_once() {
    let run = Run::new();
    let served = run.serve();
    place(&served, BUSY);
    let (_, letters) = listed(&served);
    let (status, answer) = replay(&served, &letters[0]);
    assert_eq!(status, 200, "{answer}");
    let answer = json(&answer);
    assert_eq!(answer["result"], "refused", "{answer}");
    assert_eq!(answer["outcome"], "busy", "{answer}");
    let (_, again) = listed(&served);
    assert_eq!(
        again.len(),
        1,
        "the refused replay is kept again: {again:?}"
    );
    assert_eq!(again[0]["id"], answer["dead_letter"], "{answer}");
    assert_ne!(again[0]["id"], letters[0]["id"]);
    assert_eq!(
        again[0]["attempts"], 1,
        "a replay runs once, without the binding's retries: {again:?}"
    );
}

#[test]
fn a_replayed_dead_letter_is_not_replayed_again() {
    let run = Run::new();
    let served = run.serve();
    place(&served, BUSY);
    let (_, letters) = listed(&served);
    let (status, answer) = replay(&served, &letters[0]);
    assert_eq!(status, 200, "{answer}");
    let (status, answer) = replay(&served, &letters[0]);
    assert_eq!(status, 404, "{answer}");
    assert!(json(&answer)["refused"].is_string(), "{answer}");
    let (status, answer) = exchange(
        served.address,
        "POST",
        "/api/dead-letters/replay",
        Some(&json!({"component": "shipping"})),
    );
    assert_eq!(status, 400, "a replay names its letter's id: {answer}");
}

#[test]
fn an_escalated_command_is_no_dead_letter() {
    let run = Run::new();
    let served = run.serve();
    place(&served, STUCK);
    let (body, letters) = listed(&served);
    assert!(letters.is_empty(), "{body}");
}
