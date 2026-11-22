//! The request log keeps no resource identifier (`story:private-local-state`): a record keeps
//! the route template a request resolved to, never the raw path with the identifiers in it, and
//! of a request no route serves only the first segment of its path. In 0.17.0 a record kept the
//! raw path beside the template, so `GET /__spindle/requests` listed every identity a caller
//! asked for.
//!
//! The fixture `tests/fixtures/shelf` serves `library.shelf.BookById` at `/books/{id}` with the
//! binding `tests/fixtures/shelf-http.yaml`.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ess_compiler::EssIr;
use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{Listener, Surface};

const COMPONENT: &str = "shelf";
const IDENTITY: &str = "abc-123";
/// How many exchanges one request may wait for before the test gives up.
const STEPS: usize = 50;
/// How long one wait for an exchange lasts.
const WAIT: Duration = Duration::from_millis(200);

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn ir() -> EssIr {
    spindle_spec::load(&fixtures().join("shelf"))
        .expect("the shelf fixture compiles")
        .ir
}

fn binding(ir: &EssIr) -> ess_gen::http::Binding {
    let text = std::fs::read_to_string(fixtures().join("shelf-http.yaml")).expect("the binding");
    let spec = ess_http::HttpSpec::from_yaml(&text).expect("the binding parses");
    let http = ess_http::compile(&spec, ir, &ess_gen::http::derived(ir))
        .unwrap_or_else(|refused| panic!("ESS refuses the binding: {refused}"));
    ess_gen::http::Binding::new(http).unwrap_or_else(|refused| panic!("not servable: {refused}"))
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"private-record\"\n",
        "os = \"linux\"\n",
        "arch = \"x86_64\"\n",
        "\n",
        "[provides]\n",
        "\"store.durable\" = [\"memory\"]\n",
        "\"store.ephemeral\" = [\"memory\"]\n",
        "clock = [\"system\"]\n",
        "timer = [\"tokio\"]\n",
        "channel = [\"in-process\"]\n",
        "projection = [\"in-process\"]\n",
        "\"ingress.network\" = [\"http-loopback\"]\n",
    ))
    .expect("the in-memory environment parses")
}

/// One HTTP/1.1 request with no body, `Connection: close`: the status and the body as text.
fn request(address: SocketAddr, method: &str, target: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(address).expect("the listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a read timeout");
    write!(
        stream,
        "{method} {target} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    )
    .expect("the request is written");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("the response is read");
    let (head, body) = response
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("a response without a body: {response}"));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a response without a status: {head}"));
    (status, body.to_owned())
}

/// Sends `GET target` and serves it on `host`: the status and the body.
fn get(listener: &Listener, host: &mut Host, target: &str) -> (u16, String) {
    let address = listener.local_addr();
    let target = target.to_owned();
    let client = thread::spawn(move || request(address, "GET", &target));
    for _ in 0..STEPS {
        if client.is_finished() {
            break;
        }
        if let Some(exchange) = listener.next(WAIT) {
            exchange.serve(host);
        }
    }
    assert!(client.is_finished(), "the request was not answered in time");
    client.join().expect("the client thread")
}

#[test]
fn a_request_record_keeps_the_route_not_the_identifier() {
    let ir = ir();
    let binding = binding(&ir);
    let surface = Surface::new(&ir, &[COMPONENT.to_owned()], Some(&binding))
        .unwrap_or_else(|refused| panic!("the shelf surface is served: {refused}"))
        .observing(16);
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .unwrap_or_else(|error| panic!("the host admits the shelf: {error}"));
    let Value::Object(input) = json!({"book_id": IDENTITY, "title": "Atlas", "year": 1990}) else {
        unreachable!("an object")
    };
    host.submit("library.shelf.ShelveBook", input)
        .unwrap_or_else(|error| panic!("the book is shelved: {error}"));

    let (status, body) = get(&listener, &mut host, &format!("/books/{IDENTITY}"));
    assert_eq!(status, 200, "{body}");
    assert!(
        body.contains(IDENTITY),
        "the answer itself names the book: {body}"
    );
    let (status, body) = get(&listener, &mut host, &format!("/books/{IDENTITY}/pages"));
    assert_eq!(status, 404, "{body}");

    let (status, log) = request(listener.local_addr(), "GET", "/__spindle/requests");
    assert_eq!(status, 200, "{log}");
    assert!(
        !log.contains(IDENTITY),
        "the request log names no identifier a caller asked for: {log}"
    );
    let log: Value = serde_json::from_str(&log).expect("the log is JSON");
    let records = log["requests"].as_array().expect("requests");
    assert_eq!(records.len(), 2, "{log}");
    let (unrouted, served) = (&records[0], &records[1]);
    assert_eq!(served["route"], "/books/{id}", "{log}");
    assert_eq!(served["path"], "/books/{id}", "{log}");
    assert_eq!(served["class"], "served", "{log}");
    assert!(unrouted.get("route").is_none(), "{log}");
    assert_eq!(
        unrouted["path"], "/books/…",
        "an unrouted request keeps its first segment only: {log}"
    );
}
