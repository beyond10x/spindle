//! An event binding whose failure policy is chosen per refusal: `on_failure: {drop: [no-reader],
//! retry: {except: [no-reader]}}`, compiled by ESS to `on_refusal`. An attempt refused with
//! `no-reader` ends the occurrence at once; any other refusal, and a failure that carries no
//! declared outcome, is retried as an `on_failure: retry` binding is, up to [`RETRY_ATTEMPTS`].
//!
//! The fixture `library`: closing a loan publishes `LoanClosed`, which `thank-on-close` carries to
//! the notices' `SendThanks`. Its `no-reader` and `busy` refusals are decided outside the model;
//! the decider here answers each attempt from a script.

use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Cause, Host, ManualClock, RETRY_ATTEMPTS, Retries};
use spindle_kernel::External;

const CLOSE: &str = "library.lending.CloseLoan";
const THANK: &str = "library.notices.SendThanks";
const BINDING: &str = "thank-on-close";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("laptop.toml reads");
    Environment::parse(&text).expect("laptop.toml is an environment")
}

/// The refusal each attempt of `SendThanks` is answered with, in order; `None`, or a script run
/// out, takes it (`sent`). An attempt starts when an outcome is asked about a second time.
#[derive(Default)]
struct Script {
    answers: VecDeque<Option<&'static str>>,
    current: Option<Option<&'static str>>,
    asked: BTreeSet<String>,
    attempts: u32,
}

#[derive(Clone, Default)]
struct Notices(Arc<Mutex<Script>>);

impl Notices {
    fn answering(answers: impl IntoIterator<Item = Option<&'static str>>) -> Self {
        let notices = Self::default();
        notices.lock().answers = answers.into_iter().collect();
        notices
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Script> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// How many attempts asked the decider anything.
    fn attempts(&self) -> u32 {
        self.lock().attempts
    }

    fn decide(&self, question: &External<'_>) -> bool {
        assert_eq!(question.command, THANK, "the only external causes");
        let mut script = self.lock();
        if script.current.is_none() || script.asked.contains(question.outcome) {
            let next = script.answers.pop_front().flatten();
            script.current = Some(next);
            script.asked.clear();
            script.attempts += 1;
        }
        script.asked.insert(question.outcome.to_owned());
        script.current == Some(Some(question.outcome))
    }
}

fn host(notices: &Notices, retries: Retries) -> (Host, tempfile::TempDir) {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let loaded = spindle_spec::load(&root().join("crates/spindle-host/tests/fixtures/refusal"))
        .expect("the fixture compiles");
    let answering = notices.clone();
    let decider = move |question: &External<'_>| Ok(answering.decide(question));
    let host = Host::deciding_with_retries(
        loaded.ir,
        &laptop(),
        Arc::new(ManualClock::new()),
        state.path(),
        Box::new(decider),
        retries,
    )
    .expect("the host starts with a binding whose policy is chosen per refusal");
    (host, state)
}

fn close(host: &mut Host, loan: &str) -> spindle_host::Report {
    let Value::Object(input) = json!({"loan_id": loan, "member_id": "m-7"}) else {
        unreachable!("an object")
    };
    host.submit(CLOSE, input).expect("the loan closes")
}

fn thanked(host: &mut Host) -> Vec<Map<String, Value>> {
    host.list("library.notices.AllThanks")
        .expect("the thanks are listed")
}

fn delivered() -> Cause {
    Cause::Delivered {
        binding: BINDING.to_owned(),
    }
}

#[test]
fn a_listed_refusal_is_dropped_after_one_attempt() {
    let notices = Notices::answering([Some("no-reader"), None]);
    let (mut host, _state) = host(&notices, Retries::default());
    let report = close(&mut host, "loan-1");

    assert_eq!(notices.attempts(), 1, "no-reader ends the occurrence");
    assert!(
        report.ran.iter().all(|ran| ran.command != THANK),
        "{report:?}"
    );
    assert_eq!(report.refused.len(), 1, "{report:?}");
    let refused = &report.refused[0];
    assert_eq!(refused.command, THANK);
    assert_eq!(refused.outcome, "no-reader");
    assert_eq!(refused.attempts, 1, "one attempt is counted");
    assert_eq!(refused.cause, delivered());
    assert!(report.failed.is_empty(), "{report:?}");
    assert!(thanked(&mut host).is_empty());

    let later = host.advance().expect("the host goes on");
    assert!(
        later.ran.is_empty() && later.refused.is_empty() && later.failed.is_empty(),
        "a dropped occurrence is not run again: {later:?}"
    );
    assert_eq!(notices.attempts(), 1);
}

#[test]
fn another_refusal_falls_back_to_retry() {
    let notices = Notices::answering([Some("busy"), Some("busy")]);
    let (mut host, _state) = host(&notices, Retries::default());
    let report = close(&mut host, "loan-1");

    assert_eq!(notices.attempts(), 3, "refused twice as busy, then sent");
    let sent = report
        .ran
        .iter()
        .find(|ran| ran.command == THANK)
        .unwrap_or_else(|| panic!("SendThanks took effect: {report:?}"));
    assert_eq!(sent.executed.outcome, "sent");
    assert_eq!(sent.attempts, 3, "{sent:?}");
    assert_eq!(sent.cause, delivered());
    assert!(
        report.refused.is_empty() && report.failed.is_empty(),
        "a retried attempt is not a final failure: {report:?}"
    );
    let rows = thanked(&mut host);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].get("reader_id"), Some(&json!("m-7")), "{rows:?}");
}

