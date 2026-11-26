//! A `when_related:` row-set guard whose selector or `forall:` test walks the rows of a second
//! entity (`exists: {in: {entity: …}, as: …, that: …}`, ess/27), decided as ESS's interpreter
//! decides it (`interpret::execute::row_set::Reader::members`): the walked entity's stored rows are
//! read from the store as it was before selection, and the commit is conditioned on that read
//! (`story:related-selector-rows-quantifier`).
//!
//! The fixture `tests/fixtures/metering` opens accounts, each of an organization, and places and
//! lifts holds on organizations. `ChargeUsage` is refused with `on-hold` where the account's
//! selector finds an active hold of its organization (a rows quantifier in `where:`); `Bill` is
//! refused with `frozen` where every account the bill names has one (a rows quantifier in
//! `forall:`).

use std::path::Path;

use serde_json::{Value, json};
use spindle_kernel::{
    Change, ExecuteError, Executed, MemoryStore, NoExternals, Removal, Row, Scan, Store,
    StoreError, Stored, Write, execute,
};

use super::related_guards::{interpreted, object, spindled};

const ACCOUNT: &str = "metering.usage.Account";
const HOLD: &str = "metering.usage.Hold";
const CHARGE: &str = "metering.usage.Charge";
const OPEN: &str = "metering.usage.OpenAccount";
const PLACE: &str = "metering.usage.PlaceHold";
const LIFT: &str = "metering.usage.LiftHold";
const CHARGE_USAGE: &str = "metering.usage.ChargeUsage";
const BILL: &str = "metering.usage.Bill";

const NORTH_ACCOUNT: &str = "a0a0a0a0-0000-4000-8000-000000000001";
const SOUTH_ACCOUNT: &str = "a0a0a0a0-0000-4000-8000-000000000002";
const NO_ACCOUNT: &str = "a0a0a0a0-0000-4000-8000-0000000000ff";
const SOUTH_HOLD: &str = "b0b0b0b0-0000-4000-8000-000000000001";
const NORTH_HOLD: &str = "b0b0b0b0-0000-4000-8000-000000000002";

const VIEWS: [(&str, &str, &str); 2] = [
    ("metering.usage.Charges", CHARGE, "charge_id"),
    ("metering.usage.Holds", HOLD, "hold_id"),
];

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/metering"))
        .expect("the metering fixture compiles")
        .ir
}

fn charge(command: &'static str, number: u8, account: &str) -> (&'static str, Value) {
    (
        command,
        json!({
            "charge_id": format!("c0c0c0c0-0000-4000-8000-0000000000{number:02x}"),
            "account_id": account,
        }),
    )
}

/// The steps both acceptance cases share up to the charges: two accounts of two organizations and
/// an active hold of the second.
fn opened() -> Vec<(&'static str, Value)> {
    vec![
        (OPEN, json!({"account_id": NORTH_ACCOUNT, "org": "north"})),
        (OPEN, json!({"account_id": SOUTH_ACCOUNT, "org": "south"})),
        (PLACE, json!({"hold_id": SOUTH_HOLD, "org": "south"})),
    ]
}

/// The charges of `command` read before and after the holds change: no hold of the account's
/// organization (another organization's hold only), an active one, a lifted one, and an account
/// nobody opened.
fn charges(command: &'static str) -> Vec<(&'static str, Value)> {
    let mut steps = opened();
    steps.extend([
        charge(command, 1, NORTH_ACCOUNT),
        charge(command, 2, SOUTH_ACCOUNT),
        (LIFT, json!({"hold_id": SOUTH_HOLD})),
        charge(command, 3, SOUTH_ACCOUNT),
        (PLACE, json!({"hold_id": NORTH_HOLD, "org": "north"})),
        charge(command, 4, NORTH_ACCOUNT),
        charge(command, 5, SOUTH_ACCOUNT),
        charge(command, 6, NO_ACCOUNT),
    ]);
    steps
}

fn agrees(command: &'static str, expected: &[&str]) {
    let ir = ir();
    let steps = charges(command);
    let (ess_answers, ess_rows) = interpreted(&ir, &VIEWS, &steps);
    let outcomes: Vec<&str> = ess_answers
        .iter()
        .map(|(outcome, _, _)| outcome.as_str())
        .collect();
    assert_eq!(
        outcomes, expected,
        "the steps reach a matching row and none, through the walked entity's field and state"
    );
    let (answers, rows) = spindled(&ir, &VIEWS, &steps);
    assert_eq!(
        answers, ess_answers,
        "outcomes, errors and published payloads agree"
    );
    assert_eq!(
        rows, ess_rows,
        "the stored charges and holds agree with ESS's view rows"
    );
}

