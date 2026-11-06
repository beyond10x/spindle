//! Adversary cases for `run` serving commands an `ess-http/1` binding puts on `method: delete`.
//!
//! Each case states what ESS publishes for the same request (the `OpenAPI` document
//! `ess_gen::openapi::bound_json` projects for the binding) or what the server ESS synthesises
//! for the binding does (`ess-synth`, `rust/http/bound.rs`), and asserts that `run` does it too.
//!
//! `tests/fixtures/ingress-delete` is the unit's fixture: `RemoveBook` on `DELETE /books/{id}`,
//! every input field carried by the path. `tests/fixtures/adversary-delete` adds `LendBook` on
//! `PUT /books/{id}`, so one path answers two methods, and gives `RemoveBook` a required `reason`
//! the path does not carry.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

const LAPTOP: &str = "examples/environments/laptop.toml";
const UNIT_SPEC: &str = "crates/spindle-cli/tests/fixtures/ingress-delete";
const UNIT_BINDING: &str = "crates/spindle-cli/tests/fixtures/ingress-delete-http.yaml";
const SPEC: &str = "crates/spindle-cli/tests/fixtures/adversary-delete";
const BINDING: &str = "crates/spindle-cli/tests/fixtures/adversary-delete-http.yaml";
const COMPONENT: &str = "shelf";

const REMOVE_BOOK: &str = "library.shelf.RemoveBook";
const SHELVED_BOOKS: &str = "library.shelf.ShelvedBooks";

const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The `OpenAPI` document ESS projects for `shelf` with a binding.
struct Contract(Value);

impl Contract {
    fn bound(spec: &str, binding: &str) -> Self {
        let loaded = spindle_spec::load(&root().join(spec)).expect("the fixture compiles");
        let text = std::fs::read_to_string(root().join(binding)).expect("the binding is read");
        let parsed = ess_http::HttpSpec::from_yaml(&text).expect("the binding parses");
        let http = ess_http::compile(&parsed, &loaded.ir, &ess_gen::http::derived(&loaded.ir))
            .unwrap_or_else(|refused| panic!("ESS refuses the binding: {refused}"));
        let binding = ess_gen::http::Binding::new(http)
            .unwrap_or_else(|refused| panic!("the binding cannot be served: {refused}"));
        let component = loaded
            .ir
            .components()
            .values()
            .find(|component| component.name.as_str() == COMPONENT)
            .expect("the fixture declares the component");
        let document = ess_gen::openapi::bound_json(&loaded.ir, component, Some(&binding));
        Self(serde_json::from_str(&document).expect("the projection is JSON"))
    }

    fn operation(&self, id: &str) -> (String, String, &Value) {
        let paths = self.0["paths"].as_object().expect("the document has paths");
        for (path, item) in paths {
            for (method, operation) in item.as_object().expect("a path item is an object") {
                if operation["operationId"] == id {
                    return (method.to_uppercase(), path.clone(), operation);
                }
            }
        }
        panic!("the contract declares no operation {id}")
    }

    fn validate(&self, id: &str, status: u16, body: &Value) {
        let (method, path, _) = self.operation(id);
        let schema = &self.0["paths"][&path][method.to_lowercase()]["responses"]
            [status.to_string()]["content"]["application/json"]["schema"];
        assert!(
            !schema.is_null(),
            "the contract declares no JSON answer of {id} under {status}"
        );
        let mut document = self.0.clone();
        document["allOf"] = json!([schema]);
        let validator =
            jsonschema::draft202012::new(&document).expect("the contract compiles as a schema");
        let errors = validator
            .iter_errors(body)
            .map(|error| format!("{} at {}", error, error.instance_path()))
            .collect::<Vec<_>>();
        assert!(
            errors.is_empty(),
            "{id} answered {status} with {body}, which its projected schema refuses: {errors:?}"
        );
    }
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

fn serve(spec: &str, binding: &str) -> Served {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", spec, "--environment", LAPTOP, "--state"])
        .arg(state.path().join("state"))
        .args(["--http", binding])
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

/// One answer: the status, the head as written, and the body text.
struct Reply {
    status: u16,
    head: String,
    text: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.text)
            .unwrap_or_else(|error| panic!("{error}: {}\n{:?}", self.head, self.text))
    }

    fn header(&self, name: &str) -> Option<String> {
        self.head.lines().find_map(|line| {
            line.split_once(':')
                .filter(|(found, _)| found.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.trim().to_owned())
        })
    }
}

