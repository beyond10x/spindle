//! `run` serving a command whose outcome deletes its subject (`deletes:`) on an `ess-http/1`
//! `method: delete` route.
//!
//! The fixture `tests/fixtures/kernel-delete` declares `RemoveBook`, whose `removed` outcome
//! deletes the book, on `DELETE /books/{id}`; `tests/fixtures/kernel-delete-http.yaml` binds it.
//! Every answer is validated against the schema the `OpenAPI` document ESS projects for the
//! binding declares for it under its status.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/kernel-delete";
const BINDING: &str = "crates/spindle-cli/tests/fixtures/kernel-delete-http.yaml";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "shelf";

const SHELVE_BOOK: &str = "library.shelf.ShelveBook";
const REMOVE_BOOK: &str = "library.shelf.RemoveBook";
const SHELVED_BOOKS: &str = "library.shelf.ShelvedBooks";

const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The `OpenAPI` document ESS projects for `shelf` with the binding.
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

    fn route(&self, id: &str) -> (String, String) {
        let paths = self.0["paths"].as_object().expect("the document has paths");
        for (path, item) in paths {
            for (method, operation) in item.as_object().expect("a path item is an object") {
                if operation["operationId"] == id {
                    return (method.to_uppercase(), path.clone());
                }
            }
        }
        panic!("the contract declares no operation {id}")
    }

    fn validate(&self, id: &str, status: u16, body: &Value) {
        let (method, path) = self.route(id);
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

    /// The status the binding lists for `outcome` of `id`.
    fn status(&self, id: &str, outcome: &str) -> u16 {
        let (method, path) = self.route(id);
        let responses = self.0["paths"][&path][method.to_lowercase()]["responses"]
            .as_object()
            .expect("the operation has responses");
        responses
            .iter()
            .find(|(_, response)| response.to_string().contains(&format!("\"{outcome}\"")))
            .and_then(|(status, _)| status.parse().ok())
            .unwrap_or_else(|| panic!("the contract lists no status for {id}/{outcome}"))
    }
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

/// `run` on the fixture with its binding, its state at `state`.
fn serve(state: &Path) -> Served {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--environment", LAPTOP, "--state"])
        .arg(state)
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
    Served { child, address }
}

/// One HTTP/1.1 exchange on its own connection: the status and the JSON body.
fn exchange(address: SocketAddr, method: &str, path: &str, body: Option<&Value>) -> (u16, Value) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    stream
        .set_write_timeout(Some(ANSWER))
        .expect("a write timeout");
    let request = match body {
        Some(body) => {
            let body = body.to_string();
            format!(
                "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
        }
        None => format!("{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"),
    };
    stream
        .write_all(request.as_bytes())
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

#[test]
fn a_delete_route_runs_a_deleting_outcome() {
    let contract = Contract::bound();
    assert_eq!(
        contract.route(REMOVE_BOOK),
        ("DELETE".to_owned(), "/books/{id}".to_owned())
    );
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let state = state.path().join("state");
    let served = serve(&state);
    for (id, title) in [("b1", "Atlas of Rivers"), ("b2", "Field Notes")] {
        let (status, answer) = exchange(
            served.address,
            "POST",
            "/books",
            Some(&json!({"id": id, "title": title})),
        );
        assert_eq!(status, 201, "{answer}");
        contract.validate(SHELVE_BOOK, status, &answer);
    }

    let (status, answer) = exchange(served.address, "DELETE", "/books/b1", None);
    assert_eq!(
        status,
        contract.status(REMOVE_BOOK, "removed"),
        "the binding's status for removed: {answer}"
    );
    assert_eq!(status, 200, "{answer}");
    assert_eq!(
        answer,
        json!({
            "outcome": "removed",
            "published": [{"event": "library.shelf.BookRemoved", "payload": {"id": "b1"}}],
        })
    );
    contract.validate(REMOVE_BOOK, status, &answer);

    let (status, answer) = exchange(served.address, "GET", "/books", None);
    assert_eq!(status, 200, "{answer}");
    assert_eq!(
        answer,
        json!({"rows": [{"id": "b2", "title": "Field Notes"}]}),
        "the deleted book leaves the view"
    );
    contract.validate(SHELVED_BOOKS, status, &answer);

    let (status, answer) = exchange(served.address, "DELETE", "/books/b1", None);
    assert_eq!(
        answer["error"], "library.shelf.BookNotFound",
        "a second DELETE answers the unknown instance: {status} {answer}"
    );
    contract.validate(REMOVE_BOOK, status, &answer);
    drop(served);

    let served = serve(&state);
    let (status, answer) = exchange(served.address, "GET", "/books", None);
    assert_eq!(status, 200, "{answer}");
    assert_eq!(
        answer,
        json!({"rows": [{"id": "b2", "title": "Field Notes"}]}),
        "the deletion is written to the state, so a restarted run still leaves the book out"
    );
}
