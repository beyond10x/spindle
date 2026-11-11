//! Adversary cases for wave 21's grant check (`story:unauthenticated-grant-refusals`) and bounded
//! refusal reasons (`story:refusal-reason-bounded`).
//!
//! The fixture `tests/fixtures/adversary-sheet` declares one component, `sheet`: `tally.sheet.Clerk`
//! may stamp a sheet, a command that reads the clerk's `desk` attribute; no grant names
//! `tally.sheet.Tally`, whose input holds a map keyed by number and one keyed by name. The fixture
//! `tests/fixtures/grants` (with `grants-http.yaml`) serves `vault.ledger.FileEntry` from a `form`
//! body.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ess_compiler::EssIr;
use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{Caller, Listener, Request, Surface};

const STEPS: usize = 50;
const WAIT: Duration = Duration::from_millis(200);

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn loaded(name: &str) -> EssIr {
    spindle_spec::load(&fixtures().join(name))
        .unwrap_or_else(|error| panic!("the {name} fixture compiles: {error:?}"))
        .ir
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"adversary-w21\"\n",
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

fn host(ir: EssIr) -> (Host, tempfile::TempDir) {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .unwrap_or_else(|error| panic!("the host admits the fixture: {error}"));
    (host, state)
}

fn at(surface: &Surface, operation: &str) -> (String, String) {
    let route = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == operation)
        .unwrap_or_else(|| panic!("the surface serves {operation}"));
    (route.method.as_str().to_uppercase(), route.path.clone())
}