/// `head` (request line and headers, without the blank line) and `body`, written on a connection
/// of their own; `Host` and `Connection: close` are added.
fn raw(address: SocketAddr, head: &str, body: &[u8]) -> Reply {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    stream
        .set_write_timeout(Some(ANSWER))
        .expect("a write timeout");
    let mut request =
        format!("{head}\r\nHost: {address}\r\nConnection: close\r\n\r\n").into_bytes();
    request.extend_from_slice(body);
    stream.write_all(&request).expect("the request is written");
    let mut answer = Vec::new();
    stream
        .read_to_end(&mut answer)
        .expect("the answer is read before the timeout");
    let answer = String::from_utf8(answer).expect("the answer is UTF-8");
    let (head, text) = answer
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("an answer with a head: {answer:?}"));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {head:?}"));
    Reply {
        status,
        head: head.to_owned(),
        text: text.to_owned(),
    }
}

/// `method path` with a JSON body announced by `Content-Length`; no body when `body` is `None`.
fn send(address: SocketAddr, method: &str, path: &str, body: Option<&Value>) -> Reply {
    match body {
        Some(body) => {
            let text = body.to_string();
            raw(
                address,
                &format!(
                    "{method} {path} HTTP/1.1\r\nContent-Type: application/json\r\nContent-Length: {}",
                    text.len()
                ),
                text.as_bytes(),
            )
        }
        None => raw(address, &format!("{method} {path} HTTP/1.1"), b""),
    }
}

fn shelve(address: SocketAddr, id: &str, title: &str) {
    let reply = send(
        address,
        "POST",
        "/books",
        Some(&json!({"id": id, "title": title})),
    );
    assert_eq!(reply.status, 201, "{}\n{}", reply.head, reply.text);
}

