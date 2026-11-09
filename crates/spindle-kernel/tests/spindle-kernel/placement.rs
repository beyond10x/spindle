//! Spindle's own specification, `spindle.placement`, run by the kernel (story
//! `spindle-placement-rules-are-declared`): each case is one acceptance check of the story, and
//! the cases that close a design statement assert that `docs/design/observe.md` cites them.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{Map, Value, json};
use spindle_kernel::{Decider, Executed, External, MemoryStore, Store, execute};
use spindle_spec::EssIr;

const TARGET: &str = "spindle.placement.Target";
const INSTANCE: &str = "spindle.placement.Instance";
const NOT_ADMITTED: &str = "spindle.placement.DeploymentNotAdmitted";

fn workspace() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn specification() -> EssIr {
    spindle_spec::load(&workspace().join("ess"))
        .expect("ess/ compiles")
        .ir
}

/// Answers `true` to exactly the external questions it holds and records every one asked.
#[derive(Default)]
struct Holds {
    holding: BTreeSet<String>,
    asked: Vec<String>,
}

impl Holds {
    fn only(capability: &str) -> Self {
        Self {
            holding: BTreeSet::from([capability.to_owned()]),
            asked: Vec::new(),
        }
    }
}

impl Decider for Holds {
    fn decide(&mut self, question: &External<'_>) -> Result<bool, String> {
        self.asked.push(question.capability.clone());
        Ok(self.holding.contains(&question.capability))
    }
}

