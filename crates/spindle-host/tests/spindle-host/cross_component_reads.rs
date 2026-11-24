//! A value read from a row of an entity another component owns: `{related: {entity, where,
//! field}}` (ess/22) and `{related: {via: input.<field>, field}}` (ess/16). ESS's interpreter
//! holds every entity in one store and answers such a command; the kernel reads only the store of
//! the component that runs it, so the read would find no row and answer `no_value` for every
//! request. Such an outcome is refused by name instead, before anything is read or written, and
//! the kernel's admission check (`spindle_kernel::takes`) names it, so `admit` and the ingress
//! name it too.
//!
//! `tests/fixtures/cross-component-reads` declares `catalogue`, which owns resources, and
//! `grants`, which issues grants: `IssueGrant` reads the identity of the resource whose name the
//! input gives, `IssueNamedGrant` the name of the resource the input names.

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

const ADD: &str = "access.resources.AddResource";
const ISSUE: &str = "access.grants.IssueGrant";
const ISSUE_NAMED: &str = "access.grants.IssueNamedGrant";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cross-component-reads")
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

/// Whether ESS's interpreter takes `issued` for `command` after a resource `r1` named `printer`
/// was added.
fn interpreter_issues(command: &str, input: &Value) -> bool {
    let ir = spindle_spec::load(&fixture()).expect("compiles").ir;
    let interpreter = Interpreted::for_model(ir);
    let correlation = CorrelationId::new("cross").expect("a correlation");
    let mut last = None;
    for (command, input) in [
        (ADD, json!({"resource_id": "r1", "name": "printer"})),
        (command, input.clone()),
    ] {
        let fields: BTreeMap<String, Node> = serde_json::from_value(input).expect("nodes");
        last = interpreter
            .execute_command(SemanticCommandRequest {
                command: command.parse().expect("a command name"),
                actor: None,
                caller: None,
                input: fields,
                correlation: correlation.clone(),
            })
            .ok()
            .and_then(|answer| answer.outcome)
            .map(|outcome| outcome.outcome.to_string());
    }
    last.as_deref() == Some("issued")
}

/// The refusal the host answers `command` with on both components, once `r1` is added.
fn refusal(command: &str, input: Value) -> String {
    let ir = spindle_spec::load(&fixture()).expect("compiles").ir;
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut host = Host::new(ir, &in_memory(), Arc::new(ManualClock::new()), state.path())
        .expect("the host admits the fixture");
    host.submit(ADD, object(json!({"resource_id": "r1", "name": "printer"})))
        .expect("the catalogue adds r1");
    match host.submit(command, object(input)) {
        Err(HostError::Execute { error, .. }) => match *error {
            ExecuteError::Unsupported { construct } => construct,
            other => panic!("{command}: refused as unsupported, not {other:?}"),
        },
        other => panic!("{command}: refused as unsupported, not {other:?}"),
    }
}

/// What the kernel's admission check says of `command`'s `issued` outcome.
fn admission(command: &str) -> Result<(), ExecuteError> {
    let ir = spindle_spec::load(&fixture()).expect("compiles").ir;
    let (_, spec) = ir
        .commands()
        .iter()
        .find(|(name, _)| name.to_string() == command)
        .expect("the command is declared");
    let outcome = spec.outcomes.first().expect("an outcome");
    spindle_kernel::takes(&ir, spec, outcome)
}

#[test]
fn a_value_selected_from_another_components_entity_is_refused_by_name() {
    let input = json!({"grant_id": "g1", "holder": "ada", "name": "printer"});
    assert!(
        interpreter_issues(ISSUE, &input),
        "ESS's interpreter reads every component's rows and issues the grant"
    );
    let construct = refusal(ISSUE, input);
    for named in [
        "access.resources.Resource",
        "catalogue",
        "grants",
        "access.grants.IssueGrant/issued",
    ] {
        assert!(construct.contains(named), "{named} in {construct}");
    }
    assert!(admission(ISSUE).is_err(), "admission names the outcome");
}

#[test]
fn a_value_read_through_an_input_from_another_components_entity_is_refused_by_name() {
    let input = json!({"grant_id": "g1", "holder": "ada", "resource_id": "r1"});
    assert!(
        interpreter_issues(ISSUE_NAMED, &input),
        "ESS's interpreter reads every component's rows and issues the grant"
    );
    let construct = refusal(ISSUE_NAMED, input);
    for named in [
        "access.resources.Resource",
        "catalogue",
        "grants",
        "access.grants.IssueNamedGrant/issued",
    ] {
        assert!(construct.contains(named), "{named} in {construct}");
    }
    assert!(
        admission(ISSUE_NAMED).is_err(),
        "admission names the outcome"
    );
}
