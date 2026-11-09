//! `POST /api/reset` answered as `ess/domains/console.yaml` (`spindle.console`) states it
//! (`story:store-reset-and-world-specified`): each case sends a body whose answer a summary or an
//! `UNMAPPED:` line of that domain names, and asserts the run answers it so. A case goes red when
//! the run and the specification part.
//!
//! The fixture is `tests/fixtures/served-world`, served by `run` as `served_world` serves it.

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
const DONATE: &str = "shelf.books.Donate";
const ALL_BOOKS: &str = "shelf.books.AllBooks";
const FIRST: &str = "6f1d3c1e-0000-4000-8000-000000000001";
const SECOND: &str = "6f1d3c1e-0000-4000-8000-000000000002";

const STARTUP: Duration = Duration::from_secs(60);
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

/// `run --answer-default` on the fixture: `Donate`'s external cause takes its default branch,
/// `donated`, unless a world's decision table answers it.
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
    let line = receiver
        .recv_timeout(STARTUP)
        .expect("run prints where it listens");
    let ready: Value = serde_json::from_str(&line).expect("a JSON line");
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .expect("an address");
    Served {
        child,
        address,
        _state: state,
    }
}

/// One exchange with `body` sent byte for byte: the status and the body text. A write the
/// listener cut short is not an error: the answer it wrote is read all the same.
fn exchange(served: &Served, method: &str, path: &str, body: &str) -> (u16, String) {
    let mut stream = TcpStream::connect_timeout(&served.address, ANSWER).expect("listening");
    stream.set_read_timeout(Some(ANSWER)).expect("a timeout");
    let head = format!(
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Type: \
         application/json\r\nContent-Length: {}\r\n\r\n",
        served.address,
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
    let mut answer = Vec::new();
    let _ = stream.read_to_end(&mut answer);
    let answer = String::from_utf8_lossy(&answer).into_owned();
    let (head, body) = answer
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("an answer with a head: {answer:?}"));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {head:?}"));
    (status, body.to_owned())
}

fn reset(served: &Served, body: &str) -> (u16, String) {
    exchange(served, "POST", "/api/reset", body)
}

fn call(served: &Served, id: &str, body: &Value) -> (u16, Value) {
    let (method, path) = route(id);
    let (status, answer) = exchange(served, &method, &path, &body.to_string());
    let answer = serde_json::from_str(&answer).unwrap_or_else(|error| panic!("{error}: {answer}"));
    (status, answer)
}

fn book_ids(served: &Served) -> Vec<String> {
    let (method, path) = route(ALL_BOOKS);
    let (status, answer) = exchange(served, &method, &path, "");
    assert_eq!(status, 200, "{answer}");
    let answer: Value = serde_json::from_str(&answer).expect("the view answers JSON");
    let mut ids = answer["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("the view answers rows: {answer}"))
        .iter()
        .filter_map(|row| row["book_id"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

/// The outcome `Donate` answers for `book`.
fn donated(served: &Served, book: &str) -> String {
    let (_, answer) = call(
        served,
        DONATE,
        &json!({"book_id": book, "title": "Donated"}),
    );
    answer["outcome"]
        .as_str()
        .unwrap_or_else(|| panic!("an outcome: {answer}"))
        .to_owned()
}

/// Two spellings the specification states are loaded `200`. `World.mint` and `World.decisions`
/// are `presence: omitted_or_null`, so `null` is the absent value and a `null` table leaves the
/// run's own answers. `World`'s `UNMAPPED:` line: a body, and a row, written as an array of its
/// members' values in declaration order is read as that object, and its row rests.
#[test]
fn a_null_member_or_an_array_spelling_is_loaded() {
    let served = serve();
    for body in [r#"{"mint":null}"#, r#"{"decisions":null}"#] {
        let (status, answer) = reset(&served, body);
        assert_eq!(status, 200, "{body} is a world: {answer}");
    }
    assert_eq!(
        donated(&served, SECOND),
        "donated",
        "a null table answers nothing; the default branch does"
    );

    let row = format!(r#"["{BOOK}","{FIRST}","Lent",{{"title":"Seed"}}]"#);
    let (status, answer) = reset(&served, &format!(r#"{{"rows":[{row}]}}"#));
    assert_eq!(status, 200, "a row as an array is loaded: {answer}");
    assert_eq!(book_ids(&served), vec![FIRST.to_owned()], "the row rests");

    let (status, answer) = reset(&served, &format!(r#"[[{row}],null,null]"#));
    assert_eq!(status, 200, "a world as an array is loaded: {answer}");
    assert_eq!(book_ids(&served), vec![FIRST.to_owned()], "its row rests");
}

/// `World.decisions`: nested tables are joined by `.` into the command they answer for, and a
/// table that holds no Boolean answers nothing and is loaded unchecked, whatever its key.
#[test]
fn a_nested_or_empty_decision_table_is_loaded_as_specified() {
    let served = serve();
    let nested = r#"{"decisions":{"shelf":{"books":{"Donate":{"refused":true}}}}}"#;
    let (status, answer) = reset(&served, nested);
    assert_eq!(status, 200, "{nested}: {answer}");
    assert_eq!(
        donated(&served, FIRST),
        "refused",
        "the nested table answers for shelf.books.Donate"
    );

    let empty = r#"{"decisions":{"nowhere":{}}}"#;
    let (status, answer) = reset(&served, empty);
    assert_eq!(status, 200, "{empty}: {answer}");
    assert_eq!(
        donated(&served, SECOND),
        "donated",
        "an empty table answers nothing; the default branch does"
    );
}

/// `World.mint`'s `UNMAPPED:` line: an integral mint written with a fraction or an exponent is
/// refused `400`, though the schema of `Integer` admits it.
#[test]
fn an_integral_mint_with_a_fraction_or_exponent_is_refused() {
    let served = serve();
    for body in [r#"{"mint":7.0}"#, r#"{"mint":1e1}"#] {
        let (status, answer) = reset(&served, body);
        assert_eq!(status, 400, "{body}: {answer}");
        let answer: Value = serde_json::from_str(&answer).expect("a refusal is JSON");
        assert!(
            answer["refused"]
                .as_str()
                .is_some_and(|why| !why.is_empty()),
            "{body}: a ResetRefusal names why: {answer}"
        );
    }
}

/// `ResetRefusal`: a body over 1 MiB is refused `413`, with nothing changed.
#[test]
fn a_world_over_1_mib_is_refused_413() {
    let served = serve();
    let (status, answer) = reset(
        &served,
        &format!(
            r#"{{"rows":[{{"entity":"{BOOK}","identity":"{FIRST}","state":"Lent","fields":{{"title":"Seed"}}}}]}}"#
        ),
    );
    assert_eq!(status, 200, "{answer}");

    let body = format!(r#"{{"mint":7}}{}"#, " ".repeat((1 << 20) + 1));
    let (status, answer) = reset(&served, &body);
    assert_eq!(status, 413, "a body over 1 MiB: {answer}");
    assert_eq!(book_ids(&served), vec![FIRST.to_owned()], "nothing changed");
}
