//! Adversary cases for `when_related:` over one row, each run step for step against ESS 0.87.0's
//! interpreter (`interpret::execute`), which is the authority: outcome, declared error, events,
//! and the rows both views hold after the last step must agree, and a step the interpreter cannot
//! answer must be one the kernel cannot answer either.
//!
//! The fixture `tests/fixtures/adversary-related` declares desks that open and close and books
//! that are shelved at a desk (the `home` they store), lent and returned. `LendBook` names its
//! desk by input, `ReturnBook` reads the stored `home` and declares a held-state refusal
//! (`already-in`, `when_subject_state: Shelved`) beside its wrong-state move. The `…Checked`
//! commands add an `external:` refusal (`withheld`) declared before the move.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ess_compiler::refs::{CommandRef, OutcomeRef, ViewRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{
    ConformanceTarget, Deadline, ExternalOutcomeControl, ScenarioContext, SemanticCommandRequest,
    SemanticViewRequest,
};
use ess_domain::command::OutcomeName;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Map, Value, json};
use spindle_kernel::{External, MemoryStore, Store, execute};

const DESK: &str = "study.reading.Desk";
const BOOK: &str = "study.reading.Book";
const OPEN: &str = "study.reading.OpenDesk";
const CLOSE: &str = "study.reading.CloseDesk";
const SHELVE: &str = "study.reading.ShelveBook";
const LEND: &str = "study.reading.LendBook";
const LEND_BARE: &str = "study.reading.LendBookBare";
const RETURN: &str = "study.reading.ReturnBook";
const LEND_CHECKED: &str = "study.reading.LendBookChecked";
const RETURN_CHECKED: &str = "study.reading.ReturnBookChecked";

const OAK: &str = "5d6e7f80-0000-4000-8000-000000000001";
const ELM: &str = "5d6e7f80-0000-4000-8000-000000000002";
const NOWHERE: &str = "5d6e7f80-0000-4000-8000-0000000000ff";

fn book(n: u8) -> String {
    format!("9a8b7c6d-0000-4000-8000-0000000000{n:02x}")
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary-related")
}

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(&fixture())
        .expect("the adversary-related fixture compiles")
        .ir
}

/// One step: the command, its input, and the `external:` outcome the decider takes, if any.
type Step = (&'static str, Value, Option<&'static str>);

/// What one step answered: the outcome, the declared error and the events by name and payload,
/// or that no answer was given.
type Answer = Result<(String, Option<String>, Vec<(String, Map<String, Value>)>), ()>;

type Rows = Vec<Vec<Map<String, Value>>>;

const VIEWS: [(&str, &str, &str); 2] = [
    ("study.reading.DeskCard", DESK, "desk_id"),
    ("study.reading.BookCard", BOOK, "book_id"),
];

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

