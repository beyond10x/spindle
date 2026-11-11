//! How an upsert's creation answers an `exists: false` refusal over a row the input names
//! (`story:upsert-creation-related-refusal-first`): as ESS 0.92.0's interpreter answers it. From
//! `ess/27` ESS reads such a branch at the related-row step (`PresentRelated`), after the
//! existence step where `unknown_instance:` takes an absent row, so an upsert that creates is
//! answered by its creation, `recorded`, while a plain creation and an upsert that refreshes an
//! existing row are refused `no-box`. The story expected `no-box` for all three; the
//! interpreter, of 0.90.0 and 0.92.0, answers `recorded` for the upsert creation.
//!
//! The fixture `tests/fixtures/upsert-creation-related-refusal` declares boxes, notes recorded
//! under a key by one command that creates (`unknown_instance: true`) or refreshes the note, and
//! tickets opened under a minted identity; both note and ticket commands refuse first where no box
//! carries `input.box_id`. Each step list runs on a fresh store, on the kernel and on the pinned
//! interpreter.

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
use spindle_kernel::{External, MemoryStore, Store, execute};

const NOTE: &str = "store.shelf.Note";
const NOTES: &str = "store.shelf.Notes";
const PLACE_BOX: &str = "store.shelf.PlaceBox";
const RECORD_NOTE: &str = "store.shelf.RecordNote";
const OPEN_TICKET: &str = "store.shelf.OpenTicket";
const BOX: &str = "00000000-0000-4000-8000-000000000001";
const NO_BOX: &str = "00000000-0000-4000-8000-0000000000aa";

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/upsert-creation-related-refusal"),
    )
    .expect("the upsert-creation-related-refusal fixture compiles")
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

fn place_box() -> (&'static str, Value) {
    (PLACE_BOX, json!({"box_id": BOX, "label": "b"}))
}

fn record(key: &str, box_id: &str, text: &str) -> (&'static str, Value) {
    (
        RECORD_NOTE,
        json!({"key": key, "box_id": box_id, "text": text}),
    )
}

/// The plain creation, with no box.
fn plain_creation() -> Vec<(&'static str, Value)> {
    vec![(OPEN_TICKET, json!({"box_id": NO_BOX}))]
}

/// The upsert's creation, no note under the key, with no box.
fn upsert_creation() -> Vec<(&'static str, Value)> {
    vec![record("n1", NO_BOX, "t")]
}

/// The upsert's refresh: a note held under the key, onto a box that exists; then the same key
/// onto a box that does not, then onto the box again.
fn upsert_refresh() -> Vec<(&'static str, Value)> {
    vec![
        place_box(),
        record("n1", BOX, "t"),
        record("n1", NO_BOX, "t2"),
        record("n1", BOX, "t3"),
    ]
}

/// The upsert's creation onto a box that exists, beside a refused creation of another key.
fn upsert_creation_with_box() -> Vec<(&'static str, Value)> {
    vec![
        place_box(),
        record("n2", NO_BOX, "t"),
        record("n2", BOX, "t"),
    ]
}

fn interpreted(ir: &spindle_spec::EssIr, steps: &[(&str, Value)]) -> (Vec<Answer>, Rows) {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "store.shelf/authored/upsert-creation-related-refusal"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("upsert-creation-related-refusal").expect("a correlation id"),
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
            view: ViewRef::new(NOTES.parse().expect("a view name")),
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
    let mut withheld = |_: &External<'_>| -> Result<bool, String> { Ok(false) };
    let mut answers = Vec::new();
    for (command, input) in steps {
        let before = store.events().len();
        let executed = execute(
            ir,
            &mut store,
            &mut withheld,
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
    let rows = store
        .scan(NOTE)
        .expect("the store scans")
        .into_iter()
        .map(|(identity, stored)| {
            let mut row: Map<String, Value> = stored.row.fields.into_iter().collect();
            row.insert("key".to_owned(), identity);
            row.insert("state".to_owned(), json!(stored.row.state));
            present(row)
        })
        .collect();
    (answers, rows)
}

/// The refusal the three requests answer: `no-box`, its declared error, nothing emitted.
fn refused(answer: &Answer, what: &str) {
    let (outcome, error, events) = answer.as_ref().expect("the kernel answers");
    assert_eq!(outcome, "no-box", "{what}");
    assert_eq!(
        error,
        &Some(("store.shelf.BoxNotFound".to_owned(), Map::new())),
        "{what}"
    );
    assert_eq!(events, &Vec::new(), "{what}");
}

#[test]
fn a_plain_creation_naming_a_missing_box_is_refused() {
    let (answers, _) = spindled(&ir(), &plain_creation());
    refused(&answers[0], "plain creation, no box");
}

#[test]
fn an_upsert_creation_naming_a_missing_box_is_recorded_as_the_interpreter_records_it() {
    let (answers, rows) = spindled(&ir(), &upsert_creation());
    assert_eq!(
        answers[0].as_ref().map(|(outcome, _, _)| outcome.as_str()),
        Ok("recorded"),
        "upsert creation, no box"
    );
    assert_eq!(rows.len(), 1, "the creation writes its note: {rows:?}");
}

#[test]
fn an_upsert_refresh_naming_a_missing_box_is_refused() {
    let (answers, rows) = spindled(&ir(), &upsert_refresh());
    assert_eq!(
        answers[1].as_ref().map(|(outcome, _, _)| outcome.as_str()),
        Ok("recorded")
    );
    refused(&answers[2], "upsert refresh, no box");
    assert_eq!(
        answers[3].as_ref().map(|(outcome, _, _)| outcome.as_str()),
        Ok("refreshed")
    );
    assert_eq!(
        rows,
        vec![object(
            json!({"key": "n1", "box_id": BOX, "text": "t3", "state": "Kept"})
        )]
    );
}

#[test]
fn an_upsert_creation_related_refusal_agrees_with_the_interpreter() {
    let ir = ir();
    for (what, steps) in [
        ("plain creation, no box", plain_creation()),
        ("upsert creation, no box", upsert_creation()),
        ("upsert refresh, no box", upsert_refresh()),
        (
            "upsert creation, then with the box",
            upsert_creation_with_box(),
        ),
    ] {
        let (ess_answers, ess_rows) = interpreted(&ir, &steps);
        let (answers, rows) = spindled(&ir, &steps);
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
    }
}
