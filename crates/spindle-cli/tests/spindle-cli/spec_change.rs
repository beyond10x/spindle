//! `story:spec-change-over-stored-rows`: `run` on a durable store an earlier specification wrote
//! starts where every stored row stays readable, and otherwise exits 3 before listening, naming
//! each entity and field on stdout, the rows and the store's record left as they were.
//!
//! `tests/fixtures/spec-change-before` keeps books under a title and cards under a code;
//! `tests/fixtures/spec-change-after` adds an optional note to a book, which fits every stored
//! book, and drops the card's code for a label, which no stored card fits.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use spindle_kernel::{Shapes, SqliteStore, Store};

const BEFORE: &str = "crates/spindle-cli/tests/fixtures/spec-change-before";
const AFTER: &str = "crates/spindle-cli/tests/fixtures/spec-change-after";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "archive";
const CARD: &str = "archive.shelf.Card";

const STARTUP: Duration = Duration::from_secs(30);
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ir(spec: &str) -> spindle_spec::EssIr {
    spindle_spec::load(&root().join(spec))
        .unwrap_or_else(|error| panic!("{spec} compiles: {error}"))
        .ir
}

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

/// What a run printed before it exited without listening.
#[derive(Debug)]
struct Refused {
    status: ExitStatus,
    stdout: String,
}

/// `run` serving `spec` with its state in `state`: listening, or what it printed once it exited
/// first.
fn started(spec: &str, state: &Path) -> Result<Served, Refused> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", spec, "--environment", LAPTOP, "--state"])
        .arg(state)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("b10x-spindle starts");
    let stdout = child.stdout.take().expect("stdout is piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap_or(0) > 0 {
            text.push_str(&line);
            if line.contains("\"listening\"") {
                let _ = sender.send(text.clone());
            }
            line.clear();
        }
        let _ = sender.send(text);
    });
    let printed = receiver.recv_timeout(STARTUP).unwrap_or_default();
    if let Some(address) = printed.lines().find_map(|line| {
        serde_json::from_str::<Value>(line)
            .ok()
            .and_then(|ready| ready["listening"].as_str().map(str::to_owned))
            .and_then(|address| address.parse().ok())
    }) {
        return Ok(Served { child, address });
    }
    let deadline = Instant::now() + STARTUP;
    let status = loop {
        if let Some(status) = child.try_wait().expect("the run is waited on") {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "run printed {printed:?} and neither listened nor exited"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    Err(Refused {
        status,
        stdout: printed,
    })
}

fn listening(spec: &str, state: &Path) -> Served {
    started(spec, state).unwrap_or_else(|refused| panic!("run exited first: {refused:?}"))
}

fn call(address: SocketAddr, method: &str, path: &str, body: Option<&Value>) -> (u16, Value) {
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
        .expect("the answer is read");
    let (head, text) = answer
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("an answer with a head: {answer:?}"));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {head:?}"));
    let body = serde_json::from_str(text).unwrap_or(Value::Null);
    (status, body)
}

/// Sends `operation` of [`BEFORE`] or [`AFTER`] on its derived route.
fn send(served: &Served, spec: &str, operation: &str, body: Option<&Value>) -> (u16, Value) {
    let ir = ir(spec);
    let surface = spindle_ingress::Surface::new(&ir, &[COMPONENT.to_owned()], None)
        .expect("the fixture's surface is routed");
    let route = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == operation)
        .unwrap_or_else(|| panic!("a route for {operation}"));
    call(
        served.address,
        &route.method.as_str().to_uppercase(),
        &route.path,
        body,
    )
}

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory")
}

/// The card `c1` filed under the code `X1`, and the record of [`BEFORE`], in the store at `file`:
/// what a refused run leaves.
fn unchanged(file: &Path, recorded: &Shapes) {
    let mut store = SqliteStore::open(file).expect("the store opens");
    assert_eq!(
        store.recorded().expect("the record reads").as_ref(),
        Some(recorded),
        "the record is unchanged"
    );
    let cards: Vec<(Value, Value)> = store
        .scan(CARD)
        .expect("the cards are read")
        .into_iter()
        .map(|(identity, stored)| (identity, json!(stored.row.fields)))
        .collect();
    assert_eq!(cards, vec![(json!("c1"), json!({"code": "X1"}))]);
}

