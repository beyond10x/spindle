//! A command a binding invoked whose last attempt was refused or did not execute, and that its
//! binding did not escalate, is kept as a dead letter in its component's store: listed with its
//! binding, event, command, error and attempts, kept across a restart, and replayed once
//! (story:dead-letters-kept-and-replayed).
//!
//! The fixture `escalation` ships each placed order through `ship-on-order`, which drops
//! `cancelled`, escalates `no-address` and `oversize` and retries `busy`; which outcome `Ship`
//! takes is the decider's, set per test.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Attempted, Host, ManualClock, RETRY_ATTEMPTS, Stores, Undelivered};
use spindle_kernel::{Change, External, MemoryStore, Removal, Scan, Store, StoreError, Stored};

const PLACE: &str = "depot.orders.PlaceOrder";
const SHIP: &str = "depot.shipping.Ship";
const SHIPPING: &str = "ship-on-order";
const PLACED: &str = "depot.orders.OrderPlaced";

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

/// The outcome of `Ship` the carrier answers, `shipped` for none of its refusals.
#[derive(Clone)]
struct Carrier(Arc<Mutex<&'static str>>);

impl Carrier {
    fn new(outcome: &'static str) -> Self {
        Self(Arc::new(Mutex::new(outcome)))
    }

    fn answers(&self, outcome: &'static str) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = outcome;
    }
}

fn host(carrier: &Carrier, state: &Path) -> Host {
    let loaded = spindle_spec::load(&root().join("crates/spindle-host/tests/fixtures/escalation"))
        .expect("the fixture compiles");
    let answering = carrier.clone();
    let decider = move |question: &External<'_>| {
        let outcome = *answering.0.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(question.command == SHIP && question.outcome == outcome)
    };
    Host::deciding(
        loaded.ir,
        &laptop(),
        Arc::new(ManualClock::new()),
        state,
        Box::new(decider),
    )
    .expect("the host starts")
}

fn state() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory")
}

fn place(host: &mut Host, order: &str) -> spindle_host::Report {
    host.submit(PLACE, object(json!({"order_id": order})))
        .expect("the order is placed")
}

fn letters(host: &mut Host) -> Vec<Undelivered> {
    host.dead_letters().expect("the dead letters read")
}

#[test]
fn an_exhausted_command_is_kept_as_a_dead_letter() {
    let directory = state();
    let carrier = Carrier::new("busy");
    let mut host = host(&carrier, directory.path());
    let report = place(&mut host, "o-1");
    assert_eq!(report.refused.len(), 1, "{report:?}");
    assert_eq!(report.refused[0].attempts, RETRY_ATTEMPTS, "{report:?}");
    carrier.answers("cancelled");
    place(&mut host, "o-2");
    let kept = letters(&mut host);
    assert_eq!(kept.len(), 2, "{kept:?}");
    let busy = &kept[0];
    assert_eq!(busy.component, "shipping");
    assert_eq!(busy.letter.binding, SHIPPING);
    assert_eq!(busy.letter.event.as_deref(), Some(PLACED));
    assert_eq!(busy.letter.command, SHIP);
    assert_eq!(busy.letter.outcome.as_deref(), Some("busy"));
    assert_eq!(busy.letter.error, "depot.shipping.Busy");
    assert_eq!(busy.letter.attempts, RETRY_ATTEMPTS);
    assert_eq!(busy.letter.input, object(json!({"order_id": "o-1"})));
    let cancelled = &kept[1];
    assert_eq!(cancelled.letter.outcome.as_deref(), Some("cancelled"));
    assert_eq!(cancelled.letter.error, "depot.shipping.Cancelled");
    assert_eq!(cancelled.letter.attempts, 1, "a dropped refusal ran once");
    assert_ne!(
        busy.id, cancelled.id,
        "each dead letter has an id of its own"
    );
}

#[test]
fn a_command_the_kernel_rejects_on_every_retry_is_kept_with_its_kind() {
    let directory = state();
    let loaded = spindle_spec::load(&root().join("crates/spindle-cli/tests/fixtures/retried"))
        .expect("the fixture compiles");
    let mut host = Host::new(
        loaded.ir,
        &laptop(),
        Arc::new(ManualClock::new()),
        directory.path(),
    )
    .expect("the host starts");
    let report = host
        .submit(
            "notes.pad.WriteNote",
            object(json!({"note_id": "n1", "text": "kept"})),
        )
        .expect("the note is written");
    assert_eq!(report.failed.len(), 1, "{report:?}");
    let kept = letters(&mut host);
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(report.failed[0].dead_letter, Some(kept[0].id), "{report:?}");
    assert_eq!(kept[0].letter.binding, "index-note");
    assert_eq!(kept[0].letter.command, "notes.index.IndexNote");
    assert_eq!(kept[0].letter.outcome, None);
    assert_eq!(
        kept[0].letter.error,
        report.failed[0].kind.unwrap_or_default()
    );
    assert_eq!(kept[0].letter.attempts, RETRY_ATTEMPTS);
}

