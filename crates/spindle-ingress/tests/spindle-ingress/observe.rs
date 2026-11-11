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

/// One `Connection: close` request carrying `body` as JSON: the status and the body as JSON.
fn send_json(address: SocketAddr, method: &str, target: &str, body: &Value) -> (u16, Value) {
    let body = body.to_string();
    let mut stream = TcpStream::connect(address).expect("the listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
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
    let answered =
        serde_json::from_str(answered).unwrap_or_else(|error| panic!("{error}: {answered}"));
    (status, answered)
}

/// `story:binding-delivery-specified`: an event binding whose input the event does not fill
/// (`member-0042` is no `ReaderRef`, `value.count <= 8`) invokes nothing, and the request that
/// published the event lists it under `unmet` (`spindle.observe.UnmetInput`): the binding, the
/// component that accepts its command, the command and why. A request whose bindings all map
/// lists none. The fixture `tests/fixtures/library` is `spindle-channel`'s, with `lending` reached
/// over the network.
#[test]
fn an_unmet_binding_input_is_listed_in_the_request_that_published_the_event() {
    let ir = spindle_spec::load(&fixtures().join("library"))
        .expect("the library fixture compiles")
        .ir;
    let surface = Surface::new(&ir, &["lending".to_owned()], None)
        .expect("the lending surface is served")
        .observing(16);
    let close = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == "library.lending.CloseLoan")
        .expect("the surface serves CloseLoan");
    let (method, path) = (close.method.to_string().to_uppercase(), close.path.clone());
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .unwrap_or_else(|error| panic!("the host admits the library: {error}"));
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();

    for (loan, member) in [("loan-1", "m-7"), ("loan-2", "member-0042")] {
        let (method, path) = (method.clone(), path.clone());
        let body = serde_json::json!({ "loan_id": loan, "member_id": member });
        let asked = thread::spawn(move || send_json(address, &method, &path, &body));
        listener
            .next(ARRIVAL)
            .expect("the request arrives as an exchange")
            .serve(&mut host);
        let (status, answer) = asked.join().expect("the request thread ends");
        assert!((200..300).contains(&status), "{status} {answer}");
    }

    let (status, log) = send(address, "GET", "/__spindle/requests");
    assert_eq!(status, 200, "{log}");
    let records = log["requests"].as_array().expect("requests");
    assert_eq!(records.len(), 2, "{log}");
    let (unmet, met) = (&records[0], &records[1]);
    assert_eq!(met["unmet"], serde_json::json!([]), "{log}");
    let listed = unmet["unmet"].as_array().expect("unmet is a list");
    assert_eq!(listed.len(), 1, "{log}");
    assert_eq!(listed[0]["binding"], "thank-on-close", "{log}");
    assert_eq!(listed[0]["component"], "notices", "{log}");
    assert_eq!(listed[0]["command"], "library.notices.SendThanks", "{log}");
    let reason = listed[0]["reason"].as_str().expect("a reason");
    assert!(reason.contains("the input reader_id is unmet"), "{reason}");
}

/// `story:refusal-reason-bounded`, for an unmet input as for a record's reason
/// (`spindle.observe.UnmetInput.reason`): the value that did not map is never recorded, and the
/// reason is at most 512 bytes. A member id of 800 KB is no `ReaderRef`; the record's unmet
/// input names the input and the newtype, holds `…` where the value was, and nothing of it.
#[test]
fn an_unmet_inputs_reason_records_no_value_and_is_bounded() {
    let ir = spindle_spec::load(&fixtures().join("library"))
        .expect("the library fixture compiles")
        .ir;
    let surface = Surface::new(&ir, &["lending".to_owned()], None)
        .expect("the lending surface is served")
        .observing(16);
    let close = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == "library.lending.CloseLoan")
        .expect("the surface serves CloseLoan");
    let (method, path) = (close.method.to_string().to_uppercase(), close.path.clone());
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .unwrap_or_else(|error| panic!("the host admits the library: {error}"));
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();

    for (loan, member) in [
        ("loan-1", "member-0042".to_owned()),
        ("loan-2", format!("m{}", "q".repeat(800_000))),
    ] {
        let (method, path) = (method.clone(), path.clone());
        let body = serde_json::json!({ "loan_id": loan, "member_id": member });
        let asked = thread::spawn(move || send_json(address, &method, &path, &body));
        listener
            .next(ARRIVAL)
            .expect("the request arrives as an exchange")
            .serve(&mut host);
        let (status, answer) = asked.join().expect("the request thread ends");
        assert!((200..300).contains(&status), "{status} {answer}");
    }

    let (status, log) = send(address, "GET", "/__spindle/requests");
    assert_eq!(status, 200, "{log}");
    let records = log["requests"].as_array().expect("requests");
    assert_eq!(records.len(), 2, "{log}");
    for (record, value) in records.iter().zip(["qqq", "member-0042"]) {
        let listed = record["unmet"].as_array().expect("unmet is a list");
        assert_eq!(listed.len(), 1, "{log}");
        let reason = listed[0]["reason"].as_str().expect("a reason");
        assert!(reason.len() <= 512, "{} bytes: {reason}", reason.len());
        assert!(
            reason.contains("the input reader_id is unmet"),
            "the input is named: {reason}"
        );
        assert!(
            reason.contains("library.notices.ReaderRef"),
            "the newtype is named: {reason}"
        );
        assert!(reason.contains('…'), "the value is elided: {reason}");
        assert!(
            !reason.contains(value),
            "no part of the value {value:?} is recorded: {reason}"
        );
    }
}

