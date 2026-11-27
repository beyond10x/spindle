//! Adversary cases for wave 30 U5, story:create-on-held-identity-unavailable: a create on a held
//! identity answered on the host as ESS's interpreter answers it, beyond the one outcome, one
//! command and one `String` identity the unit's own cases send.

use std::collections::BTreeMap;
use std::path::Path;

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
use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_conformance::HostTarget;

const FIXTURE: &str = "tests/fixtures/adversary-w30-u5";

fn laptop() -> Environment {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/environments/laptop.toml");
    let text = std::fs::read_to_string(path).expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

fn fixture_ir() -> EssIr {
    spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE))
        .expect("the fixture loads")
        .ir
}

fn node(value: Value) -> Node {
    serde_json::from_value(value).expect("a node")
}

fn context() -> ScenarioContext {
    ScenarioContext::new(
        "shelf.box/authored/adversary-w30-u5"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("adversary-w30-u5").expect("a correlation id"),
    )
}

fn error_text(error: &TargetError) -> String {
    match error {
        TargetError::Unsupported { .. } => format!("unsupported {error:?}"),
        TargetError::Unavailable { operation, detail } => {
            format!("unavailable {operation}: {detail}")
        }
        other => format!("other {other:?}"),
    }
}

/// One step of a scenario, sent alike to both targets.
enum Step {
    Establish(&'static str, Value, Value, &'static str),
    Send(&'static str, Value),
    List(&'static str),
}

fn object(value: Value) -> BTreeMap<String, Node> {
    let Value::Object(map) = value else {
        panic!("an object");
    };
    map.into_iter()
        .map(|(field, value)| (field, node(value)))
        .collect()
}

/// What `target` answered to each of `steps`, in one fresh scenario.
fn drive(target: &dyn ConformanceTarget, steps: &[Step]) -> Vec<String> {
    let context = context();
    target.begin_scenario(&context).expect("open");
    let answers = steps
        .iter()
        .map(|step| match step {
            Step::Establish(entity, identity, fields, state) => target
                .establish_entity(EntitySetupRequest {
                    entity: entity.parse::<EntityRef>().expect("an entity"),
                    identity: node(identity.clone()),
                    fields: object(fields.clone()),
                    state: StateName::new(*state).expect("a state"),
                    correlation: context.correlation.clone(),
                })
                .map_or_else(
                    |error| format!("establish error: {}", error_text(&error)),
                    |()| "established".to_owned(),
                ),
            Step::Send(command, input) => match target.execute_command(SemanticCommandRequest {
                command: command.parse::<CommandRef>().expect("a command"),
                actor: None,
                caller: None,
                input: object(input.clone()),
                correlation: context.correlation.clone(),
            }) {
                Ok(result) => format!(
                    "{} {}",
                    result
                        .outcome
                        .map_or_else(|| "<undeclared>".to_owned(), |o| o.outcome.to_string()),
                    result
                        .error
                        .map_or_else(String::new, |error| error.error.to_string())
                ),
                Err(error) => format!("error: {}", error_text(&error)),
            },
            Step::List(view) => {
                let rows = target
                    .query_view(SemanticViewRequest {
                        view: view.parse::<ViewRef>().expect("a view"),
                        params: BTreeMap::new(),
                        consistency: QueryConsistency::Current,
                        correlation: context.correlation.clone(),
                        deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
                    })
                    .unwrap_or_else(|error| panic!("{view} is read: {error:?}"))
                    .rows;
                format!(
                    "{view}: {}",
                    serde_json::to_string(&rows).expect("rows are JSON")
                )
            }
        })
        .collect();
    target.end_scenario(&context).expect("close");
    answers
}

/// The host's answers and the interpreter's to `steps`, host first.
fn both(steps: &[Step]) -> (Vec<String>, Vec<String>) {
    let ir = fixture_ir();
    let host = HostTarget::new(ir.clone(), laptop());
    let interpreter = Interpreted::for_model(ir);
    (drive(&host, steps), drive(&interpreter, steps))
}

/// The answer ESS 0.99.2's interpreter gives a create of `command` on a held identity, written
/// from `interpret::execute::{select, create}` and `Undetermined::Request`'s `Display`.
fn held(command: &str, outcome: &str, entity: &str) -> String {
    format!(
        "error: unavailable invoking `{command}`: the request is not what the model declares: no \
         branch the model allows is described by the given values — `{command}/{outcome}`: a \
         supplied identity is already held by `{entity}`, and creation never replaces it"
    )
}

/// A command of two creating outcomes, its guarded one taken on a held identity: the refusal
/// names the guarded outcome, on both targets, in ESS's words.
#[test]
fn w30_u5_a_held_create_names_the_guarded_outcome_it_took() {
    let steps = [
        Step::Send(
            "shelf.box.PackBox",
            json!({"box_id": "b1", "label": "first", "rush": false}),
        ),
        Step::Send(
            "shelf.box.PackBox",
            json!({"box_id": "b1", "label": "second", "rush": true}),
        ),
        Step::List("shelf.box.AllBoxes"),
    ];
    let (spindle, ess) = both(&steps);
    assert_eq!(
        spindle[1],
        held("shelf.box.PackBox", "rushed", "shelf.box.Box"),
        "{spindle:#?}"
    );
    assert_eq!(spindle, ess, "host left, interpreter right");
}

/// An identity read from an optional input whose fallback the runtime mints, the input sent:
/// the identity is supplied, not minted, and a held one is refused as unavailable on both.
#[test]
fn w30_u5_a_supplied_identity_behind_a_generated_fallback_is_refused_as_held() {
    let steps = [
        Step::Send(
            "shelf.box.PackBoxMaybe",
            json!({"box_id": "b1", "label": "first"}),
        ),
        Step::Send(
            "shelf.box.PackBoxMaybe",
            json!({"box_id": "b1", "label": "second"}),
        ),
        Step::List("shelf.box.AllBoxes"),
    ];
    let (spindle, ess) = both(&steps);
    assert_eq!(
        spindle[1],
        held("shelf.box.PackBoxMaybe", "packed-maybe", "shelf.box.Box"),
        "{spindle:#?}"
    );
    assert_eq!(spindle, ess, "host left, interpreter right");
}

/// An identity the model names as a literal, held by the first create: the second is refused on
/// both as a held identity, though no request supplied it.
#[test]
fn w30_u5_a_literal_identity_held_is_refused_as_the_interpreter_refuses_it() {
    let steps = [
        Step::Send("shelf.box.PackFixed", json!({"label": "first"})),
        Step::Send("shelf.box.PackFixed", json!({"label": "second"})),
        Step::List("shelf.box.AllBoxes"),
    ];
    let (spindle, ess) = both(&steps);
    assert_eq!(spindle, ess, "host left, interpreter right");
}

/// A second command creating the same entity on an identity the first command's create holds:
/// the refusal names the second command and its own outcome.
#[test]
fn w30_u5_a_create_by_another_command_names_that_command() {
    let steps = [
        Step::Send(
            "shelf.box.PackBox",
            json!({"box_id": "b1", "label": "first", "rush": false}),
        ),
        Step::Send(
            "shelf.box.PackBoxMaybe",
            json!({"box_id": "b1", "label": "second"}),
        ),
        Step::List("shelf.box.AllBoxes"),
    ];
    let (spindle, ess) = both(&steps);
    assert_eq!(
        spindle[1],
        held("shelf.box.PackBoxMaybe", "packed-maybe", "shelf.box.Box"),
        "{spindle:#?}"
    );
    assert_eq!(spindle, ess, "host left, interpreter right");
}

/// An `Integer` identity established as `7` and sent as `7` and as `7.0`: both creates are
/// refused as held, on both targets, and the established row stays as it was.
#[test]
fn w30_u5_an_integer_identity_held_is_refused_in_either_spelling() {
    let steps = [
        Step::Establish(
            "shelf.box.Tally",
            json!(7),
            json!({"label": "established"}),
            "Open",
        ),
        Step::Send("shelf.box.OpenTally", json!({"tally_id": 7, "label": "a"})),
        Step::Send(
            "shelf.box.OpenTally",
            json!({"tally_id": 7.0, "label": "b"}),
        ),
        Step::List("shelf.box.AllTallies"),
    ];
    let (spindle, ess) = both(&steps);
    assert_eq!(spindle, ess, "host left, interpreter right");
    assert_eq!(
        spindle[1],
        held("shelf.box.OpenTally", "opened", "shelf.box.Tally"),
        "{spindle:#?}"
    );
}

/// An identity held by an instance of another entity, in another component's store, is not
/// held for this one: the create takes its outcome on both targets.
#[test]
fn w30_u5_an_identity_held_by_another_components_entity_is_free() {
    let steps = [
        Step::Establish(
            "shelf.box.Box",
            json!("same"),
            json!({"label": "a box"}),
            "Packed",
        ),
        Step::Send(
            "shelf.load.PackCrate",
            json!({"crate_id": "same", "label": "a crate"}),
        ),
        Step::Send(
            "shelf.load.PackCrate",
            json!({"crate_id": "same", "label": "again"}),
        ),
        Step::List("shelf.load.AllCrates"),
        Step::List("shelf.box.AllBoxes"),
    ];
    let (spindle, ess) = both(&steps);
    assert_eq!(spindle[1], "crated ", "{spindle:#?}");
    assert_eq!(
        spindle[2],
        held("shelf.load.PackCrate", "crated", "shelf.load.Crate"),
        "{spindle:#?}"
    );
    assert_eq!(spindle, ess, "host left, interpreter right");
}

/// After a create refused on a held identity the scenario goes on: a create of a free identity
/// takes its outcome, and the views hold the first and the third instance alone, on both.
#[test]
fn w30_u5_a_refused_create_leaves_the_next_create_as_it_was() {
    let steps = [
        Step::Send(
            "shelf.box.PackBox",
            json!({"box_id": "b1", "label": "first", "rush": false}),
        ),
        Step::Send(
            "shelf.box.PackBox",
            json!({"box_id": "b1", "label": "second", "rush": false}),
        ),
        Step::Send(
            "shelf.box.PackBox",
            json!({"box_id": "b2", "label": "third", "rush": true}),
        ),
        Step::List("shelf.box.AllBoxes"),
    ];
    let (spindle, ess) = both(&steps);
    assert_eq!(spindle[2], "rushed ", "{spindle:#?}");
    assert_eq!(spindle, ess, "host left, interpreter right");
}
