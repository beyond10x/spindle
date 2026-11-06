//! Scenarios that arrange their starting state with `establish` steps (story:scenario-establish-steps),
//! on the host and on ESS 0.87.0's interpreter, over the neutral two-component `post` fixture.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ess_compiler::EssIr;
use ess_compiler::refs::{CommandRef, EntityRef, ViewRef};
use ess_conformance::authored::{self, Source};
use ess_conformance::interpret::Interpreted;
use ess_conformance::synthesize::{AdmittedSeeds, SeedSelection, synthesize_with_seeds};
use ess_conformance::target::EntitySetupRequest;
use ess_conformance::{
    AdmittedSuite, ConformanceSuite, ConformanceTarget, Deadline, InstanceName, ScenarioContext,
    ScenarioStep, SemanticCommandRequest, SemanticViewRequest, Status, SuiteProvenance,
};
use ess_domain::entity::StateName;
use ess_primitives::consistency::QueryConsistency;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_conformance::HostTarget;

const FIXTURE: &str = "tests/fixtures/establish";
const SCENARIOS: &str = "tests/fixtures/establish-scenarios";

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

fn host() -> HostTarget {
    HostTarget::new(ir(), laptop())
}

fn interpreter() -> Interpreted {
    Interpreted::for_model(ir())
}

fn node(value: Value) -> Node {
    serde_json::from_value(value).expect("a node")
}

fn json_of(node: &Node) -> Value {
    serde_json::to_value(node).expect("a node is JSON")
}

