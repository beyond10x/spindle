//! Adversary cases of wave 29 unit U2 (`story:payload-field-through-conversion`,
//! `story:value-source-through-two-rows`): the in-process host against ESS's interpreter on the
//! same model, each run in one fresh scenario.
//!
//! `tests/fixtures/adversary_w29_u2` is the kernel's `related-chain` fixture with the two changes
//! its unit made to pass `conform` undone: the instance fields declare no `presence:`, and the tag
//! is bounded from above (`value.count <= 8`) as the conformance fixture's short code is. It adds
//! `PinInstance`, which reads the identity field of the row two hops from the input, and
//! `NoteInstance`, a one-hop read through an `Optional` input, the path the unit does not change.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ess_compiler::EssIr;
use ess_compiler::refs::{CommandRef, ViewRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::{
    AdmittedSuite, ConformanceTarget, Deadline, Runner, ScenarioContext, SemanticCommandRequest,
    SemanticViewRequest, TargetError,
};
use ess_primitives::consistency::QueryConsistency;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_conformance::HostTarget;

const CHAIN: &str = "tests/fixtures/adversary_w29_u2";
const CONVERSION: &str = "tests/fixtures/payload-conversion";
const INSTANCES: &str = "forge.plant.Instances";
const BADGES: &str = "forge.plant.Badges";

fn load(fixture: &str) -> EssIr {
    spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join(fixture))
        .expect("the fixture loads")
        .ir
}

