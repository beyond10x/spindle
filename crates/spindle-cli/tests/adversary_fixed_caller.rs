//! Adversary cases for `run --fixed-actor` / `--fixed-caller` (`story:served-fixed-caller`).
//!
//! The fixture `tests/fixtures/adversary-fixed-caller` declares one component, `stacks`, reached
//! over the network. `Librarian` carries an enum `branch`, an `Integer` `level` and an optional
//! `nickname`, may add and shelve books and may read `Catalogue`, which makes `Catalogue`
//! read-granted (ess/22, `x-ess-may-read`). `Visitor` declares no attribute and may only add a
//! book. `ShelveBook` refuses with `other-branch` a book whose `branch` input is not the caller's,
//! and records the caller's branch.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/adversary-fixed-caller";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "stacks";
const LIBRARIAN: &str = "shelf.book.Librarian";
const VISITOR: &str = "shelf.book.Visitor";
const ADD: &str = "shelf.book.AddBook";
const SHELVE: &str = "shelf.book.ShelveBook";
const CATALOGUE: &str = "shelf.book.Catalogue";
const BOOK: &str = "3c4d5e6f-7a8b-4c9d-8e0f-1a2b3c4d5e6f";

const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn route(operation: &str) -> (String, String) {
    let ir = spindle_spec::load(&root().join(SPEC))
        .expect("the adversary fixture compiles")
        .ir;
    let surface = spindle_ingress::Surface::new(&ir, &[COMPONENT.to_owned()], None)
        .expect("the fixture's surface is routed");
    let route = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == operation)
        .unwrap_or_else(|| panic!("a route for {operation}"));
    (route.method.as_str().to_uppercase(), route.path.clone())
}

struct Served {
    child: Child,
    address: SocketAddr,
    stderr: mpsc::Receiver<String>,
}

impl Served {
    fn stop(mut self) -> String {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.stderr
            .recv_timeout(ANSWER)
            .expect("stderr is read to its end once the run stops")
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn serve(arguments: &[&str]) -> Result<Served, (Option<i32>, String, String)> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--environment", LAPTOP])
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let (printed, stderr_read) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        let _ = printed.send(text);
    });
    let stdout = child.stdout.take().expect("stdout is piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let _ = reader.read_line(&mut line);
        let mut rest = String::new();
        if line.is_empty() {
            let _ = reader.read_to_string(&mut rest);
        }
        let _ = sender.send((line, rest));
    });
    let Ok((line, rest)) = receiver.recv_timeout(STARTUP) else {
        let _ = child.kill();
        let _ = child.wait();
        panic!("run printed no line within {STARTUP:?}");
    };
    if line.is_empty() {
        let status = child.wait().expect("the run is reaped");
        let stderr = stderr_read
            .recv_timeout(ANSWER)
            .expect("stderr is read to its end");
        return Err((status.code(), rest, stderr));
    }
    let ready: Value = serde_json::from_str(&line)
        .unwrap_or_else(|error| panic!("run's first line {line:?} is not JSON ({error})"));
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("run's first line names no address: {ready}"));
    Ok(Served {
        child,
        address,
        stderr: stderr_read,
    })
}

fn serving(arguments: &[&str]) -> Served {
    serve(arguments).unwrap_or_else(|stopped| panic!("run stopped before listening: {stopped:?}"))
}

fn exchange_with(
    address: SocketAddr,
    method: &str,
    path: &str,
    headers: &str,
    body: Option<&Value>,
) -> (u16, Value) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    let body = body.map(Value::to_string).unwrap_or_default();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n{headers}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
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

fn call(served: &Served, operation: &str, body: Option<&Value>) -> (u16, Value) {
    let (method, path) = route(operation);
    exchange_with(served.address, &method, &path, "", body)
}

/// ESS 0.87.0 publishes `Catalogue` with `x-ess-may-read: [Librarian]` and a `403` carrying the
/// standard refusal: "a request authenticated as no actor, or as one the grant does not name, is
/// answered 403 with the same standard refusal before the view is read". With the flag the run
/// authenticates every request as the fixed actor, so a `Visitor` must not read it.
#[test]
fn a_read_granted_view_refuses_a_fixed_actor_its_grant_does_not_name() {
    let served = serving(&["--fixed-actor", VISITOR]);
    let (status, added) = call(
        &served,
        ADD,
        Some(&json!({"book_id": BOOK, "title": "tide tables"})),
    );
    assert_eq!(status, 202, "the visitor may add a book: {added}");
    let (status, body) = call(&served, CATALOGUE, None);
    assert_eq!(
        (status, body.clone()),
        (403, json!({"refused": "not granted", "actor": VISITOR})),
        "Catalogue is read-granted to Librarian only; the fixed Visitor read {body}"
    );
}

#[test]
fn an_actor_without_attributes_is_fixed_with_no_caller() {
    let served = serving(&["--fixed-actor", VISITOR]);
    let (status, added) = call(
        &served,
        ADD,
        Some(&json!({"book_id": BOOK, "title": "tide tables"})),
    );
    assert_eq!(status, 202, "{added}");
    assert_eq!(added["outcome"], "added", "{added}");
    let (status, refused) = call(
        &served,
        SHELVE,
        Some(&json!({"book_id": BOOK, "branch": "North"})),
    );
    assert_eq!(
        (status, refused.clone()),
        (403, json!({"refused": "not granted", "actor": VISITOR})),
        "{refused}"
    );
    let stderr = served.stop();
    assert!(
        stderr
            .lines()
            .any(|line| line.contains("--fixed-actor") && line.contains(VISITOR)),
        "{stderr}"
    );
}

