//! Adversary cases of wave 27 unit 8 (`story:cross-component-related-reads`): a host running
//! several components reads each entity another component owns from its owner's store.
//!
//! `tests/fixtures/adversary-w27-u8/quantifiers`: `meter` charges guarded by a `count:`, a
//! `forall:` and an `exists: false` row set over the holds `holds` owns, which `holds` places,
//! levels and releases. `tests/fixtures/adversary-w27-u8/subject`: `meter` also accepts a command
//! whose subject is a hold, an entity `holds` owns.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{ConformanceTarget, SemanticCommandRequest};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, HostError, ManualClock, Stores};
use spindle_kernel::{Change, MemoryStore, Removal, Store, StoreError, Stored, Write};

const PLACE: &str = "metering.holds.PlaceHold";
const LEVEL: &str = "metering.holds.LevelHold";
const RELEASE: &str = "metering.holds.ReleaseHold";
const COUNTED: &str = "metering.usage.ChargeCounted";
const ALL_HIGH: &str = "metering.usage.ChargeUnlessAllHigh";
const NONE: &str = "metering.usage.ChargeUnlessNone";
const NOTE: &str = "metering.usage.NoteHeld";
const TAG: &str = "metering.usage.TagCharges";
const DIRECT: &str = "metering.usage.ChargeDirect";
const HOLD_ORG: &str = "metering.usage.HoldOrg";
const HOLD: &str = "metering.holds.Hold";
const CHARGE: &str = "metering.usage.Charge";

const H1: &str = "b0b0b0b0-0000-4000-8000-000000000001";
const H2: &str = "b0b0b0b0-0000-4000-8000-000000000002";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/adversary-w27-u8")
        .join(name)
}

fn ir(path: &Path) -> spindle_spec::EssIr {
    spindle_spec::load(path).expect("the fixture compiles").ir
}

fn charge(number: u8) -> String {
    format!("c0c0c0c0-0000-4000-8000-0000000000{number:02x}")
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

fn environment(durable: &str) -> Environment {
    Environment::parse(&format!(
        "format = \"spindle-environment/1\"\nname = \"adversary\"\nos = \"linux\"\n\
         arch = \"x86_64\"\n\n[provides]\n\"store.durable\" = [\"{durable}\"]\n\
         \"store.ephemeral\" = [\"memory\"]\nclock = [\"system\"]\nchannel = [\"in-process\"]\n\
         projection = [\"in-process\"]\n"
    ))
    .expect("the environment parses")
}

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory")
}

/// The outcome the host answers: taken, refused by the model, or `unsupported: <construct>`.
fn outcome(host: &mut Host, command: &str, input: &Value) -> String {
    match host.submit(command, object(input.clone())) {
        Ok(report) => report
            .ran
            .first()
            .unwrap_or_else(|| panic!("{command} ran"))
            .executed
            .outcome
            .clone(),
        Err(HostError::CommandRefused { outcome, .. }) => outcome,
        Err(HostError::Execute { error, .. }) => match *error {
            spindle_kernel::ExecuteError::Unsupported { construct } => {
                format!("unsupported: {construct}")
            }
            other => format!("error: {other}"),
        },
        Err(other) => format!("host error: {other}"),
    }
}

/// The outcome ESS's interpreter answers for each step, in one store.
fn interpreted(path: &Path, steps: &[(&str, Value)]) -> Vec<String> {
    let interpreter = Interpreted::for_model(ir(path));
    let correlation = CorrelationId::new("adversary").expect("a correlation");
    steps
        .iter()
        .map(|(command, input)| {
            let fields: BTreeMap<String, Node> =
                serde_json::from_value(input.clone()).expect("nodes");
            interpreter
                .execute_command(SemanticCommandRequest {
                    command: command.parse().expect("a command name"),
                    actor: None,
                    caller: None,
                    input: fields,
                    correlation: correlation.clone(),
                })
                .unwrap_or_else(|error| panic!("the interpreter runs {command}: {error}"))
                .outcome
                .map(|outcome| outcome.outcome.to_string())
                .unwrap_or_default()
        })
        .collect()
}

fn rows_of(host: &mut Host, entity: &str) -> usize {
    host.rows()
        .expect("the host counts its rows")
        .into_iter()
        .find_map(|(name, rows)| (name == entity).then_some(rows))
        .unwrap_or_default()
}

