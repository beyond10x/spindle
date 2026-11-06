//! Adversary cases for `POST /api/reset` carrying a world.
//!
//! The fixture `tests/fixtures/adversary-world` is the `served-world` fixture plus
//! `shelf.books.Desk`, whose identity is a `String` the caller supplies, and its view
//! `shelf.books.AllDesks`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/adversary-world";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "library";

const BOOK: &str = "shelf.books.Book";
const DESK: &str = "shelf.books.Desk";
const ADD_BOOK: &str = "shelf.books.AddBook";
const ALL_BOOKS: &str = "shelf.books.AllBooks";
const ALL_DESKS: &str = "shelf.books.AllDesks";

const FIRST: &str = "6f1d3c1e-0000-4000-8000-000000000001";

const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn route(id: &str) -> (String, String) {
    let loaded = spindle_spec::load(&root().join(SPEC)).expect("the fixture compiles");
    let component = loaded
        .ir
        .components()
        .values()
        .find(|component| component.name.as_str() == COMPONENT)
        .expect("the fixture declares the component");
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

/// `run --answer-default` on the fixture, its state in SQLite.
fn serve() -> Served {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args([
            "run",
            SPEC,
            "--environment",
            LAPTOP,
            "--answer-default",
            "--state",
        ])
        .arg(state.path().join("state"))
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
    Served {
        child,
        address,
        _state: state,
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

fn call(served: &Served, id: &str, body: Option<&Value>) -> (u16, Value) {
    let (method, path) = route(id);
    let body = body.map(Value::to_string).unwrap_or_default();
    exchange(served.address, &method, &path, &body)
}

fn reset(served: &Served, world: Option<&Value>) -> (u16, Value) {
    let body = world.map(Value::to_string).unwrap_or_default();
    exchange(served.address, "POST", "/api/reset", &body)
}

fn rows(served: &Served, view: &str) -> Vec<Value> {
    let (status, body) = call(served, view, None);
    assert_eq!(status, 200, "{body}");
    let mut rows = body["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("the view answers rows: {body}"))
        .clone();
    rows.sort_by_key(Value::to_string);
    rows
}

fn add(served: &Served, title: &str) -> (u16, Value) {
    call(served, ADD_BOOK, Some(&json!({"title": title})))
}

fn added(served: &Served, title: &str) -> String {
    let (status, answer) = add(served, title);
    assert!((200..300).contains(&status), "{status} {answer}");
    answer["published"][0]["payload"]["book_id"]
        .as_str()
        .unwrap_or_else(|| panic!("the added book's identity is published: {answer}"))
        .to_owned()
}

fn desk(identity: &str) -> Value {
    json!({"entity": DESK, "identity": identity, "state": "Free", "fields": {"label": "by the window"}})
}

/// A world whose second row the SQLite store cannot take (an identity ESS admits as a `String`,
/// which the event log refuses as a stream id) must not empty the stores, half-write the world,
/// or answer `500`: `reset_to` promises "every row is checked before any store is emptied".
#[test]
fn a_world_a_store_refuses_half_way_changes_nothing() {
    let served = serve();
    added(&served, "Kept");
    let books_before = rows(&served, ALL_BOOKS);
    assert_eq!(books_before.len(), 1);

    let world = json!({"rows": [desk("reading-room-1"), desk("salle-de-lecture-\u{e9}")]});
    let (status, answer) = reset(&served, Some(&world));
    let books_after = rows(&served, ALL_BOOKS);
    let desks_after = rows(&served, ALL_DESKS);
    assert!(
        status == 400 && books_after == books_before && desks_after.is_empty(),
        "a world a store refuses half way: status {status} {answer}; books before {books_before:?}, \
         after {books_after:?}; desks after {desks_after:?}"
    );
}

/// A world seeded with an identity its own mint seed draws next: the creating request must still
/// create a book, as with any seed, instead of failing on the seeded row.
#[test]
fn a_seeded_mint_does_not_mint_a_seeded_identity() {
    let served = serve();
    assert_eq!(reset(&served, Some(&json!({"mint": 7}))).0, 200);
    let drawn = added(&served, "Captured");

    let world = json!({
        "rows": [{"entity": BOOK, "identity": drawn, "state": "Shelved", "fields": {"title": "Captured"}}],
        "mint": 7,
    });
    let (status, answer) = reset(&served, Some(&world));
    assert_eq!(status, 200, "{answer}");
    let (status, answer) = add(&served, "Next");
    assert!(
        (200..300).contains(&status),
        "the first creation after a world seeded with the identity the mint draws first: {status} {answer}"
    );
    assert_ne!(
        answer["published"][0]["payload"]["book_id"], drawn,
        "a minted identity is never a seeded one: {answer}"
    );
}

/// Seed rows ESS's establish step would refuse are refused here too.
#[test]
fn seed_rows_establish_refuses_are_refused() {
    let served = serve();
    added(&served, "Kept");
    let before = rows(&served, ALL_BOOKS);
    let refusals = [
        (
            "an undeclared field",
            json!({"entity": BOOK, "identity": FIRST, "state": "Lent",
                   "fields": {"title": "Seed", "author": "Anon"}}),
        ),
        (
            "no required field",
            json!({"entity": BOOK, "identity": FIRST, "state": "Lent", "fields": {}}),
        ),
        (
            "a required field as null",
            json!({"entity": BOOK, "identity": FIRST, "state": "Lent", "fields": {"title": null}}),
        ),
        (
            "the identity among the fields",
            json!({"entity": BOOK, "identity": FIRST, "state": "Lent",
                   "fields": {"title": "Seed", "book_id": FIRST}}),
        ),
        (
            "an identity of null",
            json!({"entity": BOOK, "identity": null, "state": "Lent", "fields": {"title": "Seed"}}),
        ),
    ];
    for (what, row) in refusals {
        let (status, answer) = reset(&served, Some(&json!({"rows": [row]})));
        assert_eq!(status, 400, "{what}: {answer}");
        assert_eq!(rows(&served, ALL_BOOKS), before, "{what} changes nothing");
    }
}

/// A plain reset drops a world's mint as it drops its table; the suite checks only the table.
#[test]
fn a_plain_reset_drops_the_seeded_mint() {
    let served = serve();
    assert_eq!(reset(&served, Some(&json!({"mint": 7}))).0, 200);
    let seeded = added(&served, "One");
    assert_eq!(reset(&served, None).0, 200);
    assert_ne!(
        added(&served, "One"),
        seeded,
        "a plain reset mints at random again"
    );
}

/// A refused world leaves the last world's mint where it was.
#[test]
fn a_refused_world_keeps_the_mint_sequence() {
    let served = serve();
    assert_eq!(reset(&served, Some(&json!({"mint": 7}))).0, 200);
    let first = added(&served, "One");
    assert_eq!(reset(&served, Some(&json!({"mint": 7}))).0, 200);
    let refused =
        json!({"mint": 8, "rows": [{"entity": BOOK, "identity": "nope", "state": "Lent"}]});
    assert_eq!(reset(&served, Some(&refused)).0, 400);
    assert_eq!(
        added(&served, "One"),
        first,
        "the refused world did not reseed"
    );
}
