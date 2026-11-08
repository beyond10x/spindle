//! Adversary cases for `story:kernel-creation-takes-unknown-row`: an absent addressed row read
//! beside creations, `unknown_instance:`, a declared not-found refusal, `wrong_state:`, a present
//! related-row refusal and deletes, each step list run on a fresh store on the kernel and on ESS's
//! interpreter at 0.90.0 and compared step by step (outcome, error and its fields, events, rows).
//!
//! The fixture `tests/fixtures/adversary-creation-unknown` declares bays and lockers. The
//! interpreter's externals are withheld; the kernel's decider answers every external question
//! `false` alike.

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
use spindle_kernel::{ExecuteError, External, MemoryStore, Store, execute};

const LOCKER: &str = "depot.hall.Locker";
const LOCKERS: &str = "depot.hall.Lockers";
const ABSENT: &str = "locker-7";

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary-creation-unknown"),
    )
    .expect("the adversary-creation-unknown fixture compiles")
    .ir
}

/// One step's answer: outcome, declared error and its fields, events by name and payload; `Err`
/// where no outcome was given: [`UNDECLARED`] for the kernel's `Undeclared` and the
/// interpreter's undeclared step (no outcome, error or event), [`UNANSWERED`] otherwise.
type Answer = Result<
    (
        String,
        Option<(String, Map<String, Value>)>,
        Vec<(String, Map<String, Value>)>,
    ),
    &'static str,
>;

const UNDECLARED: &str = "undeclared";
const UNANSWERED: &str = "unanswered";

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

