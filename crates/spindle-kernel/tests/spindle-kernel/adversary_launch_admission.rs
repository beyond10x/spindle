//! Adversary cases against story `placement-refusals-match-target`, run through the kernel:
//! `LaunchInstance`'s `not-admitted` is a guard over the deployment its instance names, true
//! unless the deployment is `Admitted` or `Running` (`docs/design/observe.md` § 5a), and only a
//! `Placed` instance's deployment is read. `PlanInstance` refuses an unknown deployment first.
//!
//! `SPINDLE_ADVERSARY_ESS` names another `ess/` directory to load instead of the workspace's,
//! so the same cases can be run against a mutated copy of the specification.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use spindle_kernel::{Decider, Executed, External, MemoryStore, Store, execute};
use spindle_spec::EssIr;

const INSTANCE: &str = "spindle.placement.Instance";
const NOT_ADMITTED: &str = "spindle.placement.DeploymentNotAdmitted";
const CONFLICT: &str = "spindle.placement.InstanceStateConflict";

fn specification() -> EssIr {
    let root = std::env::var_os("SPINDLE_ADVERSARY_ESS").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ess"),
        PathBuf::from,
    );
    spindle_spec::load(&root).expect("ess/ compiles").ir
}

/// Answers every external question with one fixed answer, and records each one asked.
struct Answer {
    holds: bool,
    asked: Vec<String>,
}

impl Answer {
    fn no() -> Self {
        Self {
            holds: false,
            asked: Vec::new(),
        }
    }

    fn yes() -> Self {
        Self {
            holds: true,
            asked: Vec::new(),
        }
    }
}

impl Decider for Answer {
    fn decide(&mut self, question: &External<'_>) -> Result<bool, String> {
        self.asked.push(question.capability.clone());
        Ok(self.holds)
    }
}

fn send(
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
    execute(ir, store, decider, command, &map)
        .unwrap_or_else(|error| panic!("{command} executes: {error}"))
}

/// Sends `command` with every external cause answered `false` and asserts its outcome.
fn expect(ir: &EssIr, store: &mut dyn Store, command: &str, request: Value, outcome: &str) {
    let executed = send(ir, store, &mut Answer::no(), command, request);
    assert_eq!(executed.outcome, outcome, "{command}: {executed:?}");
}

fn state(store: &mut dyn Store, instance: &str) -> String {
    store
        .load(INSTANCE, &json!(instance))
        .expect("the store reads")
        .expect("the instance is held")
        .row
        .state
        .clone()
}

/// Declares environment `laptop` and its target, offers deployment `d1` and admits it.
fn admitted(ir: &EssIr, store: &mut dyn Store) {
    expect(
        ir,
        store,
        "spindle.deployment.DeclareEnvironment",
        json!({"environment_id": "laptop", "os": "linux", "arch": "x86_64", "provides": []}),
        "declared",
    );
    expect(
        ir,
        store,
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
        ir,
        store,
        "spindle.deployment.OfferBundle",
        json!({"deployment_id": "d1", "bundle": "sha256:0f", "environment_id": "laptop"}),
        "offered",
    );
    expect(
        ir,
        store,
        "spindle.deployment.DecideAdmission",
        json!({"deployment_id": "d1", "requires": [], "bindings": [], "missing": []}),
        "admitted",
    );
}

fn plan_and_place(ir: &EssIr, store: &mut dyn Store, instance: &str) {
    expect(
        ir,
        store,
        "spindle.placement.PlanInstance",
        json!({
            "instance_id": instance,
            "deployment_id": "d1",
            "component": "spindle-control",
            "settings": [],
            "target_id": "t1",
            "request": {"vcpus": 1, "memory_mib": 64, "disk_mib": 0},
        }),
        "planned",
    );
    expect(
        ir,
        store,
        "spindle.placement.PlaceInstance",
        json!({"instance_id": instance}),
        "placed",
    );
}

fn launch(ir: &EssIr, store: &mut dyn Store, instance: &str) -> Executed {
    let mut none = Answer::no();
    let launched = send(
        ir,
        store,
        &mut none,
        "spindle.placement.LaunchInstance",
        json!({"instance_id": instance}),
    );
    assert!(
        none.asked.is_empty(),
        "the deployment's state decides the launch: {:?}",
        none.asked
    );
    launched
}

/// observe.md § 5a: "a running deployment counts as admitted". A placed instance of a deployment
/// that was started launches. Nothing else in the suite sends `LaunchInstance` for a `Running`
/// deployment, so the predicate `{not: state == Admitted}` passes every other case.
#[test]
fn adversary_a_placed_instance_of_a_running_deployment_launches() {
    let ir = specification();
    let mut store = MemoryStore::default();
    admitted(&ir, &mut store);
    expect(
        &ir,
        &mut store,
        "spindle.deployment.StartDeployment",
        json!({"deployment_id": "d1"}),
        "started",
    );
    plan_and_place(&ir, &mut store, "i1");
    let launched = launch(&ir, &mut store, "i1");
    assert_eq!(
        (
            launched.outcome.as_str(),
            launched.error.as_ref().map(|e| e.name.as_str())
        ),
        ("launched", None),
        "a running deployment counts as admitted: {launched:?}"
    );
    assert_eq!(state(&mut store, "i1"), "Running");
}

