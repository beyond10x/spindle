//! An `existing_instance:` outcome is selected as ESS's interpreter selects it
//! (`story:kernel-existing-instance`, `interpret::execute::existence::existing`): where the plan
//! places the branch, a creation whose identity the input names and a row already carries answers
//! the branch's declared error and writes nothing; otherwise selection goes on and the creation
//! takes the identity.
//!
//! The fixture `tests/fixtures/existing-instance` declares items and two commands. `AddItem`
//! creates an item under the input's `id` beside an `existing_instance:` duplicate. `AddCoded`
//! creates it under `id` or `code`, as its input guard selects: creations of different addresses,
//! which ESS reads only by selecting the accepting branch first, and the kernel refuses by name.
//! The fixture `tests/fixtures/existing-instance-related` is in `ess/26`, before the one rule, and
//! adds `StowItem`, which also reads the shelf the input names: there the plan places
//! `existing_instance:` at step 1, ahead of the missing shelf's `exists: false` branch.
//! Each step list runs on a fresh store, on the kernel and on the pinned interpreter.

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
use spindle_kernel::{ExecuteError, MemoryStore, NoExternals, Store, execute};

const ITEM: &str = "stock.items.Item";
const ITEMS: &str = "stock.items.Items";
const ADD_ITEM: &str = "stock.items.AddItem";
const ADD_CODED: &str = "stock.items.AddCoded";
const STOW_ITEM: &str = "stock.items.StowItem";

fn fixture(name: &str) -> spindle_spec::EssIr {
    spindle_spec::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap_or_else(|error| panic!("the {name} fixture compiles: {error}"))
    .ir
}

fn ir() -> spindle_spec::EssIr {
    fixture("existing-instance")
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
    serde_json::from_value(value.clone()).expect("a JSON scalar is a node")
}

fn present(row: Map<String, Value>) -> Map<String, Value> {
    row.into_iter()
        .filter(|(_, value)| !value.is_null())
        .collect()
}

fn adding(id: &str, label: &str) -> (&'static str, Value) {
    (ADD_ITEM, json!({"id": id, "label": label}))
}

fn interpreted(ir: &spindle_spec::EssIr, steps: &[(&str, Value)]) -> (Vec<Answer>, Rows) {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "stock.items/authored/existing-instance"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("existing-instance").expect("a correlation id"),
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

#[test]
fn a_command_declaring_existing_instance_is_served() {
    let ir = ir();
    let spec = ir
        .commands()
        .iter()
        .find_map(|(name, spec)| (name.to_string() == ADD_ITEM).then_some(spec))
        .expect("the fixture declares AddItem");
    spindle_kernel::runs(&ir, spec).expect("every outcome of AddItem runs");
}

#[test]
fn a_first_add_creates_the_item() {
    let (mut answers, rows) = spindled(&ir(), &[adding("item-1", "bolt")]);
    let (outcome, error, events) = answers.remove(0).expect("the kernel answers");
    assert_eq!(outcome, "added");
    assert_eq!(error, None);
    assert_eq!(
        events,
        vec![(
            "stock.items.ItemAdded".to_owned(),
            object(json!({"id": "item-1"}))
        )]
    );
    assert_eq!(
        rows,
        vec![object(
            json!({"id": "item-1", "label": "bolt", "state": "Listed"})
        )]
    );
}

#[test]
fn a_second_add_of_the_same_identity_answers_the_duplicate_and_writes_nothing() {
    let ir = ir();
    let mut store = MemoryStore::default();
    let first = object(json!({"id": "item-1", "label": "bolt"}));
    execute(&ir, &mut store, &mut NoExternals, ADD_ITEM, &first).expect("the first add runs");
    let events = store.events().len();
    let held = rows(&mut store);
    let second = object(json!({"id": "item-1", "label": "nut"}));
    let executed = execute(&ir, &mut store, &mut NoExternals, ADD_ITEM, &second)
        .expect("the kernel answers the second add");
    assert_eq!(executed.outcome, "duplicate");
    assert_eq!(
        executed.error.map(|error| (error.name, error.fields)),
        Some((
            "stock.items.ItemExists".to_owned(),
            object(json!({"id": "item-1"}))
        ))
    );
    assert_eq!(executed.events, Vec::new());
    assert_eq!(store.events().len(), events, "the duplicate emits nothing");
    assert_eq!(rows(&mut store), held, "the duplicate changes no row");
}

#[test]
fn existing_instance_agrees_with_the_interpreter() {
    let ir = ir();
    for (what, steps) in [
        ("a first add", vec![adding("item-1", "bolt")]),
        (
            "a second add of the same identity",
            vec![adding("item-1", "bolt"), adding("item-1", "nut")],
        ),
        (
            "a second add of another identity",
            vec![adding("item-1", "bolt"), adding("item-2", "nut")],
        ),
    ] {
        agrees(&ir, what, &steps);
    }
}

/// Runs `steps` on the kernel and on the interpreter, each on a fresh store, and holds every
/// answer and the rows after them equal; the kernel's answers.
fn agrees(ir: &spindle_spec::EssIr, what: &str, steps: &[(&str, Value)]) -> Vec<Answer> {
    let (ess_answers, ess_rows) = interpreted(ir, steps);
    let (answers, rows) = spindled(ir, steps);
    for (step, ((command, input), (ours, theirs))) in steps
        .iter()
        .zip(answers.iter().zip(&ess_answers))
        .enumerate()
    {
        assert_eq!(
            ours, theirs,
            "{what}: step {step} {command} {input}: the kernel's answer (left) is the \
             interpreter's (right)"
        );
    }
    assert_eq!(rows, ess_rows, "{what}: the stored rows agree");
    answers
}

#[test]
fn before_the_one_rule_existing_instance_answers_ahead_of_a_missing_related_row() {
    let ir = fixture("existing-instance-related");
    let stowing = |id: &str| {
        (
            STOW_ITEM,
            json!({"id": id, "shelf": "shelf-9", "label": "nut"}),
        )
    };
    let answers = agrees(
        &ir,
        "a held identity stowed onto a missing shelf",
        &[adding("item-1", "bolt"), stowing("item-1")],
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
    let answers = agrees(
        &ir,
        "a new identity stowed onto a missing shelf",
        &[adding("item-1", "bolt"), stowing("item-2")],
    );
    let (outcome, ..) = answers[1].clone().expect("the kernel answers");
    assert_eq!(outcome, "no-shelf");
}

#[test]
fn existing_instance_over_creations_of_different_addresses_is_refused_by_name() {
    let ir = ir();
    let spec = ir
        .commands()
        .iter()
        .find_map(|(name, spec)| (name.to_string() == ADD_CODED).then_some(spec))
        .expect("the fixture declares AddCoded");
    let Err(ExecuteError::Unsupported { construct }) = spindle_kernel::selects(spec) else {
        panic!("AddCoded is refused for every request");
    };
    assert!(
        construct.contains("`existing_instance:`") && construct.contains("different"),
        "{construct}"
    );
    let mut store = MemoryStore::default();
    let input = object(json!({"id": "item-1", "code": "c-1", "label": "bolt", "by_code": false}));
    let refused = execute(&ir, &mut store, &mut NoExternals, ADD_CODED, &input);
    assert!(
        matches!(refused, Err(ExecuteError::Unsupported { .. })),
        "{refused:?}"
    );
    assert!(
        store.events().is_empty(),
        "a refused command writes nothing"
    );
}
