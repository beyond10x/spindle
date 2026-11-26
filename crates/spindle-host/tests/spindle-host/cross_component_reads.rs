//! A read of an entity another component owns: a value (`{related: {entity, where, field}}`,
//! ess/22, and `{related: {via: input.<field>, field}}`, ess/16), a `when_related:` guard over a
//! row the input names, and a row-set guard, with or without a rows quantifier
//! (`story:cross-component-related-reads`). ESS's interpreter holds every entity in one store and
//! reads them. A host running both components reads the row from its owner's store and answers
//! what the interpreter answers; a host running only the component that runs the command, as
//! `play --component` runs it, refuses the read by name, naming the entity, its owner and the
//! running component, before anything is read. An `affects:` entry over another component's
//! entity is refused by name in either host, and writes nothing.
//!
//! `tests/fixtures/cross-component-reads` declares `catalogue`, which owns resources, and
//! `grants`, which issues grants: `IssueGrant` reads the identity of the resource whose name the
//! input gives, `IssueNamedGrant` the name of the resource the input names, and
//! `LabelGrantedResource` labels a resource of the catalogue (`affects:`).
//! `spindle-kernel/tests/fixtures/metering-split` declares `meter`, which owns accounts and
//! charges, and `holds`, which owns holds: `ChargeHeld` guards a row the input names,
//! `ChargeDirect` a row set of holds, `ChargeUsage` a row set of accounts whose selector walks the
//! holds, and `ChargeOnce` a row set of the meter's own rows.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ess_compiler::refs::ViewRef;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{ConformanceTarget, SemanticCommandRequest, SemanticViewRequest};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, HostError, ManualClock};
use spindle_kernel::ExecuteError;

const ADD: &str = "access.resources.AddResource";
const ISSUE: &str = "access.grants.IssueGrant";
const ISSUE_NAMED: &str = "access.grants.IssueNamedGrant";
const LABEL: &str = "access.grants.LabelGrantedResource";
const GRANTS: &str = "access.grants.Grants";

const OPEN: &str = "metering.usage.OpenAccount";
const PLACE: &str = "metering.holds.PlaceHold";
const CHARGE_HELD: &str = "metering.usage.ChargeHeld";
const CHARGE_DIRECT: &str = "metering.usage.ChargeDirect";
const CHARGE_USAGE: &str = "metering.usage.ChargeUsage";
const CHARGE_ONCE: &str = "metering.usage.ChargeOnce";
const CHARGE: &str = "metering.usage.Charge";

const NORTH: &str = "a0a0a0a0-0000-4000-8000-000000000001";
const SOUTH: &str = "a0a0a0a0-0000-4000-8000-000000000002";
const NORTH_HOLD: &str = "b0b0b0b0-0000-4000-8000-000000000001";
const SOUTH_HOLD: &str = "b0b0b0b0-0000-4000-8000-000000000002";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cross-component-reads")
}

fn metering() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../spindle-kernel/tests/fixtures/metering-split")
}

fn ir(path: &Path) -> spindle_spec::EssIr {
    spindle_spec::load(path).expect("the fixture compiles").ir
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

/// `row` without its absent fields, as a JSON object: ESS answers one as `null`, the host leaves
/// it out.
fn present(row: Map<String, Value>) -> Value {
    Value::Object(
        row.into_iter()
            .filter(|(_, value)| !value.is_null())
            .collect(),
    )
}

fn charge(number: u8) -> String {
    format!("c0c0c0c0-0000-4000-8000-0000000000{number:02x}")
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"cross\"\n",
        "os = \"linux\"\n",
        "arch = \"x86_64\"\n",
        "\n",
        "[provides]\n",
        "\"store.durable\" = [\"memory\"]\n",
        "\"store.ephemeral\" = [\"memory\"]\n",
        "clock = [\"system\"]\n",
        "channel = [\"in-process\"]\n",
        "projection = [\"in-process\"]\n",
    ))
    .expect("the environment parses")
}

/// A host running every component of the fixture at `path`, as `run` and `play` run them.
fn one_host(path: &Path, state: &Path) -> Host {
    Host::new(ir(path), &in_memory(), Arc::new(ManualClock::new()), state)
        .expect("the host admits the fixture")
}

