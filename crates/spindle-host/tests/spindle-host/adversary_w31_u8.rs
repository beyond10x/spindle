//! Adversary cases of wave 31 U8 (story:dead-letters-kept-and-replayed): the dead letters a host
//! keeps across a reset, a replay its binding would escalate, and a replay after a restart.
//!
//! The fixture `escalation` ships each placed order through `ship-on-order`, which drops
//! `cancelled`, escalates `no-address` and `oversize` and retries `busy`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock, Undelivered};
use spindle_kernel::External;

const PLACE: &str = "depot.orders.PlaceOrder";
const SHIP: &str = "depot.shipping.Ship";

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

fn place(host: &mut Host, order: &str) {
    host.submit(PLACE, object(json!({"order_id": order})))
        .expect("the order is placed");
}

fn letters(host: &mut Host) -> Vec<Undelivered> {
    host.dead_letters().expect("the dead letters read")
}

/// `spindle.console.ReplayAnswer` promises `404` for a dead letter "a reset forgot", and
/// `spindle.observe.DeadLetter.id` that ids "grow and are never reused while the store is kept";
/// `Store::keep_dead_letter` promises an id "greater than every id this store answered before, a
/// dead letter's it forgot included". A console reset of a run with `--state` replaces each
/// SQLite file by an empty one, so the next dead letter takes the forgotten one's id, and a replay
/// naming the forgotten letter runs a command of another order.
#[test]
fn adv_w31_u8_a_replay_naming_a_letter_a_reset_forgot_runs_nothing() {
    let directory = state();
    let carrier = Carrier::new("busy");
    let mut host = host(&carrier, directory.path());
    place(&mut host, "o-forgotten");
    let forgotten = letters(&mut host);
    assert_eq!(forgotten.len(), 1, "{forgotten:?}");
    let forgotten = forgotten[0].id;
    host.reset().expect("the reset empties every store");
    place(&mut host, "o-later");
    let later = letters(&mut host);
    assert_eq!(later.len(), 1, "{later:?}");
    assert!(
        later[0].id > forgotten,
        "a dead letter kept after a reset reuses the forgotten id {forgotten}: {later:?}"
    );
    assert!(
        host.replay("shipping", forgotten)
            .expect("the replay is answered")
            .is_none(),
        "the letter the reset forgot is not replayed"
    );
}

/// A replay runs "without its binding's retries or escalation" (`Host::replay`,
/// `spindle.console.ReplayAnswer`): a replay the model refuses with an outcome its binding
/// escalates publishes no escalation event and is kept as a new dead letter, with the outcome it
/// took this time, the report naming that new letter.
#[test]
fn adv_w31_u8_a_replay_refused_with_an_escalating_outcome_is_not_escalated() {
    let directory = state();
    let carrier = Carrier::new("busy");
    let mut host = host(&carrier, directory.path());
    place(&mut host, "o-1");
    let kept = letters(&mut host);
    assert_eq!(kept.len(), 1, "{kept:?}");
    carrier.answers("no-address");
    let report = host
        .replay("shipping", kept[0].id)
        .expect("the replay runs")
        .expect("the dead letter waits for its replay");
    assert!(report.escalated.is_empty(), "{report:?}");
    assert_eq!(report.refused.len(), 1, "{report:?}");
    assert_eq!(report.refused[0].outcome, "no-address", "{report:?}");
    assert!(
        report.ran.is_empty(),
        "no FlagOrder ran, so no ShipmentStuck was published: {report:?}"
    );
    let again = letters(&mut host);
    assert_eq!(again.len(), 1, "{again:?}");
    assert_eq!(
        report.refused[0].dead_letter,
        Some(again[0].id),
        "{report:?}"
    );
    assert_eq!(again[0].letter.outcome.as_deref(), Some("no-address"));
    assert_eq!(again[0].letter.error, "depot.shipping.NoAddress");
    assert_eq!(again[0].letter.input, object(json!({"order_id": "o-1"})));
    let published = host
        .published()
        .expect("the events read")
        .into_iter()
        .map(|published| published.event.name)
        .collect::<Vec<_>>();
    assert_eq!(published, ["depot.orders.OrderPlaced"]);
}

/// A dead letter kept by one process is replayed by the next on the same state, with the input
/// it was kept with, and is then taken there too.
#[test]
fn adv_w31_u8_a_letter_kept_before_a_restart_is_replayed_with_its_input() {
    let directory = state();
    let carrier = Carrier::new("busy");
    let id = {
        let mut host = host(&carrier, directory.path());
        place(&mut host, "o-kept");
        let kept = letters(&mut host);
        assert_eq!(kept.len(), 1, "{kept:?}");
        kept[0].id
    };
    carrier.answers("shipped");
    let mut reopened = host(&carrier, directory.path());
    let report = reopened
        .replay("shipping", id)
        .expect("the replay runs")
        .expect("the letter of the earlier process waits");
    assert_eq!(report.ran.len(), 1, "{report:?}");
    assert_eq!(report.ran[0].command, SHIP);
    assert_eq!(
        reopened
            .list("depot.shipping.Shipments")
            .expect("the shipments read"),
        vec![object(json!({"order_id": "o-kept"}))]
    );
    assert!(letters(&mut reopened).is_empty());
    drop(reopened);
    let mut third = host(&carrier, directory.path());
    assert!(letters(&mut third).is_empty(), "the take was recorded");
}

