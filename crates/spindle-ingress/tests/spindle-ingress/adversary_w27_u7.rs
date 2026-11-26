//! Adversary cases of wave 27, unit U7 (`story:refused-bound-command-recorded`): a refused bound
//! command listed under `refused_bound` in its request's record and counted in
//! `spindle_bound_refusals_total`.
//!
//! The fixture `tests/fixtures/adversary-w27-u7`: the till's `Post`, reached over the network,
//! publishes `Posted`, which two bindings carry, `apply-posted` to the balance's `Apply` and
//! `record-posted` to the audit's `Record`. `Apply` refuses 0 as `unchanged`, applies any other
//! amount and publishes `Applied`, and the kernel rejects one over 1000 (an invariant).
//! `Record` refuses 0 as `empty` and one over 1000 as `large`. `review-applied`
//! (`on_failure: retry`) carries `Applied` to the audit's `Review`, which refuses a change over
//! 100 as `flagged`. So one publish stages two envelopes, and a refusal can be caused by a
//! bound command.
//!
//! The first case drives the unit's own document `ess/fixtures/observe/requests/refused-bound.json`
//! against the unit's fixture `tests/fixtures/refused-bound`, times and durations aside.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock, RETRY_ATTEMPTS, Retries};
use spindle_ingress::{Listener, Surface};

const POST: &str = "ledger.till.Post";
const APPLY: &str = "ledger.balance.Apply";
const RECORD: &str = "ledger.audit.Record";
const REVIEW: &str = "ledger.audit.Review";
const FAMILY: &str = "spindle_bound_refusals_total";
const ARRIVAL: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

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

/// One `Connection: close` request with `body` as its JSON body, or none: the status and the body
/// as text.
fn exchange(
    address: SocketAddr,
    method: &str,
    target: &str,
    body: Option<&Value>,
) -> (u16, String) {
    let body = body.map(Value::to_string).unwrap_or_default();
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
    (status, answered.to_owned())
}

fn read_json(address: SocketAddr, target: &str) -> Value {
    let (status, body) = exchange(address, "GET", target, None);
    assert_eq!(status, 200, "{target}: {body}");
    serde_json::from_str(&body).unwrap_or_else(|error| panic!("{error}: {body}"))
}

/// A served, observing run of the fixture `fixture`, the till served, its host opened with
/// `retries`.
struct Run {
    host: Host,
    listener: Listener,
    address: SocketAddr,
    method: String,
    path: String,
    _state: tempfile::TempDir,
}

impl Run {
    fn new(fixture: &str, capacity: usize, retries: Retries) -> Self {
        let ir = spindle_spec::load(&fixtures().join(fixture))
            .unwrap_or_else(|error| panic!("the fixture {fixture} compiles: {error:?}"))
            .ir;
        let surface = Surface::new(&ir, &["till".to_owned()], None)
            .expect("the till surface is served")
            .with_console(spindle_ingress::HTTP_LOOPBACK)
            .expect("the console is served")
            .observing(capacity);
        let post = surface
            .routes()
            .iter()
            .find(|route| route.operation.name() == POST)
            .expect("the surface serves Post");
        let (method, path) = (post.method.to_string().to_uppercase(), post.path.clone());
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let host = Host::with_retries(
            ir,
            &in_memory(),
            Arc::new(ManualClock::new()),
            state.path(),
            retries,
        )
        .unwrap_or_else(|error| panic!("the host admits the fixture: {error}"));
        let listener =
            Listener::bind_reading(0, surface, host.reader()).expect("the listener binds");
        let address = listener.local_addr();
        Self {
            host,
            listener,
            address,
            method,
            path,
            _state: state,
        }
    }

    fn ledger() -> Self {
        Self::new("adversary-w27-u7", 16, Retries::Retried)
    }

    /// Posts `amount` as the entry `entry`, served on the host; the status.
    fn post(&mut self, entry: &str, amount: i64) -> u16 {
        let (address, method, path) = (self.address, self.method.clone(), self.path.clone());
        let body = json!({ "entry_id": entry, "amount": amount });
        let asked = thread::spawn(move || exchange(address, &method, &path, Some(&body)));
        self.listener
            .next(ARRIVAL)
            .expect("the request arrives as an exchange")
            .serve(&mut self.host);
        let (status, answer) = asked.join().expect("the request thread ends");
        assert!((200..300).contains(&status), "{status} {answer}");
        status
    }

    fn log(&self) -> Value {
        read_json(self.address, "/__spindle/requests")
    }

