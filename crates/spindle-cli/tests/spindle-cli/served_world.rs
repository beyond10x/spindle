//! `POST /api/reset` carrying a world: seed rows written where their entity rests, a decision
//! table that answers external causes until the next reset, and a seed identities are minted
//! from.
//!
//! The fixture `tests/fixtures/served-world` declares one component, `library`, reached over the
//! network: `shelf.books.Book`, shelved or lent, `AddBook`, whose new book's identity the runtime
//! mints, `ReturnBook`, guarded on the book being lent, and `Donate`, whose external outcome sits
//! beside a default branch. Every path and method is read from the `OpenAPI` document ESS projects
//! for that component.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/served-world";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "library";

const BOOK: &str = "shelf.books.Book";
const ADD_BOOK: &str = "shelf.books.AddBook";
const RETURN_BOOK: &str = "shelf.books.ReturnBook";
const DONATE: &str = "shelf.books.Donate";
const ALL_BOOKS: &str = "shelf.books.AllBooks";

const FIRST: &str = "6f1d3c1e-0000-4000-8000-000000000001";
const SECOND: &str = "6f1d3c1e-0000-4000-8000-000000000002";

/// How long `run` may take to admit, open its state and print where it listens.
const STARTUP: Duration = Duration::from_secs(60);
/// How long one request may take to be answered.
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The method and path ESS publishes for the operation `id` of the served component.
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

/// A running `b10x-spindle run --answer-default` serving HTTP; stopped when dropped.
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

/// `run` on the fixture and laptop, its state in SQLite, each external cause the decisions do
/// not answer taking the default branch.
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

/// One HTTP/1.1 exchange on its own connection, `body` sent as is: the status and the JSON body.
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

/// `POST /api/reset` with `world` as its body, or none.
fn reset(served: &Served, world: Option<&Value>) -> (u16, Value) {
    let body = world.map(Value::to_string).unwrap_or_default();
    exchange(served.address, "POST", "/api/reset", &body)
}

fn books(served: &Served) -> Vec<Value> {
    let (status, body) = call(served, ALL_BOOKS, None);
    assert_eq!(status, 200, "{body}");
    let mut rows = body["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("the view answers rows: {body}"))
        .clone();
    rows.sort_by(|left, right| {
        left["book_id"]
            .to_string()
            .cmp(&right["book_id"].to_string())
    });
    rows
}

fn lent(identity: &str, title: &str) -> Value {
    json!({"entity": BOOK, "identity": identity, "state": "Lent", "fields": {"title": title}})
}

/// Adds a book titled `title` and answers the identity the run minted for it.
fn add(served: &Served, title: &str) -> String {
    let (status, answer) = call(served, ADD_BOOK, Some(&json!({"title": title})));
    assert!((200..300).contains(&status), "{status} {answer}");
    answer["published"][0]["payload"]["book_id"]
        .as_str()
        .unwrap_or_else(|| panic!("the added book's identity is published: {answer}"))
        .to_owned()
}

#[test]
fn a_reset_with_seed_rows_serves_them() {
    let served = serve();
    add(&served, "Already here");

    let world = json!({"rows": [lent(FIRST, "Moby-Dick"), lent(SECOND, "Middlemarch")]});
    let (status, answer) = reset(&served, Some(&world));
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["emptied"], json!([COMPONENT]), "{answer}");

    assert_eq!(
        books(&served),
        vec![
            json!({"book_id": FIRST, "state": "Lent", "title": "Moby-Dick"}),
            json!({"book_id": SECOND, "state": "Lent", "title": "Middlemarch"}),
        ],
        "the view lists both seed rows, and nothing from before the reset"
    );

    let (status, answer) = call(&served, RETURN_BOOK, Some(&json!({"book_id": FIRST})));
    assert!((200..300).contains(&status), "{status} {answer}");
    assert_eq!(
        answer["outcome"], "returned",
        "a command guarded on Lent runs against a seed row: {answer}"
    );
    let (status, answer) = call(&served, RETURN_BOOK, Some(&json!({"book_id": FIRST})));
    assert!(!(200..300).contains(&status), "{status} {answer}");
    assert_eq!(answer["outcome"], "not-lent", "{answer}");
    let (status, answer) = call(&served, RETURN_BOOK, Some(&json!({"book_id": SECOND})));
    assert_eq!(answer["outcome"], "returned", "{status} {answer}");
}