/// The policy is chosen after every attempt by that attempt's refusal: a retried occurrence that
/// is then refused with `no-reader` stops there.
#[test]
fn a_listed_refusal_after_a_retry_ends_the_occurrence() {
    let notices = Notices::answering([Some("busy"), Some("no-reader"), None]);
    let (mut host, _state) = host(&notices, Retries::default());
    let report = close(&mut host, "loan-1");

    assert_eq!(notices.attempts(), 2);
    assert_eq!(report.refused.len(), 1, "{report:?}");
    assert_eq!(report.refused[0].outcome, "no-reader");
    assert_eq!(report.refused[0].attempts, 2);
    assert!(thanked(&mut host).is_empty());
}

#[test]
fn the_fallback_retry_stops_at_the_bound_limit() {
    let notices = Notices::answering([Some("busy"); 10]);
    let (mut host, _state) = host(&notices, Retries::default());
    let report = close(&mut host, "loan-1");

    assert_eq!(notices.attempts(), RETRY_ATTEMPTS);
    assert_eq!(report.refused.len(), 1, "{report:?}");
    assert_eq!(report.refused[0].outcome, "busy");
    assert_eq!(report.refused[0].attempts, RETRY_ATTEMPTS);
    assert!(thanked(&mut host).is_empty());
}

#[test]
fn dropped_retries_make_one_attempt_whatever_the_refusal() {
    let notices = Notices::answering([Some("busy"), None]);
    let (mut host, _state) = host(&notices, Retries::Dropped);
    let report = close(&mut host, "loan-1");

    assert_eq!(notices.attempts(), 1);
    assert_eq!(report.refused.len(), 1, "{report:?}");
    assert_eq!(report.refused[0].outcome, "busy");
    assert_eq!(report.refused[0].attempts, 1);
}

/// Every attempt is counted; an occurrence a listed refusal drops is not counted as exhausted.
#[test]
fn refusal_attempts_are_counted() {
    let notices = Notices::answering([Some("busy"), Some("no-reader")]);
    let (mut host, _state) = host(&notices, Retries::default());
    close(&mut host, "loan-1");
    let counters = host.counters("notices").expect("the notices run");
    assert_eq!(
        counters.value(&format!("binding.{BINDING}.attempted")),
        Some(2)
    );
    assert_eq!(
        counters.value(&format!("binding.{BINDING}.exhausted")),
        Some(0)
    );
    assert_eq!(counters.uncatalogued(), 0);
}
