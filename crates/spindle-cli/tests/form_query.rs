//! `run` serving commands an `ess-http/1` binding reads as `request: {encoding: form}` or
//! `{encoding: query}`.
//!
//! Each case states what the server ESS synthesises for the binding does (`ess-synth`,
//! `rust/http/bound.rs`: `flat_object`, `content_type_is`, `refusal_malformed`) and asserts that
//! `run` does it too; a command's answer is checked against the `OpenAPI` document ESS projects
//! for the binding (`ess_gen::openapi::bound_json`).
//!
//! `tests/fixtures/form-query`: `CatalogueBook` on `POST /books/{id}` read from a form body,
//! `LendBook` on `GET /books/{id}/lend` and `RetireBook` on `DELETE /books/{id}` read from the
//! query string, and `CataloguedBooks` on `GET /books`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

const LAPTOP: &str = "examples/environments/laptop.toml";
const SPEC: &str = "crates/spindle-cli/tests/fixtures/form-query";
const BINDING: &str = "crates/spindle-cli/tests/fixtures/form-query-http.yaml";
const COMPONENT: &str = "catalogue";

const CATALOGUE_BOOK: &str = "library.catalogue.CatalogueBook";
const LEND_BOOK: &str = "library.catalogue.LendBook";
const RETIRE_BOOK: &str = "library.catalogue.RetireBook";
const CATALOGUED_BOOKS: &str = "library.catalogue.CataloguedBooks";

const FORM: &str = "application/x-www-form-urlencoded";

const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The `OpenAPI` document ESS projects for `catalogue` with the binding.
struct Contract(Value);