#[test]
fn a_reset_with_a_decision_table_answers_from_it() {
    let served = serve();
    let donate = |book: &str| {
        let (status, answer) = call(
            &served,
            DONATE,
            Some(&json!({"book_id": book, "title": "Donated"})),
        );
        (status, answer["outcome"].clone(), answer)
    };

    let world = json!({"decisions": {DONATE: {"refused": true}}});
    let (status, answer) = reset(&served, Some(&world));
    assert_eq!(status, 200, "{answer}");
    let (status, outcome, answer) = donate(FIRST);
    assert!(!(200..300).contains(&status), "{status} {answer}");
    assert_eq!(outcome, "refused", "the table decides the cause: {answer}");

    let world = json!({"decisions": {"refused": false}});
    let (status, answer) = reset(&served, Some(&world));
    assert_eq!(status, 200, "{answer}");
    let (status, outcome, answer) = donate(FIRST);
    assert!((200..300).contains(&status), "{status} {answer}");
    assert_eq!(
        outcome, "donated",
        "a different table, the other branch: {answer}"
    );

    let world = json!({"decisions": {DONATE: {"refused": true}}});
    assert_eq!(reset(&served, Some(&world)).0, 200);
    assert_eq!(donate(FIRST).1, "refused");
    let (status, answer) = reset(&served, None);
    assert_eq!(status, 200, "{answer}");
    let (_, outcome, answer) = donate(FIRST);
    assert_eq!(
        outcome, "donated",
        "the table answers until the next reset, then the run's own answers do: {answer}"
    );
}

#[test]
fn a_seeded_mint_repeats_its_identities() {
    let world = json!({"mint": 7});
    let minted = |served: &Served| {
        let (status, answer) = reset(served, Some(&world));
        assert_eq!(status, 200, "{answer}");
        [add(served, "One"), add(served, "Two")]
    };

    let first = serve();
    let once = minted(&first);
    assert_ne!(once[0], once[1], "each creation mints its own identity");
    assert_eq!(
        minted(&first),
        once,
        "the same requests after the same reset, the same identities"
    );
    drop(first);

    let second = serve();
    assert_eq!(minted(&second), once, "another run, the same identities");
    let (status, answer) = reset(&second, Some(&json!({"mint": 8})));
    assert_eq!(status, 200, "{answer}");
    assert_ne!(
        add(&second, "One"),
        once[0],
        "another seed, another identity"
    );
}

#[test]
fn a_reset_without_a_body_empties_as_today() {
    let served = serve();
    add(&served, "One");
    add(&served, "Two");
    assert_eq!(books(&served).len(), 2);

    let (status, answer) = reset(&served, None);
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer, json!({"emptied": [COMPONENT]}));
    assert_eq!(books(&served), Vec::<Value>::new());
}

#[test]
fn a_world_that_does_not_validate_is_refused_and_changes_nothing() {
    let served = serve();
    let kept = add(&served, "Kept");
    let before = books(&served);
    assert_eq!(before.len(), 1);

    let refusals = [
        (
            "a row of an undeclared entity",
            json!({"rows": [lent(FIRST, "Seed"),
                            {"entity": "shelf.books.Shelf", "identity": SECOND, "state": "Lent"}]}),
        ),
        (
            "a row in an undeclared state",
            json!({"rows": [{"entity": BOOK, "identity": FIRST, "state": "Lost",
                             "fields": {"title": "Seed"}}]}),
        ),
        (
            "a row whose field is not of its type",
            json!({"rows": [{"entity": BOOK, "identity": FIRST, "state": "Lent",
                             "fields": {"title": 12}}]}),
        ),
        (
            "a row whose identity is not of its type",
            json!({"rows": [{"entity": BOOK, "identity": "not a uuid", "state": "Lent",
                             "fields": {"title": "Seed"}}]}),
        ),
        (
            "two rows of one identity",
            json!({"rows": [lent(FIRST, "Seed"), lent(FIRST, "Again")]}),
        ),
        (
            "a table naming an undeclared cause",
            json!({"rows": [lent(FIRST, "Seed")], "decisions": {DONATE: {"withdrawn": true}}}),
        ),
        (
            "a table naming an undeclared command",
            json!({"decisions": {"shelf.books.Burn": {"refused": true}}}),
        ),
        (
            "an answer that is not a boolean",
            json!({"decisions": {"refused": "yes"}}),
        ),
        ("a mint seed that is not a number", json!({"mint": "seven"})),
        ("an unknown key", json!({"rows": [], "weather": "fair"})),
    ];
    for (what, world) in refusals {
        let (status, answer) = reset(&served, Some(&world));
        assert_eq!(status, 400, "{what} is refused: {answer}");
        assert!(
            answer["refused"]
                .as_str()
                .is_some_and(|why| !why.is_empty()),
            "{what}: the refusal names why: {answer}"
        );
        assert_eq!(books(&served), before, "{what} changes nothing");
    }
    let (status, answer) = exchange(served.address, "POST", "/api/reset", "{not json");
    assert_eq!(status, 400, "a body that is not JSON is refused: {answer}");
    assert_eq!(
        books(&served),
        before,
        "a body that is not JSON changes nothing"
    );

    let (status, answer) = call(&served, RETURN_BOOK, Some(&json!({"book_id": kept})));
    assert_eq!(
        answer["outcome"], "not-lent",
        "the book from before is still shelved: {status} {answer}"
    );
}