fn interpreted(ir: &spindle_spec::EssIr, steps: &[(&str, Value)]) -> (Vec<Answer>, Rows) {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "depot.hall/authored/adversary-creation-unknown"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("adversary-creation-unknown").expect("a correlation id"),
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
        answers.push(executed.map_err(|_| UNANSWERED).and_then(|executed| {
            if executed.outcome.is_none()
                && executed.error.is_none()
                && executed.direct_events.is_empty()
            {
                return Err(UNDECLARED);
            }
            Ok((
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
            ))
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
        answers.push(
            executed
                .map_err(|error| match error {
                    ExecuteError::Undeclared { .. } => UNDECLARED,
                    _ => UNANSWERED,
                })
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

/// Runs `steps` on a fresh store on both sides and asserts each answer and the rows equal.
fn agrees(what: &str, steps: &[(&str, Value)]) {
    let ir = ir();
    let (ess_answers, ess_rows) = interpreted(&ir, steps);
    let (answers, rows) = spindled(&ir, steps);
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
    assert_eq!(rows, ess_rows, "{what}: the stored lockers agree");
}

/// No creation, no `unknown_instance:`, one declared not-found refusal and `wrong_state:`: ESS's
/// `unknown_instance` answers the declared not-found before `wrong_state:` (expected `not-found`
/// with `{locker: "locker-7"}`).
#[test]
fn an_absent_row_without_a_creation_answers_the_declared_not_found_before_wrong_state() {
    agrees(
        "Shut of an absent locker",
        &[("depot.hall.Shut", json!({"locker": ABSENT}))],
    );
}

/// No creation, no `unknown_instance:`, no declared not-found: `wrong_state:` answers.
#[test]
fn an_absent_row_with_wrong_state_and_no_not_found_answers_wrong_state() {
    agrees(
        "Seal of an absent locker",
        &[("depot.hall.Seal", json!({"locker": ABSENT}))],
    );
}

/// Two declared not-found candidates: neither is the answer.
#[test]
fn two_declared_not_found_candidates_answer_as_the_interpreter() {
    agrees(
        "Claim of an absent locker, the creation's guard failing",
        &[(
            "depot.hall.Claim",
            json!({"locker": ABSENT, "label": "north", "allow": false}),
        )],
    );
}

/// A creation whose identity comes from another input field does not take the addressed row.
#[test]
fn a_creation_naming_another_field_leaves_the_absent_row_unknown() {
    agrees(
        "Adopt of an absent locker, the creation reading `spare`",
        &[(
            "depot.hall.Adopt",
            json!({"locker": ABSENT, "spare": "locker-9", "label": "north", "allow": true}),
        )],
    );
}

/// Several creation branches, the second taking the absent row; then the update takes the present
/// row.
#[test]
fn several_creations_and_a_present_row_answer_as_the_interpreter() {
    agrees(
        "Open by the second creation, then relabelled on the present row",
        &[
            (
                "depot.hall.Open",
                json!({"locker": ABSENT, "label": "north", "allow": false, "quiet": true}),
            ),
            (
                "depot.hall.Open",
                json!({"locker": ABSENT, "label": "east", "allow": false, "quiet": false}),
            ),
        ],
    );
}

/// A creation selected on a present row creates nothing twice: neither side gives an outcome for
/// it (the interpreter refuses the step, the kernel answers it undeclared), and one row rests,
/// as the interpreter keeps it.
#[test]
fn a_creation_selected_on_a_present_row_creates_nothing_twice() {
    let steps = [
        (
            "depot.hall.Open",
            json!({"locker": ABSENT, "label": "north", "allow": false, "quiet": true}),
        ),
        (
            "depot.hall.Open",
            json!({"locker": ABSENT, "label": "south", "allow": true, "quiet": false}),
        ),
    ];
    let ir = ir();
    let (ess_answers, ess_rows) = interpreted(&ir, &steps);
    let (answers, rows) = spindled(&ir, &steps);
    assert_eq!(answers[0], ess_answers[0], "the first creation agrees");
    assert!(
        ess_answers[1].is_err(),
        "the interpreter gives no outcome for a creation on a present row: {:?}",
        ess_answers[1]
    );
    assert!(
        answers[1].is_err(),
        "the kernel gives no outcome for a creation on a present row: {:?}",
        answers[1]
    );
    assert_eq!(ess_rows.len(), 1, "the interpreter keeps one locker");
    assert_eq!(rows, ess_rows, "the stored lockers agree");
}

/// An absent locker beside a present-related refusal that holds (the bay is shut): ESS reads the
/// branch the request selects without that refusal (`relabelled`, acting on the absent row) and
/// answers `unknown_instance:` first (`selected_subject_refusal`), expected `unknown-locker`.
#[test]
fn an_absent_row_beside_a_holding_related_refusal_answers_unknown_instance_first() {
    agrees(
        "Assign of an absent locker in a shut bay, the creation's guard failing",
        &[
            ("depot.hall.OpenBay", json!({"bay": "bay-1"})),
            ("depot.hall.ShutBay", json!({"bay": "bay-1"})),
            (
                "depot.hall.Assign",
                json!({"locker": ABSENT, "bay": "bay-1", "label": "north", "allow": false}),
            ),
        ],
    );
}

/// The same command, where the creation is selected: the related refusal answers.
#[test]
fn an_absent_row_a_creation_takes_beside_a_holding_related_refusal_answers_it() {
    agrees(
        "Assign of an absent locker in a shut bay, the creation's guard holding",
        &[
            ("depot.hall.OpenBay", json!({"bay": "bay-1"})),
            ("depot.hall.ShutBay", json!({"bay": "bay-1"})),
            (
                "depot.hall.Assign",
                json!({"locker": ABSENT, "bay": "bay-1", "label": "north", "allow": true}),
            ),
        ],
    );
}

/// A delete with no creation and no `unknown_instance:`, beside one declared not-found refusal:
/// ESS's deleting branch answers the absent row with it (expected `not-found`).
#[test]
fn a_delete_of_an_absent_row_answers_the_declared_not_found() {
    agrees(
        "Discard of an absent locker",
        &[("depot.hall.Discard", json!({"locker": ABSENT}))],
    );
}

/// A delete beside a creation whose guard fails, and one declared not-found refusal.
#[test]
fn a_delete_beside_a_creation_answers_the_declared_not_found() {
    agrees(
        "Recycle of an absent locker, the creation's guard failing",
        &[(
            "depot.hall.Recycle",
            json!({"locker": ABSENT, "label": "north", "allow": false}),
        )],
    );
}

/// A creation guarded by a related row rather than the input: an open bay takes the absent row,
/// a shut one leaves it to the update, which answers `unknown_instance:`, and a missing bay
/// answers `exists: false`.
#[test]
fn a_creation_guarded_by_a_related_row_answers_as_the_interpreter() {
    agrees(
        "Place of an absent locker in an open bay",
        &[
            ("depot.hall.OpenBay", json!({"bay": "bay-1"})),
            (
                "depot.hall.Place",
                json!({"locker": ABSENT, "bay": "bay-1", "label": "north"}),
            ),
        ],
    );
    agrees(
        "Place of an absent locker in a shut bay",
        &[
            ("depot.hall.OpenBay", json!({"bay": "bay-1"})),
            ("depot.hall.ShutBay", json!({"bay": "bay-1"})),
            (
                "depot.hall.Place",
                json!({"locker": ABSENT, "bay": "bay-1", "label": "north"}),
            ),
        ],
    );
    agrees(
        "Place of an absent locker in a missing bay",
        &[(
            "depot.hall.Place",
            json!({"locker": ABSENT, "bay": "bay-9", "label": "north"}),
        )],
    );
}

/// The declared not-found refusal whose error field no payload sources: its field is filled as
/// the interpreter fills it.
#[test]
fn a_declared_not_found_with_an_unsourced_field_answers_as_the_interpreter() {
    agrees(
        "Enrol of an absent locker, the creation's guard failing",
        &[(
            "depot.hall.Enrol",
            json!({"locker": ABSENT, "label": "north", "allow": false}),
        )],
    );
}

/// A subject-less `external:` refusal beside a creation on an absent row, the decider declining:
/// the creation takes the row, or, its guard failing, `unknown_instance:` answers.
#[test]
fn an_external_refusal_beside_a_creation_on_an_absent_row_answers_as_the_interpreter() {
    agrees(
        "Screen of an absent locker, the creation's guard holding",
        &[(
            "depot.hall.Screen",
            json!({"locker": ABSENT, "label": "north", "allow": true}),
        )],
    );
    agrees(
        "Screen of an absent locker, the creation's guard failing",
        &[(
            "depot.hall.Screen",
            json!({"locker": ABSENT, "label": "north", "allow": false}),
        )],
    );
}

/// A creation beside a move and `wrong_state:`, no `unknown_instance:` and no declared not-found:
/// the move on an absent row answers `wrong_state:`; the creation takes it; the move then shuts
/// the present row, and once more answers `wrong_state:`.
#[test]
fn a_creation_beside_wrong_state_without_a_not_found_answers_as_the_interpreter() {
    agrees(
        "Reseal of an absent locker, then created, shut and shut again",
        &[
            (
                "depot.hall.Reseal",
                json!({"locker": ABSENT, "label": "north", "allow": false}),
            ),
            (
                "depot.hall.Reseal",
                json!({"locker": ABSENT, "label": "north", "allow": true}),
            ),
            (
                "depot.hall.Reseal",
                json!({"locker": ABSENT, "label": "north", "allow": false}),
            ),
            (
                "depot.hall.Reseal",
                json!({"locker": ABSENT, "label": "north", "allow": false}),
            ),
        ],
    );
}