/// A host running `component` alone, as `play --component` runs it.
fn split_host(path: &Path, state: &Path, component: &str) -> Host {
    Host::component(
        ir(path),
        &in_memory(),
        Arc::new(ManualClock::new()),
        state,
        component,
    )
    .expect("the host admits the component")
}

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory")
}

/// The outcome the host answers `command` with: taken, or refused by the model.
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
        Err(other) => panic!("{command} {input}: {other}"),
    }
}

/// The construct the host names refusing `command` as unsupported.
fn unsupported(host: &mut Host, command: &str, input: &Value) -> String {
    match host.submit(command, object(input.clone())) {
        Err(HostError::Execute { error, .. }) => match *error {
            ExecuteError::Unsupported { construct } => construct,
            other => panic!("{command}: refused as unsupported, not {other:?}"),
        },
        other => panic!("{command}: refused as unsupported, not {other:?}"),
    }
}

/// The outcome ESS's interpreter answers for each step, and the rows of `views` after them.
fn interpreted(
    path: &Path,
    steps: &[(&str, Value)],
    views: &[&str],
) -> (Vec<String>, Vec<Vec<Value>>) {
    let interpreter = Interpreted::for_model(ir(path));
    let correlation = CorrelationId::new("cross").expect("a correlation");
    let outcomes = steps
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
        .collect();
    let rows = views
        .iter()
        .map(|view| {
            interpreter
                .query_view(SemanticViewRequest {
                    view: ViewRef::new(view.parse().expect("a view name")),
                    params: BTreeMap::new(),
                    consistency: ess_primitives::consistency::QueryConsistency::Current,
                    correlation: correlation.clone(),
                    deadline: ess_conformance::target::Deadline::at(Timestamp::from_epoch_millis(
                        0,
                    )),
                })
                .expect("the interpreter reads the view")
                .rows
                .into_iter()
                .map(|row| {
                    present(
                        row.iter()
                            .map(|(field, node)| {
                                (
                                    field.clone(),
                                    serde_json::to_value(node).expect("a node is JSON"),
                                )
                            })
                            .collect(),
                    )
                })
                .collect()
        })
        .collect();
    (outcomes, rows)
}

/// What a host running every component answers for each step, and the rows of `views` after them.
fn hosted(path: &Path, steps: &[(&str, Value)], views: &[&str]) -> (Vec<String>, Vec<Vec<Value>>) {
    let state = scratch();
    let mut host = one_host(path, state.path());
    let outcomes = steps
        .iter()
        .map(|(command, input)| outcome(&mut host, command, input))
        .collect();
    let rows = views
        .iter()
        .map(|view| {
            host.list(view)
                .expect("the host reads the view")
                .into_iter()
                .map(present)
                .collect()
        })
        .collect();
    (outcomes, rows)
}

/// The steps both agree on, and the outcomes they answer.
fn agrees(path: &Path, steps: &[(&str, Value)], views: &[&str], expected: &[&str]) {
    let (ess_outcomes, ess_rows) = interpreted(path, steps, views);
    assert_eq!(
        ess_outcomes, expected,
        "ESS's interpreter answers the steps"
    );
    let (outcomes, rows) = hosted(path, steps, views);
    assert_eq!(
        outcomes, ess_outcomes,
        "the host answers the interpreter's outcomes"
    );
    assert_eq!(rows, ess_rows, "the host stores the interpreter's rows");
}

/// How many rows the host holds of `entity`.
fn rows_of(host: &mut Host, entity: &str) -> usize {
    host.rows()
        .expect("the host counts its rows")
        .into_iter()
        .find_map(|(name, rows)| (name == entity).then_some(rows))
        .unwrap_or_default()
}

