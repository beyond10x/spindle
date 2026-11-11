//! An event binding with `on_failure: retry`: the host retries a failed invocation, at once and
//! on the calling thread, until it takes effect or [`RETRY_ATTEMPTS`] attempts were made, and
//! counts every attempt.
//!
//! The fixture `filing` files with `desk`, whose `File` publishes `Filed` and then `Noted`.
//! `record-filing` (`on_failure: retry`) carries `Filed` to the ledger's `Record`, whose `held`
//! outcome is decided outside the model; `log-note` (`on_failure: drop`) carries `Noted` to `Log`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Cause, Host, ManualClock, RETRY_ATTEMPTS, Retries};
use spindle_kernel::External;

const RECORD: &str = "filing.ledger.Record";
const BINDING: &str = "record-filing";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("laptop.toml reads");
    Environment::parse(&text).expect("laptop.toml is an environment")
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

/// How the ledger answers `Record`'s `held` cause: it holds the next `hold` entries back, and
/// every question it was asked is counted in `asked`.
#[derive(Clone, Default)]
struct Ledger {
    hold: Arc<AtomicU32>,
    asked: Arc<AtomicU32>,
}

impl Ledger {
    fn holding(&self, entries: u32) {
        self.hold.store(entries, Ordering::SeqCst);
    }

    fn asked(&self) -> u32 {
        self.asked.load(Ordering::SeqCst)
    }
}

fn host(ledger: &Ledger, retries: Retries) -> (Host, tempfile::TempDir) {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let loaded = spindle_spec::load(&root().join("crates/spindle-host/tests/fixtures/retry"))
        .expect("the fixture compiles");
    let answering = ledger.clone();
    let decider = move |question: &External<'_>| {
        assert_eq!(
            (question.command, question.outcome),
            (RECORD, "held"),
            "the only external cause"
        );
        answering.asked.fetch_add(1, Ordering::SeqCst);
        let held = answering
            .hold
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                left.checked_sub(1)
            })
            .is_ok();
        Ok(held)
    };
    let host = Host::deciding_with_retries(
        loaded.ir,
        &laptop(),
        Arc::new(ManualClock::new()),
        state.path(),
        Box::new(decider),
        retries,
    )
    .expect("the host starts with a retry binding");
    (host, state)
}

fn file(host: &mut Host, id: &str) -> spindle_host::Report {
    host.submit("filing.desk.File", object(json!({"filing_id": id})))
        .expect("the filing is taken")
}

fn entries(host: &mut Host) -> Vec<Value> {
    host.list("filing.ledger.Entries")
        .expect("the entries are listed")
        .into_iter()
        .map(|row| row["filing_id"].clone())
        .collect()
}

#[test]
fn a_failing_invocation_is_retried_until_it_succeeds() {
    let ledger = Ledger::default();
    let (mut host, _state) = host(&ledger, Retries::default());
    ledger.holding(2);
    let report = file(&mut host, "f-1");

    assert_eq!(ledger.asked(), 3, "refused twice, then taken");
    let recorded = report
        .ran
        .iter()
        .find(|ran| ran.command == RECORD)
        .unwrap_or_else(|| panic!("Record took effect: {report:?}"));
    assert_eq!(recorded.executed.outcome, "recorded");
    assert_eq!(recorded.attempts, 3, "{recorded:?}");
    assert_eq!(
        recorded.cause,
        Cause::Delivered {
            binding: BINDING.to_owned()
        }
    );
    assert!(
        report.refused.is_empty() && report.failed.is_empty(),
        "an attempt that is retried is not a final failure: {report:?}"
    );
    assert_eq!(
        entries(&mut host),
        [json!("f-1")],
        "the state is the success"
    );
}