/// One row to establish: entity, identity, fields and state.
struct Row(&'static str, &'static str, Value, &'static str);

/// What a target answered: each command's outcome or error, and each view's rows.
#[derive(Debug, PartialEq)]
struct Observed {
    answers: Vec<String>,
    rows: BTreeMap<String, Vec<Value>>,
}

impl Observed {
    /// The answers, with every view member answered `null` left out: ESS's interpreter answers an
    /// `Optional` view field a row holds nothing in as `null`, the host leaves it out, for a row a
    /// command wrote as for one a scenario established. Both read as "holds nothing".
    fn read(&self) -> Self {
        let rows = self
            .rows
            .iter()
            .map(|(view, rows)| {
                let rows = rows
                    .iter()
                    .map(|row| match row {
                        Value::Object(members) => Value::Object(
                            members
                                .iter()
                                .filter(|(_, value)| !value.is_null())
                                .map(|(field, value)| (field.clone(), value.clone()))
                                .collect(),
                        ),
                        other => other.clone(),
                    })
                    .collect();
                (view.clone(), rows)
            })
            .collect();
        Self {
            answers: self.answers.clone(),
            rows,
        }
    }
}

/// Establishes `rows`, sends `commands` in order, then reads `views`: one scenario on `target`.
fn drive<T: ConformanceTarget>(
    target: &T,
    rows: &[Row],
    commands: &[(&str, Value)],
    views: &[String],
) -> Observed {
    let context = ScenarioContext::new(
        "post.parcel/authored/establish"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("establish").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    for Row(entity, identity, fields, state) in rows {
        let Value::Object(fields) = fields else {
            panic!("fields are an object");
        };
        target
            .establish_entity(EntitySetupRequest {
                entity: entity.parse::<EntityRef>().expect("an entity"),
                identity: Node::Text((*identity).to_owned()),
                fields: fields
                    .iter()
                    .map(|(field, value)| (field.clone(), node(value.clone())))
                    .collect(),
                state: StateName::new(state).expect("a state"),
                correlation: context.correlation.clone(),
            })
            .unwrap_or_else(|error| panic!("{entity} {identity} is established: {error:?}"));
    }
    let answers = commands
        .iter()
        .map(|(command, input)| {
            let Value::Object(input) = input else {
                panic!("input is an object");
            };
            let result = target
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
                .unwrap_or_else(|error| panic!("{command} runs: {error:?}"));
            format!(
                "{} {}",
                result
                    .outcome
                    .map_or_else(|| "<undeclared>".to_owned(), |o| o.outcome.to_string()),
                result
                    .error
                    .map_or_else(String::new, |error| error.error.to_string())
            )
        })
        .collect();
    let deadline = Deadline::at(Timestamp::from_epoch_millis(0));
    let rows = views
        .iter()
        .map(|view| {
            let rows = target
                .query_view(SemanticViewRequest {
                    view: view.parse::<ViewRef>().expect("a view"),
                    params: BTreeMap::new(),
                    consistency: QueryConsistency::Current,
                    correlation: context.correlation.clone(),
                    deadline,
                })
                .unwrap_or_else(|error| panic!("{view} is read: {error:?}"))
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
            (view.clone(), rows)
        })
        .collect();
    target.end_scenario(&context).expect("the scenario closes");
    Observed { answers, rows }
}

/// Every view whose source is `entity`.
fn views_over(ir: &EssIr, entity: &str) -> Vec<String> {
    ir.views()
        .iter()
        .filter(|(_, view)| ir.entity(&view.source).name.to_string() == entity)
        .map(|(name, _)| name.to_string())
        .collect()
}

/// The status ESS's runner gives the authored document `file` on `target`.
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

const PARCEL: &str = "post.parcel.Parcel";
const BAG: &str = "post.bag.Bag";

#[test]
fn an_established_row_is_read_by_the_next_command() {
    let rows = [Row(
        PARCEL,
        "parcel-1",
        json!({"label": "books", "missing": false}),
        "Sent",
    )];
    let commands = [
        ("post.parcel.SendParcel", json!({"parcel_id": "parcel-1"})),
        (
            "post.parcel.DeliverParcel",
            json!({"parcel_id": "parcel-1"}),
        ),
        (
            "post.parcel.DeliverParcel",
            json!({"parcel_id": "parcel-1"}),
        ),
    ];
    let views = views_over(&ir(), PARCEL);
    let spindle = drive(&host(), &rows, &commands, &views);
    let ess = drive(&interpreter(), &rows, &commands, &views);
    assert_eq!(spindle.read(), ess.read());
    assert_eq!(
        spindle.answers,
        [
            "not-packed post.parcel.NotPacked",
            "delivered ",
            "not-sent post.parcel.NotSent",
        ]
    );

    for (target, status) in [
        ("spindle", replay(&ir(), "deliver.yaml", &host())),
        ("ess", replay(&ir(), "deliver.yaml", &interpreter())),
    ] {
        assert_eq!(status.0, Status::Passed, "{target}: {}", status.1);
    }
}

#[test]
fn an_established_row_is_published_by_its_views() {
    let rows = [
        Row(
            PARCEL,
            "parcel-1",
            json!({"label": "books", "missing": false}),
            "Packed",
        ),
        Row(
            PARCEL,
            "parcel-2",
            json!({"label": "plates", "note": "fragile", "missing": false}),
            "Sent",
        ),
        Row(
            PARCEL,
            "parcel-3",
            json!({"label": "lamps", "note": null, "missing": true}),
            "Lost",
        ),
    ];
    let views = views_over(&ir(), PARCEL);
    assert_eq!(views.len(), 2, "{views:?}");
    let packed = [(
        "post.parcel.PackParcel",
        json!({"parcel_id": "parcel-0", "label": "cups"}),
    )];
    let spindle = drive(&host(), &rows, &packed, &views);
    let ess = drive(&interpreter(), &rows, &packed, &views);
    assert_eq!(spindle.read(), ess.read());
    assert_eq!(
        ess.rows["post.parcel.AllParcels"][0]["note"],
        Value::Null,
        "the interpreter answers the note of a row a command wrote as null too: {ess:?}"
    );
    assert_eq!(
        spindle.rows["post.parcel.AllParcels"].len(),
        4,
        "{spindle:?}"
    );
    assert_eq!(
        spindle.rows["post.parcel.ParcelsOnTheirWay"],
        [json!({"parcel_id": "parcel-2", "label": "plates"})]
    );
}

#[test]
fn establishing_an_entity_another_component_owns_is_written_there() {
    let ir = ir();
    let owner = |entity: &str| {
        let domain = &ir
            .entities()
            .iter()
            .find(|(name, _)| name.to_string() == entity)
            .expect("declared")
            .1
            .domain;
        ir.components()
            .values()
            .find(|component| component.owns.contains(domain))
            .map(|component| component.name.as_str().to_owned())
            .expect("owned")
    };
    assert_eq!(owner(BAG), "counter");
    assert_eq!(owner(PARCEL), "depot");

    let rows = [
        Row(BAG, "bag-1", json!({"route": "north"}), "Open"),
        Row(
            PARCEL,
            "parcel-1",
            json!({"label": "books", "missing": true}),
            "Sent",
        ),
    ];
    let commands = [
        ("post.bag.SealBag", json!({"bag_id": "bag-1"})),
        ("post.parcel.ReportLoss", json!({"parcel_id": "parcel-1"})),
        (
            "post.parcel.RecoverParcel",
            json!({"parcel_id": "parcel-1"}),
        ),
    ];
    let mut views = views_over(&ir, BAG);
    views.extend(views_over(&ir, PARCEL));
    let spindle = drive(&host(), &rows, &commands, &views);
    let ess = drive(&interpreter(), &rows, &commands, &views);
    assert_eq!(spindle.read(), ess.read());
    assert_eq!(
        spindle.answers,
        ["sealed ", "lost ", "recovered "],
        "{spindle:?}"
    );
    assert_eq!(
        spindle.rows["post.bag.AllBags"],
        [json!({"bag_id": "bag-1", "state": "Sealed", "route": "north"})]
    );
}

#[test]
fn the_synthesized_suite_executes_its_establishing_scenarios() {
    let ir = ir();
    let selections = [("missing.yaml", "adrift"), ("lost.yaml", "found")]
        .into_iter()
        .map(|(file, instance)| SeedSelection {
            source: Source::new(
                format!("seeds/{file}"),
                std::fs::read_to_string(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join(SCENARIOS)
                        .join(file),
                )
                .expect("the seed reads"),
            ),
            instance: InstanceName::new(instance).expect("an instance name"),
        })
        .collect::<Vec<_>>();
    let seeds = AdmittedSeeds::compile(&ir, &selections).expect("the seeds are admitted");
    let synthesis = synthesize_with_seeds(&ir, &seeds).expect("the suite is synthesized");
    let suite = AdmittedSuite::from_suite(&synthesis.suite).expect("the suite is admitted");
    let establishing = synthesis
        .suite
        .scenarios
        .iter()
        .filter(|(_, scenario)| {
            scenario
                .steps
                .iter()
                .any(|step| matches!(step, ScenarioStep::EstablishEntity { .. }))
        })
        .map(|(id, _)| id.to_string())
        .collect::<Vec<_>>();
    assert!(!establishing.is_empty(), "the seeds were applied");

    let run = spindle_conformance::execute(&suite, &host());
    let ess = spindle_conformance::execute(&suite, &interpreter());
    for id in &establishing {
        let status = |run: &spindle_conformance::Run| {
            run.report()
                .scenarios
                .iter()
                .find(|result| result.scenario.to_string() == *id)
                .map(|result| result.status)
                .expect("the scenario ran")
        };
        assert_eq!(status(&ess), Status::Passed, "ess: {id}\n{}", ess.report());
        assert_eq!(status(&run), Status::Passed, "{id}\n{}", run.report());
    }
    assert_eq!(run.counts().skipped, 0, "{}", run.report());
    assert!(run.passed(), "{}", run.report());
}
