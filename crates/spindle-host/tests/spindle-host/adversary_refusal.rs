//! Adversary cases for policies chosen per refusal, on the host: the exhausted counter when the
//! last attempt's refusal changes the policy, a failure with no declared outcome on the last
//! attempt, and a second fixture (`retry: [busy]`, `drop: {except: [busy]}`) whose universal
//! `on_failure` word is `drop` while one refusal retries.

use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock, RETRY_ATTEMPTS, Retries};
use spindle_kernel::External;

const CLOSE: &str = "library.lending.CloseLoan";
const THANK: &str = "library.notices.SendThanks";
const BINDING: &str = "thank-on-close";
const RETRYING: &str = "crates/spindle-host/tests/fixtures/refusal";
const DROPPING: &str = "crates/spindle-host/tests/fixtures/adversary_refusal";

/// One attempt's answer: a declared refusal, `None` for no answer at all (the command fails with
/// no declared outcome).
type Answer = Option<&'static str>;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("laptop.toml reads");
    Environment::parse(&text).expect("laptop.toml is an environment")
}

#[derive(Default)]
struct Script {
    answers: VecDeque<Answer>,
    current: Option<Option<Answer>>,
    asked: BTreeSet<String>,
    attempts: u32,
}

#[derive(Clone, Default)]
struct Notices(Arc<Mutex<Script>>);

impl Notices {
    fn answering(answers: impl IntoIterator<Item = Answer>) -> Self {
        let notices = Self::default();
        notices.lock().answers = answers.into_iter().collect();
        notices
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Script> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn attempts(&self) -> u32 {
        self.lock().attempts
    }

    fn decide(&self, question: &External<'_>) -> Result<bool, String> {
        assert_eq!(question.command, THANK, "the only external causes");
        let mut script = self.lock();
        if script.current.is_none() || script.asked.contains(question.outcome) {
            let next = script.answers.pop_front();
            script.current = Some(next);
            script.asked.clear();
            script.attempts += 1;
        }
        script.asked.insert(question.outcome.to_owned());
        match script.current {
            Some(Some(None)) => Err("no answer".to_owned()),
            Some(Some(Some(outcome))) => Ok(outcome == question.outcome),
            _ => Ok(false),
        }
    }
}

fn host(fixture: &str, notices: &Notices, retries: Retries) -> (Host, tempfile::TempDir) {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let loaded = spindle_spec::load(&root().join(fixture)).expect("the fixture compiles");
    let answering = notices.clone();
    let decider = move |question: &External<'_>| answering.decide(question);
    let host = Host::deciding_with_retries(
        loaded.ir,
        &laptop(),
        Arc::new(ManualClock::new()),
        state.path(),
        Box::new(decider),
        retries,
    )
    .expect("the host starts");
    (host, state)
}

fn close(host: &mut Host) -> spindle_host::Report {
    let Value::Object(input) = json!({"loan_id": "loan-1", "member_id": "m-7"}) else {
        unreachable!("an object")
    };
    host.submit(CLOSE, input).expect("the loan closes")
}

fn counter(host: &Host, name: &str) -> Option<u64> {
    host.counters("notices")
        .expect("the notices run")
        .value(&format!("binding.{BINDING}.{name}"))
}

/// Four `busy` retries then `no-reader` on the fifth: the policy for the last attempt is `drop`,
/// so the occurrence ended by its policy, not by the limit.
#[test]
fn adversary_a_drop_on_the_last_attempt_is_not_exhausted() {
    let notices = Notices::answering([
        Some("busy"),
        Some("busy"),
        Some("busy"),
        Some("busy"),
        Some("no-reader"),
    ]);
    let (mut host, _state) = host(RETRYING, &notices, Retries::default());
    let report = close(&mut host);
    assert_eq!(notices.attempts(), RETRY_ATTEMPTS);
    assert_eq!(report.refused.len(), 1, "{report:?}");
    assert_eq!(report.refused[0].outcome, "no-reader");
    assert_eq!(report.refused[0].attempts, RETRY_ATTEMPTS);
    assert_eq!(counter(&host, "attempted"), Some(u64::from(RETRY_ATTEMPTS)));
    assert_eq!(counter(&host, "exhausted"), Some(0));
}

/// Four `busy` retries then a failure with no declared outcome: the fallback retries, so the
/// limit gave the occurrence up.
#[test]
fn adversary_an_untyped_failure_on_the_last_attempt_is_exhausted() {
    let notices =
        Notices::answering([Some("busy"), Some("busy"), Some("busy"), Some("busy"), None]);
    let (mut host, _state) = host(RETRYING, &notices, Retries::default());
    let report = close(&mut host);
    assert_eq!(notices.attempts(), RETRY_ATTEMPTS);
    assert!(report.refused.is_empty(), "{report:?}");
    assert_eq!(report.failed.len(), 1, "{report:?}");
    assert_eq!(report.failed[0].attempts, RETRY_ATTEMPTS);
    assert_eq!(counter(&host, "exhausted"), Some(1));
}

/// The universal word of `retry: [busy]`, `drop: {except: [busy]}` is `drop`; `busy` still
/// retries, is counted, and is given up at the limit.
#[test]
fn adversary_a_retried_refusal_under_a_dropping_fallback_is_retried_and_counted() {
    let notices = Notices::answering([Some("busy"); 10]);
    let (mut host, _state) = host(DROPPING, &notices, Retries::default());
    let report = close(&mut host);
    assert_eq!(notices.attempts(), RETRY_ATTEMPTS);
    assert_eq!(report.refused.len(), 1, "{report:?}");
    assert_eq!(report.refused[0].attempts, RETRY_ATTEMPTS);
    assert_eq!(counter(&host, "attempted"), Some(u64::from(RETRY_ATTEMPTS)));
    assert_eq!(counter(&host, "exhausted"), Some(1));
    let counters = host.counters("notices").expect("the notices run");
    assert_eq!(counters.uncatalogued(), 0);
}

/// Under the dropping fallback a failure with no declared outcome ends the occurrence at once,
/// and so does a declared refusal the table drops.
#[test]
fn adversary_the_dropping_fallback_ends_an_untyped_failure_at_once() {
    for script in [
        vec![None, None],
        vec![Some("busy"), Some("no-reader"), None],
    ] {
        let expected = u32::try_from(script.len()).expect("a count") - 1;
        let notices = Notices::answering(script.clone());
        let (mut host, _state) = host(DROPPING, &notices, Retries::default());
        let report = close(&mut host);
        let expected = if script[0].is_none() { 1 } else { expected };
        assert_eq!(notices.attempts(), expected, "{script:?}: {report:?}");
        assert_eq!(counter(&host, "exhausted"), Some(0), "{script:?}");
    }
}

/// `--drop-retries` on the dropping-fallback fixture: `busy` is no longer retried.
#[test]
fn adversary_dropped_retries_stop_a_retried_refusal_under_a_dropping_fallback() {
    let notices = Notices::answering([Some("busy"), None]);
    let (mut host, _state) = host(DROPPING, &notices, Retries::Dropped);
    let report = close(&mut host);
    assert_eq!(notices.attempts(), 1, "{report:?}");
    assert_eq!(report.refused.len(), 1, "{report:?}");
    assert_eq!(report.refused[0].outcome, "busy");
}