/// Asserts `refused` is a misfit's exit 3, naming the card and its code on stdout, never a
/// listening line or an admission refusal.
fn names_the_card_code(refused: &Refused) {
    assert_eq!(refused.status.code(), Some(3), "{refused:?}");
    assert!(
        refused.stdout.contains(&format!("{CARD}.code")),
        "stdout names the entity and the field: {:?}",
        refused.stdout
    );
    assert!(
        !refused.stdout.contains("listening") && !refused.stdout.contains("refused on"),
        "nothing else is printed: {:?}",
        refused.stdout
    );
}

#[test]
fn a_compatible_change_starts_on_the_old_store() {
    let scratch = scratch();
    let state = scratch.path().join("state");
    let first = listening(BEFORE, &state);
    let (status, answer) = send(
        &first,
        BEFORE,
        "archive.shelf.ShelveBook",
        Some(&json!({"id": "b1", "title": "Dune"})),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    drop(first);
    let second = listening(AFTER, &state);
    let (status, answer) = send(&second, AFTER, "archive.shelf.ShelvedBooks", None);
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["rows"], json!([{"id": "b1", "title": "Dune"}]));
    let (status, events) = call(second.address, "GET", "/api/events", None);
    assert_eq!(status, 200, "{events}");
    let names: Vec<&str> = events["events"]
        .as_array()
        .expect("a list of events")
        .iter()
        .filter_map(|event| event["event"].as_str())
        .collect();
    assert_eq!(
        names,
        vec!["archive.shelf.BookShelved"],
        "the record is no published event: {events}"
    );
}