/// The desk surface, observed, and a host for it on memory stores, with the listener bound.
fn served_desk() -> (Listener, Host, Surface, tempfile::TempDir) {
    let ir = spindle_spec::load(&fixtures().join("desk"))
        .expect("the desk fixture compiles")
        .ir;
    let surface = Surface::new(&ir, &[COMPONENT.to_owned()], None)
        .expect("the desk surface is served")
        .observing(16);
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .unwrap_or_else(|error| panic!("the host admits the desk: {error}"));
    let listener = Listener::bind(0, surface.clone()).expect("the listener binds");
    (listener, host, surface, state)
}

/// `GET /__spindle/metrics`, served on `host` as one exchange.
fn scraped(listener: &Listener, host: &mut Host, address: SocketAddr) -> String {
    let asked = thread::spawn(move || send_text(address, "GET", "/__spindle/metrics"));
    listener
        .next(ARRIVAL)
        .expect("the scrape arrives as an exchange")
        .serve(host);
    let (status, metrics) = asked.join().expect("the scrape thread ends");
    assert_eq!(status, 200, "{metrics}");
    metrics
}

/// `story:metrics-observed-series`: a view read held over 1 s on the host's thread shows in the
/// `+Inf` and `le="1"` difference of its own route's duration series, and in no other.
#[test]
fn a_slow_view_read_shows_in_its_route_duration_series_only() {
    let (listener, mut host, surface, _state) = served_desk();
    let view = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == "study.desk.AllDesks")
        .expect("the surface serves the view")
        .path
        .clone();
    let address = listener.local_addr();
    let slow = view.clone();
    let asked = thread::spawn(move || send(address, "GET", &slow));
    let exchange = listener
        .next(ARRIVAL)
        .expect("the request arrives as an exchange");
    thread::sleep(Duration::from_millis(1_100));
    exchange.serve(&mut host);
    let (status, body) = asked.join().expect("the request thread ends");
    assert_eq!(status, 200, "{body}");
    let (status, body) = send(address, "GET", "/no/such/route");
    assert_eq!(status, 404, "{body}");

    let metrics = scraped(&listener, &mut host, address);
    let bucket = |route: &str, le: &str| {
        format!(
            "spindle_http_request_duration_seconds_bucket{{method=\"GET\",route=\"{route}\",le=\"{le}\"}}"
        )
    };
    let value = |series: String| {
        metrics
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{series} ")))
            .unwrap_or_else(|| panic!("no {series}:\n{metrics}"))
            .to_owned()
    };
    assert_eq!(value(bucket(&view, "1")), "0", "{metrics}");
    assert_eq!(value(bucket(&view, "+Inf")), "1", "{metrics}");
    assert_eq!(value(bucket("", "1")), "1", "{metrics}");
    assert_eq!(value(bucket("", "+Inf")), "1", "{metrics}");
}

