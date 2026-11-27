//! An outcome field filled through a declared conversion (`conversions:`), run by the in-process
//! host and by ESS's interpreter on the same neutral fixture (`story:payload-field-through-conversion`).
//!
//! `tests/fixtures/payload-conversion` converts a `Code` into a `ShortCode`, a newtype bounded to
//! eight characters. `IssueBadge` fills a `sets:` field and an event field of type `ShortCode`
//! from the `Code` input; `LabelDesk` publishes the `Code` its desk holds as a `ShortCode`. ESS's
//! interpreter passes a value through a conversion unchanged, and checks a value read from a row
//! against the source and the target type, refusing one the target refuses as a request that is
//! not what the model declares (`interpret::execute::determined::checked_read`).
//! `tests/fixtures/payload-conversion-response` publishes a response field through a conversion,
//! which ESS does not interpret (`determined::response`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ess_compiler::EssIr;
use ess_compiler::refs::{CommandRef, EntityRef, ViewRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::EntitySetupRequest;
use ess_conformance::{
    ConformanceTarget, Deadline, ScenarioContext, SemanticCommandRequest, SemanticViewRequest,
    TargetError,
};
use ess_domain::entity::StateName;
use ess_primitives::consistency::QueryConsistency;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_conformance::HostTarget;
use spindle_kernel::{ExecuteError, MemoryStore, NoExternals, Store};

const FIXTURE: &str = "tests/fixtures/payload-conversion";
const RESPONSE: &str = "tests/fixtures/payload-conversion-response";
const DESK: &str = "beacon.desk.Desk";
const ISSUE: &str = "beacon.desk.IssueBadge";
const LABEL: &str = "beacon.desk.LabelDesk";
const REGISTER: &str = "beacon.desk.RegisterDesk";
const ASK: &str = "beacon.counter.AskBadge";
const VIEWS: [&str; 2] = ["beacon.desk.Desks", "beacon.desk.Badges"];

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

fn node(value: Value) -> Node {
    serde_json::from_value(value).expect("a node")
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

/// What a target answered one command: the outcome with every event it published, or the kind of
/// its refusal.
#[derive(Debug, PartialEq)]
enum Answer {
    Took {
        outcome: String,
        events: Vec<(String, Value)>,
    },
    Refused(&'static str),
}

/// What one scenario observed: each command's answer, and every view's rows at its end.
#[derive(Debug, PartialEq)]
struct Observed {
    answers: Vec<Answer>,
    rows: BTreeMap<String, Vec<Value>>,
}

/// Establishes each desk (`identity`, its `code`) as `Registered`, sends `commands` in order, and
/// reads `views` at the end: one scenario on `target`.
fn observe<T: ConformanceTarget>(
    target: &T,
    desks: &[(&str, &str)],
    commands: &[(&str, Value)],
    views: &[&str],
) -> Observed {
    let context = ScenarioContext::new(
        "beacon.desk/authored/conversions"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("conversions").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    for (identity, code) in desks {
        target
            .establish_entity(EntitySetupRequest {
                entity: DESK.parse::<EntityRef>().expect("an entity"),
                identity: Node::Text((*identity).to_owned()),
                fields: BTreeMap::from([("code".to_owned(), Node::Text((*code).to_owned()))]),
                state: StateName::new("Registered").expect("a state"),
                correlation: context.correlation.clone(),
            })
            .unwrap_or_else(|error| panic!("desk {identity} is established: {error:?}"));
    }
    let answers = commands
        .iter()
        .map(|(command, input)| {
            let answered = target.execute_command(SemanticCommandRequest {
                command: command.parse::<CommandRef>().expect("a command"),
                actor: None,
                caller: None,
                input: object(input.clone())
                    .into_iter()
                    .map(|(field, value)| (field, node(value)))
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
                            (
                                event.event.to_string(),
                                Value::Object(
                                    event
                                        .payload
                                        .iter()
                                        .map(|(field, value)| (field.clone(), json_of(value)))
                                        .collect(),
                                ),
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

/// The same scenario on ESS's interpreter and on the in-process host: ESS's observation first.
fn both(
    ir: &EssIr,
    desks: &[(&str, &str)],
    commands: &[(&str, Value)],
    views: &[&str],
) -> (Observed, Observed) {
    (
        observe(&Interpreted::for_model(ir.clone()), desks, commands, views),
        observe(
            &HostTarget::new(ir.clone(), laptop()),
            desks,
            commands,
            views,
        ),
    )
}

#[test]
fn a_payload_field_through_a_conversion_is_written_as_ess_converts_it() {
    let ir = load(FIXTURE);
    let commands = [(ISSUE, json!({"badge_id": "badge-1", "code": "AB-12"}))];
    let (ess, spindle) = both(&ir, &[], &commands, &VIEWS);
    assert_eq!(
        ess.answers,
        [Answer::Took {
            outcome: format!("{ISSUE}/issued"),
            events: vec![(
                "beacon.desk.BadgeIssued".to_owned(),
                json!({"badge_id": "badge-1", "code": "AB-12"}),
            )],
        }],
        "ESS's interpreter passes the code through the conversion unchanged"
    );
    assert_eq!(
        ess.rows["beacon.desk.Badges"],
        [json!({"badge_id": "badge-1", "code": "AB-12"})],
        "and the row holds it as sent"
    );
    assert_eq!(spindle, ess, "the host answers and writes as ESS does");
}

#[test]
fn a_row_read_the_conversion_target_refuses_is_refused_as_ess_refuses_it() {
    let ir = load(FIXTURE);
    let desks = [("desk-1", "AB-12"), ("desk-2", "CODE-TOO-LONG")];
    let commands = [
        (LABEL, json!({"desk_id": "desk-1"})),
        (LABEL, json!({"desk_id": "desk-2"})),
    ];
    let (ess, spindle) = both(&ir, &desks, &commands, &VIEWS);
    assert_eq!(
        ess.answers,
        [
            Answer::Took {
                outcome: format!("{LABEL}/labelled"),
                events: vec![(
                    "beacon.desk.DeskLabelled".to_owned(),
                    json!({"desk_id": "desk-1", "label": "AB-12"}),
                )],
            },
            Answer::Refused("unavailable"),
        ],
        "ESS's interpreter refuses a held code its short code does not admit as the request's"
    );
    assert_eq!(spindle, ess, "the host answers as ESS does");

    // The kernel's own answer: the request's, not a construct it does not run, and nothing is
    // written.
    let mut store = MemoryStore::default();
    let register = json!({"desk_id": "desk-2", "code": "CODE-TOO-LONG"});
    spindle_kernel::execute(
        &ir,
        &mut store,
        &mut NoExternals,
        REGISTER,
        &object(register),
    )
    .expect("the desk registers");
    let events = store.events().len();
    let refused = spindle_kernel::execute(
        &ir,
        &mut store,
        &mut NoExternals,
        LABEL,
        &object(json!({"desk_id": "desk-2"})),
    );
    match refused {
        Err(ExecuteError::InvalidInput(why)) => {
            assert!(why.contains("label"), "the refusal names the field: {why}");
        }
        other => panic!("refused as the request's, not {other:?}"),
    }
    assert_eq!(store.events().len(), events, "nothing is published");
    let desk = store
        .load(DESK, &json!("desk-2"))
        .expect("the store reads")
        .expect("the desk is held")
        .row;
    assert_eq!(desk.state, "Registered", "the desk is not moved");
}

#[test]
fn a_response_field_through_a_conversion_stays_unsupported() {
    let ir = load(RESPONSE);
    let command = ir
        .commands()
        .values()
        .find(|command| command.name.to_string() == ASK)
        .expect("the command");
    match spindle_kernel::runs(&ir, command) {
        Err(ExecuteError::Unsupported { construct }) => assert!(
            construct.contains("conversion") && construct.contains("`code`"),
            "the refusal names the conversion and the field: {construct}"
        ),
        other => panic!("refused by name, not {other:?}"),
    }
    let commands = [(ASK, json!({"badge_id": "badge-1"}))];
    let (ess, spindle) = both(&ir, &[], &commands, &[]);
    assert_eq!(
        ess.answers,
        [Answer::Refused("unsupported")],
        "ESS's interpreter does not interpret it"
    );
    assert_eq!(spindle, ess, "nor does the host");
}
