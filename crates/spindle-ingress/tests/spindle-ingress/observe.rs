//! A listener whose surface records requests (`story:ingress-request-log`): a record's
//! `duration_us` runs from the request's head read to its answer handed back to the connection,
//! measured on the listener thread, so the time a request waits for the host's thread is in it.
//!
//! The fixture `tests/fixtures/desk` declares one component, `desk`, reached over the network.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde_json::Value;
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_ingress::{AnswerSource, ExternalAnswer, Listener, Surface};

const COMPONENT: &str = "desk";

/// How long the test holds a request on its own thread before it answers it.
const HELD: Duration = Duration::from_millis(50);
/// How long the test waits for the held request to arrive.
const ARRIVAL: Duration = Duration::from_secs(10);

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

/// One `Connection: close` request: the status and the body as JSON.
fn send(address: SocketAddr, method: &str, target: &str) -> (u16, Value) {
    let (status, body) = send_text(address, method, target);
    let body = serde_json::from_str(&body).unwrap_or_else(|error| panic!("{error}: {body}"));
    (status, body)
}

/// One `Connection: close` request: the status and the body as text.
fn send_text(address: SocketAddr, method: &str, target: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(address).expect("the listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a read timeout");
    write!(
        stream,
        "{method} {target} HTTP/1.1\r\nHost: {address}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
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

#[test]
fn a_request_held_on_the_host_thread_is_timed_with_its_wait() {
    let ir = spindle_spec::load(&fixtures().join("desk"))
        .expect("the desk fixture compiles")
        .ir;
    let surface = Surface::new(&ir, &[COMPONENT.to_owned()], None)
        .expect("the desk surface is served")
        .observing(16);
    let view = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == "study.desk.AllDesks")
        .expect("the surface serves the view")
        .path
        .clone();
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .unwrap_or_else(|error| panic!("the host admits the desk: {error}"));
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();

    let asked = thread::spawn(move || send(address, "GET", &view));
    let exchange = listener
        .next(ARRIVAL)
        .expect("the request arrives as an exchange");
    thread::sleep(HELD);
    exchange.serve(&mut host);
    let (status, body) = asked.join().expect("the request thread ends");
    assert_eq!(status, 200, "{body}");

    let (status, body) = send(address, "GET", "/no/such/route");
    assert_eq!(status, 404, "{body}");

    let (status, log) = send(address, "GET", "/__spindle/requests");
    assert_eq!(status, 200, "{log}");
    let records = log["requests"].as_array().expect("requests");
    assert_eq!(records.len(), 2, "{log}");
    let unrouted = &records[0];
    let held = &records[1];
    assert_eq!(held["status"], 200, "{log}");
    assert_eq!(unrouted["status"], 404, "{log}");
    let micros = |record: &Value| record["duration_us"].as_u64().expect("duration_us");
    let held_for = u64::try_from(HELD.as_micros()).expect("50 ms in microseconds");
    assert!(
        micros(held) >= held_for,
        "the wait on the host thread is timed: {log}"
    );
    assert!(
        micros(unrouted) < held_for,
        "a 404 answered on the listener thread is timed there: {log}"
    );
}

/// An `Exchange` answered with `respond` is classed by the answer it was given: a refusal is
/// `refused` with its text as the reason, rows are `served`. No other test calls `respond`, so
/// `Answered::of` answering "the model" for every answer goes unnoticed without this case.
#[test]
fn a_responded_exchange_is_classed_by_its_answer() {
    let ir = spindle_spec::load(&fixtures().join("desk"))
        .expect("the desk fixture compiles")
        .ir;
    let surface = Surface::new(&ir, &[COMPONENT.to_owned()], None)
        .expect("the desk surface is served")
        .observing(16);
    let view = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == "study.desk.AllDesks")
        .expect("the surface serves the view")
        .path
        .clone();
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();

    let refused_view = view.clone();
    let asked = thread::spawn(move || send(address, "GET", &refused_view));
    listener
        .next(ARRIVAL)
        .expect("the request arrives as an exchange")
        .respond(spindle_ingress::Answer::refusal(
            400,
            "held back by the test",
        ));
    let (status, body) = asked.join().expect("the request thread ends");
    assert_eq!(status, 400, "{body}");

    let asked = thread::spawn(move || send(address, "GET", &view));
    listener
        .next(ARRIVAL)
        .expect("the request arrives as an exchange")
        .respond(spindle_ingress::Answer {
            status: 200,
            body: Some(serde_json::json!({"rows": []})),
        });
    let (status, body) = asked.join().expect("the request thread ends");
    assert_eq!(status, 200, "{body}");

    let (status, log) = send(address, "GET", "/__spindle/requests");
    assert_eq!(status, 200, "{log}");
    assert_eq!(log["requests"][0]["class"], "served", "{log}");
    assert_eq!(log["requests"][1]["class"], "refused", "{log}");
    assert_eq!(
        log["requests"][1]["reason"], "held back by the test",
        "{log}"
    );
    assert_eq!(log["totals"]["refused"], 1, "{log}");
}

/// A cause asked while no request is served, as a timer-fired command asks it, is counted in the
/// metrics and listed in no record: the next request served lists only what it asked itself.
#[test]
fn a_cause_asked_outside_a_request_is_counted_and_listed_in_no_later_request() {
    let ir = spindle_spec::load(&fixtures().join("desk"))
        .expect("the desk fixture compiles")
        .ir;
    let surface = Surface::new(&ir, &[COMPONENT.to_owned()], None)
        .expect("the desk surface is served")
        .observing(16);
    let view = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == "study.desk.AllDesks")
        .expect("the surface serves the view")
        .path
        .clone();
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .unwrap_or_else(|error| panic!("the host admits the desk: {error}"));
    let recorder = surface.recorder().expect("the surface observes").clone();
    for _ in 0..3 {
        recorder.answered(ExternalAnswer {
            command: "study.desk.Tick".to_owned(),
            outcome: "skipped".to_owned(),
            holds: Some(false),
            source: AnswerSource::File,
        });
    }
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();

    let asked = thread::spawn(move || send(address, "GET", &view));
    listener
        .next(ARRIVAL)
        .expect("the request arrives as an exchange")
        .serve(&mut host);
    let (status, body) = asked.join().expect("the request thread ends");
    assert_eq!(status, 200, "{body}");

    let (status, log) = send(address, "GET", "/__spindle/requests");
    assert_eq!(status, 200, "{log}");
    assert_eq!(log["recorded"], 1, "{log}");
    assert_eq!(
        log["requests"][0]["externals"],
        serde_json::json!([]),
        "{log}"
    );

    let asked = thread::spawn(move || send_text(address, "GET", "/__spindle/metrics"));
    listener
        .next(ARRIVAL)
        .expect("the scrape arrives as an exchange")
        .serve(&mut host);
    let (status, metrics) = asked.join().expect("the scrape thread ends");
    assert_eq!(status, 200, "{metrics}");
    let sample = concat!(
        "spindle_external_causes_total",
        "{command=\"study.desk.Tick\",outcome=\"skipped\",source=\"file\"} 3"
    );
    assert!(metrics.lines().any(|line| line == sample), "{metrics}");
}