/// Holds placed, levelled and released between charges guarded by a count, a universal test and
/// an absence over them; the owner starts with no rows.
fn quantifier_steps() -> Vec<(&'static str, Value)> {
    vec![
        (COUNTED, json!({"charge_id": charge(1), "org": "north"})),
        (NONE, json!({"charge_id": charge(2), "org": "north"})),
        (PLACE, json!({"hold_id": H1, "org": "north", "level": 1})),
        (COUNTED, json!({"charge_id": charge(3), "org": "north"})),
        (PLACE, json!({"hold_id": H2, "org": "north", "level": 5})),
        (COUNTED, json!({"charge_id": charge(4), "org": "north"})),
        (ALL_HIGH, json!({"charge_id": charge(5), "org": "north"})),
        (ALL_HIGH, json!({"charge_id": charge(6), "org": "south"})),
        (LEVEL, json!({"hold_id": H1, "level": 4})),
        (ALL_HIGH, json!({"charge_id": charge(7), "org": "north"})),
        (NONE, json!({"charge_id": charge(8), "org": "south"})),
        (NONE, json!({"charge_id": charge(9), "org": "north"})),
        (RELEASE, json!({"hold_id": H2})),
        (COUNTED, json!({"charge_id": charge(10), "org": "north"})),
        (NOTE, json!({"org": "north"})),
        (NOTE, json!({"org": "south"})),
        (
            TAG,
            json!({"charge_id": charge(1), "org": "north", "tag": "audit"}),
        ),
        (
            TAG,
            json!({"charge_id": charge(1), "org": "south", "tag": "audit"}),
        ),
    ]
}

/// The last four: an outcome that only publishes (`Store::commit_events`) and one that updates
/// its subject beside an `affects:` entry over the meter's own charges (`Store::commit_all`), each
/// guarded by a row set of holds, so each write is conditioned on a scan of the holds' store.
const QUANTIFIER_OUTCOMES: [&str; 18] = [
    "charged",
    "unheld",
    "placed",
    "charged",
    "placed",
    "over-held",
    "charged",
    "all-high",
    "levelled",
    "all-high",
    "unheld",
    "charged",
    "released",
    "charged",
    "noted",
    "not-held",
    "tagged",
    "not-held",
];

/// A `count:`, a `forall:` (true of no rows) and an `exists: false` row set over another
/// component's rows answer what ESS's interpreter answers, on a host whose stores are `durable`.
fn quantifiers_agree(durable: &str) {
    let path = fixture("quantifiers");
    let steps = quantifier_steps();
    let expected = interpreted(&path, &steps);
    assert_eq!(expected, QUANTIFIER_OUTCOMES, "ESS's interpreter");
    let state = scratch();
    let mut host = Host::new(
        ir(&path),
        &environment(durable),
        Arc::new(ManualClock::new()),
        state.path(),
    )
    .expect("the host admits the fixture");
    let answered: Vec<String> = steps
        .iter()
        .map(|(command, input)| outcome(&mut host, command, input))
        .collect();
    assert_eq!(answered, expected, "the host on {durable} stores");
    assert_eq!(rows_of(&mut host, CHARGE), 5, "each charge taken is kept");
    assert_eq!(rows_of(&mut host, HOLD), 1, "the released hold is gone");
}

#[test]
fn count_forall_and_absence_over_another_components_rows_agree_in_memory() {
    quantifiers_agree("memory");
}

#[test]
fn count_forall_and_absence_over_another_components_rows_agree_on_sqlite() {
    quantifiers_agree("sqlite");
}

/// A host running the meter alone refuses every quantifier command by name and keeps no charge.
#[test]
fn a_split_meter_refuses_each_quantifier_by_name() {
    let path = fixture("quantifiers");
    let state = scratch();
    let mut meter = Host::component(
        ir(&path),
        &environment("sqlite"),
        Arc::new(ManualClock::new()),
        state.path(),
        "meter",
    )
    .expect("the host admits the meter");
    for (command, guarded) in [
        (COUNTED, "over-held"),
        (ALL_HIGH, "all-high"),
        (NONE, "unheld"),
    ] {
        let answered = outcome(
            &mut meter,
            command,
            &json!({"charge_id": charge(1), "org": "north"}),
        );
        assert!(
            answered.starts_with(&format!(
                "unsupported: the `when_related:` guard of `{command}/{guarded}`, which reads the \
                 rows of `metering.holds.Hold`, owned by component `holds`: the kernel reads only \
                 the store of component `meter`, which runs the command"
            )),
            "{answered}"
        );
    }
    assert_eq!(rows_of(&mut meter, CHARGE), 0);
}

