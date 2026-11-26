//! Adversary, wave 26 unit U1 (`story:related-selector-rows-quantifier`): a `when_related:`
//! row-set selector that walks the rows of an entity another component owns. ESS's interpreter
//! holds every entity in one store and reads the holds; the host runs the command on the store of
//! the component that accepts it, which keeps no hold. The command must answer as the interpreter
//! does, or be refused by name as a value read from another component's entity is
//! (`cross_component_reads`).
//!
//! `tests/fixtures/adversary-w26-u1` declares `meter`, which owns accounts and charges, and
//! `holds`, which owns holds. `ChargeUsage` walks the holds inside its account selector (the
//! construct this unit serves); `ChargeDirect` selects the holds themselves (a row-set guard the
//! base already served), for comparison.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{ConformanceTarget, SemanticCommandRequest};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, HostError, ManualClock};
use spindle_kernel::ExecuteError;

const OPEN: &str = "ledger.usage.OpenAccount";
const PLACE: &str = "ledger.holds.PlaceHold";
const ACCOUNT: &str = "a2000000-0000-4000-8000-000000000001";
const HOLD: &str = "b2000000-0000-4000-8000-000000000001";
const CHARGE: &str = "c2000000-0000-4000-8000-000000000001";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary-w26-u1")
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

fn in_memory() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"walked\"\n",
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

/// An account of `north`, a hold of `north` placed by the `holds` component, then `charge`.
fn steps(charge: (&'static str, Value)) -> Vec<(&'static str, Value)> {
    vec![
        (OPEN, json!({"account_id": ACCOUNT, "org": "north"})),
        (PLACE, json!({"hold_id": HOLD, "org": "north"})),
        charge,
    ]
}

/// The outcome ESS's interpreter takes for the last of `steps`.
fn interpreted(steps: &[(&str, Value)]) -> String {
    let ir = spindle_spec::load(&fixture()).expect("compiles").ir;
    let interpreter = Interpreted::for_model(ir);
    let correlation = CorrelationId::new("walked").expect("a correlation");
    let mut last = String::new();
    for (command, input) in steps {
        let fields: BTreeMap<String, Node> = serde_json::from_value(input.clone()).expect("nodes");
        last = interpreter
            .execute_command(SemanticCommandRequest {
                command: command.parse().expect("a command name"),
                actor: None,
                caller: None,
                input: fields,
                correlation: correlation.clone(),
            })
            .map_or_else(
                |error| format!("error: {error}"),
                |answer| {
                    answer
                        .outcome
                        .map(|outcome| outcome.outcome.to_string())
                        .unwrap_or_default()
                },
            );
    }
    last
}

/// What the host answers the last of `steps`: the outcome it took, the refusing outcome, or
/// `unsupported: …` where the command is refused by name.
fn hosted(steps: &[(&str, Value)]) -> String {
    let ir = spindle_spec::load(&fixture()).expect("compiles").ir;
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .expect("the host admits the fixture");
    let (last, setup) = steps.split_last().expect("a step");
    for (command, input) in setup {
        host.submit(command, object(input.clone()))
            .unwrap_or_else(|error| panic!("{command} runs: {error}"));
    }
    match host.submit(last.0, object(last.1.clone())) {
        Ok(report) => report
            .ran
            .first()
            .map(|ran| ran.executed.outcome.clone())
            .unwrap_or_default(),
        Err(HostError::CommandRefused { outcome, .. }) => outcome,
        Err(HostError::Execute { error, .. }) => match *error {
            ExecuteError::Unsupported { construct } => format!("unsupported: {construct}"),
            other => format!("error: {other}"),
        },
        Err(other) => format!("host error: {other}"),
    }
}

fn answers_as_the_interpreter_or_is_refused(steps: &[(&str, Value)]) {
    let ess = interpreted(steps);
    assert_eq!(
        ess, "on-hold",
        "ESS's interpreter reads the hold another component owns"
    );
    let ours = hosted(steps);
    assert!(
        ours == ess || ours.starts_with("unsupported: "),
        "the host answers `{ours}` where ESS's interpreter answers `{ess}`: it neither reads the \
         other component's holds nor refuses the command"
    );
}

/// The construct this unit serves: a rows quantifier over another component's entity. At the
/// base it was refused at start ("a rows quantifier in a `when_related:` row-set selector").
#[test]
fn a_walk_over_another_components_entity_answers_as_the_interpreter_or_is_refused() {
    answers_as_the_interpreter_or_is_refused(&steps((
        "ledger.usage.ChargeUsage",
        json!({"charge_id": CHARGE, "account_id": ACCOUNT}),
    )));
}

/// The same gap for a row set over another component's entity, served at the base already.
#[test]
fn a_row_set_over_another_components_entity_answers_as_the_interpreter_or_is_refused() {
    answers_as_the_interpreter_or_is_refused(&steps((
        "ledger.usage.ChargeDirect",
        json!({"charge_id": CHARGE, "org": "north"}),
    )));
}
