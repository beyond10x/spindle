//! Adversary cases for values read from a related row (`{related: {via: input.<field>, …}}`),
//! each run step for step against the pinned ESS interpreter (`interpret::execute`), which is the
//! authority: outcome, declared error with its fields, events with their payloads, and the rows of
//! every view after the last step must agree, and a step the interpreter cannot answer must be
//! one the kernel cannot answer either.
//!
//! The fixture `tests/fixtures/adversary-related-values` declares shelves in aisles and three
//! commands that read them through an input: `NoteShelf` (no existence guard; reads the shelf's
//! identity, an `Optional` field and a struct of the aisle and state), `Transfer` (two inputs that
//! may name one shelf) and `PickFrom` (a `when_related:` refusal whose error carries the related
//! aisle and state). `CloseShelf`'s `wrong_state:` error carries the same.

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

const SHELF: &str = "depot.racks.Shelf";
const NOTE: &str = "depot.racks.Note";
const MOVE: &str = "depot.racks.Move";
const OPEN: &str = "depot.racks.OpenShelf";
const CLOSE: &str = "depot.racks.CloseShelf";
const NOTE_SHELF: &str = "depot.racks.NoteShelf";
const TRANSFER: &str = "depot.racks.Transfer";
const PICK_FROM: &str = "depot.racks.PickFrom";

const NORTH: &str = "2b3c4d5e-0000-4000-8000-000000000001";
const SOUTH: &str = "2b3c4d5e-0000-4000-8000-000000000002";
const NOWHERE: &str = "2b3c4d5e-0000-4000-8000-0000000000ff";

fn note(n: u8) -> String {
    format!("6f7a8b9c-0000-4000-8000-0000000000{n:02x}")
}

fn moved(n: u8) -> String {
    format!("3d4e5f6a-0000-4000-8000-0000000000{n:02x}")
}

const VIEWS: [(&str, &str, &str); 3] = [
    ("depot.racks.ShelfCard", SHELF, "shelf_id"),
    ("depot.racks.NoteCard", NOTE, "note_id"),
    ("depot.racks.MoveCard", MOVE, "move_id"),
];

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary-related-values"),
    )
    .expect("the adversary-related-values fixture compiles")
    .ir
}

/// One step's answer: outcome, declared error and its fields, events by name and payload; `Err`
/// where no answer was given.
type Answer = Result<
    (
        String,
        Option<(String, Map<String, Value>)>,
        Vec<(String, Map<String, Value>)>,
    ),
    (),
>;

type Rows = Vec<Vec<Map<String, Value>>>;

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
    serde_json::from_value(value.clone()).expect("a JSON scalar is a node")
}

fn present(row: Map<String, Value>) -> Map<String, Value> {
    row.into_iter()
        .filter(|(_, value)| !value.is_null())
        .collect()
}