#[test]
fn a_rows_quantifier_in_a_related_selector_decides_as_the_interpreter() {
    agrees(
        CHARGE_USAGE,
        &[
            "opened", "opened", "placed", "charged", "on-hold", "lifted", "charged", "placed",
            "on-hold", "charged", "charged",
        ],
    );
}

#[test]
fn a_rows_quantifier_in_a_row_set_forall_decides_as_the_interpreter() {
    agrees(
        BILL,
        &[
            "opened", "opened", "placed", "billed", "frozen", "lifted", "billed", "placed",
            "frozen", "billed", "frozen",
        ],
    );
}

/// `admit` and `run` serve a command whose `when_related:` selector or test walks another entity's
/// rows: both ask [`spindle_kernel::selects`], and nothing of the command is refused.
#[test]
fn a_rows_quantifier_in_a_related_selector_is_served() {
    let ir = ir();
    for name in [CHARGE_USAGE, BILL] {
        let command = ir
            .commands()
            .values()
            .find(|command| command.name.to_string() == name)
            .unwrap_or_else(|| panic!("the fixture declares {name}"));
        spindle_kernel::selects(command)
            .unwrap_or_else(|error| panic!("`{name}` is selected among: {error}"));
        spindle_kernel::runs(&ir, command)
            .unwrap_or_else(|error| panic!("`{name}` runs every outcome: {error}"));
    }
}

/// `tests/fixtures/metering-split`: the component `meter` owns accounts and charges, `holds` owns
/// holds.
fn split() -> spindle_spec::EssIr {
    spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/metering-split"))
        .expect("the split metering fixture compiles")
        .ir
}

fn command<'ir>(
    ir: &'ir spindle_spec::EssIr,
    name: &str,
) -> &'ir ess_compiler::ir::ResolvedCommand {
    ir.commands()
        .values()
        .find(|command| command.name.to_string() == name)
        .unwrap_or_else(|| panic!("the fixture declares {name}"))
}

/// A guard over the rows of an entity another component owns, on a store that reaches only the
/// running component's (a split run, `play --component`), would be decided over a store that
/// keeps none of them, where ESS's interpreter reads them: a rows quantifier walking it, a row set
/// selecting it, and a row an input names. Every outcome of the command is refused by name at
/// admission ([`spindle_kernel::takes`], and [`spindle_kernel::takes_reaching`] reaching the
/// running component alone), so `admit --component` and the route name it, and `execute` writes
/// nothing. Reaching the owner's store too, as a host running both does, every outcome runs
/// (`story:cross-component-related-reads`).
#[test]
fn a_guard_over_another_components_rows_is_refused_by_name() {
    let split_run = std::collections::BTreeSet::from(["meter".to_owned()]);
    let one_host = std::collections::BTreeSet::from(["meter".to_owned(), "holds".to_owned()]);
    let ir = split();
    let mut store = MemoryStore::default();
    send(
        &mut store,
        &ir,
        (
            "metering.usage.OpenAccount",
            json!({"account_id": NORTH_ACCOUNT, "org": "north"}),
        ),
    )
    .expect("the account opens");
    for (name, guarded, input) in [
        (
            "metering.usage.ChargeUsage",
            "on-hold",
            json!({"charge_id": "c0c0c0c0-0000-4000-8000-000000000001", "account_id": NORTH_ACCOUNT}),
        ),
        (
            "metering.usage.ChargeDirect",
            "on-hold",
            json!({"charge_id": "c0c0c0c0-0000-4000-8000-000000000002", "account_id": NORTH_ACCOUNT, "org": "north"}),
        ),
        (
            "metering.usage.ChargeHeld",
            "no-hold",
            json!({"charge_id": "c0c0c0c0-0000-4000-8000-000000000003", "account_id": NORTH_ACCOUNT, "hold_id": SOUTH_HOLD}),
        ),
    ] {
        let why = format!(
            "the `when_related:` guard of `{name}/{guarded}`, which reads the rows of \
             `metering.holds.Hold`, owned by component `holds`: the kernel reads only the store \
             of component `meter`, which runs the command"
        );
        let spec = command(&ir, name);
        for outcome in &spec.outcomes {
            let refused = spindle_kernel::takes(&ir, spec, outcome)
                .expect_err("every outcome of the command is refused");
            assert_eq!(
                refused.to_string(),
                format!("unsupported: {why}, in `{name}/{}`", outcome.name.as_str()),
            );
            let split = spindle_kernel::takes_reaching(&ir, spec, outcome, &split_run)
                .expect_err("a split run refuses every outcome");
            assert_eq!(split.to_string(), refused.to_string());
        }
        spindle_kernel::runs_reaching(&ir, spec, &one_host)
            .unwrap_or_else(|error| panic!("`{name}` runs reaching the holds' store: {error}"));
        assert!(
            spindle_kernel::runs_reaching(&ir, spec, &split_run).is_err(),
            "`{name}` is refused reaching the meter's store alone"
        );
        let charge_id = input["charge_id"].clone();
        match send(&mut store, &ir, (name, input)) {
            Err(ExecuteError::Unsupported { construct }) => assert!(
                construct.starts_with(&why),
                "`{name}` is refused by name: {construct}"
            ),
            other => panic!("`{name}` is refused by name, not {other:?}"),
        }
        assert!(
            store
                .load(CHARGE, &charge_id)
                .expect("the store reads")
                .is_none(),
            "`{name}` writes nothing"
        );
    }

    let once = command(&ir, "metering.usage.ChargeOnce");
    spindle_kernel::runs(&ir, once).expect("a walk over the running component's own rows runs");
    let charge = |number: u8| {
        (
            "metering.usage.ChargeOnce",
            json!({
                "charge_id": format!("c0c0c0c0-0000-4000-8000-0000000000{number:02x}"),
                "account_id": NORTH_ACCOUNT,
            }),
        )
    };
    let outcomes: Vec<String> = [charge(4), charge(5)]
        .into_iter()
        .map(|step| {
            send(&mut store, &ir, step)
                .expect("ChargeOnce runs")
                .outcome
        })
        .collect();
    assert_eq!(outcomes, ["charged", "already-charged"]);
}

