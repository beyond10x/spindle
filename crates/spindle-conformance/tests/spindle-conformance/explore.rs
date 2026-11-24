//! A seeded random walk over Spindle's own specification, `ess/`: every command sequence is sent
//! to [`HostTarget`] and to ESS's interpreter of the same IR, and after each step the two must
//! answer alike (outcome, error, the events published and their payloads) and publish the same
//! rows in every view. Every declared outcome must be reached by some seed. A disagreement is
//! shrunk to the shortest sequence that still disagrees and printed with its seed. Each sequence
//! is also sent twice, and the two runs must answer alike; and a step answered `already-ended`
//! must change nothing, so the walk without it answers every other step the same.
//!
//! Techniques 2, 4 and 5 of the hardening round `docs/hardening/2026-11-23.md`. Each check is
//! proven to catch a defect the synthesized suite passes, planted in [`HostTarget`].

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use ess_compiler::EssIr;
use ess_compiler::refs::{ActorRef, CommandRef, OutcomeRef, ViewRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::{
    ConformanceTarget, Deadline, ExternalOutcomeControl, ScenarioContext, SemanticCommandRequest,
    SemanticViewRequest, SemanticViewResult, TargetError,
};
use ess_domain::command::OutcomeName;
use ess_primitives::consistency::QueryConsistency;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_conformance::HostTarget;

const SEEDS: u64 = 150;
const STEPS: usize = 40;
const SETUP_STEPS: usize = 16;

const VIEWS: [&str; 4] = [
    "spindle.deployment.DeploymentState",
    "spindle.deployment.EnvironmentState",
    "spindle.placement.InstanceState",
    "spindle.placement.TargetState",
];

const ENVIRONMENTS: [&str; 2] = ["laptop", "microvm"];
const DEPLOYMENTS: [&str; 2] = ["d-1", "d-2"];
const TARGETS: [&str; 2] = ["t-1", "t-2"];
const INSTANCES: [&str; 3] = ["i-1", "i-2", "i-3"];
const CAPABILITIES: [&str; 3] = ["store.durable", "net.http", "external.a.b"];
const KEYS: [&str; 4] = ["store.durable", "net.http", "external.*", "timer.periodic"];
const IMPLEMENTATIONS: [&str; 2] = ["sqlite", "memory"];
const KINDS: [&str; 3] = ["Process", "QemuMicrovm", "Browser"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ir() -> EssIr {
    spindle_spec::load(&root().join("ess"))
        .expect("ess/ loads")
        .ir
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

/// splitmix64: the walk needs no more than a seeded, portable sequence.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n as u64).expect("below n")
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())]
    }

    fn chance(&mut self, one_in: usize) -> bool {
        self.below(one_in) == 0
    }

    fn list(&mut self, most: usize, mut item: impl FnMut(&mut Self) -> Value) -> Value {
        let count = self.below(most + 1);
        Value::Array((0..count).map(|_| item(self)).collect())
    }
}

/// One step of a walk: an external outcome forced before the send, where one is, and the send.
#[derive(Debug, Clone)]
struct Step {
    command: &'static str,
    actor: &'static str,
    force: Option<&'static str>,
    input: Value,
}

/// Resources an instance may take or asks for, each a value of `Resources`. None is below zero:
/// ESS 0.98.0's interpreter does not check a struct input's invariants before its input-guarded
/// refusals, where the kernel refuses the input first, so the two answer such an input apart
/// (`docs/hardening/2026-11-23.md`). The kernel's refusal is pinned by
/// `placement::a_resource_below_zero_is_refused`.
fn resources(rng: &mut Rng) -> Value {
    json!({
        "vcpus": rng.pick(&[0, 1, 2]),
        "memory_mib": rng.pick(&[0, 64, 128]),
        "disk_mib": rng.pick(&[0, 1]),
    })
}