fn interpreted(ir: &spindle_spec::EssIr, steps: &[(&str, Value)]) -> (Vec<Answer>, Rows) {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "depot.racks/authored/adversary-related-values"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("adversary-related-values").expect("a correlation id"),
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
        answers.push(executed.map_err(|_| ()).map(|executed| {
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
        }));
    }
    let rows = VIEWS
        .iter()
        .map(|(view, _, _)| {
            target
                .query_view(SemanticViewRequest {
                    view: ViewRef::new(view.parse().expect("a view name")),
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
                .collect()
        })
        .collect();
    (answers, rows)
}

fn spindled(ir: &spindle_spec::EssIr, steps: &[(&str, Value)]) -> (Vec<Answer>, Rows) {
    let mut store = MemoryStore::default();
    let mut answers = Vec::new();
    for (command, input) in steps {
        let before = store.events().len();
        let executed = execute(
            ir,
            &mut store,
            &mut NoExternals,
            command,
            &object(input.clone()),
        );
        if executed.is_err() {
            assert_eq!(
                store.events().len(),
                before,
                "{command} {input}: a step the kernel does not answer writes nothing"
            );
        }
        answers.push(executed.map_err(|_| ()).map(|executed| {
            (
                executed.outcome,
                executed.error.map(|error| (error.name, error.fields)),
                executed
                    .events
                    .into_iter()
                    .map(|event| (event.name, event.payload))
                    .collect(),
            )
        }));
    }
    let rows = VIEWS
        .iter()
        .map(|(_, entity, identity_field)| {
            store
                .scan(entity)
                .expect("the store scans")
                .into_iter()
                .map(|(identity, stored)| {
                    let mut row: Map<String, Value> = stored.row.fields.into_iter().collect();
                    row.insert((*identity_field).to_owned(), identity);
                    row.insert("state".to_owned(), json!(stored.row.state));
                    present(row)
                })
                .collect()
        })
        .collect();
    (answers, rows)
}

fn opened() -> Vec<(&'static str, Value)> {
    vec![
        (
            OPEN,
            json!({"shelf_id": NORTH, "label": "north", "aisle": "A7"}),
        ),
        (
            OPEN,
            json!({"shelf_id": SOUTH, "label": "south", "aisle": "B2"}),
        ),
    ]
}

/// The kernel and the interpreter agree on every step of `steps`, the steps after the two shelves
/// open, and on the rows after the last.
fn agree(what: &str, steps: Vec<(&'static str, Value)>) {
    let ir = ir();
    let mut all = opened();
    all.extend(steps);
    let (ess_answers, ess_rows) = interpreted(&ir, &all);
    let (answers, rows) = spindled(&ir, &all);
    for (step, ((command, input), (ours, theirs))) in
        all.iter().zip(answers.iter().zip(&ess_answers)).enumerate()
    {
        assert_eq!(
            ours, theirs,
            "{what}: step {step} {command} {input}: the kernel's answer (left) is the \
             interpreter's (right)"
        );
    }
    assert_eq!(rows, ess_rows, "{what}: the stored rows agree");
}

#[test]
fn adversary_a_related_identity_optional_field_and_struct_agree_with_the_interpreter() {
    agree(
        "NoteShelf reads the shelf's identity, its unset Optional field and a struct of its aisle \
         and state",
        vec![(NOTE_SHELF, json!({"note_id": note(1), "shelf_id": NORTH}))],
    );
}

#[test]
fn adversary_two_inputs_naming_one_row_agree_with_the_interpreter() {
    agree(
        "Transfer reads two shelves, and one shelf through both inputs",
        vec![
            (
                TRANSFER,
                json!({"move_id": moved(1), "from_shelf": NORTH, "to_shelf": SOUTH}),
            ),
            (
                TRANSFER,
                json!({"move_id": moved(2), "from_shelf": SOUTH, "to_shelf": SOUTH}),
            ),
            (
                TRANSFER,
                json!({"move_id": moved(3), "from_shelf": NOWHERE, "to_shelf": SOUTH}),
            ),
            (
                TRANSFER,
                json!({"move_id": moved(4), "from_shelf": NORTH, "to_shelf": NOWHERE}),
            ),
        ],
    );
}

#[test]
fn adversary_a_related_value_in_a_related_refusal_error_agrees_with_the_interpreter() {
    agree(
        "PickFrom's `shut` error carries the closed shelf's aisle and state",
        vec![
            (PICK_FROM, json!({"note_id": note(2), "shelf_id": SOUTH})),
            (CLOSE, json!({"shelf_id": SOUTH})),
            (PICK_FROM, json!({"note_id": note(3), "shelf_id": SOUTH})),
            (PICK_FROM, json!({"note_id": note(4), "shelf_id": NOWHERE})),
        ],
    );
}

#[test]
fn adversary_a_related_value_in_a_wrong_state_error_agrees_with_the_interpreter() {
    agree(
        "CloseShelf's `wrong-state` error carries the held shelf's aisle and state",
        vec![
            (CLOSE, json!({"shelf_id": NORTH})),
            (CLOSE, json!({"shelf_id": NORTH})),
        ],
    );
}

#[test]
fn adversary_a_related_value_in_the_unknown_instance_wrong_state_error_agrees_with_the_interpreter()
{
    agree(
        "CloseShelf's `wrong-state` refusal for a shelf no row carries reads that row",
        vec![(CLOSE, json!({"shelf_id": NOWHERE}))],
    );
}

#[test]
fn adversary_a_missing_related_row_without_a_guard_agrees_with_the_interpreter() {
    agree(
        "NoteShelf names a shelf no row carries, and no branch guards its existence",
        vec![(NOTE_SHELF, json!({"note_id": note(9), "shelf_id": NOWHERE}))],
    );
}

/// The answers the agreement cases compare, pinned as literals: neither side may agree by
/// answering nothing.
#[test]
fn adversary_the_compared_answers_carry_the_related_values() {
    let ir = ir();
    let mut steps = opened();
    steps.extend([
        (NOTE_SHELF, json!({"note_id": note(1), "shelf_id": NORTH})),
        (
            TRANSFER,
            json!({"move_id": moved(2), "from_shelf": SOUTH, "to_shelf": SOUTH}),
        ),
        (CLOSE, json!({"shelf_id": SOUTH})),
        (PICK_FROM, json!({"note_id": note(3), "shelf_id": SOUTH})),
        (CLOSE, json!({"shelf_id": SOUTH})),
    ]);
    let (answers, _) = spindled(&ir, &steps);
    let answer = |step: usize| answers[step].clone().expect("the kernel answers");
    let (outcome, _, events) = answer(2);
    assert_eq!(outcome, "noted");
    assert_eq!(
        events[0].1,
        object(json!({
            "note_id": note(1),
            "shelf": NORTH,
            "spot": {"aisle": "A7", "state": "Open"}
        }))
    );
    let (outcome, _, events) = answer(3);
    assert_eq!(outcome, "transferred");
    assert_eq!(events[0].1["from"], json!("B2"));
    assert_eq!(events[0].1["to"], json!("B2"));
    for step in [5, 6] {
        let (_, error, _) = answer(step);
        assert_eq!(
            error,
            Some((
                "depot.racks.ShelfShut".to_owned(),
                object(json!({"aisle": "B2", "seen": "Closed"}))
            )),
            "step {step}"
        );
    }
}