    /// The `refused_bound` list of the record `index` places from the newest.
    fn refused_bound(&self, index: usize) -> Value {
        self.log()["requests"][index]["refused_bound"].clone()
    }

    /// Every series of the refusal family, as lines.
    fn refusal_series(&self) -> Vec<String> {
        let (status, metrics) = exchange(self.address, "GET", "/__spindle/metrics", None);
        assert_eq!(status, 200, "{metrics}");
        metrics
            .lines()
            .filter(|line| line.starts_with(&format!("{FAMILY}{{")))
            .map(str::to_owned)
            .collect()
    }
}

/// The entries of `listed`, ordered by command, so a case does not rest on the order two
/// receivers are read in.
fn by_command(listed: &Value) -> Vec<Value> {
    let mut entries = listed.as_array().expect("a list").clone();
    entries.sort_by_key(|entry| entry["command"].as_str().unwrap_or_default().to_owned());
    entries
}

/// The unit's document `ess/fixtures/observe/requests/refused-bound.json` is what a run of the
/// unit's fixture records for the three posts it shows (an applied entry, one refused
/// `unchanged`, one the kernel rejects), times and durations aside.
#[test]
fn the_refused_bound_document_is_what_a_run_records() {
    let document: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join("ess/fixtures/observe/requests/refused-bound.json"))
            .expect("the document reads"),
    )
    .expect("the document is JSON");
    let capacity = document["capacity"]
        .as_u64()
        .and_then(|capacity| usize::try_from(capacity).ok())
        .expect("a capacity");
    let mut run = Run::new("refused-bound", capacity, Retries::Retried);
    run.post("entry-1", 10);
    run.post("entry-2", 0);
    run.post("entry-3", 5000);

    let strip = |mut log: Value| {
        for record in log["requests"].as_array_mut().expect("requests") {
            let record = record.as_object_mut().expect("a record");
            record.remove("at");
            record.remove("duration_us");
        }
        log
    };
    assert_eq!(strip(run.log()), strip(document));
}

/// One publish that stages two envelopes, each refused: both entries name the command whose
/// publish staged them. A host that kept the cause of the first envelope of a publish alone
/// would leave the second without one.
#[test]
fn both_envelopes_of_one_publish_name_their_cause() {
    let mut run = Run::ledger();
    run.post("entry-1", 0);

    assert_eq!(
        by_command(&run.refused_bound(0)),
        [
            json!({
                "binding": "record-posted",
                "component": "audit",
                "command": RECORD,
                "caused_by": POST,
                "outcome": "empty",
                "error": "ledger.audit.Empty",
                "attempts": 1,
            }),
            json!({
                "binding": "apply-posted",
                "component": "balance",
                "command": APPLY,
                "caused_by": POST,
                "outcome": "unchanged",
                "error": "ledger.balance.Unchanged",
                "attempts": 1,
            }),
        ]
    );
}

