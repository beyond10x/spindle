//! Adversary pass 1 on `story:kernel-existing-instance` (wave 18 U2): the `existing_instance:`
//! answer driven against ESS 0.90.0's interpreter on shapes the unit's fixtures do not hold.
//!
//! The fixture `tests/fixtures/adversary-existing-instance` adds `AddWrapped`, whose creation
//! takes its identity from a member of a struct input (`input.wrap.id`, ess/22 Family F A4), and
//! `RetireItem` / `RemoveItem`, which leave a row in its terminal state or remove it. ESS's
//! `existence::existing` reads the identity with `Context::get`, which is
//! `ess_compiler::ir::read_input` and follows `.`-separated members; each step list runs on a
//! fresh store, on the kernel and on the pinned interpreter, and every answer and the rows after
//! them must agree.

use std::collections::BTreeMap;
use std::path::Path;

use ess_compiler::refs::{CommandRef, ViewRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{
    ConformanceTarget, Deadline, ScenarioContext, SemanticCommandRequest, SemanticViewRequest,
};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Map, Value, json};
use spindle_kernel::{MemoryStore, NoExternals, Store, execute};

const ITEM: &str = "stock.items.Item";
const ITEMS: &str = "stock.items.Items";
const ADD_ITEM: &str = "stock.items.AddItem";
const ADD_WRAPPED: &str = "stock.items.AddWrapped";
const RETIRE_ITEM: &str = "stock.items.RetireItem";
const REMOVE_ITEM: &str = "stock.items.RemoveItem";

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary-existing-instance"),
    )
    .unwrap_or_else(|error| panic!("the adversary-existing-instance fixture compiles: {error}"))
    .ir
}

/// One step's answer: outcome, declared error and its fields, events by name and payload; `Err`
/// with the refusal's text where no answer was given.
type Answer = Result<
    (
        String,
        Option<(String, Map<String, Value>)>,
        Vec<(String, Map<String, Value>)>,
    ),
    String,
>;

type Rows = Vec<Map<String, Value>>;

fn object(value: Value) -> Map<String, Value> {
    let Value::Object(object) = value else {
        panic!("an object, not {value}")
    };
    object
}

fn json_of(node: &Node) -> Value {
    serde_json::to_value(node).expect("a node is JSON")
}

fn node_of(value: &Value) -> Node {
    serde_json::from_value(value.clone()).expect("a JSON value is a node")
}

fn present(row: Map<String, Value>) -> Map<String, Value> {
    row.into_iter()
        .filter(|(_, value)| !value.is_null())
        .collect()
}

fn adding(id: &str, label: &str) -> (&'static str, Value) {
    (ADD_ITEM, json!({"id": id, "label": label}))
}

fn wrapped(id: &str, label: &str) -> (&'static str, Value) {
    (ADD_WRAPPED, json!({"wrap": {"id": id}, "label": label}))
}

fn interpreted(ir: &spindle_spec::EssIr, steps: &[(&str, Value)]) -> (Vec<Answer>, Rows) {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "stock.items/authored/adversary-existing-instance"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("adversary-existing-instance").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let mut answers = Vec::new();
    for (command, input) in steps {
        let input = object(input.clone())
            .iter()
            .map(|(field, value)| (field.clone(), node_of(value)))
            .collect();
        let executed = target.execute_command(SemanticCommandRequest {
            command: CommandRef::new(command.parse().expect("a command name")),
            actor: None,
            caller: None,
            input,
            correlation: context.correlation.clone(),
        });
        answers.push(
            executed
                .map_err(|error| format!("{error:?}"))
                .map(|executed| {
                    (
                        executed
                            .outcome
                            .map(|outcome| outcome.outcome.as_str().to_owned())
                            .unwrap_or_default(),
                        executed.error.map(|error| {
                            (
                                error.error.to_string(),
                                error
                                    .fields
                                    .iter()
                                    .map(|(field, node)| (field.clone(), json_of(node)))
                                    .collect(),
                            )
                        }),
                        executed
                            .direct_events
                            .iter()
                            .map(|event| {
                                (
                                    event.event.to_string(),
                                    event
                                        .payload
                                        .iter()
                                        .map(|(field, node)| (field.clone(), json_of(node)))
                                        .collect(),
                                )
                            })
                            .collect(),
                    )
                }),
        );
    }
    let rows = target
        .query_view(SemanticViewRequest {
            view: ViewRef::new(ITEMS.parse().expect("a view name")),
            params: BTreeMap::new(),
            consistency: ess_primitives::consistency::QueryConsistency::Current,
            correlation: context.correlation.clone(),
            deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
        })
        .expect("the interpreter reads the view")
        .rows
        .into_iter()
        .map(|row| {
            present(
                row.iter()
                    .map(|(field, node)| (field.clone(), json_of(node)))
                    .collect(),
            )
        })
        .collect();
    (answers, rows)
}

