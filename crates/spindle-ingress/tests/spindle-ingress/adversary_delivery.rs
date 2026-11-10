//! Adversary cases of `story:binding-delivery-specified`: which failed deliveries a request record
//! lists under `unmet` (`spindle.observe.RequestRecord.unmet`, `spindle.observe.UnmetInput`).
//!
//! The field's summary in `ess/domains/observe.yaml`: "Every event binding whose input was unmet
//! while the request was answered, in the order the runtime reported them: bindings of the events
//! the routed command published and of the events every command an event binding invoked on the
//! way published."

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_host::{Cause, Failed, Host, ManualClock};
use spindle_ingress::{Listener, Surface};

const ARRIVAL: Duration = Duration::from_secs(30);

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"observe\"\n",
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
        "\"identity.mint\" = [\"uuid-v4\"]\n",
        "\"ingress.network\" = [\"http-loopback\"]\n",
    ))
    .expect("the in-memory environment parses")
}

/// One `Connection: close` request, with `body` as JSON where given: the status and the body.
fn exchange(
    address: SocketAddr,
    method: &str,
    target: &str,
    body: Option<&Value>,
) -> (u16, String) {
    let body = body.map(Value::to_string).unwrap_or_default();
    let mut stream = TcpStream::connect(address).expect("the listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .expect("a read timeout");
    write!(
        stream,
        "{method} {target} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .expect("the request is written");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("the response is read");
    let (head, answered) = response
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("a response without a body: {response}"));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a response without a status: {head}"));
    (status, answered.to_owned())
}

/// A host and an observing listener serving `component` of the fixture `name`.
struct Served {
    host: Host,
    listener: Listener,
    address: SocketAddr,
    surface_paths: Vec<(String, String, String)>,
    _state: tempfile::TempDir,
}

fn served(name: &str, component: &str) -> Served {
    let ir = spindle_spec::load(&fixtures().join(name))
        .unwrap_or_else(|error| panic!("the {name} fixture compiles: {error}"))
        .ir;
    let surface = Surface::new(&ir, &[component.to_owned()], None)
        .expect("the surface is served")
        .observing(16);
    let surface_paths = surface
        .routes()
        .iter()
        .map(|route| {
            (
                route.operation.name().to_owned(),
                route.method.to_string().to_uppercase(),
                route.path.clone(),
            )
        })
        .collect();
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .unwrap_or_else(|error| panic!("the host admits {name}: {error}"));
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    Served {
        host,
        listener,
        address,
        surface_paths,
        _state: state,
    }
}

impl Served {
    /// Sends `operation` with `body` and serves it: the status, the answer and what the host
    /// reported as not executed.
    fn ask(&mut self, operation: &str, body: Value) -> (u16, String, Vec<Failed>) {
        let (_, method, path) = self
            .surface_paths
            .iter()
            .find(|(name, _, _)| name == operation)
            .unwrap_or_else(|| panic!("the surface serves {operation}"))
            .clone();
        let address = self.address;
        let asked = thread::spawn(move || exchange(address, &method, &path, Some(&body)));
        let failed = self
            .listener
            .next(ARRIVAL)
            .expect("the request arrives as an exchange")
            .serve(&mut self.host);
        let (status, answer) = asked.join().expect("the request thread ends");
        (status, answer, failed)
    }

    /// The request log, as `GET /__spindle/requests` answers it.
    fn log(&self) -> Value {
        let (status, body) = exchange(self.address, "GET", "/__spindle/requests", None);
        assert_eq!(status, 200, "{body}");
        serde_json::from_str(&body).unwrap_or_else(|error| panic!("{error}: {body}"))
    }
}

/// The routed command `Loop` publishes `Looped`, which two bindings carry: `thank-looped`, whose
/// input is unmet (`member-0042` is no `ReaderRef`, `value.count <= 8`), and `ping`, which starts
/// an echo between `left` and `right` that never settles, so the host stops the call as a
/// runaway. `Loop` committed and `thank-looped` was never invoked while the request was answered;
/// the record lists every external cause asked on the way all the same, and must list the unmet
/// binding too.
#[test]
fn adversary_an_unmet_input_of_a_request_that_ran_away_is_listed() {
    let mut served = served("adversary-unmet", "left");
    let (status, answer, _) = served.ask(
        "relay.left.Loop",
        json!({ "n": 1, "member": "member-0042" }),
    );
    assert!(
        answer.contains("triggering each other") || status >= 500,
        "the loop is expected to run away: {status} {answer}"
    );
    let log = served.log();
    let record = &log["requests"][0];
    assert_eq!(
        record["unmet"],
        json!([{
            "binding": "thank-looped",
            "component": "notices",
            "command": "relay.notices.Thank",
            "reason": record["unmet"][0]["reason"].clone(),
        }]),
        "status {status}, answer {answer}; the record lists no unmet input of a request that ran away: {log}"
    );
}

/// `Send` publishes `Sent`; `relay` carries it into `right.Relay`, which publishes `Relayed`;
/// `thank-relayed` carries that into `notices.Thank`, and its input is unmet. The record of the
/// request lists the binding two hops down, as the field's summary says ("and of the events every
/// command an event binding invoked on the way published").
#[test]
fn adversary_an_unmet_input_two_bindings_down_is_listed() {
    let mut served = served("adversary-unmet", "left");
    let (status, answer, failed) = served.ask(
        "relay.left.Send",
        json!({ "n": 1, "member": "member-0042" }),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    assert!(
        failed.iter().any(|failed| failed.attempts == 0
            && failed.cause
                == Cause::Delivered {
                    binding: "thank-relayed".to_owned()
                }),
        "the host reports thank-relayed as unmet: {failed:?}"
    );
    let log = served.log();
    let unmet = &log["requests"][0]["unmet"];
    assert_eq!(unmet.as_array().map(Vec::len), Some(1), "{log}");
    assert_eq!(unmet[0]["binding"], "thank-relayed", "{log}");
    assert_eq!(unmet[0]["component"], "notices", "{log}");
    assert_eq!(unmet[0]["command"], "relay.notices.Thank", "{log}");
}

/// The unit's own `library` fixture: closing a second loan for `m-7` maps `thank-on-close`, so its
/// input is met, and `SendThanks` runs and does not execute (the `Thanks` of `m-7` exists): a
/// failed delivery after one attempt. That is no unmet input, and the record must not list it.
/// Neither test the unit added produces a delivery that failed after an attempt, so dropping the
/// `attempts == 0` test of `UnmetInput::of` leaves the unit's suite green.
#[test]
fn adversary_a_delivery_that_failed_after_an_attempt_is_not_unmet() {
    let mut served = served("library", "lending");
    let (status, answer, failed) = served.ask(
        "library.lending.CloseLoan",
        json!({ "loan_id": "loan-1", "member_id": "m-7" }),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    assert!(failed.is_empty(), "the first thanks runs: {failed:?}");
    let (status, answer, failed) = served.ask(
        "library.lending.CloseLoan",
        json!({ "loan_id": "loan-2", "member_id": "m-7" }),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    assert!(
        failed.iter().any(|failed| failed.attempts >= 1
            && failed.cause
                == Cause::Delivered {
                    binding: "thank-on-close".to_owned()
                }),
        "the second thanks is delivered and does not execute: {failed:?}"
    );
    let log = served.log();
    assert_eq!(log["requests"][0]["unmet"], json!([]), "{log}");
    assert_eq!(log["requests"][1]["unmet"], json!([]), "{log}");
}