#[test]
fn an_incompatible_change_exits_before_listening() {
    let scratch = scratch();
    let state = scratch.path().join("state");
    let first = listening(BEFORE, &state);
    let (status, answer) = send(
        &first,
        BEFORE,
        "archive.shelf.FileCard",
        Some(&json!({"id": "c1", "code": "X1"})),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    drop(first);
    let refused = match started(AFTER, &state) {
        Ok(served) => panic!("run listened on {}", served.address),
        Err(refused) => refused,
    };
    names_the_card_code(&refused);
    unchanged(
        &state.join(format!("{COMPONENT}.sqlite")),
        &Shapes::owned(&ir(BEFORE), COMPONENT),
    );
}

/// `run --stdin` with `spec` on the state file `file`, `lines` on stdin.
fn stdin_run(spec: &str, file: &Path, lines: &str) -> (ExitStatus, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", spec, "--environment", LAPTOP, "--stdin", "--state"])
        .arg(file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("b10x-spindle starts");
    let written = child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(lines.as_bytes());
    if let Err(error) = written {
        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe, "{error}");
    }
    let output = child.wait_with_output().expect("b10x-spindle runs");
    (
        output.status,
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

#[test]
fn a_stdin_run_refuses_a_misfit_state_file() {
    let scratch = scratch();
    let file = scratch.path().join("state.sqlite");
    let (status, stdout) = stdin_run(
        BEFORE,
        &file,
        "{\"command\": \"archive.shelf.FileCard\", \"input\": {\"id\": \"c1\", \"code\": \"X1\"}}\n",
    );
    assert!(status.success(), "{status}: {stdout}");
    let (status, stdout) = stdin_run(
        AFTER,
        &file,
        "{\"command\": \"archive.shelf.FileCard\", \"input\": {\"id\": \"c2\", \"label\": \"L\"}}\n",
    );
    names_the_card_code(&Refused { status, stdout });
    unchanged(&file, &Shapes::of(&ir(BEFORE)));
}

// `story:entity-invariant-over-stored-rows`: `tests/fixtures/spec-change-invariant` is
// `spec-change-before` with the invariant `title.count <= 10` on every book.

const INVARIANT: &str = "crates/spindle-cli/tests/fixtures/spec-change-invariant";
const BOOK: &str = "archive.shelf.Book";
const STATEMENT: &str = "title.count <= 10";
const LONG: &str = "The Left Hand of Darkness";

/// The books of the store at `file`, each identity with its fields.
fn books(file: &Path) -> Vec<(Value, Value)> {
    let mut store = SqliteStore::open(file).expect("the store opens");
    store
        .scan(BOOK)
        .expect("the books are read")
        .into_iter()
        .map(|(identity, stored)| (identity, json!(stored.row.fields)))
        .collect()
}

/// Asserts `refused` is a misfit's exit 3 naming the book's invariant and one row on stdout, and
/// that the store at `file` keeps `recorded` and the one book `b1` with the long title.
fn names_the_book_invariant(refused: &Refused, file: &Path, recorded: &Shapes) {
    assert_eq!(refused.status.code(), Some(3), "{refused:?}");
    let line = refused
        .stdout
        .lines()
        .find(|line| line.contains(STATEMENT))
        .unwrap_or_else(|| panic!("stdout names the invariant: {:?}", refused.stdout));
    assert!(
        line.contains(BOOK) && line.trim_end().ends_with(": 1"),
        "the line names the entity and one row: {line:?}"
    );
    assert!(
        !refused.stdout.contains("listening") && !refused.stdout.contains("refused on"),
        "nothing else is printed: {:?}",
        refused.stdout
    );
    let store = SqliteStore::open(file).expect("the store opens");
    assert_eq!(
        store.recorded().expect("the record reads").as_ref(),
        Some(recorded),
        "the record is unchanged"
    );
    drop(store);
    assert_eq!(books(file), vec![(json!("b1"), json!({"title": LONG}))]);
}

#[test]
fn an_entity_invariant_a_stored_row_breaks_exits_before_listening() {
    let scratch = scratch();
    let state = scratch.path().join("state");
    let first = listening(BEFORE, &state);
    let (status, answer) = send(
        &first,
        BEFORE,
        "archive.shelf.ShelveBook",
        Some(&json!({"id": "b1", "title": LONG})),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    drop(first);
    let refused = match started(INVARIANT, &state) {
        Ok(served) => panic!("run listened on {}", served.address),
        Err(refused) => refused,
    };
    names_the_book_invariant(
        &refused,
        &state.join(format!("{COMPONENT}.sqlite")),
        &Shapes::owned(&ir(BEFORE), COMPONENT),
    );
}

#[test]
fn an_entity_invariant_every_stored_row_holds_starts() {
    let scratch = scratch();
    let state = scratch.path().join("state");
    let first = listening(BEFORE, &state);
    let (status, answer) = send(
        &first,
        BEFORE,
        "archive.shelf.ShelveBook",
        Some(&json!({"id": "b1", "title": "Dune"})),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    drop(first);
    let second = listening(INVARIANT, &state);
    let (status, answer) = send(&second, INVARIANT, "archive.shelf.ShelvedBooks", None);
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["rows"], json!([{"id": "b1", "title": "Dune"}]));
}

#[test]
fn an_entity_invariant_a_stored_row_breaks_refuses_a_stdin_run() {
    let scratch = scratch();
    let file = scratch.path().join("state.sqlite");
    let (status, stdout) = stdin_run(
        BEFORE,
        &file,
        &format!(
            "{{\"command\": \"archive.shelf.ShelveBook\", \"input\": {{\"id\": \"b1\", \"title\": \"{LONG}\"}}}}\n"
        ),
    );
    assert!(status.success(), "{status}: {stdout}");
    let (status, stdout) = stdin_run(
        INVARIANT,
        &file,
        "{\"command\": \"archive.shelf.ShelveBook\", \"input\": {\"id\": \"b2\", \"title\": \"Emma\"}}\n",
    );
    names_the_book_invariant(&Refused { status, stdout }, &file, &Shapes::of(&ir(BEFORE)));
}