fn spindled(ir: &spindle_spec::EssIr, steps: &[(&str, Value)]) -> (Vec<Answer>, Rows) {
    let mut store = MemoryStore::default();
    let mut answers = Vec::new();
    for (command, input) in steps {
        let executed = execute(
            ir,
            &mut store,
            &mut NoExternals,
            command,
            &object(input.clone()),
        );
        answers.push(
            executed
                .map_err(|error| format!("{error:?}"))
                .map(|executed| {
                    (
                        executed.outcome,
                        executed.error.map(|error| (error.name, error.fields)),
                        executed
                            .events
                            .into_iter()
                            .map(|event| (event.name, event.payload))
                            .collect(),
                    )
                }),
        );
    }
    (answers, rows(&mut store))
}

fn rows(store: &mut MemoryStore) -> Rows {
    store
        .scan(ITEM)
        .expect("the store scans")
        .into_iter()
        .map(|(identity, stored)| {
            let mut row: Map<String, Value> = stored.row.fields.into_iter().collect();
            row.insert("id".to_owned(), identity);
            row.insert("state".to_owned(), json!(stored.row.state));
            present(row)
        })
        .collect()
}

/// Runs `steps` on both, each on a fresh store; holds every answer equal (both refusing counts as
/// equal, whatever each says) and the rows after them equal. The kernel's answers.
fn agrees(what: &str, steps: &[(&str, Value)]) -> Vec<Answer> {
    let ir = ir();
    let (ess_answers, ess_rows) = interpreted(&ir, steps);
    let (answers, rows) = spindled(&ir, steps);
    for (step, ((command, input), (ours, theirs))) in steps
        .iter()
        .zip(answers.iter().zip(&ess_answers))
        .enumerate()
    {
        if ours.is_err() && theirs.is_err() {
            continue;
        }
        assert_eq!(
            ours, theirs,
            "{what}: step {step} {command} {input}: the kernel's answer (left) is the \
             interpreter's (right)"
        );
    }
    assert_eq!(rows, ess_rows, "{what}: the stored rows agree");
    answers
}

/// The creation reads its identity from `input.wrap.id`. ESS's `existing` reads it through
/// `read_input` and answers the duplicate; the kernel's `Run::existing` reads
/// `self.input.get("wrap.id")`, finds nothing, skips the branch, and the creation meets the held
/// row.
#[test]
fn a_held_identity_named_by_a_nested_input_member_answers_the_duplicate() {
    let answers = agrees(
        "a held identity added again through a nested input member",
        &[adding("item-1", "bolt"), wrapped("item-1", "nut")],
    );
    let (outcome, error, events) = answers[1].clone().expect("the kernel answers");
    assert_eq!(outcome, "duplicate");
    assert_eq!(
        error,
        Some((
            "stock.items.ItemExists".to_owned(),
            object(json!({"id": "item-1"}))
        ))
    );
    assert_eq!(events, Vec::new());
}

#[test]
fn a_new_identity_named_by_a_nested_input_member_is_added() {
    let answers = agrees(
        "a new identity added through a nested input member",
        &[adding("item-1", "bolt"), wrapped("item-2", "nut")],
    );
    let (outcome, ..) = answers[1].clone().expect("the kernel answers");
    assert_eq!(outcome, "added");
}

#[test]
fn an_item_held_in_its_terminal_state_answers_the_duplicate() {
    let answers = agrees(
        "an item retired, then added again",
        &[
            adding("item-1", "bolt"),
            (RETIRE_ITEM, json!({"id": "item-1"})),
            adding("item-1", "nut"),
        ],
    );
    let (outcome, ..) = answers[2].clone().expect("the kernel answers");
    assert_eq!(outcome, "duplicate");
}

#[test]
fn a_removed_item_is_added_again() {
    let answers = agrees(
        "an item removed, then added again",
        &[
            adding("item-1", "bolt"),
            (REMOVE_ITEM, json!({"id": "item-1"})),
            adding("item-1", "nut"),
        ],
    );
    let (outcome, ..) = answers[2].clone().expect("the kernel answers");
    assert_eq!(outcome, "added");
}