fn interpreted(ir: &spindle_spec::EssIr, steps: &[Step]) -> (Vec<Answer>, Rows) {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "study.reading/authored/adversary-related"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("adversary-related").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let mut answers = Vec::new();
    for (command, input, forced) in steps {
        if let Some(outcome) = forced {
            target
                .configure_external_outcome(ExternalOutcomeControl {
                    force: OutcomeRef::new(
                        command.parse::<CommandRef>().expect("a command"),
                        OutcomeName::new(*outcome).expect("an outcome name"),
                    ),
                    correlation: context.correlation.clone(),
                })
                .expect("the interpreter takes the forcing");
        }
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
                executed.error.map(|error| error.error.to_string()),
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

/// What the kernel answers for each step, and each stored row as its view projects it. A step
/// the kernel does not answer must leave the store's event log as it was.
fn spindled(ir: &spindle_spec::EssIr, steps: &[Step]) -> (Vec<Answer>, Rows) {
    let mut store = MemoryStore::default();
    let mut answers = Vec::new();
    for (command, input, forced) in steps {
        let before = store.events().len();
        let mut decider = |question: &External<'_>| Ok(Some(question.outcome) == *forced);
        let executed = execute(
            ir,
            &mut store,
            &mut decider,
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
                executed.error.map(|error| error.name),
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

fn outcome(answer: &Answer) -> String {
    match answer {
        Ok((outcome, _, _)) => outcome.clone(),
        Err(()) => "<no answer>".to_owned(),
    }
}

/// Runs `steps` on both and asserts, step by step, that the kernel answers what the interpreter
/// does; then that the rows agree.
fn agree(ir: &spindle_spec::EssIr, steps: &[Step]) {
    let (expected, expected_rows) = interpreted(ir, steps);
    let (answers, rows) = spindled(ir, steps);
    let differing: Vec<String> = steps
        .iter()
        .zip(expected.iter().zip(&answers))
        .filter(|(_, (ess, kernel))| ess != kernel)
        .map(|((command, input, forced), (ess, kernel))| {
            format!(
                "{command} {input} forced {forced:?}: interpreter `{}`, kernel `{}`",
                outcome(ess),
                outcome(kernel)
            )
        })
        .collect();
    assert!(
        differing.is_empty(),
        "the kernel answers differently from the interpreter:\n  {}",
        differing.join("\n  ")
    );
    assert_eq!(
        rows, expected_rows,
        "the rows agree with the interpreter's views"
    );
}

/// Two desks, `ELM` closed; books shelved at the open desk (1), the closed one (2), a desk no row
/// carries (3) and none (4); then books 5, 6, 7 shelved the same way as 1, 2, 3 and lent from
/// `OAK`.
fn library() -> Vec<Step> {
    let mut steps: Vec<Step> = vec![
        (OPEN, json!({"desk_id": OAK, "label": "oak"}), None),
        (OPEN, json!({"desk_id": ELM, "label": "elm"}), None),
        (SHELVE, json!({"book_id": book(1), "home": OAK}), None),
        (SHELVE, json!({"book_id": book(2), "home": ELM}), None),
        (SHELVE, json!({"book_id": book(3), "home": NOWHERE}), None),
        (SHELVE, json!({"book_id": book(4)}), None),
        (SHELVE, json!({"book_id": book(5), "home": OAK}), None),
        (SHELVE, json!({"book_id": book(6), "home": ELM}), None),
        (SHELVE, json!({"book_id": book(7), "home": NOWHERE}), None),
    ];
    for n in 5..=7 {
        steps.push((LEND, json!({"book_id": book(n), "desk_id": OAK}), None));
    }
    steps.push((CLOSE, json!({"desk_id": ELM}), None));
    steps
}

/// The interpreter's stated gap: on a stored-reference command, a held-state guard
/// (`when_subject_state: Shelved`) beside the wrong-state move. Every shelved book, whatever its
/// home, and every lent one.
#[test]
fn a_held_state_guard_beside_a_wrong_state_move_on_a_stored_reference_agrees() {
    let ir = ir();
    let mut steps = library();
    for n in 1..=7 {
        steps.push((RETURN, json!({"book_id": book(n)}), None));
    }
    steps.push((RETURN, json!({"book_id": book(0x99)}), None));
    agree(&ir, &steps);
}

/// The interpreter's stated gap, in the shapes a valid document admits: an addressed row no row
/// carries beside a missing, a closed and an open related row, with and without
/// `unknown_instance:`; and a lent book beside each.
#[test]
fn an_unknown_addressed_row_beside_a_missing_related_row_agrees() {
    let ir = ir();
    let mut steps = library();
    for command in [LEND, LEND_BARE] {
        for desk in [NOWHERE, ELM, OAK] {
            steps.push((
                command,
                json!({"book_id": book(0x99), "desk_id": desk}),
                None,
            ));
            steps.push((command, json!({"book_id": book(5), "desk_id": desk}), None));
        }
    }
    agree(&ir, &steps);
}

/// The look-ahead before the present-related refusals never asks the decider. Where the decider
/// takes an `external:` refusal declared before the move, the interpreter's look-ahead
/// (`subject_refusals_before_present_related`, read under `Externals::Forced`) selects that
/// refusal, finds no move from the wrong state, and goes on to the related row: a lent book asked
/// to be lent again at a closed desk is `desk-closed`, at an open one `withheld`.
#[test]
fn an_external_refusal_the_decider_takes_is_read_by_the_look_ahead_over_an_input_row() {
    let ir = ir();
    let mut steps = library();
    for desk in [ELM, OAK, NOWHERE] {
        steps.push((
            LEND_CHECKED,
            json!({"book_id": book(5), "desk_id": desk}),
            Some("withheld"),
        ));
    }
    agree(&ir, &steps);
}

/// The same over a stored reference (`stored_reference`'s look-ahead, read under
/// `Externals::Forced`): a shelved book asked to be returned while the reading room withholds it.
#[test]
fn an_external_refusal_the_decider_takes_is_read_by_the_look_ahead_over_a_stored_row() {
    let ir = ir();
    let mut steps = library();
    for n in 1..=4 {
        steps.push((
            RETURN_CHECKED,
            json!({"book_id": book(n)}),
            Some("withheld"),
        ));
    }
    agree(&ir, &steps);
}

/// With the decider declining, every branch of the checked commands agrees: the look-ahead and
/// the decider are each read where the interpreter reads them.
#[test]
fn the_checked_commands_agree_where_the_decider_declines() {
    let ir = ir();
    let mut steps = library();
    for desk in [ELM, OAK, NOWHERE] {
        for n in [1, 5] {
            steps.push((
                LEND_CHECKED,
                json!({"book_id": book(n), "desk_id": desk}),
                None,
            ));
        }
    }
    for n in 1..=7 {
        steps.push((RETURN_CHECKED, json!({"book_id": book(n)}), None));
    }
    agree(&ir, &steps);
}

/// The `unknown_instance:` branches, which a document may not declare beside `when_related:`
/// before ess/27, and the commands declaring an `external:` beside it, left out.
fn before_the_one_rule() -> (tempfile::TempDir, spindle_spec::EssIr) {
    const NO_BOOK: &str = "      - name: no-book
        unknown_instance: true
        error: study.reading.NoBook
        summary: No book carries the id.
";
    const ALREADY_IN: &str = "      - name: already-in
        when_subject_state: Shelved
        error: study.reading.AlreadyIn
        summary: The book is already on its shelf.
";
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    std::fs::create_dir(dir.path().join("domains")).expect("the domains directory");
    for file in ["system.yaml", "domains/reading.yaml"] {
        let text = std::fs::read_to_string(fixture().join(file)).expect("the fixture reads");
        let text = text
            .replacen("format: ess/34", "format: ess/26", 1)
            .replace(NO_BOOK, "")
            .replace(ALREADY_IN, "");
        std::fs::write(dir.path().join(file), text).expect("the copy writes");
    }
    let ir = spindle_spec::load(dir.path())
        .unwrap_or_else(|error| panic!("the fixture compiles at ess/26: {error}"))
        .ir;
    (dir, ir)
}

/// Both stated gaps, and the decider's, before the one rule, where a missing row the input names
/// answers at step 1.
#[test]
fn the_stated_gaps_agree_before_the_one_rule() {
    let (_dir, ir) = before_the_one_rule();
    let mut steps = library();
    for n in 1..=7 {
        steps.push((RETURN, json!({"book_id": book(n)}), None));
    }
    for command in [LEND, LEND_BARE] {
        for desk in [NOWHERE, ELM, OAK] {
            steps.push((
                command,
                json!({"book_id": book(0x99), "desk_id": desk}),
                None,
            ));
            steps.push((command, json!({"book_id": book(5), "desk_id": desk}), None));
        }
    }
    for desk in [ELM, OAK] {
        steps.push((
            LEND_CHECKED,
            json!({"book_id": book(5), "desk_id": desk}),
            Some("withheld"),
        ));
    }
    agree(&ir, &steps);
}