/// A refusal two bindings down: the till's `Post` publishes, `Apply` runs and publishes, and
/// `Review` is refused. Its entry names `Apply`, the bound command whose run published the event,
/// not the routed `Post`; the retrying binding ran it [`RETRY_ATTEMPTS`] times, and the request
/// is counted once in the metrics.
#[test]
fn a_refusal_down_a_chain_names_the_bound_command_that_published() {
    let mut run = Run::ledger();
    let value = "entry-9d2c71";
    run.post(value, 500);

    let listed = run.refused_bound(0);
    assert_eq!(
        listed,
        json!([{
            "binding": "review-applied",
            "component": "audit",
            "command": REVIEW,
            "caused_by": APPLY,
            "outcome": "flagged",
            "error": "ledger.audit.Flagged",
            "attempts": RETRY_ATTEMPTS,
        }])
    );
    assert!(!listed.to_string().contains(value), "{listed}");
    assert_eq!(
        run.refusal_series(),
        [format!(
            "{FAMILY}{{binding=\"review-applied\",command=\"{REVIEW}\",refusal=\"flagged\"}} 1"
        )]
    );
    let events = read_json(run.address, "/api/events");
    let mut commands = events["events"]
        .as_array()
        .expect("events")
        .iter()
        .map(|event| event["command"].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    commands.sort();
    assert_eq!(commands, [RECORD, APPLY, POST], "{events}");
}

/// `run --drop-retries`: the retrying binding runs as a `drop` binding, so its refused command is
/// listed after one attempt.
#[test]
fn a_dropped_retry_is_listed_after_one_attempt() {
    let mut run = Run::new("adversary-w27-u7", 16, Retries::Dropped);
    run.post("entry-1", 500);

    assert_eq!(
        run.refused_bound(0),
        json!([{
            "binding": "review-applied",
            "component": "audit",
            "command": REVIEW,
            "caused_by": APPLY,
            "outcome": "flagged",
            "error": "ledger.audit.Flagged",
            "attempts": 1,
        }])
    );
}

/// An outcome's refusal and a kernel rejection in one request: listed as `RequestRecord`'s
/// summary says, the outcome's refusal first, then the kernel's rejection, each naming the routed
/// command, the rejection with its kind and no outcome or error.
#[test]
fn outcome_refusals_are_listed_before_kernel_rejections() {
    let mut run = Run::ledger();
    run.post("entry-1", 5000);

    assert_eq!(
        run.refused_bound(0),
        json!([
            {
                "binding": "record-posted",
                "component": "audit",
                "command": RECORD,
                "caused_by": POST,
                "outcome": "large",
                "error": "ledger.audit.Large",
                "attempts": 1,
            },
            {
                "binding": "apply-posted",
                "component": "balance",
                "command": APPLY,
                "caused_by": POST,
                "kind": "broken_invariant",
                "attempts": 1,
            },
        ])
    );
}

/// The host's report, on the fixture `adversary-unmet`: an unmet binding input two bindings down
/// names the bound command whose event it could not fill (`Relay`), not the submitted `Send`; and
/// after a call that ran away, whose envelopes in flight were discarded, the causes of the next
/// call are still its own.
#[test]
fn an_unmet_input_names_the_command_whose_event_it_could_not_fill() {
    let ir = spindle_spec::load(&fixtures().join("adversary-unmet"))
        .expect("the fixture compiles")
        .ir;
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .unwrap_or_else(|error| panic!("the host admits the fixture: {error}"));
    let unmet = |failed: &[spindle_host::Failed], binding: &str| {
        failed
            .iter()
            .find(|failed| {
                failed.attempts == 0
                    && failed.cause
                        == spindle_host::Cause::Delivered {
                            binding: binding.to_owned(),
                        }
            })
            .map(|failed| (failed.caused_by.clone(), failed.kind))
    };
    let input = |value: Value| match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    };

    let report = host
        .submit(
            "relay.left.Send",
            input(json!({ "n": 1, "member": "member-0042" })),
        )
        .expect("Send settles");
    assert_eq!(
        unmet(&report.failed, "thank-relayed"),
        Some((Some("relay.right.Relay".to_owned()), None)),
        "{report:?}"
    );

    match host.submit(
        "relay.left.Loop",
        input(json!({ "n": 1, "member": "member-0042" })),
    ) {
        Err(spindle_host::HostError::Runaway { failed, .. }) => assert_eq!(
            unmet(&failed, "thank-looped"),
            Some((Some("relay.left.Loop".to_owned()), None)),
            "{failed:?}"
        ),
        other => panic!("Loop is expected to run away: {other:?}"),
    }

    let report = host
        .submit(
            "relay.left.Send",
            input(json!({ "n": 2, "member": "member-0042" })),
        )
        .expect("Send settles after the runaway");
    assert_eq!(
        unmet(&report.failed, "thank-relayed"),
        Some((Some("relay.right.Relay".to_owned()), None)),
        "{report:?}"
    );
}

/// Each record lists its own request's refusals: a request after a refused one lists none, the
/// refused one keeps its entries, and the metrics count every entry of every record.
#[test]
fn a_refusal_stays_in_its_own_request() {
    let mut run = Run::ledger();
    run.post("entry-1", 0);
    run.post("entry-2", 10);
    run.post("entry-3", 0);

    let log = run.log();
    let lists = log["requests"]
        .as_array()
        .expect("requests")
        .iter()
        .map(|record| {
            record["refused_bound"]
                .as_array()
                .map_or(usize::MAX, Vec::len)
        })
        .collect::<Vec<_>>();
    assert_eq!(lists, [2, 0, 2], "{log}");
    assert_eq!(
        run.refusal_series(),
        [
            format!(
                "{FAMILY}{{binding=\"apply-posted\",command=\"{APPLY}\",refusal=\"unchanged\"}} 2"
            ),
            format!(
                "{FAMILY}{{binding=\"record-posted\",command=\"{RECORD}\",refusal=\"empty\"}} 2"
            ),
        ]
    );
}
