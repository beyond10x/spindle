//! A creation takes an absent addressed row beside `unknown_instance:`, outside row sets
//! (`story:kernel-creation-takes-unknown-row`), as ESS's interpreter answers it
//! (`interpret::execute::addressed_unknown`, `unknown_instance`): the creation branch may take the
//! row, the `unknown_instance:` branch answers only where no creation is selected, and a command
//! declaring none answers with its one declared not-found refusal.
//!
//! The fixture `tests/fixtures/creation-takes-unknown-row` declares lockers and two commands that
//! create a locker where `allow` holds and relabel it otherwise: `Open`, beside `unknown_instance:`,
//! and `Register`, beside an `external:` refusal whose error carries the locker's identity type,
//! sourced from the input, so neither side leaves it to the implementation.
//! Each step list runs on a fresh store, on the kernel and on the pinned interpreter, whose
//! externals are withheld; the kernel's decider answers every external question `false` alike.

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

const LOCKER: &str = "depot.lockers.Locker";
const LOCKERS: &str = "depot.lockers.Lockers";
const OPEN: &str = "depot.lockers.Open";
const REGISTER: &str = "depot.lockers.Register";
const ABSENT: &str = "locker-7";

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/creation-takes-unknown-row"),
    )
    .expect("the creation-takes-unknown-row fixture compiles")
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

fn opening(allow: bool) -> Vec<(&'static str, Value)> {
    vec![(
        OPEN,
        json!({"locker": ABSENT, "label": "north", "allow": allow}),
    )]
}

fn registering() -> Vec<(&'static str, Value)> {
    vec![(
        REGISTER,
        json!({"locker": ABSENT, "label": "north", "allow": false}),
    )]
}

fn interpreted(ir: &spindle_spec::EssIr, steps: &[(&str, Value)]) -> (Vec<Answer>, Rows) {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "depot.lockers/authored/creation-takes-unknown-row"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("creation-takes-unknown-row").expect("a correlation id"),
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
            view: ViewRef::new(LOCKERS.parse().expect("a view name")),
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
        .scan(LOCKER)
        .expect("the store scans")
        .into_iter()
        .map(|(identity, stored)| {
            let mut row: Map<String, Value> = stored.row.fields.into_iter().collect();
            row.insert("locker".to_owned(), identity);
            row.insert("state".to_owned(), json!(stored.row.state));
            present(row)
        })
        .collect();
    (answers, rows)
}

/// The kernel's answer to the one step of `steps`, and the rows after it.
fn answered(steps: &[(&str, Value)]) -> (Answer, Rows) {
    let (mut answers, rows) = spindled(&ir(), steps);
    (answers.remove(0), rows)
}

#[test]
fn a_creation_takes_an_absent_row_beside_unknown_instance() {
    let (answer, rows) = answered(&opening(true));
    let (outcome, error, events) = answer.expect("the kernel answers");
    assert_eq!(outcome, "opened");
    assert_eq!(error, None);
    assert_eq!(
        events,
        vec![(
            "depot.lockers.LockerOpened".to_owned(),
            object(json!({"locker": ABSENT}))
        )]
    );
    assert_eq!(
        rows,
        vec![object(
            json!({"locker": ABSENT, "label": "north", "state": "Open"})
        )]
    );
}

#[test]
fn an_absent_row_no_creation_takes_answers_unknown_instance() {
    let (answer, rows) = answered(&opening(false));
    let (outcome, error, events) = answer.expect("the kernel answers");
    assert_eq!(outcome, "unknown-locker");
    assert_eq!(
        error,
        Some(("depot.lockers.UnknownLocker".to_owned(), Map::new()))
    );
    assert_eq!(events, Vec::new());
    assert_eq!(rows, Rows::new());
}

#[test]
fn an_absent_row_without_unknown_instance_answers_the_declared_not_found() {
    let (answer, rows) = answered(&registering());
    let (outcome, error, events) = answer.expect("the kernel answers");
    assert_eq!(outcome, "not-found");
    assert_eq!(
        error,
        Some((
            "depot.lockers.LockerNotFound".to_owned(),
            object(json!({"locker": ABSENT}))
        ))
    );
    assert_eq!(events, Vec::new());
    assert_eq!(rows, Rows::new());
}

#[test]
fn addressed_unknown_agrees_with_the_interpreter() {
    let ir = ir();
    for (what, steps) in [
        ("a creation takes the absent row", opening(true)),
        ("no creation takes the absent row", opening(false)),
        (
            "no `unknown_instance:`, one declared not-found",
            registering(),
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