#[test]
fn retries_stop_at_the_bound_limit() {
    let ledger = Ledger::default();
    let (mut host, _state) = host(&ledger, Retries::default());
    ledger.holding(u32::MAX);
    let report = file(&mut host, "f-1");

    assert_eq!(ledger.asked(), RETRY_ATTEMPTS, "exactly the bound");
    assert!(
        report.ran.iter().all(|ran| ran.command != RECORD),
        "{report:?}"
    );
    assert_eq!(report.refused.len(), 1, "one final failure: {report:?}");
    let refused = &report.refused[0];
    assert_eq!(refused.command, RECORD);
    assert_eq!(refused.outcome, "held");
    assert_eq!(refused.attempts, RETRY_ATTEMPTS);
    assert_eq!(
        refused.cause,
        Cause::Delivered {
            binding: BINDING.to_owned()
        }
    );
    assert!(entries(&mut host).is_empty());

    let later = host.advance().expect("the host goes on");
    assert!(
        later.ran.is_empty() && later.refused.is_empty() && later.failed.is_empty(),
        "an exhausted invocation is not run again: {later:?}"
    );
    assert_eq!(ledger.asked(), RETRY_ATTEMPTS);
}

#[test]
fn a_retry_does_not_reorder_later_events() {
    let ledger = Ledger::default();
    let (mut host, _state) = host(&ledger, Retries::default());
    ledger.holding(2);
    let report = file(&mut host, "f-1");

    assert_eq!(
        report
            .ran
            .iter()
            .map(|ran| (ran.command.as_str(), ran.attempts))
            .collect::<Vec<_>>(),
        [
            ("filing.desk.File", 1),
            (RECORD, 3),
            ("filing.ledger.Log", 1)
        ],
        "Noted, published after Filed, is invoked once Filed's retry settled"
    );
    assert_eq!(
        host.published()
            .expect("the events read")
            .iter()
            .map(|published| published.event.name.as_str())
            .collect::<Vec<_>>(),
        [
            "filing.desk.Filed",
            "filing.desk.Noted",
            "filing.ledger.Recorded",
            "filing.ledger.Logged"
        ]
    );
}

#[test]
fn retry_attempts_are_counted() {
    let ledger = Ledger::default();
    let (mut host, _state) = host(&ledger, Retries::default());
    ledger.holding(2);
    file(&mut host, "f-1");
    ledger.holding(u32::MAX);
    file(&mut host, "f-2");

    let counters = host.counters("ledger").expect("the ledger runs");
    let attempted = format!("binding.{BINDING}.attempted");
    let exhausted = format!("binding.{BINDING}.exhausted");
    assert_eq!(
        counters.value(&attempted),
        Some(3 + u64::from(RETRY_ATTEMPTS))
    );
    assert_eq!(counters.value(&exhausted), Some(1));
    assert_eq!(
        counters.value(&format!("command.{RECORD}.held")),
        Some(2 + u64::from(RETRY_ATTEMPTS)),
        "every attempt is a command the kernel ran"
    );
    assert_eq!(
        counters.value(&format!("command.{RECORD}.recorded")),
        Some(1)
    );
    assert_eq!(
        counters.value("binding.log-note.attempted"),
        None,
        "a drop binding has no retry counters"
    );
    assert_eq!(counters.uncatalogued(), 0);
    let desk = host.counters("desk").expect("the desk runs");
    assert_eq!(
        desk.value(&attempted),
        None,
        "the component that invokes counts it"
    );
}

#[test]
fn dropped_retries_make_one_attempt() {
    let ledger = Ledger::default();
    let (mut host, _state) = host(&ledger, Retries::Dropped);
    ledger.holding(u32::MAX);
    let report = file(&mut host, "f-1");

    assert_eq!(ledger.asked(), 1);
    assert_eq!(report.refused.len(), 1, "{report:?}");
    assert_eq!(report.refused[0].attempts, 1);
    let counters = host.counters("ledger").expect("the ledger runs");
    assert_eq!(
        counters.value(&format!("binding.{BINDING}.attempted")),
        Some(0)
    );
    assert_eq!(
        counters.value(&format!("binding.{BINDING}.exhausted")),
        Some(0)
    );
}
