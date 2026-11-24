//! An `Optional` event field whose source holds nothing, as ESS's interpreter emits it
//! (`interpret::execute::emit`, beyond10x/ess#765): a stored field a scenario established as
//! `null` is copied as `null`; a field never written is left out, unless the event field declares
//! `presence: null_when_absent`, which carries it as `null`. Over the neutral `relay` fixture,
//! whose `HandDesk` publishes the keeper the desk had in `previous_keeper_id` and again in
//! `noted_keeper_id`, which declares `null_when_absent`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ess_compiler::EssIr;
use ess_compiler::refs::{CommandRef, EntityRef};
use ess_conformance::authored::{self, Source};
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::EntitySetupRequest;
use ess_conformance::{
    AdmittedSuite, ConformanceSuite, ConformanceTarget, ScenarioContext, SemanticCommandRequest,
    Status, SuiteProvenance,
};
use ess_domain::entity::StateName;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_conformance::HostTarget;

const FIXTURE: &str = "tests/fixtures/held-null";
const SCENARIOS: &str = "tests/fixtures/held-null-scenarios";

fn ir() -> EssIr {
    spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE))
        .expect("the fixture loads")
        .ir
}

fn laptop() -> Environment {
    let path: PathBuf =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/environments/laptop.toml");
    let text = std::fs::read_to_string(path).expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

fn node(value: Value) -> Node {
    serde_json::from_value(value).expect("a node")
}

fn json_of(node: &Node) -> Value {
    serde_json::to_value(node).expect("a node is JSON")
}

/// Establishes each desk (`identity`, `fields`), sends `commands` in order, and answers the
/// payload of every event each command published: one scenario on `target`.
fn payloads<T: ConformanceTarget>(
    target: &T,
    desks: &[(&str, Value)],
    commands: &[(&str, Value)],
) -> Vec<Vec<BTreeMap<String, Value>>> {
    let context = ScenarioContext::new(
        "relay.desk/authored/held-null"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("held-null").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    for (identity, fields) in desks {
        let Value::Object(fields) = fields else {
            panic!("fields are an object");
        };
        target
            .establish_entity(EntitySetupRequest {
                entity: "relay.desk.Desk".parse::<EntityRef>().expect("an entity"),
                identity: Node::Text((*identity).to_owned()),
                fields: fields
                    .iter()
                    .map(|(field, value)| (field.clone(), node(value.clone())))
                    .collect(),
                state: StateName::new("Open").expect("a state"),
                correlation: context.correlation.clone(),
            })
            .unwrap_or_else(|error| panic!("desk {identity} is established: {error:?}"));
    }
    let published = commands
        .iter()
        .map(|(command, input)| {
            let Value::Object(input) = input else {
                panic!("input is an object");
            };
            target
                .execute_command(SemanticCommandRequest {
                    command: command.parse::<CommandRef>().expect("a command"),
                    actor: None,
                    caller: None,
                    input: input
                        .iter()
                        .map(|(field, value)| (field.clone(), node(value.clone())))
                        .collect(),
                    correlation: context.correlation.clone(),
                })
                .unwrap_or_else(|error| panic!("{command} runs: {error:?}"))
                .direct_events
                .iter()
                .map(|event| {
                    event
                        .payload
                        .iter()
                        .map(|(field, value)| (field.clone(), json_of(value)))
                        .collect()
                })
                .collect()
        })
        .collect();
    target.end_scenario(&context).expect("the scenario closes");
    published
}

/// The status ESS's runner gives the authored document `file` on `target`, with its report.
fn replay<T: ConformanceTarget>(ir: &EssIr, file: &str, target: &T) -> (Status, String) {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(SCENARIOS)
            .join(file),
    )
    .expect("the document reads");
    let authoring = authored::compile(ir, &[Source::new(file, text)]);
    assert!(authoring.is_complete(), "{:?}", authoring.refusals);
    let mut suite = ConformanceSuite::new(SuiteProvenance::of(ir));
    for (id, scenario) in authoring.scenarios {
        suite.insert(id, scenario).expect("one scenario");
    }
    suite.select_fresh_format_for(ir);
    let admitted = AdmittedSuite::from_suite(&suite).expect("the suite is admitted");
    let run = spindle_conformance::execute(&admitted, target);
    let result = run
        .report()
        .scenarios
        .first()
        .expect("one scenario")
        .clone();
    (result.status, format!("{}", run.report()))
}

#[test]
fn an_event_carries_a_held_null_as_null() {
    let ir = ir();
    for (target, (status, report)) in [
        (
            "ess",
            replay(&ir, "handed.yaml", &Interpreted::for_model(ir.clone())),
        ),
        (
            "spindle",
            replay(&ir, "handed.yaml", &HostTarget::new(ir.clone(), laptop())),
        ),
    ] {
        assert_eq!(status, Status::Passed, "{target}: {report}");
    }
}

#[test]
fn an_absent_source_is_left_out_unless_the_field_says_null_when_absent() {
    let ir = ir();
    let desks = [
        ("desk-1", json!({"keeper_id": null})),
        ("desk-2", json!({"keeper_id": "keeper-a"})),
    ];
    let commands = [
        ("relay.desk.OpenDesk", json!({"desk_id": "desk-3"})),
        (
            "relay.desk.HandDesk",
            json!({"desk_id": "desk-1", "keeper_id": "keeper-b"}),
        ),
        (
            "relay.desk.HandDesk",
            json!({"desk_id": "desk-2", "keeper_id": "keeper-c"}),
        ),
        (
            "relay.desk.HandDesk",
            json!({"desk_id": "desk-3", "keeper_id": "keeper-d"}),
        ),
    ];
    let ess = payloads(&Interpreted::for_model(ir.clone()), &desks, &commands);
    let spindle = payloads(&HostTarget::new(ir, laptop()), &desks, &commands);
    assert_eq!(
        spindle, ess,
        "the published payloads agree with the interpreter"
    );
    let handed = |index: usize| -> &BTreeMap<String, Value> { &ess[index][0] };
    assert_eq!(handed(1).get("previous_keeper_id"), Some(&Value::Null));
    assert_eq!(handed(1).get("noted_keeper_id"), Some(&Value::Null));
    assert_eq!(
        handed(2).get("previous_keeper_id"),
        Some(&json!("keeper-a"))
    );
    assert_eq!(
        handed(3).get("previous_keeper_id"),
        None,
        "a field never written is left out"
    );
    assert_eq!(
        handed(3).get("noted_keeper_id"),
        Some(&Value::Null),
        "`null_when_absent` carries it as null"
    );
}