/// What another writer does to the store, once.
type Race = Box<dyn FnOnce(&mut MemoryStore)>;

/// A store another writer writes to right after the kernel's first scan of `entity`.
struct Raced {
    inner: MemoryStore,
    entity: &'static str,
    race: Option<Race>,
}

impl Store for Raced {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.inner.load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.inner.commit(change)
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.inner.remove(removal)
    }

    fn commit_all(&mut self, writes: &[Write]) -> Result<(), StoreError> {
        self.inner.commit_all(writes)
    }

    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError> {
        let scan = self.inner.scan(entity)?;
        if entity == self.entity
            && let Some(race) = self.race.take()
        {
            race(&mut self.inner);
        }
        Ok(scan)
    }
}

fn send(
    store: &mut dyn Store,
    ir: &spindle_spec::EssIr,
    step: (&str, Value),
) -> Result<Executed, ExecuteError> {
    execute(ir, store, &mut NoExternals, step.0, &object(step.1))
}

/// A row of `entity` another writer creates, of the organization `north`.
fn written(entity: &'static str, identity: &'static str) -> Race {
    Box::new(move |inner: &mut MemoryStore| {
        inner
            .commit(&Change {
                entity: entity.to_owned(),
                identity: json!(identity),
                expected: None,
                row: Row {
                    state: if entity == HOLD { "Active" } else { "Open" }.to_owned(),
                    fields: [("org".to_owned(), json!("north"))].into_iter().collect(),
                },
                events: Vec::new(),
                scanned: Vec::new(),
            })
            .expect("the other writer commits");
    })
}

/// `ChargeUsage` of the north account, raced right after the kernel read `entity`: the outcome
/// the kernel answers and whether the charge landed.
fn raced_charge(ir: &spindle_spec::EssIr, entity: &'static str, race: Race) -> (String, bool) {
    let mut store = Raced {
        inner: MemoryStore::default(),
        entity,
        race: None,
    };
    for step in opened() {
        send(&mut store, ir, step).expect("the setup runs");
    }
    store.race = Some(race);
    let (command, input) = charge(CHARGE_USAGE, 1, NORTH_ACCOUNT);
    let charge_id = input["charge_id"].clone();
    let result = send(&mut store, ir, (command, input));
    assert!(store.race.is_none(), "the kernel read `{entity}`");
    let answered = match &result {
        Ok(executed) => format!("Ok({})", executed.outcome),
        Err(ExecuteError::Store(StoreError::Conflict { .. })) => "Conflict".to_owned(),
        Err(error) => format!("Err({error:?})"),
    };
    let landed = store
        .inner
        .load(CHARGE, &charge_id)
        .expect("the store reads")
        .is_some();
    (answered, landed)
}

/// A hold of the account's organization placed by another writer between the kernel's read of the
/// holds and its commit leaves the decision resting on rows that no longer hold: the charge is
/// refused as a conflict, as it is where another writer adds a row of the selected entity.
#[test]
fn a_write_to_the_walked_entity_conditions_the_commit() {
    let ir = ir();
    let selected = raced_charge(
        &ir,
        ACCOUNT,
        written(ACCOUNT, "a0a0a0a0-0000-4000-8000-000000000003"),
    );
    assert_eq!(
        selected,
        ("Conflict".to_owned(), false),
        "a write to the selected entity refuses the commit"
    );
    let walked = raced_charge(
        &ir,
        HOLD,
        written(HOLD, "b0b0b0b0-0000-4000-8000-000000000003"),
    );
    assert_eq!(
        walked, selected,
        "a write to the walked entity refuses the commit as one to the selected entity does"
    );
}