/// The meter accepts a command whose subject is a hold, an entity `holds` owns. In ESS's
/// interpreter the hold it creates is the one the meter's own row-set guard then reads. A host
/// running both either answers the same, or refuses the command by name before it writes, as it
/// refuses an `affects:` entry over that entity: it must not write the hold where no read of a
/// hold looks.
#[test]
fn a_hold_the_meter_creates_is_the_one_its_guard_reads() {
    let path = fixture("subject");
    let steps = [
        (HOLD_ORG, json!({"hold_id": H1, "org": "north"})),
        (DIRECT, json!({"charge_id": charge(1), "org": "north"})),
    ];
    let expected = interpreted(&path, &steps);
    assert_eq!(expected, ["held", "on-hold"], "ESS's interpreter");
    let state = scratch();
    let mut host = Host::new(
        ir(&path),
        &environment("memory"),
        Arc::new(ManualClock::new()),
        state.path(),
    )
    .expect("the host admits the fixture");
    let first = outcome(&mut host, steps[0].0, &steps[0].1);
    if first.starts_with("unsupported: ") {
        assert!(first.contains(HOLD), "{first}");
        assert_eq!(rows_of(&mut host, HOLD), 0, "nothing is written");
        return;
    }
    let second = outcome(&mut host, steps[1].0, &steps[1].1);
    assert_eq!(
        vec![first, second],
        expected,
        "the host answers the interpreter's outcomes"
    );
}

/// What runs once, before the next write a [`Hooked`] store takes.
type Hook = Box<dyn FnOnce() + Send>;

/// A memory store shared with the test, which hands every write to `before` first.
#[derive(Clone)]
struct Hooked {
    inner: Arc<Mutex<MemoryStore>>,
    before: Arc<Mutex<Option<Hook>>>,
}

impl Hooked {
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MemoryStore::default())),
            before: Arc::new(Mutex::new(None)),
        }
    }

    fn fire(&self) {
        let hook = self.before.lock().expect("the hook").take();
        if let Some(hook) = hook {
            hook();
        }
    }
}

impl Store for Hooked {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.inner.lock().expect("the store").load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.fire();
        self.inner.lock().expect("the store").commit(change)
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.fire();
        self.inner.lock().expect("the store").remove(removal)
    }

    fn commit_all(&mut self, writes: &[Write]) -> Result<(), StoreError> {
        self.fire();
        self.inner.lock().expect("the store").commit_all(writes)
    }

    fn scan(&mut self, entity: &str) -> Result<spindle_kernel::Scan, StoreError> {
        self.inner.lock().expect("the store").scan(entity)
    }
}

/// A charge is decided over one hold (`count: {gte: 2}` false). Another writer of the holds'
/// store places a second hold after the host checked the holds' version and before the meter's
/// store commits, the window `Routed`'s two steps leave open. A store that conditions a write on
/// the entity it scanned refuses it then; the charge must not be kept.
#[test]
fn a_hold_placed_between_the_check_and_the_commit_refuses_the_charge() {
    let path = fixture("quantifiers");
    let meter = Hooked::new();
    let holds = Hooked::new();
    let stores = Stores::from([
        (
            "meter".to_owned(),
            Box::new(meter.clone()) as Box<dyn Store>,
        ),
        (
            "holds".to_owned(),
            Box::new(holds.clone()) as Box<dyn Store>,
        ),
    ]);
    let mut host = Host::supplied(
        ir(&path),
        &environment("elsewhere"),
        Arc::new(ManualClock::new()),
        None,
        stores,
    )
    .unwrap_or_else(|error| panic!("the host opens on the supplied stores: {error}"));
    assert_eq!(
        outcome(
            &mut host,
            PLACE,
            &json!({"hold_id": H1, "org": "north", "level": 1})
        ),
        "placed"
    );
    let other = Arc::clone(&holds.inner);
    *meter.before.lock().expect("the hook") = Some(Box::new(move || {
        let mut store = other.lock().expect("the holds' store");
        let first = store
            .load(HOLD, &json!(H1))
            .expect("the hold reads")
            .expect("the first hold is kept");
        store
            .commit(&Change {
                entity: HOLD.to_owned(),
                identity: json!(H2),
                expected: None,
                row: first.row,
                events: Vec::new(),
                scanned: Vec::new(),
            })
            .expect("another writer places a second hold");
    }));
    let answered = outcome(
        &mut host,
        COUNTED,
        &json!({"charge_id": charge(1), "org": "north"}),
    );
    assert_eq!(
        holds
            .inner
            .lock()
            .expect("the holds' store")
            .scan(HOLD)
            .expect("the holds read")
            .rows
            .len(),
        2,
        "the second hold landed inside the window"
    );
    let kept = meter
        .inner
        .lock()
        .expect("the meter's store")
        .scan(CHARGE)
        .expect("the charges read")
        .rows
        .len();
    assert_eq!(
        kept, 0,
        "a charge decided over holds that moved before it committed is kept (answered {answered})"
    );
}