#[test]
fn a_value_over_another_components_entity_is_read_from_its_owner() {
    let steps = [
        (ADD, json!({"resource_id": "r1", "name": "printer"})),
        (ADD, json!({"resource_id": "r2", "name": "scanner"})),
        (
            ISSUE,
            json!({"grant_id": "g1", "holder": "ada", "name": "scanner"}),
        ),
        (
            ISSUE_NAMED,
            json!({"grant_id": "g2", "holder": "bo", "resource_id": "r1"}),
        ),
    ];
    agrees(
        &fixture(),
        &steps,
        &[GRANTS],
        &["added", "added", "issued", "issued"],
    );
    let (_, rows) = hosted(&fixture(), &steps, &[GRANTS]);
    assert_eq!(
        rows[0],
        [
            json!({"grant_id": "g1", "holder": "ada", "resource_id": "r2"}),
            json!({"grant_id": "g2", "holder": "bo", "resource_id": "r1", "resource_name": "printer"}),
        ],
        "each grant holds the value read from the catalogue's row"
    );
    let ir = ir(&fixture());
    let reached = every_component(&ir);
    for name in [ISSUE, ISSUE_NAMED] {
        let spec = command(&ir, name);
        for outcome in &spec.outcomes {
            spindle_kernel::takes_reaching(&ir, spec, outcome, &reached)
                .unwrap_or_else(|error| panic!("`{name}` runs reaching every store: {error}"));
        }
    }
}

#[test]
fn a_guard_over_another_components_row_is_read_from_its_owner() {
    agrees(
        &metering(),
        &[
            (OPEN, json!({"account_id": NORTH, "org": "north"})),
            (
                CHARGE_HELD,
                json!({"charge_id": charge(1), "account_id": NORTH, "hold_id": NORTH_HOLD}),
            ),
            (PLACE, json!({"hold_id": NORTH_HOLD, "org": "north"})),
            (
                CHARGE_HELD,
                json!({"charge_id": charge(2), "account_id": NORTH, "hold_id": NORTH_HOLD}),
            ),
            (
                CHARGE_HELD,
                json!({"charge_id": charge(3), "account_id": NORTH, "hold_id": SOUTH_HOLD}),
            ),
        ],
        &[],
        &["opened", "no-hold", "placed", "charged", "no-hold"],
    );
    let ir = ir(&metering());
    spindle_kernel::runs_reaching(&ir, command(&ir, CHARGE_HELD), &every_component(&ir))
        .expect("every outcome runs reaching every store");
}

#[test]
fn a_row_set_guard_over_another_components_entity_is_read_from_its_owner() {
    let steps = [
        (OPEN, json!({"account_id": NORTH, "org": "north"})),
        (OPEN, json!({"account_id": SOUTH, "org": "south"})),
        (PLACE, json!({"hold_id": SOUTH_HOLD, "org": "south"})),
        (
            CHARGE_USAGE,
            json!({"charge_id": charge(1), "account_id": NORTH}),
        ),
        (
            CHARGE_USAGE,
            json!({"charge_id": charge(2), "account_id": SOUTH}),
        ),
        (
            CHARGE_DIRECT,
            json!({"charge_id": charge(3), "account_id": NORTH, "org": "north"}),
        ),
        (
            CHARGE_DIRECT,
            json!({"charge_id": charge(4), "account_id": SOUTH, "org": "south"}),
        ),
    ];
    agrees(
        &metering(),
        &steps,
        &[],
        &[
            "opened", "opened", "placed", "charged", "on-hold", "charged", "on-hold",
        ],
    );
    let state = scratch();
    let mut host = one_host(&metering(), state.path());
    for (command, input) in &steps {
        outcome(&mut host, command, input);
    }
    assert_eq!(
        rows_of(&mut host, CHARGE),
        2,
        "each charge decided over the holds' rows is committed"
    );
}