fn external(command: &str, outcome: &str) -> String {
    format!("external.spindle.placement.{command}.{outcome}")
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

fn declare(ir: &EssIr, store: &mut dyn Store, target: &str, kind: &str) -> Executed {
    run(
        ir,
        store,
        &mut Holds::default(),
        "DeclareTarget",
        json!({
            "target_id": target,
            "kind": kind,
            "environment_id": "laptop",
            "ceiling": {"vcpus": 1, "memory_mib": 128, "disk_mib": 0},
        }),
    )
}

fn plan(ir: &EssIr, store: &mut dyn Store, decider: &mut dyn Decider, instance: &str) -> Executed {
    admitted(ir, store);
    run(
        ir,
        store,
        decider,
        "PlanInstance",
        json!({
            "instance_id": instance,
            "deployment_id": "d1",
            "component": "rally-table",
            "settings": [{"name": "side", "value": "west"}],
            "target_id": "t1",
            "request": {"vcpus": 1, "memory_mib": 64, "disk_mib": 0},
        }),
    )
}

/// Offers deployment `d1` and admits it, unless it is already held: `PlanInstance` refuses an
/// instance of a deployment no one offered (`unknown-deployment`).
fn admitted(ir: &EssIr, store: &mut dyn Store) {
    if store
        .load("spindle.deployment.Deployment", &json!("d1"))
        .expect("the store reads")
        .is_some()
    {
        return;
    }
    let offered = deployment(
        ir,
        store,
        "OfferBundle",
        json!({"deployment_id": "d1", "bundle": "sha256:0f", "environment_id": "laptop", "requires": []}),
    );
    assert_eq!(offered.outcome, "offered", "{offered:?}");
    let admitted = deployment(
        ir,
        store,
        "DecideAdmission",
        json!({"deployment_id": "d1", "bindings": [], "missing": []}),
    );
    assert_eq!(admitted.outcome, "admitted", "{admitted:?}");
}

/// Runs `spindle.deployment.<command>`, which asks no external cause on these inputs.
fn deployment(ir: &EssIr, store: &mut dyn Store, command: &str, request: Value) -> Executed {
    let Value::Object(map) = request else {
        panic!("an input is an object");
    };
    execute(
        ir,
        store,
        &mut NoExternal,
        &format!("spindle.deployment.{command}"),
        &map,
    )
    .unwrap_or_else(|error| panic!("{command} executes: {error}"))
}

/// Answers every external cause `false`.
struct NoExternal;

impl Decider for NoExternal {
    fn decide(&mut self, _question: &External<'_>) -> Result<bool, String> {
        Ok(false)
    }
}

fn planned(ir: &EssIr, store: &mut dyn Store, instance: &str) {
    let planned = plan(ir, store, &mut Holds::default(), instance);
    assert_eq!(planned.outcome, "planned", "{planned:?}");
}

fn row(store: &mut dyn Store, entity: &str, id: &str) -> (String, BTreeMap<String, Value>) {
    let stored = store
        .load(entity, &json!(id))
        .expect("the store reads")
        .unwrap_or_else(|| panic!("{entity} {id} exists"));
    (stored.row.state, stored.row.fields)
}

fn error_of(executed: &Executed) -> Option<&str> {
    executed.error.as_ref().map(|error| error.name.as_str())
}

/// Every scenario `ess conform synthesize --path ess` writes, by id.
fn synthesised(ir: &EssIr) -> BTreeSet<String> {
    ess_conformance::synthesize(ir)
        .suite
        .scenarios
        .keys()
        .map(ToString::to_string)
        .collect()
}

fn assert_synthesised(scenarios: &BTreeSet<String>, id: &str) {
    assert!(
        scenarios.contains(id),
        "`{id}` is synthesised; placement scenarios are {:?}",
        scenarios
            .iter()
            .filter(|scenario| scenario.starts_with("spindle.placement"))
            .collect::<Vec<_>>()
    );
}

/// `docs/design/observe.md` holds exactly `statement`, cited by `case`, as its text reads with
/// every run of whitespace collapsed to one space: a statement that says anything else fails.
fn assert_stated(case: &str, statement: &str) {
    let collapse = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    let design = collapse(
        &std::fs::read_to_string(workspace().join("docs/design/observe.md"))
            .expect("the observe design reads"),
    );
    let cited = format!("{} (`{case}`).", collapse(statement));
    assert!(
        design.contains(&cited),
        "docs/design/observe.md states, cited by `{case}`: {cited}"
    );
}

#[test]
fn an_instance_of_a_refused_deployment_cannot_be_placed() {
    let ir = specification();
    let mut store = MemoryStore::default();
    declare(&ir, &mut store, "t1", "Process");

    let mut refusing = Holds::only(&external("PlanInstance", "not-admitted"));
    let refused = plan(&ir, &mut store, &mut refusing, "i1");
    assert_eq!(
        (refused.outcome.as_str(), error_of(&refused)),
        ("not-admitted", Some(NOT_ADMITTED)),
        "{refused:?}"
    );
    assert_eq!(
        refused
            .error
            .as_ref()
            .map(|error| Value::Object(error.fields.clone())),
        Some(json!({"deployment_id": "d1"}))
    );
    assert!(refused.events.is_empty(), "{refused:?}");
    assert!(
        store.load(INSTANCE, &json!("i1")).unwrap().is_none(),
        "nothing of a deployment that is not admitted is planned"
    );

    planned(&ir, &mut store, "i1");
    let mut refusing = Holds::only(&external("PlaceInstance", "not-admitted"));
    let placed = run(
        &ir,
        &mut store,
        &mut refusing,
        "PlaceInstance",
        json!({"instance_id": "i1"}),
    );
    assert_eq!(placed.outcome, "not-admitted", "{placed:?}");
    assert_eq!(row(&mut store, INSTANCE, "i1").0, "Refused");

    planned(&ir, &mut store, "i2");
    let placed = run(
        &ir,
        &mut store,
        &mut Holds::default(),
        "PlaceInstance",
        json!({"instance_id": "i2"}),
    );
    assert_eq!(placed.outcome, "placed", "{placed:?}");
    let stopped = deployment(
        &ir,
        &mut store,
        "StopDeployment",
        json!({"deployment_id": "d1"}),
    );
    assert_eq!(stopped.outcome, "stopped", "{stopped:?}");
    let mut none = Holds::default();
    let launched = run(
        &ir,
        &mut store,
        &mut none,
        "LaunchInstance",
        json!({"instance_id": "i2"}),
    );
    assert!(
        none.asked.is_empty(),
        "the deployment's state decides the launch: {:?}",
        none.asked
    );
    assert_eq!(
        (launched.outcome.as_str(), error_of(&launched)),
        ("not-admitted", Some(NOT_ADMITTED)),
        "{launched:?}"
    );
    assert!(launched.events.is_empty(), "{launched:?}");
    assert_eq!(row(&mut store, INSTANCE, "i2").0, "Placed");

    let scenarios = synthesised(&ir);
    for command in ["PlanInstance", "PlaceInstance", "LaunchInstance"] {
        assert_synthesised(
            &scenarios,
            &format!("spindle.placement.{command}/outcome/not-admitted"),
        );
    }
}

#[test]
fn a_placement_refusal_names_its_cause() {
    let ir = specification();
    let causes = [
        ("target-retired", "TargetRetired"),
        ("environment-mismatch", "EnvironmentMismatch"),
        ("not-admitted", "NotAdmitted"),
        ("above-ceiling", "AboveCeiling"),
    ];
    for (outcome, cause) in causes {
        let mut store = MemoryStore::default();
        declare(&ir, &mut store, "t1", "Process");
        planned(&ir, &mut store, "i1");
        let mut refusing = Holds::only(&external("PlaceInstance", outcome));
        let refused = run(
            &ir,
            &mut store,
            &mut refusing,
            "PlaceInstance",
            json!({"instance_id": "i1"}),
        );
        assert_eq!(refused.outcome, outcome, "{refused:?}");
        assert_eq!(refused.error, None);
        assert_eq!(refused.events.len(), 1, "{refused:?}");
        assert_eq!(refused.events[0].name, "spindle.placement.InstanceRefused");
        assert_eq!(
            Value::Object(refused.events[0].payload.clone()),
            json!({"instance_id": "i1", "cause": cause})
        );
        let (state, fields) = row(&mut store, INSTANCE, "i1");
        assert_eq!(state, "Refused");
        assert_eq!(fields.get("refusal"), Some(&json!(cause)), "{fields:?}");
    }

    let mut store = MemoryStore::default();
    declare(&ir, &mut store, "t1", "Process");
    planned(&ir, &mut store, "i1");
    let mut none = Holds::default();
    let placed = run(
        &ir,
        &mut store,
        &mut none,
        "PlaceInstance",
        json!({"instance_id": "i1"}),
    );
    assert_eq!(placed.outcome, "placed", "{placed:?}");
    assert_eq!(
        none.asked,
        causes
            .iter()
            .map(|(outcome, _)| external("PlaceInstance", outcome))
            .collect::<Vec<_>>(),
        "the causes are asked in spindle_target::place's order"
    );
    assert_eq!(row(&mut store, INSTANCE, "i1").1.get("refusal"), None);

    let scenarios = synthesised(&ir);
    for (outcome, _) in causes {
        assert_synthesised(
            &scenarios,
            &format!("spindle.placement.PlaceInstance/outcome/{outcome}"),
        );
    }
}

#[test]
fn targets_and_requests_are_published_by_a_view() {
    let ir = specification();
    let published = |entity: &str| -> BTreeSet<String> {
        ir.views()
            .values()
            .filter(|view| ir.entity(&view.source).name.to_string() == entity)
            .flat_map(|view| view.fields.iter().map(|field| field.name.clone()))
            .collect()
    };
    let target = published(TARGET);
    for field in ["target_id", "state", "kind", "environment_id", "ceiling"] {
        assert!(
            target.contains(field),
            "a view of {TARGET} publishes {field}: {target:?}"
        );
    }
    let instance = published(INSTANCE);
    for field in [
        "instance_id",
        "state",
        "deployment_id",
        "component",
        "settings",
        "target_id",
        "request",
        "refusal",
    ] {
        assert!(
            instance.contains(field),
            "a view of {INSTANCE} publishes {field}: {instance:?}"
        );
    }
}

#[test]
fn retire_target_is_synthesised() {
    let ir = specification();
    let synthesis = ess_conformance::synthesize(&ir);
    let id = "spindle.placement.Target/state/Retired/refuses/spindle.placement.RetireTarget";
    let refused: Vec<String> = synthesis
        .refusals
        .iter()
        .filter(|refusal| {
            refusal
                .scenario
                .as_ref()
                .is_some_and(|scenario| scenario.to_string() == id)
        })
        .map(|refusal| refusal.code().to_string())
        .collect();
    assert!(refused.is_empty(), "{id} is refused: {refused:?}");
    assert_synthesised(&synthesised(&ir), id);
}

#[test]
fn retire_target_is_stated() {
    let ir = specification();
    let mut store = MemoryStore::default();
    declare(&ir, &mut store, "t1", "Process");
    let retired = run(
        &ir,
        &mut store,
        &mut Holds::default(),
        "RetireTarget",
        json!({"target_id": "t1"}),
    );
    assert_eq!(retired.outcome, "retired", "{retired:?}");
    assert_eq!(row(&mut store, TARGET, "t1").0, "Retired");
    let again = run(
        &ir,
        &mut store,
        &mut Holds::default(),
        "RetireTarget",
        json!({"target_id": "t1"}),
    );
    assert_eq!(
        (again.outcome.as_str(), error_of(&again)),
        (
            "already-retired",
            Some("spindle.placement.TargetAlreadyRetired")
        ),
        "{again:?}"
    );
    assert_stated(
        "retire_target_is_stated",
        "`RetireTarget` moves a target from `Available` to `Retired`, which is terminal; \
         retiring it again answers `TargetAlreadyRetired` and changes nothing. An instance \
         planned onto a retired target is refused `target-retired` when it is placed. Instances \
         already placed or running there are not moved; the operator retires a target to stop \
         placing on it, not to stop what runs",
    );
}

#[test]
fn target_ceiling_is_stated() {
    let ir = specification();
    let mut store = MemoryStore::default();
    declare(&ir, &mut store, "t1", "Process");
    assert_eq!(
        row(&mut store, TARGET, "t1").1.get("ceiling"),
        Some(&json!({"vcpus": 1, "memory_mib": 128, "disk_mib": 0}))
    );
    planned(&ir, &mut store, "i1");
    let refused = run(
        &ir,
        &mut store,
        &mut Holds::only(&external("PlaceInstance", "above-ceiling")),
        "PlaceInstance",
        json!({"instance_id": "i1"}),
    );
    assert_eq!(refused.outcome, "above-ceiling", "{refused:?}");
    assert_stated(
        "target_ceiling_is_stated",
        "`Target.ceiling` is the most one instance may take on the target, in vCPUs, memory MiB \
         and disk MiB, declared with the target. An instance whose request exceeds the ceiling \
         in any resource is refused `above-ceiling` with cause `AboveCeiling`. The model records \
         the request as the instance asked for it, a zero left as zero, and \
         `InstanceState.request` publishes it as requested; `spindle_target::place` raises each \
         zero to the ceiling and the launcher applies those limits, so a placed instance never \
         runs unbounded on a target that bounds it",
    );
}

#[test]
fn browser_target_kind_is_stated() {
    let ir = specification();
    let mut store = MemoryStore::default();
    let declared = declare(&ir, &mut store, "tab", "Browser");
    assert_eq!(declared.outcome, "declared", "{declared:?}");
    assert_eq!(
        row(&mut store, TARGET, "tab").1.get("kind"),
        Some(&json!("Browser"))
    );
    assert_stated(
        "browser_target_kind_is_stated",
        "Beside the kinds in § 3's transport table, `TargetKind` declares `Browser`, a browser \
         tab, as `spindle_target::TargetKind` does. A target of that kind can be declared and \
         planned onto; no launcher starts one yet, and § 3 gives it no telemetry endpoint",
    );
}

#[test]
fn report_exit_of_a_planned_instance_is_stated() {
    let ir = specification();
    let mut store = MemoryStore::default();
    declare(&ir, &mut store, "t1", "Process");
    planned(&ir, &mut store, "i1");
    let before = store.load(INSTANCE, &json!("i1")).unwrap().unwrap();
    let reported = run(
        &ir,
        &mut store,
        &mut Holds::default(),
        "ReportExit",
        json!({"instance_id": "i1", "code": 0}),
    );
    assert_eq!(reported.outcome, "already-ended", "{reported:?}");
    assert_eq!(reported.error, None);
    assert!(reported.events.is_empty(), "{reported:?}");
    assert_eq!(store.load(INSTANCE, &json!("i1")).unwrap().unwrap(), before);
    assert_stated(
        "report_exit_of_a_planned_instance_is_stated",
        "`ReportExit` moves only a `Placed` or `Running` instance to `Exited`. For a `Planned` \
         instance, as for a `Refused` or `Exited` one, it is accepted as `already-ended` and \
         changes nothing: no launcher ran it, so the report carries nothing to record",
    );
}