/// "A command a periodic binding invoked is kept the same way, without an event"
/// (`website/docs/concepts/components-and-bindings.md`): rally's `tick` on a match nobody opened
/// is refused, kept with no event and the input its mapping read from the host context, and its
/// replay runs as a tick of that binding again, a refused replay kept anew without an event.
#[test]
fn adv_w31_u8_a_refused_tick_is_kept_without_an_event_and_replayed_as_a_tick() {
    let state = state();
    let clock = Arc::new(ManualClock::new());
    let loaded = spindle_spec::load(&root().join("examples/rally/ess")).expect("rally compiles");
    let mut host =
        Host::new(loaded.ir, &laptop(), clock.clone(), state.path()).expect("laptop admits rally");
    host.start("tick", object(json!({"match_id": "nobody-opened-this"})))
        .expect("the tick binding starts");
    clock.advance(std::time::Duration::from_secs(1));
    let advanced = host.advance().expect("the host advances");
    assert_eq!(advanced.refused.len(), 1, "{advanced:?}");
    let kept = letters(&mut host);
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(advanced.refused[0].dead_letter, Some(kept[0].id));
    assert_eq!(kept[0].component, "rally-table");
    assert_eq!(kept[0].letter.binding, "tick");
    assert_eq!(kept[0].letter.event, None);
    assert_eq!(kept[0].letter.command, "rally.table.Tick");
    assert_eq!(kept[0].letter.outcome.as_deref(), Some("no-such-match"));
    assert_eq!(kept[0].letter.error, "rally.table.MatchNotFound");
    assert_eq!(kept[0].letter.attempts, 1);
    assert_eq!(
        kept[0].letter.input.get("match_id"),
        Some(&json!("nobody-opened-this"))
    );
    let report = host
        .replay("rally-table", kept[0].id)
        .expect("the replay runs")
        .expect("the letter waits");
    assert_eq!(report.refused.len(), 1, "{report:?}");
    assert_eq!(
        report.refused[0].cause,
        spindle_host::Cause::Tick {
            binding: "tick".to_owned()
        }
    );
    let again = letters(&mut host);
    assert_eq!(again.len(), 1, "{again:?}");
    assert_ne!(again[0].id, kept[0].id);
    assert_eq!(again[0].letter.event, None);
}

/// The one dead letter `host` lists now.
fn only(host: &mut Host) -> u64 {
    let kept = letters(host);
    assert_eq!(kept.len(), 1, "{kept:?}");
    kept[0].id
}

/// Round 2: ids grow across resets in a row, one with no dead letter, and across a restart on
/// the files a reset left; no forgotten id is replayed, and the one waiting is.
#[test]
fn adv_w31_u8_r2_ids_grow_across_resets_in_a_row_and_a_restart() {
    let directory = state();
    let carrier = Carrier::new("busy");
    let mut forgotten = Vec::new();
    let mut host = host(&carrier, directory.path());
    place(&mut host, "o-1");
    forgotten.push(only(&mut host));
    host.reset().expect("the first reset");
    host.reset().expect("a reset with no dead letter");
    place(&mut host, "o-2");
    let second = only(&mut host);
    assert!(second > forgotten[0], "{second} after {forgotten:?}");
    forgotten.push(second);
    host.reset().expect("the third reset");
    host.reset().expect("the fourth reset");
    drop(host);
    let mut reopened = self::host(&carrier, directory.path());
    assert!(letters(&mut reopened).is_empty());
    place(&mut reopened, "o-3");
    let third = only(&mut reopened);
    assert!(third > second, "{third} after {forgotten:?}");
    for id in &forgotten {
        assert!(
            reopened
                .replay("shipping", *id)
                .expect("the replay is answered")
                .is_none(),
            "forgotten id {id} is not replayed"
        );
    }
    drop(reopened);
    let mut again = self::host(&carrier, directory.path());
    carrier.answers("shipped");
    let replayed = again
        .replayed("shipping", third)
        .expect("the replay runs")
        .expect("the letter waits after a restart");
    assert!(
        matches!(&replayed.own, spindle_host::Attempted::Ran(ran) if ran.command == SHIP),
        "{replayed:?}"
    );
    carrier.answers("busy");
    place(&mut again, "o-4");
    let fourth = only(&mut again);
    assert!(fourth > third, "{fourth} after {third}");
}

/// Round 2: a reset that cannot move a component's file aside (a read-only state directory)
/// reopens the file as it was (`restored: true`): its dead letter still waits under its id, and
/// a later reset, the directory writable again, forgets it and continues the ids past it.
#[cfg(unix)]
#[test]
fn adv_w31_u8_r2_a_failed_reset_keeps_the_letters_and_the_next_continues_past_them() {
    use std::os::unix::fs::PermissionsExt;

    let directory = state();
    let carrier = Carrier::new("busy");
    let mut host = host(&carrier, directory.path());
    place(&mut host, "o-1");
    let kept = only(&mut host);
    let mode = |bits: u32| {
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(bits))
            .expect("the state directory's mode is set");
    };
    mode(0o555);
    let failed = host.reset();
    mode(0o755);
    assert!(
        matches!(
            &failed,
            Err(spindle_host::HostError::Reset { restored: true, .. })
        ),
        "{failed:?}"
    );
    assert_eq!(only(&mut host), kept, "the failed reset kept the letter");
    host.reset()
        .expect("the reset, the directory writable again");
    place(&mut host, "o-2");
    let later = only(&mut host);
    assert!(later > kept, "{later} after {kept}");
}