/// observe.md § 5a: an admitted deployment launches, an offered one does not, and the one
/// refused names its deployment. One store, both halves.
#[test]
fn adversary_admitted_launches_and_offered_does_not() {
    let ir = specification();
    let mut store = MemoryStore::default();
    admitted(&ir, &mut store);
    plan_and_place(&ir, &mut store, "i1");
    let launched = launch(&ir, &mut store, "i1");
    assert_eq!(launched.outcome, "launched", "{launched:?}");

    expect(
        &ir,
        &mut store,
        "spindle.deployment.OfferBundle",
        json!({"deployment_id": "d2", "bundle": "sha256:0f", "environment_id": "laptop"}),
        "offered",
    );
    expect(
        &ir,
        &mut store,
        "spindle.placement.PlanInstance",
        json!({
            "instance_id": "i2",
            "deployment_id": "d2",
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
        json!({"instance_id": "i2"}),
        "placed",
    );
    let refused = launch(&ir, &mut store, "i2");
    assert_eq!(refused.outcome, "not-admitted", "{refused:?}");
    let error = refused.error.as_ref().expect("the launch is refused");
    assert_eq!(error.name, NOT_ADMITTED);
    assert_eq!(
        Value::Object(error.fields.clone()),
        json!({"deployment_id": "d2"})
    );
    assert_eq!(state(&mut store, "i2"), "Placed");
}

/// observe.md § 5a: "`LaunchInstance` reads the deployment only for a `Placed` instance, so a
/// planned, refused, running or exited one answers `wrong-state` first". Planned, running and
/// exited instances of a stopped deployment.
#[test]
fn adversary_only_a_placed_instance_reads_its_deployment() {
    let ir = specification();
    let mut store = MemoryStore::default();
    admitted(&ir, &mut store);
    expect(
        &ir,
        &mut store,
        "spindle.placement.PlanInstance",
        json!({
            "instance_id": "planned",
            "deployment_id": "d1",
            "component": "spindle-control",
            "settings": [],
            "target_id": "t1",
            "request": {"vcpus": 1, "memory_mib": 64, "disk_mib": 0},
        }),
        "planned",
    );
    plan_and_place(&ir, &mut store, "running");
    assert_eq!(launch(&ir, &mut store, "running").outcome, "launched");
    plan_and_place(&ir, &mut store, "exited");
    assert_eq!(launch(&ir, &mut store, "exited").outcome, "launched");
    expect(
        &ir,
        &mut store,
        "spindle.placement.ReportExit",
        json!({"instance_id": "exited", "code": 0, "signal": null}),
        "exited",
    );
    expect(
        &ir,
        &mut store,
        "spindle.deployment.StopDeployment",
        json!({"deployment_id": "d1"}),
        "stopped",
    );
    for (instance, held) in [
        ("planned", "Planned"),
        ("running", "Running"),
        ("exited", "Exited"),
    ] {
        let launched = launch(&ir, &mut store, instance);
        assert_eq!(
            (
                launched.outcome.as_str(),
                launched.error.as_ref().map(|e| e.name.as_str())
            ),
            ("wrong-state", Some(CONFLICT)),
            "a {held} instance answers its state before its deployment's: {launched:?}"
        );
        assert_eq!(state(&mut store, instance), held);
    }
}

/// The story: "PlanInstance refuses an unknown deployment first". With every external cause
/// answered `true`, a deployment no one offered is still `unknown-deployment`, nothing outside
/// the model is asked, and no instance is created.
#[test]
fn adversary_an_unknown_deployment_is_refused_before_any_external_cause() {
    let ir = specification();
    let mut store = MemoryStore::default();
    admitted(&ir, &mut store);
    let mut yes = Answer::yes();
    let planned = send(
        &ir,
        &mut store,
        &mut yes,
        "spindle.placement.PlanInstance",
        json!({
            "instance_id": "i9",
            "deployment_id": "nobody-offered",
            "component": "spindle-control",
            "settings": [],
            "target_id": "t1",
            "request": {"vcpus": 1, "memory_mib": 64, "disk_mib": 0},
        }),
    );
    assert_eq!(planned.outcome, "unknown-deployment", "{planned:?}");
    assert!(yes.asked.is_empty(), "{:?}", yes.asked);
    let error = planned.error.as_ref().expect("the plan is refused");
    assert_eq!(error.name, NOT_ADMITTED);
    assert_eq!(
        Value::Object(error.fields.clone()),
        json!({"deployment_id": "nobody-offered"})
    );
    assert!(
        store
            .load(INSTANCE, &json!("i9"))
            .expect("the store reads")
            .is_none()
    );
}

/// Gives no answer to any external question, as a run's decider does for a cause neither the
/// decisions file nor `--answer-default` answers (`spindle-cli` `decisions.rs`).
struct Unanswered;

impl Decider for Unanswered {
    fn decide(&mut self, question: &External<'_>) -> Result<bool, String> {
        Err(format!("no decision for {}", question.capability))
    }
}

/// The same refusal with a decider that cannot answer: a deployment no one offered is refused by
/// the model alone, so a request for one is `unknown-deployment` whatever the decider knows.
#[test]
fn adversary_an_unknown_deployment_is_refused_without_a_decider() {
    let ir = specification();
    let mut store = MemoryStore::default();
    admitted(&ir, &mut store);
    let input = json!({
        "instance_id": "i9",
        "deployment_id": "nobody-offered",
        "component": "spindle-control",
        "settings": [],
        "target_id": "t1",
        "request": {"vcpus": 1, "memory_mib": 64, "disk_mib": 0},
    });
    let Value::Object(map) = input else {
        unreachable!()
    };
    let planned = execute(
        &ir,
        &mut store,
        &mut Unanswered,
        "spindle.placement.PlanInstance",
        &map,
    );
    assert_eq!(
        planned
            .as_ref()
            .map(|executed| executed.outcome.as_str())
            .map_err(ToString::to_string),
        Ok("unknown-deployment"),
        "{planned:?}"
    );
}