#[test]
fn a_dead_letter_survives_a_restart() {
    let directory = state();
    let carrier = Carrier::new("busy");
    let before = {
        let mut host = host(&carrier, directory.path());
        place(&mut host, "o-1");
        letters(&mut host)
    };
    assert_eq!(before.len(), 1, "{before:?}");
    let mut reopened = host(&carrier, directory.path());
    assert_eq!(letters(&mut reopened), before);
}

#[test]
fn a_replay_runs_the_command_once() {
    let directory = state();
    let carrier = Carrier::new("busy");
    let mut host = host(&carrier, directory.path());
    place(&mut host, "o-1");
    let kept = letters(&mut host);
    assert_eq!(kept.len(), 1, "{kept:?}");
    carrier.answers("shipped");
    let report = host
        .replay("shipping", kept[0].id)
        .expect("the replay runs")
        .expect("the dead letter waits for its replay");
    let shipped = report
        .ran
        .iter()
        .filter(|ran| ran.command == SHIP)
        .collect::<Vec<_>>();
    assert_eq!(shipped.len(), 1, "{report:?}");
    assert_eq!(shipped[0].attempts, 1, "{report:?}");
    assert_eq!(
        host.list("depot.shipping.Shipments")
            .expect("the shipments read"),
        vec![object(json!({"order_id": "o-1"}))]
    );
    assert!(letters(&mut host).is_empty(), "the replayed letter is done");

    carrier.answers("busy");
    place(&mut host, "o-2");
    let kept = letters(&mut host);
    let report = host
        .replay("shipping", kept[0].id)
        .expect("the replay runs")
        .expect("the dead letter waits for its replay");
    assert_eq!(report.refused.len(), 1, "{report:?}");
    assert_eq!(
        report.refused[0].attempts, 1,
        "a replay runs the command once, without the binding's retries: {report:?}"
    );
    let again = letters(&mut host);
    assert_eq!(again.len(), 1, "a refused replay is kept again: {again:?}");
    assert_ne!(again[0].id, kept[0].id);
    assert_eq!(again[0].letter.attempts, 1);
    assert_eq!(again[0].letter.event.as_deref(), Some(PLACED));
}

#[test]
fn a_replayed_dead_letter_is_not_replayed_again() {
    let directory = state();
    let carrier = Carrier::new("busy");
    let mut host = host(&carrier, directory.path());
    place(&mut host, "o-1");
    let id = letters(&mut host)[0].id;
    carrier.answers("shipped");
    assert!(
        host.replay("shipping", id)
            .expect("the replay runs")
            .is_some()
    );
    assert!(
        host.replay("shipping", id)
            .expect("the second replay is answered")
            .is_none(),
        "a dead letter is replayed once"
    );
    assert!(
        host.replay("orders", id)
            .expect("a replay on another component is answered")
            .is_none()
    );
    drop(host);
    let mut reopened = self::host(&carrier, directory.path());
    assert!(
        reopened
            .replay("shipping", id)
            .expect("the replay after a restart is answered")
            .is_none(),
        "the replay is recorded with the letter"
    );
}

#[test]
fn an_escalated_command_is_no_dead_letter() {
    let directory = state();
    let carrier = Carrier::new("no-address");
    let mut host = host(&carrier, directory.path());
    let report = place(&mut host, "o-1");
    assert_eq!(report.escalated.len(), 1, "{report:?}");
    assert!(letters(&mut host).is_empty());
}

#[test]
fn a_dead_letter_is_no_published_event() {
    let directory = state();
    let carrier = Carrier::new("busy");
    let mut host = host(&carrier, directory.path());
    place(&mut host, "o-1");
    assert_eq!(letters(&mut host).len(), 1);
    let published = host
        .published()
        .expect("the events read")
        .into_iter()
        .map(|published| published.event.name)
        .collect::<Vec<_>>();
    assert_eq!(published, [PLACED]);
    assert_eq!(
        host.reader()
            .published()
            .expect("the events read beside the host")
            .len(),
        1
    );
}

#[test]
fn a_reset_forgets_the_dead_letters() {
    let directory = state();
    let carrier = Carrier::new("busy");
    let mut host = host(&carrier, directory.path());
    place(&mut host, "o-1");
    assert_eq!(letters(&mut host).len(), 1);
    host.reset().expect("the reset empties every store");
    assert!(letters(&mut host).is_empty());
}

/// An environment whose only store is `elsewhere`, which this host has none of its own for: each
/// component keeps its state in the store the test gives it.
fn elsewhere() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"elsewhere\"\n",
        "os = \"linux\"\n",
        "arch = \"x86_64\"\n",
        "\n",
        "[provides]\n",
        "\"store.durable\" = [\"elsewhere\"]\n",
        "\"store.ephemeral\" = [\"elsewhere\"]\n",
        "\"identity.mint\" = [\"uuid-v4\"]\n",
        "clock = [\"system\"]\n",
        "channel = [\"in-process\"]\n",
        "projection = [\"in-process\"]\n",
        "\"external.*\" = [\"operator-prompt\"]\n",
    ))
    .expect("the environment parses")
}