/// One raw HTTP/1.1 exchange: the status and the body as text.
fn exchange(address: SocketAddr, text: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(address).expect("the listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .expect("a read timeout");
    stream
        .write_all(text.as_bytes())
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

fn request(method: &str, path: &str, content_type: &str, body: &[u8]) -> String {
    format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: \
         {content_type}\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        String::from_utf8_lossy(body)
    )
}

/// `text` sent to `listener`, its exchange served on `host`.
fn served(listener: &Listener, host: &mut Host, text: String) -> (u16, String) {
    let address = listener.local_addr();
    let client = thread::spawn(move || exchange(address, &text));
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

/// Every record's reason in `GET /__spindle/requests`, newest first.
fn reasons(listener: &Listener) -> Vec<String> {
    let (status, log) = exchange(
        listener.local_addr(),
        "GET /__spindle/requests HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    );
    assert_eq!(status, 200, "{log}");
    let log: Value = serde_json::from_str(&log).expect("the log is JSON");
    log["requests"]
        .as_array()
        .expect("requests")
        .iter()
        .filter_map(|record| record["reason"].as_str().map(str::to_owned))
        .collect()
}

/// `story:unauthenticated-grant-refusals`: without a fixed caller, a command some actor's `may:`
/// names is refused ESS's standard `403` `{"refused": "not granted", "actor": null}`, as the
/// CHANGELOG states for "a command some actor's `may:` names". A granted command that reads its
/// caller is answered `501` instead, naming the attributes it reads, because the route is marked
/// unserved before the grant check (`answer.rs` `answered_as`: the `Unserved::Ingress` refusal
/// precedes `granted`, and `surface.rs` `route` leaves `not_granted` false on that branch).
#[test]
fn a_granted_command_that_reads_its_caller_without_a_caller_is_refused_403() {
    let ir = loaded("adversary-sheet");
    let surface = Surface::fixing_callers(&ir, &["sheet".to_owned()], None, &[])
        .unwrap_or_else(|refused| panic!("the sheet surface is served: {refused}"));
    let (method, path) = at(&surface, "tally.sheet.Stamp");
    let (mut host, _state) = host(ir);
    let routed = spindle_ingress::route(
        &surface,
        &mut host,
        Request {
            method,
            path,
            query: String::new(),
            content_type: Some("application/json".to_owned()),
            body: br#"{"number": 1}"#.to_vec(),
        },
    );
    let body: Value = serde_json::from_str(&routed.response.body).expect("a JSON body");
    assert_eq!(
        (routed.response.status, body),
        (403, json!({"refused": "not granted", "actor": null})),
        "a command the clerk's grant names, sent with no caller"
    );
}

/// The sheet surface, observed, listening.
fn observed_sheet() -> (Listener, Host, Surface, tempfile::TempDir) {
    let ir = loaded("adversary-sheet");
    let surface = Surface::new(&ir, &["sheet".to_owned()], None)
        .unwrap_or_else(|refused| panic!("the sheet surface is served: {refused}"))
        .observing(16);
    let (host, state) = host(ir);
    let listener = Listener::bind(0, surface.clone()).expect("the listener binds");
    (listener, host, surface, state)
}

/// `story:refusal-reason-bounded`: "A value the caller sent is never recorded"
/// (`ess/domains/observe.yaml`, `RequestRecord.reason`). A map is a value, and its keys are part
/// of it; the kernel names a key that is not a map key of the declared type in backticks
/// (`typed.rs`: `` `{spelling}` is not a {key} map key ``), and `reason` keeps a backticked plain
/// name of up to 64 characters.
#[test]
fn a_map_key_the_caller_sent_is_never_recorded() {
    let (listener, mut host, surface, _state) = observed_sheet();
    let (method, path) = at(&surface, "tally.sheet.Tally");
    let body = json!({"number": 1, "by_number": {"Zq7hunter": "a"}}).to_string();
    let (status, answer) = served(
        &listener,
        &mut host,
        request(&method, &path, "application/json", body.as_bytes()),
    );
    assert_eq!(status, 400, "{answer}");
    let reasons = reasons(&listener);
    assert_eq!(reasons.len(), 1, "{reasons:?}");
    assert!(
        reasons[0].contains("by_number"),
        "the field is named: {reasons:?}"
    );
    assert!(
        !reasons[0].contains("Zq7hunter"),
        "the map key the caller sent is recorded: {reasons:?}"
    );
}

/// As [`a_map_key_the_caller_sent_is_never_recorded`], for a key of digits: the kernel names the
/// entry whose value is refused as `[{spelling}]` (`typed.rs`), and `reason` keeps a bracketed run
/// of digits whole as a list index.
#[test]
fn a_numeric_map_key_the_caller_sent_is_never_recorded() {
    let (listener, mut host, surface, _state) = observed_sheet();
    let (method, path) = at(&surface, "tally.sheet.Tally");
    let body = json!({"number": 1, "by_name": {"4111111111111111": "a"}}).to_string();
    let (status, answer) = served(
        &listener,
        &mut host,
        request(&method, &path, "application/json", body.as_bytes()),
    );
    assert_eq!(status, 400, "{answer}");
    let reasons = reasons(&listener);
    assert_eq!(reasons.len(), 1, "{reasons:?}");
    assert!(
        !reasons[0].contains("4111111111111111"),
        "the map key the caller sent is recorded: {reasons:?}"
    );
}

/// `story:refusal-reason-bounded`: a `form` value that is not percent-encoded is refused naming
/// it in backticks (`surface.rs` `flat_input`: ``found `{value}` ``), and a backtick in the value
/// closes that span early (`observe.rs` `quoted` reads no escape for a backtick): everything after
/// it is copied into the reason verbatim.
#[test]
fn a_form_value_holding_a_backtick_is_never_recorded() {
    let ir = loaded("grants");
    let text = std::fs::read_to_string(fixtures().join("grants-http.yaml")).expect("the binding");
    let spec = ess_http::HttpSpec::from_yaml(&text).expect("the binding parses");
    let http = ess_http::compile(&spec, &ir, &ess_gen::http::derived(&ir))
        .unwrap_or_else(|refused| panic!("ESS refuses the binding: {refused}"));
    let binding = ess_gen::http::Binding::new(http)
        .unwrap_or_else(|refused| panic!("not servable: {refused}"));
    let clerk = Caller {
        actor: "vault.ledger.Clerk".to_owned(),
        attributes: Map::new(),
    };
    let surface = Surface::fixing_callers(&ir, &["ledger".to_owned()], Some(&binding), &[clerk])
        .unwrap_or_else(|refused| panic!("the ledger surface is served: {refused}"))
        .observing(16);
    let (mut host, _state) = host(ir);
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let (status, answer) = served(
        &listener,
        &mut host,
        request(
            "POST",
            "/entries/1",
            "application/x-www-form-urlencoded",
            b"title=%ZZ`Zq7hunter",
        ),
    );
    assert_eq!(status, 400, "{answer}");
    let reasons = reasons(&listener);
    assert_eq!(reasons.len(), 1, "{reasons:?}");
    assert!(
        reasons[0].contains("body.title"),
        "the field is named: {reasons:?}"
    );
    assert!(
        !reasons[0].contains("Zq7hunter"),
        "the value the caller sent is recorded: {reasons:?}"
    );
}
