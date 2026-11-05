//! Adversary cases for `on_failure: retry`: the bound at its edge, and the one-component host
//! (`Host::component`), where the envelope arrives over a TCP link from a stand-in peer.

use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock, RETRY_ATTEMPTS, Report, Retries};
use spindle_kernel::External;

const RECORD: &str = "filing.ledger.Record";
const SOURCE: &str = "notice-source";
const SINK: &str = "notice-sink";
const PIN: &str = "notice.sink.Pin";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace")
}

fn environment(name: &str) -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments").join(name))
        .expect("the environment reads");
    Environment::parse(&text).expect("an environment")
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

/// The filing fixture on a host whose ledger holds back the next `hold` entries.
fn filing(hold: Arc<AtomicU32>) -> (Host, tempfile::TempDir) {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let loaded = spindle_spec::load(&root().join("crates/spindle-host/tests/fixtures/retry"))
        .expect("the fixture compiles");
    let decider = move |question: &External<'_>| {
        assert_eq!((question.command, question.outcome), (RECORD, "held"));
        Ok(hold
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                left.checked_sub(1)
            })
            .is_ok())
    };
    let host = Host::deciding_with_retries(
        loaded.ir,
        &environment("laptop.toml"),
        Arc::new(ManualClock::new()),
        state.path(),
        Box::new(decider),
        Retries::default(),
    )
    .expect("the host starts");
    (host, state)
}

#[test]
fn adversary_an_attempt_that_takes_effect_on_the_last_allowed_try_is_taken() {
    let hold = Arc::new(AtomicU32::new(RETRY_ATTEMPTS - 1));
    let (mut host, _state) = filing(hold.clone());
    let report = host
        .submit("filing.desk.File", object(json!({"filing_id": "f-1"})))
        .expect("the filing is taken");
    let recorded = report
        .ran
        .iter()
        .find(|ran| ran.command == RECORD)
        .unwrap_or_else(|| panic!("Record took effect on its last attempt: {report:?}"));
    assert_eq!(recorded.attempts, RETRY_ATTEMPTS);
    assert!(report.refused.is_empty(), "{report:?}");
    let counters = host.counters("ledger").expect("the ledger runs");
    assert_eq!(
        counters.value("binding.record-filing.attempted"),
        Some(u64::from(RETRY_ATTEMPTS))
    );
    assert_eq!(counters.value("binding.record-filing.exhausted"), Some(0));
}

#[test]
fn adversary_one_hold_past_the_limit_is_given_up_and_the_next_filing_runs_once() {
    let hold = Arc::new(AtomicU32::new(RETRY_ATTEMPTS));
    let (mut host, _state) = filing(hold.clone());
    let report = host
        .submit("filing.desk.File", object(json!({"filing_id": "f-1"})))
        .expect("the filing is taken");
    assert_eq!(report.refused.len(), 1, "{report:?}");
    assert_eq!(report.refused[0].attempts, RETRY_ATTEMPTS);
    assert_eq!(
        hold.load(Ordering::SeqCst),
        0,
        "exactly the bound was asked"
    );
    let next = host
        .submit("filing.desk.File", object(json!({"filing_id": "f-2"})))
        .expect("the filing is taken");
    let recorded = next
        .ran
        .iter()
        .find(|ran| ran.command == RECORD)
        .unwrap_or_else(|| panic!("{next:?}"));
    assert_eq!(recorded.attempts, 1);
}

fn send(stream: &mut TcpStream, message: &Value) {
    let body = serde_json::to_vec(message).expect("encodes");
    let mut bytes = u32::try_from(body.len())
        .expect("small")
        .to_be_bytes()
        .to_vec();
    bytes.extend_from_slice(&body);
    stream.write_all(&bytes).expect("the write");
}

/// A `pin-post` envelope from the source, its `on_failure` as the peer wrote it.
fn pin_post(id: u64, on_failure: &str) -> Value {
    let frame = json!({"deliver": {
        "id": id,
        "binding": "pin-post",
        "from": SOURCE,
        "to": SINK,
        "delivery": "at_most_once",
        "on_failure": on_failure,
        "attempt": 1,
        "command": {"name": PIN, "fields": {"board_id": "b1", "beat": "first"}},
    }});
    serde_json::from_value::<spindle_channel::Frame>(frame.clone()).expect("a channel frame");
    json!({ "frame": frame })
}

/// The sink of the notice fixture on its own, linked from a stand-in source; the stream the
/// source writes on.
fn linked_sink() -> (Host, TcpListener, TcpStream, tempfile::TempDir) {
    let loaded =
        spindle_spec::load(&root().join("crates/spindle-host/tests/fixtures/adversary/notice"))
            .expect("notice compiles");
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = Host::component(
        loaded.ir,
        &environment("laptop-processes.toml"),
        Arc::new(ManualClock::new()),
        state.path(),
        SINK,
    )
    .expect("the sink is admitted");
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let mut stream =
        TcpStream::connect(listener.local_addr().expect("bound")).expect("the source connects");
    send(&mut stream, &json!({"hello": {"from": SOURCE, "to": SINK}}));
    for _ in 0..400 {
        host.accept(&listener).expect("the listener works");
        if host.unlinked().is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(host.unlinked(), Vec::<String>::new(), "the link opens");
    (host, listener, stream, state)
}

/// Delivers until a report holds a Pin, or a bound passes.
fn delivered(host: &mut Host) -> Report {
    let mut all = Report::default();
    for _ in 0..200 {
        let report = host.deliver().expect("the sink delivers");
        all.ran.extend(report.ran);
        all.refused.extend(report.refused);
        all.failed.extend(report.failed);
        if all.refused.iter().any(|refused| refused.command == PIN)
            || all.ran.iter().any(|ran| ran.command == PIN)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    all
}

#[test]
fn adversary_a_component_host_retries_and_counts_an_envelope_from_its_peer() {
    let (mut sink, _listener, mut stream, _state) = linked_sink();
    send(&mut stream, &pin_post(1, "retry"));
    let report = delivered(&mut sink);
    let pinned = report
        .refused
        .iter()
        .find(|refused| refused.command == PIN)
        .unwrap_or_else(|| panic!("the refused Pin is reported: {report:?}"));
    assert_eq!(pinned.attempts, RETRY_ATTEMPTS, "{report:?}");
    let counters = sink.counters(SINK).expect("the sink runs");
    assert_eq!(
        counters.value("binding.pin-post.attempted"),
        Some(u64::from(RETRY_ATTEMPTS))
    );
    assert_eq!(counters.value("binding.pin-post.exhausted"), Some(1));
}

/// The specification says `pin-post` retries; the frame's `on_failure` is the sender's word for
/// it. The receiving host has the specification and should not let a frame decide the policy.
#[test]
fn adversary_a_component_host_retries_what_its_specification_says_whatever_the_frame_says() {
    let (mut sink, _listener, mut stream, _state) = linked_sink();
    send(&mut stream, &pin_post(1, "drop"));
    let report = delivered(&mut sink);
    let pinned = report
        .refused
        .iter()
        .find(|refused| refused.command == PIN)
        .unwrap_or_else(|| panic!("the refused Pin is reported: {report:?}"));
    assert_eq!(
        pinned.attempts, RETRY_ATTEMPTS,
        "pin-post says on_failure: retry in the specification the sink runs; a frame that says \
         drop made the sink attempt it {} time(s)",
        pinned.attempts
    );
}