/// A decision `DecideAdmission` takes: half of them consistent (each required capability bound or
/// missing, nothing else), so the branches past `not-decided` are reached.
fn decision(rng: &mut Rng) -> Value {
    let deployment = rng.pick(&DEPLOYMENTS);
    if rng.chance(2) {
        return json!({
            "deployment_id": deployment,
            "requires": rng.list(2, |rng| json!(rng.pick(&CAPABILITIES))),
            "bindings": rng.list(2, |rng| json!({
                "capability": rng.pick(&CAPABILITIES),
                "implementation": rng.pick(&IMPLEMENTATIONS),
            })),
            "missing": rng.list(1, |rng| json!(rng.pick(&CAPABILITIES))),
        });
    }
    let mut requires = Vec::new();
    let mut bindings = Vec::new();
    let mut missing = Vec::new();
    for capability in CAPABILITIES {
        match rng.below(4) {
            0 => {
                requires.push(json!(capability));
                bindings.push(json!({
                    "capability": capability,
                    "implementation": rng.pick(&IMPLEMENTATIONS),
                }));
            }
            1 => {
                requires.push(json!(capability));
                missing.push(json!(capability));
            }
            _ => {}
        }
    }
    json!({
        "deployment_id": deployment,
        "requires": requires,
        "bindings": bindings,
        "missing": missing,
    })
}

/// The commands that create or decide a row a later command needs: declaring an environment,
/// offering and admitting a deployment, declaring a target, planning and placing an instance.
const SETUP: [usize; 6] = [0, 1, 2, 5, 7, 8];

/// One step: during `setup` one of [`SETUP`], else any command.
fn step(rng: &mut Rng, setup: bool) -> Step {
    let pick = if setup {
        rng.pick(&SETUP)
    } else {
        rng.below(11)
    };
    let (command, actor, externals, input): (&str, &str, &[&str], Value) = match pick {
        0 => (
            "spindle.deployment.DeclareEnvironment",
            "spindle.deployment.Operator",
            &[],
            json!({
                "environment_id": rng.pick(&ENVIRONMENTS),
                "os": "linux",
                "arch": "x86_64",
                "provides": rng.list(2, |rng| json!({
                    "capability": rng.pick(&KEYS),
                    "implementations": rng.list(2, |rng| json!(rng.pick(&IMPLEMENTATIONS))),
                })),
            }),
        ),
        1 => (
            "spindle.deployment.OfferBundle",
            "spindle.deployment.Operator",
            &[],
            json!({
                "deployment_id": rng.pick(&DEPLOYMENTS),
                "bundle": rng.pick(&["sha256:0f", "sha256:1e"]),
                "environment_id": rng.pick(&ENVIRONMENTS),
            }),
        ),
        2 => (
            "spindle.deployment.DecideAdmission",
            "spindle.deployment.Admission",
            &["refused"],
            decision(rng),
        ),
        3 => (
            "spindle.deployment.StartDeployment",
            "spindle.deployment.Operator",
            &[],
            json!({"deployment_id": rng.pick(&DEPLOYMENTS)}),
        ),
        4 => (
            "spindle.deployment.StopDeployment",
            "spindle.deployment.Operator",
            &[],
            json!({"deployment_id": rng.pick(&DEPLOYMENTS)}),
        ),
        5 => (
            "spindle.placement.DeclareTarget",
            "spindle.placement.Placer",
            &[],
            json!({
                "target_id": rng.pick(&TARGETS),
                "kind": rng.pick(&KINDS),
                "environment_id": rng.pick(&ENVIRONMENTS),
                "ceiling": resources(rng),
            }),
        ),
        6 => (
            "spindle.placement.RetireTarget",
            "spindle.placement.Placer",
            &[],
            json!({"target_id": rng.pick(&TARGETS)}),
        ),
        7 => (
            "spindle.placement.PlanInstance",
            "spindle.placement.Placer",
            &["not-admitted"],
            json!({
                "instance_id": rng.pick(&INSTANCES),
                "deployment_id": rng.pick(&DEPLOYMENTS),
                "component": rng.pick(&["spindle-control", "worker"]),
                "settings": rng.list(1, |rng| json!({"name": "port", "value": rng.pick(&["1", "2"])})),
                "target_id": rng.pick(&TARGETS),
                "request": resources(rng),
            }),
        ),
        8 => (
            "spindle.placement.PlaceInstance",
            "spindle.placement.Placer",
            &[
                "target-retired",
                "environment-mismatch",
                "not-admitted",
                "above-ceiling",
            ],
            json!({"instance_id": rng.pick(&INSTANCES)}),
        ),
        9 => (
            "spindle.placement.LaunchInstance",
            "spindle.placement.Launcher",
            &[],
            json!({"instance_id": rng.pick(&INSTANCES)}),
        ),
        _ => {
            let mut input = json!({"instance_id": rng.pick(&INSTANCES)});
            if rng.chance(2) {
                input["code"] = json!(rng.pick(&[0, 1, 137]));
            }
            if rng.chance(2) {
                input["signal"] = json!(rng.pick(&[9, 15]));
            }
            (
                "spindle.placement.ReportExit",
                "spindle.placement.Launcher",
                &[],
                input,
            )
        }
    };
    let force = (!externals.is_empty() && rng.chance(2)).then(|| rng.pick(externals));
    Step {
        command,
        actor,
        force,
        input,
    }
}