#[test]
fn a_split_run_refuses_a_read_of_another_components_entity_by_name() {
    let state = scratch();
    let mut grants = split_host(&fixture(), state.path(), "grants");
    for (command, input) in [
        (
            ISSUE,
            json!({"grant_id": "g1", "holder": "ada", "name": "printer"}),
        ),
        (
            ISSUE_NAMED,
            json!({"grant_id": "g2", "holder": "ada", "resource_id": "r1"}),
        ),
    ] {
        let construct = unsupported(&mut grants, command, &input);
        for named in [
            "access.resources.Resource",
            "component `catalogue`",
            "component `grants`, which runs the command",
            &format!("{command}/issued"),
        ] {
            assert!(construct.contains(named), "{named} in {construct}");
        }
    }
    assert_eq!(rows_of(&mut grants, "access.grants.Grant"), 0);

    let state = scratch();
    let mut meter = split_host(&metering(), state.path(), "meter");
    outcome(
        &mut meter,
        OPEN,
        &json!({"account_id": NORTH, "org": "north"}),
    );
    for (command, guarded, input) in [
        (
            CHARGE_HELD,
            "no-hold",
            json!({"charge_id": charge(1), "account_id": NORTH, "hold_id": NORTH_HOLD}),
        ),
        (
            CHARGE_DIRECT,
            "on-hold",
            json!({"charge_id": charge(2), "account_id": NORTH, "org": "north"}),
        ),
        (
            CHARGE_USAGE,
            "on-hold",
            json!({"charge_id": charge(3), "account_id": NORTH}),
        ),
    ] {
        let construct = unsupported(&mut meter, command, &input);
        assert!(
            construct.starts_with(&format!(
                "the `when_related:` guard of `{command}/{guarded}`, which reads the rows of \
                 `metering.holds.Hold`, owned by component `holds`: the kernel reads only the \
                 store of component `meter`, which runs the command"
            )),
            "{construct}"
        );
    }
    assert_eq!(rows_of(&mut meter, CHARGE), 0, "nothing is charged");
}

#[test]
fn an_affects_entry_over_another_components_entity_is_refused_by_name() {
    let refused = |host: &mut Host| {
        let construct = unsupported(
            host,
            LABEL,
            &json!({"grant_id": "g1", "holder": "ada", "name": "printer", "label": "shared"}),
        );
        assert_eq!(
            construct,
            "an `affects:` entry over `access.resources.Resource`, owned by component \
             `catalogue`: the kernel writes only the store of component `grants`, which runs \
             the command, in `access.grants.LabelGrantedResource/labelled`"
        );
    };
    let state = scratch();
    let mut host = one_host(&fixture(), state.path());
    outcome(
        &mut host,
        ADD,
        &json!({"resource_id": "r1", "name": "printer"}),
    );
    outcome(
        &mut host,
        ISSUE,
        &json!({"grant_id": "g1", "holder": "ada", "name": "printer"}),
    );
    let before = host.rows().expect("the host counts its rows");
    let events = host.published().expect("the events are read").len();
    refused(&mut host);
    assert_eq!(
        host.rows().expect("the host counts its rows"),
        before,
        "no row is written"
    );
    assert_eq!(
        host.published().expect("the events are read").len(),
        events,
        "no event is published"
    );

    let state = scratch();
    let mut grants = split_host(&fixture(), state.path(), "grants");
    refused(&mut grants);
    assert_eq!(rows_of(&mut grants, "access.grants.Grant"), 0);

    let ir = ir(&fixture());
    let spec = command(&ir, LABEL);
    for outcome in &spec.outcomes {
        assert!(
            spindle_kernel::takes_reaching(&ir, spec, outcome, &every_component(&ir)).is_err(),
            "refused reaching every store"
        );
    }
}

#[test]
fn a_guard_over_the_running_components_entity_is_served() {
    let once = |number: u8| {
        (
            CHARGE_ONCE,
            json!({"charge_id": charge(number), "account_id": NORTH}),
        )
    };
    agrees(
        &metering(),
        &[
            (OPEN, json!({"account_id": NORTH, "org": "north"})),
            once(1),
            once(2),
        ],
        &[],
        &["opened", "charged", "already-charged"],
    );
    let state = scratch();
    let mut meter = split_host(&metering(), state.path(), "meter");
    let outcomes: Vec<String> = [
        (OPEN, json!({"account_id": NORTH, "org": "north"})),
        once(1),
        once(2),
    ]
    .iter()
    .map(|(command, input)| outcome(&mut meter, command, input))
    .collect();
    assert_eq!(
        outcomes,
        ["opened", "charged", "already-charged"],
        "a host running the meter alone answers its own rows' guard"
    );
}

fn every_component(ir: &spindle_spec::EssIr) -> std::collections::BTreeSet<String> {
    ir.components()
        .values()
        .map(|component| component.name.as_str().to_owned())
        .collect()
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