impl Contract {
    fn bound() -> Self {
        let loaded = spindle_spec::load(&root().join(SPEC)).expect("the fixture compiles");
        let text = std::fs::read_to_string(root().join(BINDING)).expect("the binding is read");
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

    fn validate(&self, id: &str, status: u16, body: &Value) {
        let paths = self.0["paths"].as_object().expect("the document has paths");
        let schema = paths
            .values()
            .flat_map(|item| item.as_object().expect("a path item is an object").values())
            .find(|operation| operation["operationId"] == id)
            .map(|operation| {
                &operation["responses"][status.to_string()]["content"]["application/json"]["schema"]
            })
            .unwrap_or_else(|| panic!("the contract declares no operation {id}"));
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

fn serve() -> Served {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--environment", LAPTOP, "--state"])
        .arg(state.path().join("state"))
        .args(["--http", BINDING])
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

/// One answer: the status and the body.
struct Reply {
    status: u16,
    text: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.text)
            .unwrap_or_else(|error| panic!("{error}: {} {:?}", self.status, self.text))
    }

    fn refused(&self) -> String {
        self.json()["refused"]
            .as_str()
            .unwrap_or_else(|| panic!("a refusal: {} {}", self.status, self.text))
            .to_owned()
    }
}

/// `method target`, with `body` under `content_type` where one is given, on a connection of its
/// own.
fn send(
    address: SocketAddr,
    method: &str,
    target: &str,
    content_type: Option<&str>,
    body: &[u8],
) -> Reply {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    stream
        .set_write_timeout(Some(ANSWER))
        .expect("a write timeout");
    let mut head =
        format!("{method} {target} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n");
    if let Some(content_type) = content_type {
        head.push_str(&format!("Content-Type: {content_type}\r\n"));
    }
    head.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
    let mut request = head.into_bytes();
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
        text: text.to_owned(),
    }
}

fn form(address: SocketAddr, id: &str, body: &str) -> Reply {
    send(
        address,
        "POST",
        &format!("/books/{id}"),
        Some(FORM),
        body.as_bytes(),
    )
}

fn rows(address: SocketAddr) -> Value {
    let reply = send(address, "GET", "/books", None, b"");
    assert_eq!(reply.status, 200, "{}", reply.text);
    reply.json()["rows"].clone()
}

fn events(address: SocketAddr) -> usize {
    let reply = send(address, "GET", "/api/events", None, b"");
    assert_eq!(reply.status, 200, "{}", reply.text);
    reply.json()["events"]
        .as_array()
        .unwrap_or_else(|| panic!("events: {}", reply.text))
        .len()
}

#[test]
fn a_form_encoded_command_runs() {
    let contract = Contract::bound();
    let served = serve();
    let address = served.address;

    let reply = form(address, "dune", "title=Dune&year=1965");
    assert_eq!(reply.status, 201, "{}", reply.text);
    let answer = reply.json();
    assert_eq!(answer["outcome"], "catalogued");
    assert_eq!(
        answer["published"],
        json!([{
            "event": "library.catalogue.BookCatalogued",
            "payload": {"id": "dune", "title": "Dune", "year": 1965},
        }]),
        "the path carries the id and the form the title and the year, read as an integer"
    );
    contract.validate(CATALOGUE_BOOK, 201, &answer);

    let reply = send(
        address,
        "POST",
        "/books/left-hand",
        Some("Application/X-WWW-Form-Urlencoded; charset=UTF-8"),
        b"year=1969&title=The+Left+Hand+of%20Darkness",
    );
    assert_eq!(
        reply.status, 201,
        "the media type is compared without its parameters and its case, and `+` and `%XX` are decoded: {}",
        reply.text
    );
    contract.validate(CATALOGUE_BOOK, 201, &reply.json());

    let listed = rows(address);
    assert_eq!(
        listed,
        json!([
            {"id": "dune", "title": "Dune", "year": 1965},
            {"id": "left-hand", "title": "The Left Hand of Darkness", "year": 1969},
        ])
    );
    contract.validate(CATALOGUED_BOOKS, 200, &json!({"rows": listed}));
}

#[test]
fn a_form_value_the_decoder_refuses_is_refused_400_and_writes_nothing() {
    let served = serve();
    let address = served.address;
    let before = events(address);
    for body in [
        "title=Dune&year=nineteen",
        "title=Dune&year=",
        "title=Dune&title=Dune&year=1965",
        "title=Dune",
        "title=%ZZ&year=1965",
    ] {
        let reply = form(address, "dune", body);
        assert_eq!(reply.status, 400, "{body}: {}", reply.text);
        reply.refused();
    }
    assert_eq!(events(address), before, "a refused form writes nothing");
    assert_eq!(rows(address), json!([]));
}

#[test]
fn a_query_encoded_command_runs() {
    let contract = Contract::bound();
    let served = serve();
    let address = served.address;
    assert_eq!(form(address, "dune", "title=Dune&year=1965").status, 201);
    assert_eq!(
        form(address, "solaris", "title=Solaris&year=1961").status,
        201
    );

    let reply = send(address, "GET", "/books/dune/lend?days=14", None, b"");
    assert_eq!(reply.status, 200, "{}", reply.text);
    let answer = reply.json();
    assert_eq!(answer["outcome"], "lent");
    assert_eq!(
        answer["published"],
        json!([{"event": "library.catalogue.BookLent", "payload": {"id": "dune", "days": 14}}]),
        "the query carries the days, read as an integer, on a command bound to get"
    );
    contract.validate(LEND_BOOK, 200, &answer);

    let reply = send(
        address,
        "DELETE",
        "/books/solaris?reason=water+damage%21",
        None,
        b"",
    );
    assert_eq!(reply.status, 200, "{}", reply.text);
    let answer = reply.json();
    assert_eq!(answer["outcome"], "retired");
    assert_eq!(
        answer["published"],
        json!([{
            "event": "library.catalogue.BookRetired",
            "payload": {"id": "solaris", "reason": "water damage!"},
        }]),
        "the query carries the reason, form-decoded, on a command bound to delete"
    );
    contract.validate(RETIRE_BOOK, 200, &answer);

    let reply = send(address, "DELETE", "/books/solaris?reason=again", None, b"");
    assert_eq!(reply.status, 409, "{}", reply.text);
    contract.validate(RETIRE_BOOK, 409, &reply.json());

    assert_eq!(
        rows(address),
        json!([{"id": "dune", "title": "Dune", "year": 1965}])
    );
}

#[test]
fn a_query_without_a_required_field_is_refused_400_and_writes_nothing() {
    let served = serve();
    let address = served.address;
    assert_eq!(form(address, "dune", "title=Dune&year=1965").status, 201);
    let before = events(address);
    for target in [
        "/books/dune",
        "/books/dune?",
        "/books/dune?reason=a&reason=b",
    ] {
        let reply = send(address, "DELETE", target, None, b"");
        assert_eq!(reply.status, 400, "{target}: {}", reply.text);
        reply.refused();
    }
    let reply = send(address, "GET", "/books/dune/lend?days=two", None, b"");
    assert_eq!(reply.status, 400, "{}", reply.text);
    assert_eq!(events(address), before, "a refused query writes nothing");
}

#[test]
fn a_form_field_the_command_does_not_declare_is_refused_400() {
    let served = serve();
    let address = served.address;
    assert_eq!(
        form(address, "solaris", "title=Solaris&year=1961").status,
        201
    );
    let before = events(address);

    let reply = form(address, "dune", "title=Dune&year=1965&author=Herbert");
    assert_eq!(reply.status, 400, "{}", reply.text);
    let refused = reply.refused();
    assert!(
        refused.contains("body.author"),
        "the refusal names the field: {refused}"
    );

    let reply = send(
        address,
        "DELETE",
        "/books/solaris?reason=worn&colour=red",
        None,
        b"",
    );
    assert_eq!(reply.status, 400, "{}", reply.text);
    let refused = reply.refused();
    assert!(
        refused.contains("query.colour"),
        "the refusal names the parameter: {refused}"
    );

    assert_eq!(events(address), before, "nothing is written");
    assert_eq!(
        rows(address),
        json!([{"id": "solaris", "title": "Solaris", "year": 1961}])
    );
}

#[test]
fn a_form_field_carried_in_the_path_too_is_refused_400() {
    let served = serve();
    let address = served.address;
    assert_eq!(
        form(address, "solaris", "title=Solaris&year=1961").status,
        201
    );
    let before = events(address);

    for body in [
        "id=dune&title=Dune&year=1965",
        "id=other&title=Dune&year=1965",
    ] {
        let reply = form(address, "dune", body);
        assert_eq!(reply.status, 400, "{body}: {}", reply.text);
        let refused = reply.refused();
        assert!(
            refused.contains("body.id"),
            "the refusal names the field: {refused}"
        );
    }

    let reply = send(
        address,
        "DELETE",
        "/books/solaris?id=solaris&reason=worn",
        None,
        b"",
    );
    assert_eq!(reply.status, 400, "{}", reply.text);
    assert!(reply.refused().contains("query.id"), "{}", reply.text);

    assert_eq!(events(address), before, "nothing is written");
    assert_eq!(
        rows(address),
        json!([{"id": "solaris", "title": "Solaris", "year": 1961}])
    );
}

/// ESS's served application refuses a `form` request whose `Content-Type` is not
/// `application/x-www-form-urlencoded` with its standard `malformed` refusal, `400`
/// (`ess-synth` `rust/http/bound.rs`: `content_type_is`, then `refusal_malformed`, which answers
/// `http::Response::refusal(400, detail)` where the binding overrides nothing), not `415`.
#[test]
fn a_json_body_to_a_form_route_is_refused() {
    let served = serve();
    let address = served.address;
    let before = events(address);

    let body = json!({"title": "Dune", "year": 1965}).to_string();
    for content_type in [Some("application/json"), Some("text/plain"), None] {
        let reply = send(
            address,
            "POST",
            "/books/dune",
            content_type,
            body.as_bytes(),
        );
        assert_eq!(reply.status, 400, "{content_type:?}: {}", reply.text);
        assert_eq!(
            reply.refused(),
            "the body is not `application/x-www-form-urlencoded`"
        );
    }
    let reply = send(
        address,
        "POST",
        "/books/dune",
        Some("application/json"),
        b"title=Dune&year=1965",
    );
    assert_eq!(
        reply.status, 400,
        "the declared content type decides, not the bytes: {}",
        reply.text
    );

    assert_eq!(events(address), before, "nothing is written");
    assert_eq!(rows(address), json!([]));
}