/// The steps of one seed: [`SETUP_STEPS`] steps that create rows, then any command. Without the
/// setup a `PlaceInstance` rarely meets a `Planned` instance, which needs a declared environment,
/// an offered deployment and a planned instance first: 7 of 379 forced sends did in 200 seeds of
/// any command, and no seed reached `environment-mismatch` or `above-ceiling`.
fn walk(seed: u64) -> Vec<Step> {
    let mut rng = Rng(seed);
    (0..STEPS)
        .map(|index| step(&mut rng, index < SETUP_STEPS))
        .collect()
}

/// What one target answered to one step and published after it, comparable across targets.
#[derive(Debug, PartialEq)]
struct Answer {
    outcome: Result<String, String>,
    views: BTreeMap<&'static str, Vec<String>>,
}

fn json_of(node: &Node) -> Value {
    serde_json::to_value(node).expect("a node is JSON")
}

/// A command no declared outcome answers and that changed nothing, as the kernel reports it.
const UNDECLARED: &str = "<undeclared> None []";

/// The answer either target gives where no declared outcome answers a command, such as a
/// `creates:` for an identity a row already holds: the kernel answers no outcome, the interpreter
/// refuses the request as not what the model declares. Both change nothing, which the views check.
const NO_OUTCOME: &str = "no declared outcome";

fn refusal(error: &TargetError) -> String {
    match error {
        TargetError::NotGranted { .. } => "not granted".to_owned(),
        TargetError::Unavailable { detail, .. }
            if detail.contains("the request is not what the model declares") =>
        {
            NO_OUTCOME.to_owned()
        }
        TargetError::Unsupported { .. } => format!("unsupported: {error:?}"),
        _ => format!("error: {error:?}"),
    }
}