/// An environment that keeps every store in memory.
fn memory() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("laptop.toml reads")
        .replace("[\"sqlite\", \"memory\"]", "[\"memory\"]");
    Environment::parse(&text).expect("an environment")
}

/// A decider that refuses every `Ship` as `busy`.
fn busy(question: &External<'_>) -> Result<bool, String> {
    Ok(question.command == SHIP && question.outcome == "busy")
}

/// A store in memory that keeps no dead letters: it leaves [`Store::keep_dead_letter`] to its
/// default, which refuses.
struct Forgetful(MemoryStore);

impl Store for Forgetful {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.0.load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.0.commit(change)
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.0.remove(removal)
    }

    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError> {
        self.0.scan(entity)
    }
}

/// A dead letter its store could not keep is reported with the store's reason, and with no id.
#[test]
fn a_dead_letter_its_store_cannot_keep_names_why() {
    let loaded = spindle_spec::load(&root().join("crates/spindle-host/tests/fixtures/escalation"))
        .expect("the fixture compiles");
    let stores: Stores = BTreeMap::from([
        (
            "orders".to_owned(),
            Box::new(MemoryStore::default()) as Box<dyn Store>,
        ),
        (
            "shipping".to_owned(),
            Box::new(Forgetful(MemoryStore::default())) as Box<dyn Store>,
        ),
    ]);
    let mut host = Host::supplied(
        loaded.ir,
        &elsewhere(),
        Arc::new(ManualClock::new()),
        Some(Box::new(busy)),
        stores,
    )
    .expect("the host starts");
    let report = place(&mut host, "o-1");
    assert_eq!(report.refused.len(), 1, "{report:?}");
    assert_eq!(report.refused[0].dead_letter, None, "{report:?}");
    assert_eq!(
        report.refused[0].not_kept.as_deref(),
        Some("store: this store keeps no dead letters"),
        "{report:?}"
    );
    assert!(letters(&mut host).is_empty());
}

/// A dead letter its store kept names no reason it was not kept.
#[test]
fn a_kept_dead_letter_names_no_reason() {
    let directory = state();
    let carrier = Carrier::new("busy");
    let mut host = host(&carrier, directory.path());
    let report = place(&mut host, "o-1");
    assert!(report.refused[0].dead_letter.is_some(), "{report:?}");
    assert_eq!(report.refused[0].not_kept, None, "{report:?}");
}

/// A reset of stores in memory replaces each with an empty one, which continues the ids: the next
/// dead letter's id is past the forgotten one's, and a replay naming that one runs nothing.
#[test]
fn a_reset_in_memory_continues_the_ids() {
    let loaded = spindle_spec::load(&root().join("crates/spindle-host/tests/fixtures/escalation"))
        .expect("the fixture compiles");
    let directory = state();
    let mut host = Host::deciding(
        loaded.ir,
        &memory(),
        Arc::new(ManualClock::new()),
        directory.path(),
        Box::new(busy),
    )
    .expect("the host starts");
    place(&mut host, "o-forgotten");
    let forgotten = letters(&mut host)[0].id;
    host.reset().expect("the reset empties every store");
    assert!(
        directory
            .path()
            .read_dir()
            .expect("the state reads")
            .next()
            .is_none(),
        "the stores are in memory"
    );
    place(&mut host, "o-later");
    let later = letters(&mut host);
    assert_eq!(later.len(), 1, "{later:?}");
    assert!(later[0].id > forgotten, "{later:?} after {forgotten}");
    assert!(
        host.replay("shipping", forgotten)
            .expect("the replay is answered")
            .is_none()
    );
}

/// A replay answers its own run apart from what settled after it ([`Host::replayed`]): a
/// refused replay is answered refused, a replay that ran is answered ran.
#[test]
fn a_replay_answers_its_own_run() {
    let directory = state();
    let carrier = Carrier::new("busy");
    let mut host = host(&carrier, directory.path());
    place(&mut host, "o-1");
    place(&mut host, "o-2");
    let kept = letters(&mut host);
    assert_eq!(kept.len(), 2, "{kept:?}");
    let refused = host
        .replayed("shipping", kept[0].id)
        .expect("the replay runs")
        .expect("the dead letter waits for its replay");
    match &refused.own {
        Attempted::Refused(own) => {
            assert_eq!(own.command, SHIP);
            assert_eq!(own.outcome, "busy");
            assert_eq!(Some(own), refused.report.refused.first(), "{refused:?}");
        }
        other => panic!("the replay was refused: {other:?}"),
    }
    carrier.answers("shipped");
    let ran = host
        .replayed("shipping", kept[1].id)
        .expect("the replay runs")
        .expect("the dead letter waits for its replay");
    match &ran.own {
        Attempted::Ran(own) => {
            assert_eq!(own.command, SHIP);
            assert_eq!(own.executed.outcome, "shipped");
        }
        other => panic!("the replay ran: {other:?}"),
    }
}