#[test]
fn a_mistyped_fixed_caller_is_refused_at_start() {
    for (caller, named) in [
        (json!({"branch": "East", "level": 1}), "branch"),
        (json!({"branch": null, "level": 1}), "branch"),
        (json!({"branch": "North", "level": 1.5}), "level"),
        (json!({"branch": "North", "level": "3"}), "level"),
        (
            json!({"branch": "North", "level": 18_446_744_073_709_551_615_u64}),
            "level",
        ),
        (
            json!({"branch": "North", "level": 1, "nickname": 7}),
            "nickname",
        ),
        (
            json!({"branch": "North", "level": 1, "badge": "x"}),
            "badge",
        ),
    ] {
        let text = caller.to_string();
        let Err((code, stdout, stderr)) =
            serve(&["--fixed-actor", LIBRARIAN, "--fixed-caller", &text])
        else {
            panic!("--fixed-caller {text} listened");
        };
        assert_eq!(code, Some(1), "{text}: {stderr}");
        assert!(!stdout.contains("listening"), "{text}: {stdout}");
        assert!(stderr.contains(named), "{text} names {named}: {stderr}");
    }
}

#[test]
fn an_optional_attribute_may_be_left_out() {
    let caller = json!({"branch": "North", "level": 2}).to_string();
    let served = serving(&["--fixed-actor", LIBRARIAN, "--fixed-caller", &caller]);
    let stderr = served.stop();
    let line = stderr
        .lines()
        .find(|line| line.contains("--fixed-actor"))
        .unwrap_or_else(|| panic!("{stderr}"));
    assert!(line.contains(LIBRARIAN), "{line}");
    assert!(line.contains("branch") && line.contains("North"), "{line}");
    assert!(line.contains("level"), "{line}");
}

#[test]
fn fixed_caller_without_fixed_actor_never_listens() {
    let caller = json!({"branch": "North", "level": 2}).to_string();
    let Err((code, stdout, _)) = serve(&["--fixed-caller", &caller]) else {
        panic!("--fixed-caller alone listened");
    };
    assert_ne!(code, Some(0));
    assert!(!stdout.contains("listening"), "{stdout}");
}

/// Nothing in a request adds to or overrides the fixed caller: a body member named like a caller
/// attribute is refused, and a header or query naming another actor or branch changes nothing.
#[test]
fn a_request_cannot_add_to_or_override_the_fixed_caller() {
    let caller = json!({"branch": "North", "level": 2}).to_string();
    let served = serving(&["--fixed-actor", LIBRARIAN, "--fixed-caller", &caller]);
    let (status, added) = call(
        &served,
        ADD,
        Some(&json!({"book_id": BOOK, "title": "tide tables"})),
    );
    assert_eq!(status, 202, "{added}");
    let (method, path) = route(SHELVE);
    let (status, body) = exchange_with(
        served.address,
        &method,
        &path,
        "",
        Some(&json!({"book_id": BOOK, "branch": "North", "level": 9})),
    );
    assert_eq!(status, 400, "a body member `level` is not input: {body}");
    let (status, body) = exchange_with(
        served.address,
        &method,
        &format!("{path}?branch=South&actor={VISITOR}"),
        &format!("X-Actor: {VISITOR}\r\nX-Caller: {{\"branch\":\"South\"}}\r\n"),
        Some(&json!({"book_id": BOOK, "branch": "South"})),
    );
    assert_eq!(
        body["outcome"], "other-branch",
        "the caller stays North whatever the request says: {status} {body}"
    );
    let (status, shelved) = exchange_with(
        served.address,
        &method,
        &format!("{path}?branch=South"),
        &format!("X-Actor: {VISITOR}\r\n"),
        Some(&json!({"book_id": BOOK, "branch": "North"})),
    );
    assert_eq!(status, 202, "{shelved}");
    assert_eq!(shelved["outcome"], "shelved", "{shelved}");
    assert_eq!(
        shelved["published"][0]["payload"]["branch"], "North",
        "{shelved}"
    );
}

/// `POST /api/reset` empties the stores and leaves the fixed caller in force.
#[test]
fn the_fixed_caller_survives_a_reset() {
    let caller = json!({"branch": "South", "level": 2}).to_string();
    let served = serving(&["--fixed-actor", LIBRARIAN, "--fixed-caller", &caller]);
    let (status, body) = exchange_with(served.address, "POST", "/api/reset", "", None);
    assert_eq!(status, 200, "{body}");
    let (status, added) = call(
        &served,
        ADD,
        Some(&json!({"book_id": BOOK, "title": "tide tables"})),
    );
    assert_eq!(status, 202, "{added}");
    let (_, shelved) = call(
        &served,
        SHELVE,
        Some(&json!({"book_id": BOOK, "branch": "South"})),
    );
    assert_eq!(shelved["outcome"], "shelved", "{shelved}");
    assert_eq!(
        shelved["published"][0]["payload"]["branch"], "South",
        "{shelved}"
    );
}