/// Sends `steps` to `target` in one fresh scenario and returns its answer to each.
fn run(target: &dyn ConformanceTarget, steps: &[Step]) -> Vec<Answer> {
    let context = ScenarioContext::new(
        "spindle.deployment/authored/random-walk"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("random-walk").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let answers = steps
        .iter()
        .map(|step| {
            let command = CommandRef::new(step.command.parse().expect("a command name"));
            if let Some(outcome) = step.force {
                target
                    .configure_external_outcome(ExternalOutcomeControl {
                        force: OutcomeRef::new(
                            command.clone(),
                            OutcomeName::new(outcome).expect("an outcome name"),
                        ),
                        correlation: context.correlation.clone(),
                    })
                    .expect("the target takes the forcing");
            }
            let input = serde_json::from_value::<BTreeMap<String, Node>>(step.input.clone())
                .expect("the input is a map of nodes");
            let outcome = target
                .execute_command(SemanticCommandRequest {
                    command,
                    actor: Some(ActorRef::new(step.actor.parse().expect("an actor name"))),
                    caller: None,
                    input,
                    correlation: context.correlation.clone(),
                })
                .map(|result| {
                    let events = result
                        .direct_events
                        .iter()
                        .map(|event| {
                            let payload = event
                                .payload
                                .iter()
                                .map(|(field, value)| (field.clone(), json_of(value)))
                                .collect::<BTreeMap<_, _>>();
                            format!("{} {}", event.event, json!(payload))
                        })
                        .collect::<Vec<_>>();
                    format!(
                        "{} {:?} {events:?}",
                        result
                            .outcome
                            .map_or_else(|| "<undeclared>".to_owned(), |o| o.outcome.to_string()),
                        result.error.map(|error| (
                            error.error.to_string(),
                            json!(
                                error
                                    .fields
                                    .iter()
                                    .map(|(field, value)| (field.clone(), json_of(value)))
                                    .collect::<BTreeMap<_, _>>()
                            )
                            .to_string()
                        )),
                    )
                })
                .map_err(|error| refusal(&error))
                .and_then(|answer| {
                    if answer == UNDECLARED {
                        Err(NO_OUTCOME.to_owned())
                    } else {
                        Ok(answer)
                    }
                });
            let views = VIEWS
                .iter()
                .map(|view| (*view, rows(target, &context, view)))
                .collect();
            Answer { outcome, views }
        })
        .collect();
    target.end_scenario(&context).expect("the scenario closes");
    answers
}

fn rows(target: &dyn ConformanceTarget, context: &ScenarioContext, view: &str) -> Vec<String> {
    let mut rows = target
        .query_view(SemanticViewRequest {
            view: ViewRef::new(view.parse().expect("a view name")),
            params: BTreeMap::new(),
            consistency: QueryConsistency::Current,
            correlation: context.correlation.clone(),
            deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
        })
        .map(|result| {
            result
                .rows
                .iter()
                .map(|row| {
                    // An absent Optional field: the interpreter publishes it as `null`, the
                    // kernel leaves it out.
                    json!(
                        row.iter()
                            .filter(|(_, value)| **value != Node::Null)
                            .map(|(field, value)| (field.clone(), json_of(value)))
                            .collect::<BTreeMap<_, _>>()
                    )
                    .to_string()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|error| vec![refusal(&error)]);
    rows.sort();
    rows
}

/// The first step at which `target` and the interpreter answer `steps` differently.
fn first_disagreement(
    model: &EssIr,
    target: &dyn Fn() -> Box<dyn ConformanceTarget>,
    steps: &[Step],
) -> Option<(usize, Answer, Answer)> {
    let reference = run(&Interpreted::for_model(model.clone()), steps);
    let tested = run(target().as_ref(), steps);
    reference
        .into_iter()
        .zip(tested)
        .enumerate()
        .find(|(_, (expected, actual))| expected != actual)
        .map(|(index, (expected, actual))| (index, expected, actual))
}

/// The shortest prefix that still disagrees, with every step dropped that it does not need.
fn shrink(
    model: &EssIr,
    target: &dyn Fn() -> Box<dyn ConformanceTarget>,
    steps: &[Step],
    at: usize,
) -> Vec<Step> {
    let mut steps = steps[..=at].to_vec();
    let mut index = 0;
    while index + 1 < steps.len() {
        let mut fewer = steps.clone();
        fewer.remove(index);
        if first_disagreement(model, target, &fewer).is_some() {
            steps = fewer;
        } else {
            index += 1;
        }
    }
    steps
}

/// What a walk over every seed came to: the first disagreement, shrunk, and the outcomes reached.
struct Walked {
    disagreement: Option<String>,
    reached: BTreeSet<String>,
}

fn explore(target: &dyn Fn() -> Box<dyn ConformanceTarget>) -> Walked {
    let model = ir();
    let mut reached = BTreeSet::new();
    for seed in 0..SEEDS {
        let steps = walk(seed);
        let once = run(target().as_ref(), &steps);
        let twice = run(target().as_ref(), &steps);
        if let Some(index) = once.iter().zip(&twice).position(|(one, two)| one != two) {
            return Walked {
                disagreement: Some(format!(
                    "seed {seed}: one sequence answered twice differently at step {index}: \
                     {:#?}\nthen {:#?}\nafter {:#?}",
                    once[index],
                    twice[index],
                    &steps[..=index]
                )),
                reached,
            };
        }
        let reference = run(&Interpreted::for_model(model.clone()), &steps);
        if let Some(at) = reference
            .iter()
            .zip(&once)
            .position(|(one, two)| one != two)
        {
            let shrunk = shrink(&model, target, &steps, at);
            let (index, expected, actual) = first_disagreement(&model, target, &shrunk)
                .expect("the shrunk sequence still disagrees");
            return Walked {
                disagreement: Some(format!(
                    "seed {seed}: step {index} of the shrunk sequence of {} steps {shrunk:#?}\n\
                     ESS's interpreter: {expected:#?}\ntarget: {actual:#?}",
                    shrunk.len()
                )),
                reached,
            };
        }
        for (step, answer) in steps.iter().zip(once) {
            if let Ok(text) = answer.outcome {
                let outcome = text.split(' ').next().unwrap_or_default().to_owned();
                reached.insert(format!("{}/{outcome}", step.command));
            }
        }
    }
    Walked {
        disagreement: None,
        reached,
    }
}

/// Every outcome `ess/` declares, as `<command>/<outcome>`.
fn declared() -> BTreeSet<String> {
    let model = serde_json::to_value(ir()).expect("the IR is JSON");
    model["commands"]
        .as_object()
        .expect("commands by name")
        .values()
        .flat_map(|command| {
            let name = command["name"].as_str().expect("a command name").to_owned();
            command["outcomes"]
                .as_array()
                .expect("outcomes")
                .iter()
                .map(move |outcome| {
                    format!("{name}/{}", outcome["name"].as_str().expect("an outcome"))
                })
        })
        .collect()
}

fn host() -> Box<dyn ConformanceTarget> {
    Box::new(HostTarget::new(ir(), laptop()))
}

#[test]
fn a_seeded_walk_over_spindles_own_specification_agrees_with_ess_interpreter() {
    let walked = explore(&host);
    if let Some(disagreement) = walked.disagreement {
        panic!("{disagreement}");
    }
    let unreached = declared()
        .difference(&walked.reached)
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        unreached.is_empty(),
        "no seed reached {unreached:#?}; reached {:#?}",
        walked.reached
    );
    println!(
        "{SEEDS} seeds of {STEPS} steps: all {} declared outcomes reached, no disagreement",
        walked.reached.len()
    );
}

/// The first seed on which the design's relation "a command answered `already-ended` changes
/// nothing" fails: the walk without that step answers every other step as the walk with it
/// (`docs/design/spindle.md` § 3a, "On an `Offered`, `Refused` or `Stopped` deployment it is
/// accepted and changes nothing"; `ReportExit`'s `already-ended`).
fn no_op_breaks(target: &dyn Fn() -> Box<dyn ConformanceTarget>) -> (Option<String>, u64) {
    let mut compared = 0_u64;
    for seed in 0..SEEDS {
        let steps = walk(seed);
        let answers = run(target().as_ref(), &steps);
        let Some(index) = answers.iter().position(|answer| {
            answer
                .outcome
                .as_ref()
                .is_ok_and(|text| text.starts_with("already-ended "))
        }) else {
            continue;
        };
        let mut fewer = steps.clone();
        fewer.remove(index);
        let without = run(target().as_ref(), &fewer);
        let mut with = answers;
        with.remove(index);
        compared += 1;
        if let Some(at) = with.iter().zip(&without).position(|(one, two)| one != two) {
            return (
                Some(format!(
                    "seed {seed}: without the no-op step {index} ({:?}), step {} answers {:#?}, \
                     with it {:#?}",
                    steps[index],
                    at + usize::from(at >= index),
                    without[at],
                    with[at]
                )),
                compared,
            );
        }
    }
    (None, compared)
}

#[test]
fn a_command_answered_already_ended_changes_nothing() {
    let (broken, compared) = no_op_breaks(&host);
    if let Some(broken) = broken {
        panic!("{broken}");
    }
    assert!(compared >= SEEDS / 2, "only {compared} walks held a no-op");
    println!(
        "{compared} of {SEEDS} walks held a no-op; without it, every other step answered alike"
    );
}

/// A defect planted in [`HostTarget`], each one the synthesized suite of `ess/` passes.
#[derive(Clone, Copy)]
enum Defect {
    /// Every view publishes at most its first row: no scenario reads two rows of one view.
    FirstRowOnly,
    /// After an `already-ended` answer, the next command answers no declared outcome: the no-op
    /// leaves a trace.
    TraceOfNoOp,
    /// Every second target the process opens answers no declared outcome: the answer depends on
    /// something outside the sequence, as a clock or a hash order would make it.
    Unstable,
}

/// How many [`Defect::Unstable`] targets this process has opened.
static OPENED: AtomicU64 = AtomicU64::new(0);

struct Planted {
    target: HostTarget,
    defect: Defect,
    after_no_op: Cell<bool>,
    odd: bool,
}

fn planted(defect: Defect) -> Box<dyn ConformanceTarget> {
    let odd = matches!(defect, Defect::Unstable) && OPENED.fetch_add(1, Ordering::Relaxed) % 2 == 1;
    Box::new(Planted {
        target: HostTarget::new(ir(), laptop()),
        defect,
        after_no_op: Cell::new(false),
        odd,
    })
}

impl ConformanceTarget for Planted {
    fn identity(&self) -> Result<ess_conformance::ImplementationIdentity, TargetError> {
        self.target.identity()
    }

    fn begin_scenario(&self, scenario: &ScenarioContext) -> Result<(), TargetError> {
        self.after_no_op.set(false);
        self.target.begin_scenario(scenario)
    }

    fn execute_command(
        &self,
        request: SemanticCommandRequest,
    ) -> Result<ess_conformance::SemanticCommandResult, TargetError> {
        let mut result = self.target.execute_command(request)?;
        if let Defect::TraceOfNoOp = self.defect {
            if self.after_no_op.replace(false) {
                result.outcome = None;
            }
            let no_op = result
                .outcome
                .as_ref()
                .is_some_and(|outcome| outcome.outcome.as_str() == "already-ended");
            self.after_no_op.set(no_op);
        }
        if self.odd {
            result.outcome = None;
        }
        Ok(result)
    }

    fn query_view(&self, request: SemanticViewRequest) -> Result<SemanticViewResult, TargetError> {
        let mut result = self.target.query_view(request)?;
        if let Defect::FirstRowOnly = self.defect {
            result.rows.truncate(1);
        }
        Ok(result)
    }

    fn observe_events(
        &self,
        request: ess_conformance::EventObservationRequest,
    ) -> Result<Vec<ess_conformance::ObservedEvent>, TargetError> {
        self.target.observe_events(request)
    }

    fn configure_external_outcome(
        &self,
        request: ExternalOutcomeControl,
    ) -> Result<(), TargetError> {
        self.target.configure_external_outcome(request)
    }

    fn redeliver_event(
        &self,
        request: ess_conformance::RedeliveryRequest,
    ) -> Result<(), TargetError> {
        self.target.redeliver_event(request)
    }

    fn end_scenario(&self, scenario: &ScenarioContext) -> Result<(), TargetError> {
        self.target.end_scenario(scenario)
    }
}

#[test]
fn the_walk_catches_views_that_publish_one_row() {
    let walked = explore(&|| planted(Defect::FirstRowOnly));
    let disagreement = walked
        .disagreement
        .expect("a target whose views publish one row disagrees with ESS's interpreter");
    assert!(disagreement.contains("seed "), "{disagreement}");
    println!("{}", disagreement.lines().next().unwrap_or_default());
}

#[test]
fn the_walk_catches_an_answer_that_depends_on_more_than_the_sequence() {
    let walked = explore(&|| planted(Defect::Unstable));
    let disagreement = walked
        .disagreement
        .expect("a target that answers one sequence two ways is caught");
    assert!(
        disagreement.contains("answered twice differently"),
        "{disagreement}"
    );
    println!("{}", disagreement.lines().next().unwrap_or_default());
}

#[test]
fn the_relation_catches_a_no_op_that_leaves_a_trace() {
    let (broken, _) = no_op_breaks(&|| planted(Defect::TraceOfNoOp));
    let broken = broken.expect("a no-op that changes the next answer breaks the relation");
    assert!(broken.contains("seed "), "{broken}");
    println!("{}", broken.lines().next().unwrap_or_default());
}
