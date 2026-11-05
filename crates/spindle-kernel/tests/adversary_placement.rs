//! Adversarial cases against story `spindle-placement-rules-are-declared`: each drives
//! `spindle.placement` through the kernel and asserts what `docs/design/observe.md` § 5a or the
//! specification's own declarations say.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::{Map, Value, json};
use spindle_kernel::{Decider, Executed, External, MemoryStore, Store, execute};
use spindle_spec::EssIr;

const INSTANCE: &str = "spindle.placement.Instance";

fn specification() -> EssIr {
    spindle_spec::load(&Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).join("ess"))
        .expect("ess/ compiles")
        .ir
}

#[derive(Default)]
struct Holds(BTreeSet<String>);

impl Holds {
    fn only(command: &str, outcome: &str) -> Self {
        Self(BTreeSet::from([format!(
            "external.spindle.placement.{command}.{outcome}"
        )]))
    }
}

impl Decider for Holds {
    fn decide(&mut self, question: &External<'_>) -> Result<bool, String> {
        Ok(self.0.contains(&question.capability))
    }
}

fn run(
    ir: &EssIr,
    store: &mut dyn Store,
    decider: &mut dyn Decider,
    command: &str,
    request: Value,
) -> Executed {
    let Value::Object(map) = request else {
        panic!("an input is an object");
    };
    let map: Map<String, Value> = map;
    execute(
        ir,
        store,
        decider,
        &format!("spindle.placement.{command}"),
        &map,
    )
    .unwrap_or_else(|error| panic!("{command} executes: {error}"))
}

fn declare(ir: &EssIr, store: &mut dyn Store, ceiling: Value) {
    let declared = run(
        ir,
        store,
        &mut Holds::default(),
        "DeclareTarget",
        json!({
            "target_id": "t1",
            "kind": "Process",
            "environment_id": "laptop",
            "ceiling": ceiling,
        }),
    );
    assert_eq!(declared.outcome, "declared", "{declared:?}");
}

fn plan(ir: &EssIr, store: &mut dyn Store, instance: &str, request: Value) {
    let planned = run(
        ir,
        store,
        &mut Holds::default(),
        "PlanInstance",
        json!({
            "instance_id": instance,
            "deployment_id": "d1",
            "component": "rally-table",
            "settings": [],
            "target_id": "t1",
            "request": request,
        }),
    );
    assert_eq!(planned.outcome, "planned", "{planned:?}");
}

/// observe.md § 5a: the model records the request as the instance asked for it, a zero left as
/// zero; `InstanceState.request` publishes it as requested, and the launcher applies the limits
/// `spindle_target::place` raised to the ceiling. So a placed instance keeps its asked request.
#[test]
fn a_placed_instance_records_the_request_as_asked() {
    let ir = specification();
    let mut store = MemoryStore::default();
    declare(
        &ir,
        &mut store,
        json!({"vcpus": 2, "memory_mib": 256, "disk_mib": 512}),
    );
    plan(
        &ir,
        &mut store,
        "i1",
        json!({"vcpus": 1, "memory_mib": 0, "disk_mib": 0}),
    );
    let placed = run(
        &ir,
        &mut store,
        &mut Holds::default(),
        "PlaceInstance",
        json!({"instance_id": "i1"}),
    );
    assert_eq!(placed.outcome, "placed", "{placed:?}");
    let stored = store.load(INSTANCE, &json!("i1")).unwrap().unwrap();
    assert_eq!(
        stored.row.fields.get("request"),
        Some(&json!({"vcpus": 1, "memory_mib": 0, "disk_mib": 0})),
        "the model records the request as asked; the launcher applies the raised limits"
    );
}

/// `DeploymentNotAdmitted` declares a required `deployment_id`
/// (`ess/generated/jsonschema/schema/errors/spindle.placement.DeploymentNotAdmitted.schema.json`);
/// `PlanInstance` fills it from its input, `LaunchInstance` from the held instance.
#[test]
fn a_launch_refused_for_admission_names_its_deployment() {
    let ir = specification();
    let mut store = MemoryStore::default();
    declare(
        &ir,
        &mut store,
        json!({"vcpus": 1, "memory_mib": 128, "disk_mib": 0}),
    );
    plan(
        &ir,
        &mut store,
        "i1",
        json!({"vcpus": 1, "memory_mib": 64, "disk_mib": 0}),
    );
    let placed = run(
        &ir,
        &mut store,
        &mut Holds::default(),
        "PlaceInstance",
        json!({"instance_id": "i1"}),
    );
    assert_eq!(placed.outcome, "placed", "{placed:?}");
    let launched = run(
        &ir,
        &mut store,
        &mut Holds::only("LaunchInstance", "not-admitted"),
        "LaunchInstance",
        json!({"instance_id": "i1"}),
    );
    assert_eq!(launched.outcome, "not-admitted", "{launched:?}");
    let error = launched.error.as_ref().expect("the launch is refused");
    assert_eq!(error.name, "spindle.placement.DeploymentNotAdmitted");
    assert_eq!(
        Value::Object(error.fields.clone()),
        json!({"deployment_id": "d1"}),
        "the error carries the field its schema requires"
    );
}

/// `PlaceInstance` answers a decided instance with `InstanceStateConflict` whatever the
/// externals say (its refusals move from `Planned`). `LaunchInstance`'s `not-admitted` moves
/// nothing, so a refused instance of a stopped deployment is answered `DeploymentNotAdmitted`
/// instead of the state conflict ESS synthesises for it
/// (`spindle.placement.Instance/state/Refused/refuses/spindle.placement.LaunchInstance`).
#[test]
fn a_launch_of_a_refused_instance_answers_its_state_before_admission() {
    let ir = specification();
    let mut store = MemoryStore::default();
    declare(
        &ir,
        &mut store,
        json!({"vcpus": 1, "memory_mib": 128, "disk_mib": 0}),
    );
    plan(
        &ir,
        &mut store,
        "i1",
        json!({"vcpus": 1, "memory_mib": 64, "disk_mib": 0}),
    );
    let refused = run(
        &ir,
        &mut store,
        &mut Holds::only("PlaceInstance", "target-retired"),
        "PlaceInstance",
        json!({"instance_id": "i1"}),
    );
    assert_eq!(refused.outcome, "target-retired", "{refused:?}");

    let again = run(
        &ir,
        &mut store,
        &mut Holds::only("PlaceInstance", "not-admitted"),
        "PlaceInstance",
        json!({"instance_id": "i1"}),
    );
    assert_eq!(
        again.error.as_ref().map(|error| error.name.as_str()),
        Some("spindle.placement.InstanceStateConflict"),
        "{again:?}"
    );

    let launched = run(
        &ir,
        &mut store,
        &mut Holds::only("LaunchInstance", "not-admitted"),
        "LaunchInstance",
        json!({"instance_id": "i1"}),
    );
    assert_eq!(
        (
            launched.outcome.as_str(),
            launched.error.as_ref().map(|error| error.name.as_str())
        ),
        (
            "wrong-state",
            Some("spindle.placement.InstanceStateConflict")
        ),
        "{launched:?}"
    );
}