/// `story:metrics-observed-series`: a gauge reports how long the last metrics scrape took.
#[test]
fn the_last_scrape_duration_is_a_gauge() {
    let (listener, mut host, _surface, _state) = served_desk();
    let address = listener.local_addr();
    let _ = scraped(&listener, &mut host, address);
    let metrics = scraped(&listener, &mut host, address);
    assert!(
        metrics
            .lines()
            .any(|line| line == "# TYPE spindle_metrics_scrape_duration_seconds gauge"),
        "{metrics}"
    );
    let seconds = metrics
        .lines()
        .find_map(|line| line.strip_prefix("spindle_metrics_scrape_duration_seconds "))
        .unwrap_or_else(|| panic!("no scrape duration:\n{metrics}"));
    let seconds: f64 = seconds.parse().expect("a number of seconds");
    assert!((0.0..10.0).contains(&seconds), "{seconds}");
}

/// The most bytes a refused command's record adds to the request log, its reason included
/// (`spindle.observe.RequestRecord.reason`: at most 512 bytes).
const REFUSED_RECORD_MOST: usize = 1_024;

/// `story:refusal-reason-bounded`: a command refused for a mistyped field of 800 KB adds one
/// bounded record to the log; its reason names the field and the type it expected, and holds no
/// byte of the value sent.
#[test]
fn a_refused_value_is_never_recorded_and_the_reason_is_bounded() {
    let (listener, mut host, surface, _state) = served_desk();
    let open = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == "study.desk.OpenDesk")
        .expect("the surface serves OpenDesk");
    let method = open.method.to_string().to_uppercase();
    let carried = open.path.contains("{desk_id}");
    let path = open.path.replace("{desk_id}", "d1");
    let address = listener.local_addr();
    let (status, empty) = send_text(address, "GET", "/__spindle/requests");
    assert_eq!(status, 200, "{empty}");

    let sent = "Zq".repeat(400_000);
    let mut body = serde_json::json!({ "holder": [sent] });
    if !carried {
        body["desk_id"] = serde_json::json!("d1");
    }
    let asked = thread::spawn(move || send_json(address, &method, &path, &body));
    listener
        .next(ARRIVAL)
        .expect("the command arrives as an exchange")
        .serve(&mut host);
    let (status, answer) = asked.join().expect("the request thread ends");
    assert_eq!(status, 400, "{answer}");

    let (status, log) = send_text(address, "GET", "/__spindle/requests");
    assert_eq!(status, 200);
    let grown = log.len() - empty.len();
    assert!(
        grown <= REFUSED_RECORD_MOST,
        "one refused record grew the log by {grown} bytes"
    );
    let log: Value = serde_json::from_str(&log).expect("the log is JSON");
    let record = &log["requests"][0];
    assert_eq!(record["class"], "refused", "{record}");
    let reason = record["reason"].as_str().expect("a reason");
    assert!(reason.len() <= 512, "{} bytes: {reason}", reason.len());
    assert!(reason.contains("`holder`"), "the field is named: {reason}");
    assert!(reason.contains("String"), "the type is named: {reason}");
    assert!(!reason.contains('Z') && !reason.contains('q'), "{reason}");
}

/// The most bytes of a record's path (`spindle.observe.RequestRecord.path`).
const PATH_MOST: usize = 2_048;

/// `spindle.observe.RequestRecord.path`: a request for a path of 60,000 bytes that no route
/// serves adds one record whose path is its first bytes, cut to 2,048 with `…`, so a full ring
/// of 10,000 such records holds about 20 MiB of paths rather than 600 MiB.
#[test]
fn a_long_unrouted_path_is_recorded_cut_to_its_bound() {
    let (listener, _host, _surface, _state) = served_desk();
    let address = listener.local_addr();
    let path = format!("/{}", "p".repeat(60_000));
    let (status, body) = send_text(address, "GET", &path);
    assert_eq!(status, 404, "{}", &body[..body.len().min(200)]);

    let (status, log) = send(address, "GET", "/__spindle/requests");
    assert_eq!(status, 200, "{log}");
    let record = &log["requests"][0];
    assert_eq!(record["status"], 404);
    let kept = record["path"].as_str().expect("a path");
    assert!(kept.len() <= PATH_MOST, "{} bytes", kept.len());
    assert!(kept.ends_with('…'), "the cut is marked: {kept}");
    assert!(
        path.starts_with(kept.trim_end_matches('…')),
        "the path's first bytes are kept"
    );
    assert!(kept.len() > PATH_MOST - 8, "{} bytes", kept.len());
    let (status, short) = send(address, "GET", "/no/such/route");
    assert_eq!(status, 404, "{short}");
    let (_, log) = send(address, "GET", "/__spindle/requests");
    assert_eq!(log["requests"][0]["path"], "/no/such/route", "{log}");
}
