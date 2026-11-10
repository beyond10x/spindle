//! Adversary pass 2 against `LaunchInstance`'s `not-admitted`: true unless the deployment is
//! `Admitted` or `Running`. The authored scenario `launch-not-admitted.yaml` and every kernel case
//! cover `Offered`, `Admitted`, `Running` and `Stopped`; none launches an instance whose
//! deployment is `Refused`, so the predicate `{not: {any: [state == Admitted, state == Running,
//! state == Refused]}}` passes them all.
//!
//! An instance planned and placed while its deployment was offered (both `not-admitted` causes
//! answered `false`, as the authored scenario's `of-offered` instance is), whose deployment is
//! then refused, launches nothing. `SPINDLE_ADVERSARY_ESS` names another `ess/` directory to load
//! instead of the workspace's, so the case can be run against a mutated copy.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use spindle_kernel::{Executed, External, MemoryStore, Store, execute};
use spindle_spec::EssIr;

fn specification() -> EssIr {
    let root = std::env::var_os("SPINDLE_ADVERSARY_ESS").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ess"),
        PathBuf::from,
    );
    spindle_spec::load(&root).expect("ess/ compiles").ir
}

/// Sends `command`, answering `holds` for the one external outcome named, `false` for any other.
fn send(
    ir: &EssIr,
    store: &mut dyn Store,
    command: &str,
    request: Value,
    holds: Option<&str>,
) -> Executed {
    let Value::Object(map) = request else {
        panic!("an input is an object");
    };
    let mut decider = |question: &External<'_>| Ok(Some(question.outcome) == holds);
    execute(ir, store, &mut decider, command, &map)
        .unwrap_or_else(|error| panic!("{command} executes: {error}"))
}

fn expect(ir: &EssIr, store: &mut dyn Store, command: &str, request: Value, outcome: &str) {
    let holds = (outcome == "refused").then_some("refused");
    let executed = send(ir, store, command, request, holds);
    assert_eq!(executed.outcome, outcome, "{command}: {executed:?}");
}

#[test]
fn adversary_a_placed_instance_of_a_refused_deployment_launches_nothing() {
    let ir = specification();
    let mut store = MemoryStore::default();
    expect(
        &ir,
        &mut store,
        "spindle.deployment.DeclareEnvironment",
        json!({"environment_id": "laptop", "os": "linux", "arch": "x86_64", "provides": []}),
        "declared",
    );
    expect(
        &ir,
        &mut store,
        "spindle.placement.DeclareTarget",
        json!({
            "target_id": "t1",
            "kind": "Process",
            "environment_id": "laptop",
            "ceiling": {"vcpus": 1, "memory_mib": 128, "disk_mib": 0},
        }),
        "declared",
    );
    expect(
        &ir,
        &mut store,
        "spindle.deployment.OfferBundle",
        json!({"deployment_id": "d1", "bundle": "sha256:0f", "environment_id": "laptop"}),
        "offered",
    );
    expect(
        &ir,
        &mut store,
        "spindle.placement.PlanInstance",
        json!({
            "instance_id": "i1",
            "deployment_id": "d1",
            "component": "spindle-control",
            "settings": [],
            "target_id": "t1",
            "request": {"vcpus": 1, "memory_mib": 64, "disk_mib": 0},
        }),
        "planned",
    );
    expect(
        &ir,
        &mut store,
        "spindle.placement.PlaceInstance",
        json!({"instance_id": "i1"}),
        "placed",
    );
    expect(
        &ir,
        &mut store,
        "spindle.deployment.DecideAdmission",
        json!({
            "deployment_id": "d1",
            "requires": ["store.durable"],
            "bindings": [],
            "missing": ["store.durable"],
        }),
        "refused",
    );
    let launched = send(
        &ir,
        &mut store,
        "spindle.placement.LaunchInstance",
        json!({"instance_id": "i1"}),
        None,
    );
    assert_eq!(
        (
            launched.outcome.as_str(),
            launched.error.as_ref().map(|error| error.name.as_str())
        ),
        (
            "not-admitted",
            Some("spindle.placement.DeploymentNotAdmitted")
        ),
        "a refused deployment is neither admitted nor running: {launched:?}"
    );
}