fn listed(address: SocketAddr) -> Vec<String> {
    let reply = send(address, "GET", "/books", None);
    assert_eq!(reply.status, 200, "{}", reply.text);
    reply.json()["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("rows: {}", reply.text))
        .iter()
        .map(|row| row["id"].as_str().expect("an id").to_owned())
        .collect()
}

fn events(address: SocketAddr) -> usize {
    let reply = send(address, "GET", "/api/events", None);
    assert_eq!(reply.status, 200, "{}", reply.text);
    reply.json()["events"]
        .as_array()
        .unwrap_or_else(|| panic!("events: {}", reply.text))
        .len()
}

/// The unit's fixture with `b1` and `b2` shelved.
fn unit_shelf() -> (Served, Contract) {
    let contract = Contract::bound(UNIT_SPEC, UNIT_BINDING);
    let served = serve(UNIT_SPEC, UNIT_BINDING);
    shelve(served.address, "b1", "Atlas of Rivers");
    shelve(served.address, "b2", "Field Notes");
    (served, contract)
}

/// The adversary fixture with `b1` and `b2` shelved.
fn shelf() -> (Served, Contract) {
    let contract = Contract::bound(SPEC, BINDING);
    let served = serve(SPEC, BINDING);
    shelve(served.address, "b1", "Atlas of Rivers");
    shelve(served.address, "b2", "Field Notes");
    (served, contract)
}

#[test]
fn a_delete_whose_body_carries_the_path_field_or_no_object_is_refused_and_writes_nothing() {
    let (served, _) = unit_shelf();
    let address = served.address;
    let before = events(address);
    for body in [
        json!({"id": "b1"}),
        json!({"id": "b2"}),
        json!({"title": "Atlas of Rivers"}),
        json!([]),
        json!("b1"),
    ] {
        let reply = send(address, "DELETE", "/books/b1", Some(&body));
        assert_eq!(reply.status, 400, "{body}: {}\n{}", reply.head, reply.text);
        assert!(reply.json()["refused"].is_string(), "{}", reply.text);
    }
    assert_eq!(
        events(address),
        before,
        "a refused DELETE publishes nothing"
    );
    assert_eq!(listed(address), ["b1", "b2"]);
}

#[test]
fn a_delete_with_an_empty_announced_body_or_an_empty_object_is_the_path_input() {
    let (served, contract) = unit_shelf();
    let address = served.address;

    let reply = raw(
        address,
        "DELETE /books/b1 HTTP/1.1\r\nContent-Type: application/json\r\nContent-Length: 0",
        b"",
    );
    assert_eq!(reply.status, 200, "{}\n{}", reply.head, reply.text);
    let answer = reply.json();
    assert_eq!(
        answer["published"],
        json!([{"event": "library.shelf.BookRemoved", "payload": {"id": "b1"}}])
    );
    contract.validate(REMOVE_BOOK, 200, &answer);

    let reply = raw(
        address,
        "DELETE /books/b2 HTTP/1.1\r\nContent-Type: text/plain\r\nContent-Length: 2",
        b"{}",
    );
    assert_eq!(reply.status, 200, "{}\n{}", reply.head, reply.text);
    assert_eq!(reply.json()["outcome"], "removed");
    assert!(listed(address).is_empty());
}

#[test]
fn a_chunked_delete_is_refused_411_and_writes_nothing() {
    let (served, _) = unit_shelf();
    let address = served.address;
    let before = events(address);
    let reply = raw(
        address,
        "DELETE /books/b1 HTTP/1.1\r\nTransfer-Encoding: chunked",
        b"0\r\n\r\n",
    );
    assert_eq!(reply.status, 411, "{}\n{}", reply.head, reply.text);
    assert!(reply.json()["refused"].is_string(), "{}", reply.text);
    assert_eq!(events(address), before);
    assert_eq!(listed(address), ["b1", "b2"]);
}

#[test]
fn a_delete_of_an_unknown_or_removed_book_answers_its_declared_refusal_and_writes_nothing() {
    let (served, contract) = unit_shelf();
    let address = served.address;

    let before = events(address);
    let reply = send(address, "DELETE", "/books/nobody", None);
    assert_eq!(reply.status, 404, "{}", reply.text);
    let answer = reply.json();
    assert_eq!(answer["outcome"], "no-such-book");
    assert_eq!(answer["error"], "library.shelf.BookNotFound");
    assert_eq!(answer["published"], json!([]));
    contract.validate(REMOVE_BOOK, 404, &answer);
    assert_eq!(events(address), before, "an unknown id writes nothing");

    let reply = send(address, "DELETE", "/books/b1", None);
    assert_eq!(reply.status, 200, "{}", reply.text);
    let removed = events(address);
    assert_eq!(removed, before + 1);
    let reply = send(address, "DELETE", "/books/b1", None);
    assert_eq!(reply.status, 409, "{}", reply.text);
    contract.validate(REMOVE_BOOK, 409, &reply.json());
    assert_eq!(
        events(address),
        removed,
        "a refused second DELETE writes nothing"
    );
    assert_eq!(listed(address), ["b2"]);
}

#[test]
fn a_percent_encoded_id_is_decoded_before_the_command_runs() {
    let (served, contract) = unit_shelf();
    let address = served.address;
    shelve(address, "b/1", "Slashed");
    shelve(address, "a b", "Spaced");
    shelve(address, "c+d%e", "Plussed");

    for (path, id) in [
        ("/books/b%2F1", "b/1"),
        ("/books/a%20b", "a b"),
        ("/books/c+d%25e", "c+d%e"),
    ] {
        let reply = send(address, "DELETE", path, None);
        assert_eq!(reply.status, 200, "{path}: {}", reply.text);
        let answer = reply.json();
        assert_eq!(
            answer["published"],
            json!([{"event": "library.shelf.BookRemoved", "payload": {"id": id}}]),
            "{path}"
        );
        contract.validate(REMOVE_BOOK, 200, &answer);
    }

    let before = events(address);
    for path in ["/books/%ZZ", "/books/%C3", "/books/b%"] {
        let reply = send(address, "DELETE", path, None);
        assert_eq!(reply.status, 400, "{path}: {}", reply.text);
        assert!(reply.json()["refused"].is_string(), "{}", reply.text);
    }
    assert_eq!(events(address), before);
    assert_eq!(listed(address), ["b1", "b2"]);
}

/// ESS's served application matches a template variable only to a segment that is not empty
/// (`ess-synth` `http::matched`: "each variable holding a segment that is not empty"), so
/// `DELETE /books/` is a path no route has: `404`, and nothing runs.
#[test]
fn a_delete_to_an_empty_path_segment_is_no_route_and_runs_nothing() {
    let (served, _) = unit_shelf();
    let address = served.address;
    shelve(address, "", "Untitled shelf mark");
    assert_eq!(listed(address), ["", "b1", "b2"]);
    let before = events(address);

    let reply = send(address, "DELETE", "/books/", None);
    assert_eq!(
        reply.status, 404,
        "ESS answers 404 for an empty variable segment: {}\n{}",
        reply.head, reply.text
    );
    assert!(reply.json()["refused"].is_string(), "{}", reply.text);
    assert_eq!(events(address), before, "no command ran");
    assert_eq!(listed(address), ["", "b1", "b2"]);
}

#[test]
fn head_and_options_on_a_delete_path_are_refused_405_with_allow() {
    let (served, _) = unit_shelf();
    let address = served.address;
    let before = events(address);
    for method in ["HEAD", "OPTIONS", "delete", "POST"] {
        let reply = raw(address, &format!("{method} /books/b1 HTTP/1.1"), b"");
        assert_eq!(
            reply.status, 405,
            "{method}: {}\n{}",
            reply.head, reply.text
        );
        assert_eq!(
            reply.header("allow").as_deref(),
            Some("DELETE"),
            "{method}: {}",
            reply.head
        );
    }
    assert_eq!(events(address), before);
    assert_eq!(listed(address), ["b1", "b2"]);
}

/// `RemoveBook` declares `reason` beside the `id` the path carries. ESS publishes a required
/// request body for it, and its served application reads a missing body as malformed: `400`,
/// nothing run.
#[test]
fn a_delete_whose_command_needs_more_than_the_path_is_refused_without_a_body() {
    let (served, contract) = shelf();
    let address = served.address;
    let (method, path, operation) = contract.operation(REMOVE_BOOK);
    assert_eq!((method.as_str(), path.as_str()), ("DELETE", "/books/{id}"));
    assert_eq!(
        operation["requestBody"]["required"], true,
        "ESS publishes the body as required: {operation}"
    );

    let before = events(address);
    let reply = send(address, "DELETE", "/books/b1", None);
    assert_eq!(reply.status, 400, "{}\n{}", reply.head, reply.text);
    assert!(reply.json()["refused"].is_string(), "{}", reply.text);
    let reply = send(address, "DELETE", "/books/b1", Some(&json!({})));
    assert_eq!(reply.status, 400, "{}\n{}", reply.head, reply.text);
    assert_eq!(
        events(address),
        before,
        "a DELETE missing its reason runs nothing"
    );
    assert_eq!(listed(address), ["b1", "b2"]);

    let reply = send(
        address,
        "DELETE",
        "/books/b1",
        Some(&json!({"reason": "worn"})),
    );
    assert_eq!(reply.status, 200, "{}\n{}", reply.head, reply.text);
    let answer = reply.json();
    assert_eq!(
        answer["published"],
        json!([{"event": "library.shelf.BookRemoved", "payload": {"id": "b1", "reason": "worn"}}])
    );
    contract.validate(REMOVE_BOOK, 200, &answer);
    let reply = send(address, "GET", "/books", None);
    contract.validate(SHELVED_BOOKS, reply.status, &reply.json());
    assert_eq!(listed(address), ["b2"]);
}

fn allowed(reply: &Reply) -> Vec<String> {
    let allow = reply
        .header("allow")
        .unwrap_or_else(|| panic!("a 405 names what the path answers in Allow: {}", reply.head));
    let mut methods: Vec<String> = allow.split(',').map(|m| m.trim().to_owned()).collect();
    methods.sort();
    methods
}

#[test]
fn a_405_on_a_path_with_two_methods_allows_both() {
    let (served, _) = shelf();
    let address = served.address;
    let before = events(address);
    for method in ["GET", "POST", "PATCH", "HEAD"] {
        let reply = raw(address, &format!("{method} /books/b1 HTTP/1.1"), b"");
        assert_eq!(
            reply.status, 405,
            "{method}: {}\n{}",
            reply.head, reply.text
        );
        assert_eq!(
            allowed(&reply),
            ["DELETE", "PUT"],
            "{method}: {}",
            reply.head
        );
    }
    assert_eq!(events(address), before);

    let reply = send(address, "PUT", "/books/b1", None);
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.json()["outcome"], "lent");
    let reply = send(
        address,
        "DELETE",
        "/books/b1",
        Some(&json!({"reason": "lost"})),
    );
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.json()["outcome"], "removed");
}

/// ESS's served application sorts the methods it names in `Allow` (`ess-synth`
/// `rust/http/bound.rs`, `dispatch`: `allowed.sort_unstable(); allowed.dedup();`), and spindle's
/// listener says it refuses "as ESS's served application writes it".
#[test]
fn a_405_names_its_methods_in_the_order_esss_served_application_writes_them() {
    let (served, _) = shelf();
    let reply = raw(served.address, "GET /books/b1 HTTP/1.1", b"");
    assert_eq!(reply.status, 405, "{}", reply.head);
    assert_eq!(
        reply.header("allow").as_deref(),
        Some("DELETE, PUT"),
        "{}",
        reply.head
    );
}