fn laptop() -> Environment {
    let path: PathBuf =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/environments/laptop.toml");
    let text = std::fs::read_to_string(path).expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

fn json_of(node: &Node) -> Value {
    serde_json::to_value(node).expect("a node is JSON")
}

fn object(value: Value) -> Map<String, Value> {
    let Value::Object(object) = value else {
        panic!("an object: {value}");
    };
    object
}

/// What a target answered one command: the outcome with every event payload it published, or
/// the kind of its refusal.
#[derive(Debug, PartialEq)]
enum Answer {
    Took { outcome: String, events: Vec<Value> },
    Refused(&'static str),
}

/// What one scenario observed: each command's answer, and every view's rows at its end, a field
/// held as `null` kept as `null`.
#[derive(Debug, PartialEq)]
struct Observed {
    answers: Vec<Answer>,
    rows: BTreeMap<String, Vec<Value>>,
}

fn observe<T: ConformanceTarget>(
    target: &T,
    commands: &[(&str, Value)],
    views: &[&str],
) -> Observed {
    let context = ScenarioContext::new(
        "forge.plant/authored/adversary"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("adversary").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let answers = commands
        .iter()
        .map(|(command, input)| {
            let answered = target.execute_command(SemanticCommandRequest {
                command: command.parse::<CommandRef>().expect("a command"),
                actor: None,
                caller: None,
                input: object(input.clone())
                    .into_iter()
                    .map(|(field, value)| (field, serde_json::from_value(value).expect("a node")))
                    .collect(),
                correlation: context.correlation.clone(),
            });
            match answered {
                Ok(result) => Answer::Took {
                    outcome: result
                        .outcome
                        .map(|outcome| outcome.to_string())
                        .unwrap_or_default(),
                    events: result
                        .direct_events
                        .iter()
                        .map(|event| {
                            Value::Object(
                                event
                                    .payload
                                    .iter()
                                    .map(|(field, value)| (field.clone(), json_of(value)))
                                    .collect(),
                            )
                        })
                        .collect(),
                },
                Err(TargetError::Unsupported { .. }) => Answer::Refused("unsupported"),
                Err(TargetError::Unavailable { .. }) => Answer::Refused("unavailable"),
                Err(TargetError::NotGranted { .. }) => Answer::Refused("not granted"),
            }
        })
        .collect();
    let deadline = Deadline::at(Timestamp::from_epoch_millis(0));
    let rows = views
        .iter()
        .map(|view| {
            let rows = target
                .query_view(SemanticViewRequest {
                    view: ViewRef::new(view.parse().expect("a view name")),
                    params: BTreeMap::new(),
                    consistency: QueryConsistency::Current,
                    correlation: context.correlation.clone(),
                    deadline,
                })
                .unwrap_or_else(|error| panic!("{view} reads: {error:?}"))
                .rows
                .iter()
                .map(|row| {
                    Value::Object(
                        row.iter()
                            .map(|(field, value)| (field.clone(), json_of(value)))
                            .collect(),
                    )
                })
                .collect();
            ((*view).to_owned(), rows)
        })
        .collect();
    target.end_scenario(&context).expect("the scenario closes");
    Observed { answers, rows }
}

/// The same scenario on ESS's interpreter, then on the in-process host.
fn both(ir: &EssIr, commands: &[(&str, Value)], views: &[&str]) -> (Observed, Observed) {
    (
        observe(&Interpreted::for_model(ir.clone()), commands, views),
        observe(&HostTarget::new(ir.clone(), laptop()), commands, views),
    )
}

fn formation(id: &str, code: &str) -> (&'static str, Value) {
    (
        "forge.plant.RegisterFormation",
        json!({"formation_id": id, "code": code, "slots": ["north"]}),
    )
}

fn workload(id: &str, home: &str, formation: Option<&str>) -> (&'static str, Value) {
    let mut input = json!({"workload_id": id, "home_id": home});
    if let Some(formation) = formation {
        input["formation_id"] = json!(formation);
    }
    ("forge.plant.RegisterWorkload", input)
}

/// Each scenario ESS synthesises for the chain fixture whose id starts with one of `commands`,
/// with its status on the host where that differs from its status on ESS's interpreter, and the
/// host's diagnostics; and how many such scenarios there are.
fn differing(commands: &[&str]) -> (Vec<String>, usize) {
    let loaded = spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join(CHAIN))
        .expect("the fixture loads");
    let synthesis = spindle_conformance::synthesize(&loaded).expect("the suite synthesises");
    let suite = AdmittedSuite::from_suite(&synthesis.suite).expect("the suite is admitted");
    let selected = |id: &str| {
        commands
            .iter()
            .any(|command| id.starts_with(&format!("{command}/")))
    };
    let interpreted = Runner::for_suite(suite.suite())
        .run_admitted(&suite, &Interpreted::for_model(loaded.ir.clone()))
        .into_report()
        .scenarios
        .into_iter()
        .map(|run| (run.scenario.to_string(), run.status))
        .collect::<Vec<_>>();
    let run = spindle_conformance::execute(&suite, &HostTarget::new(loaded.ir, laptop()));
    let mut differ = Vec::new();
    let mut count = 0;
    for result in &run.executed.scenarios {
        let id = result.scenario.to_string();
        if !selected(&id) {
            continue;
        }
        count += 1;
        let on_ess = interpreted
            .iter()
            .find(|(scenario, _)| *scenario == id)
            .map(|(_, status)| *status);
        if on_ess != Some(result.status) {
            let diagnostics = result
                .diagnostics()
                .map(|diagnostic| format!("{diagnostic:?}"))
                .collect::<Vec<_>>();
            differ.push(format!(
                "{id}: host {:?}, interpreter {on_ess:?}: {diagnostics:#?}",
                result.status
            ));
        }
    }
    (differ, count)
}

/// Every scenario ESS synthesises for the chained reads has the status on the host it has on
/// ESS's interpreter, where the model declares no `presence:` on the fields they fill. The unit's
/// own fixture passes `conform` only with `presence: null_when_absent` on the instance's fields.
#[test]
fn every_synthesised_scenario_of_a_chained_read_runs_on_the_host_as_on_the_interpreter() {
    let (differ, count) = differing(&[
        "forge.plant.CreateInstance",
        "forge.plant.PinInstance",
        "forge.plant.BadgeWorkload",
    ]);
    assert!(count >= 3, "ESS synthesises the chained outcomes: {count}");
    assert!(
        differ.is_empty(),
        "scenarios whose status differs:\n{}",
        differ.join("\n")
    );
}

/// The same for the one-hop read through an `Optional` input (`NoteInstance`), which the unit's
/// diff does not change: where the chained creation differs, this tells whether the one-hop read
/// does too.
#[test]
fn every_synthesised_scenario_of_a_one_hop_optional_read_runs_on_the_host_as_on_the_interpreter() {
    let (differ, count) = differing(&["forge.plant.NoteInstance"]);
    assert!(count >= 1, "ESS synthesises the one-hop outcome: {count}");
    assert!(
        differ.is_empty(),
        "scenarios whose status differs:\n{}",
        differ.join("\n")
    );
}

/// An instance created through a workload with no `formation_id` holds no slots and no state:
/// the view row the host answers is the one ESS's interpreter answers, `null` included.
#[test]
fn a_chain_through_an_absent_optional_reference_leaves_the_view_row_as_ess_does() {
    let ir = load(CHAIN);
    let commands = [
        formation("frm-1", "AB-1"),
        workload("wl-1", "frm-1", None),
        (
            "forge.plant.CreateInstance",
            json!({"instance_id": "ins-1", "workload_id": "wl-1"}),
        ),
    ];
    let (ess, spindle) = both(&ir, &commands, &[INSTANCES]);
    assert!(
        matches!(ess.answers[2], Answer::Took { .. }),
        "ESS's interpreter creates the instance: {ess:?}"
    );
    assert_eq!(spindle, ess, "the host answers and writes as ESS does");
}

/// The one-hop counterpart: an `Optional` input left absent.
#[test]
fn a_one_hop_read_through_an_absent_optional_input_leaves_the_view_row_as_ess_does() {
    let ir = load(CHAIN);
    let commands = [("forge.plant.NoteInstance", json!({"instance_id": "ins-1"}))];
    let (ess, spindle) = both(&ir, &commands, &[INSTANCES]);
    assert!(
        matches!(ess.answers[0], Answer::Took { .. }),
        "ESS's interpreter creates the instance: {ess:?}"
    );
    assert_eq!(spindle, ess, "the host answers and writes as ESS does");
}

/// `PinInstance` reads the identity field of the row two hops away: the far row's own address,
/// as ESS's interpreter reads it, never the address the input sent. The home formation differs
/// from the workload's `formation_id`, so reading the wrong reference shows too.
#[test]
fn a_chain_read_of_the_far_row_s_identity_is_the_far_address() {
    let ir = load(CHAIN);
    let commands = [
        formation("frm-1", "AB-1"),
        formation("frm-2", "CD-2"),
        workload("wl-1", "frm-1", Some("frm-2")),
        (
            "forge.plant.PinInstance",
            json!({"instance_id": "ins-1", "workload_id": "wl-1"}),
        ),
    ];
    let (ess, spindle) = both(&ir, &commands, &[INSTANCES]);
    let pinned = |observed: &Observed| {
        observed.rows[INSTANCES]
            .iter()
            .map(|row| row["formation_id"].clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(pinned(&ess), [json!("frm-1")], "ESS's interpreter: {ess:?}");
    assert_eq!(pinned(&spindle), pinned(&ess), "the host: {spindle:?}");
    assert_eq!(spindle.answers, ess.answers, "answered as ESS answers");
}

/// The home formation of a workload names no row: the chained read determines nothing, and the
/// host answers the kind of answer ESS's interpreter answers (`unsupported`, a capability gap),
/// and writes nothing.
#[test]
fn a_chain_whose_far_row_is_absent_is_answered_as_ess_answers_it() {
    let ir = load(CHAIN);
    let commands = [
        workload("wl-1", "frm-none", None),
        (
            "forge.plant.PinInstance",
            json!({"instance_id": "ins-1", "workload_id": "wl-1"}),
        ),
        (
            "forge.plant.BadgeWorkload",
            json!({"badge_id": "bdg-1", "workload_id": "wl-1"}),
        ),
    ];
    let (ess, spindle) = both(&ir, &commands, &[INSTANCES, BADGES]);
    assert_eq!(spindle, ess, "the host answers and writes as ESS does");
}

/// The one-hop counterpart: an `Optional` input that names no row.
#[test]
fn a_one_hop_read_of_an_absent_row_is_answered_as_ess_answers_it() {
    let ir = load(CHAIN);
    let commands = [(
        "forge.plant.NoteInstance",
        json!({"instance_id": "ins-1", "formation_id": "frm-none"}),
    )];
    let (ess, spindle) = both(&ir, &commands, &[INSTANCES]);
    assert_eq!(spindle, ess, "the host answers and writes as ESS does");
}

/// A far code longer than the tag's upper bound, read through the chain and the conversion: the
/// host refuses it as ESS's interpreter does, and writes nothing.
#[test]
fn a_chained_code_over_the_tag_s_upper_bound_is_refused_as_ess_refuses_it() {
    let ir = load(CHAIN);
    let commands = [
        formation("frm-1", "CODE-TOO-LONG"),
        workload("wl-1", "frm-1", None),
        (
            "forge.plant.BadgeWorkload",
            json!({"badge_id": "bdg-1", "workload_id": "wl-1"}),
        ),
    ];
    let (ess, spindle) = both(&ir, &commands, &[BADGES]);
    assert_eq!(
        ess.answers[2],
        Answer::Refused("unavailable"),
        "ESS's interpreter refuses it as the request's: {ess:?}"
    );
    assert_eq!(spindle, ess, "the host answers and writes as ESS does");
}

/// An input through the conversion into a target its value breaks: the value is passed on
/// unchanged, and whatever ESS's interpreter answers for it, the host answers too.
#[test]
fn an_input_through_a_conversion_its_target_refuses_is_answered_as_ess_answers_it() {
    let ir = load(CONVERSION);
    let commands = [(
        "beacon.desk.IssueBadge",
        json!({"badge_id": "badge-1", "code": "CODE-TOO-LONG"}),
    )];
    let (ess, spindle) = both(&ir, &commands, &["beacon.desk.Badges"]);
    assert_eq!(spindle, ess, "the host answers and writes as ESS does");
}

/// Every scenario ESS synthesises for the unit's conversion fixture has the status on the host it
/// has on ESS's interpreter.
#[test]
fn every_synthesised_scenario_of_the_conversion_fixture_runs_as_on_the_interpreter() {
    let loaded = spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join(CONVERSION))
        .expect("the fixture loads");
    let synthesis = spindle_conformance::synthesize(&loaded).expect("the suite synthesises");
    let suite = AdmittedSuite::from_suite(&synthesis.suite).expect("the suite is admitted");
    let interpreted = Runner::for_suite(suite.suite())
        .run_admitted(&suite, &Interpreted::for_model(loaded.ir.clone()))
        .into_report()
        .scenarios
        .into_iter()
        .map(|run| (run.scenario.to_string(), run.status))
        .collect::<Vec<_>>();
    let run = spindle_conformance::execute(&suite, &HostTarget::new(loaded.ir, laptop()));
    let hosted = run
        .executed
        .scenarios
        .iter()
        .map(|result| (result.scenario.to_string(), result.status))
        .collect::<Vec<_>>();
    assert_eq!(hosted, interpreted, "each scenario's status on both");
}
