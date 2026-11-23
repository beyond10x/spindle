//! An external refusal answered before the row (`before_row: true`, ess/25,
//! `story:external-refusal-before-row`): the kernel asks its cause among the input-guarded
//! refusals, in declaration order, before it reads the addressed row, and answers as ESS's
//! interpreter at the pinned tag answers.
//!
//! The fixture `tests/fixtures/before-row` opens seats and signs into them: `SignIn` refuses an
//! empty secret (`blank-secret`, an input guard), then asks a credential check the specification
//! does not describe (`bad-credential`, `before_row: true`), then refuses a seat no row carries
//! (`no-seat`) and otherwise moves the seat by its held state. Each step list runs on a fresh
//! store, on the kernel and on the interpreter, with the same causes forced.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;

use ess_compiler::refs::{CommandRef, OutcomeRef, ViewRef};
use ess_conformance::ExternalOutcomeControl;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{
    ConformanceTarget, Deadline, ScenarioContext, SemanticCommandRequest, SemanticViewRequest,
};
use ess_domain::command::OutcomeName;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Map, Value, json};
use spindle_kernel::{
    Change, External, MemoryStore, Removal, Scan, Store, StoreError, Stored, Write, execute,
};

const SEAT: &str = "lobby.desk.Seat";
const SEATS: &str = "lobby.desk.Seats";
const OPEN_SEAT: &str = "lobby.desk.OpenSeat";
const SIGN_IN: &str = "lobby.desk.SignIn";
const BAD_CREDENTIAL: &str = "bad-credential";
const ONE: &str = "00000000-0000-4000-8000-000000000001";
const NONE: &str = "00000000-0000-4000-8000-0000000000aa";

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/before-row"))
        .expect("the before-row fixture compiles")
        .ir
}

/// One step: the command, its input, and the outcome whose external cause is forced to hold.
type Step = (&'static str, Value, Option<&'static str>);

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

fn open(seat: &str) -> Step {
    (OPEN_SEAT, json!({"seat_id": seat, "label": "s"}), None)
}

fn sign_in(seat: &str, secret: &str, forced: Option<&'static str>) -> Step {
    (SIGN_IN, json!({"seat_id": seat, "secret": secret}), forced)
}

/// The step lists, each with what it shows.
fn cases() -> Vec<(&'static str, Vec<Step>)> {
    vec![
        (
            "the cause refuses, no seat carries the identity",
            vec![sign_in(NONE, "x", Some(BAD_CREDENTIAL))],
        ),
        (
            "the cause refuses an open seat, which stays open",
            vec![open(ONE), sign_in(ONE, "x", Some(BAD_CREDENTIAL))],
        ),
        (
            "the cause accepts: the seat is taken, then released",
            vec![open(ONE), sign_in(ONE, "x", None), sign_in(ONE, "x", None)],
        ),
        (
            "the cause accepts, no seat carries the identity",
            vec![sign_in(NONE, "x", None)],
        ),
        (
            "an empty secret is refused first, the cause forced or not",
            vec![
                open(ONE),
                sign_in(ONE, "", Some(BAD_CREDENTIAL)),
                sign_in(NONE, "", None),
            ],
        ),
    ]
}

fn interpreted(ir: &spindle_spec::EssIr, steps: &[Step]) -> (Vec<Answer>, Rows) {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "lobby.desk/authored/before-row"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("before-row").expect("a correlation id"),
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
            view: ViewRef::new(SEATS.parse().expect("a view name")),
            params: BTreeMap::new(),
            consistency: ess_primitives::consistency::QueryConsistency::Current,
            correlation: context.correlation.clone(),
            deadline: Deadline::at(Timestamp::from_epoch_millis(0)),
        })
        .expect("the interpreter reads the view")
        .rows
        .into_iter()
        .map(|row| {
            row.iter()
                .map(|(field, node)| (field.clone(), json_of(node)))
                .collect()
        })
        .collect();
    (answers, rows)
}

/// What a store wrapper and a decider saw, in the order they saw it.
type Log = Rc<RefCell<Vec<String>>>;

/// A [`MemoryStore`] that logs every row it is asked to load.
struct Logging {
    store: MemoryStore,
    log: Log,
}

impl Store for Logging {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.log.borrow_mut().push(format!("load {entity}"));
        self.store.load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.store.commit(change)
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.store.remove(removal)
    }

    fn commit_all(&mut self, writes: &[Write]) -> Result<(), StoreError> {
        self.store.commit_all(writes)
    }

    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError> {
        self.log.borrow_mut().push(format!("scan {entity}"));
        self.store.scan(entity)
    }
}

/// Runs `steps` on the kernel over a fresh store; returns each answer, the rows, and per step
/// what the store and the decider saw.
fn spindled(ir: &spindle_spec::EssIr, steps: &[Step]) -> (Vec<Answer>, Rows, Vec<Vec<String>>) {
    let log: Log = Rc::default();
    let mut store = Logging {
        store: MemoryStore::default(),
        log: Rc::clone(&log),
    };
    let mut answers = Vec::new();
    let mut seen = Vec::new();
    for (command, input, forced) in steps {
        log.borrow_mut().clear();
        let asked = Rc::clone(&log);
        let mut decider = |question: &External<'_>| -> Result<bool, String> {
            asked.borrow_mut().push(format!("ask {}", question.outcome));
            Ok(Some(question.outcome) == *forced)
        };
        let before = store.store.events().len();
        let executed = execute(
            ir,
            &mut store,
            &mut decider,
            command,
            &object(input.clone()),
        );
        if executed.is_err() {
            assert_eq!(
                store.store.events().len(),
                before,
                "{command} {input}: a step the kernel does not answer writes nothing"
            );
        }
        seen.push(log.borrow().clone());
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
        .store
        .scan(SEAT)
        .expect("the store scans")
        .into_iter()
        .map(|(identity, stored)| {
            let mut row: Map<String, Value> = stored.row.fields.into_iter().collect();
            row.insert("seat_id".to_owned(), identity);
            row.insert("state".to_owned(), json!(stored.row.state));
            row
        })
        .collect();
    (answers, rows, seen)
}

fn outcome(answer: &Answer) -> Option<&str> {
    answer.as_ref().ok().map(|(outcome, _, _)| outcome.as_str())
}

#[test]
fn a_command_with_an_external_refusal_before_the_row_runs() {
    let ir = ir();
    let command = ir
        .commands()
        .iter()
        .find_map(|(name, spec)| (name.to_string() == SIGN_IN).then_some(spec))
        .expect("the fixture declares SignIn");
    if let Err(refused) = spindle_kernel::runs(&ir, command) {
        panic!("the kernel runs every outcome of SignIn: {refused}");
    }
}

#[test]
fn an_external_refusal_before_the_row_answers_as_the_interpreter() {
    let ir = ir();
    for (what, steps) in cases() {
        let (ess_answers, ess_rows) = interpreted(&ir, &steps);
        let (answers, rows, _) = spindled(&ir, &steps);
        for (step, ((command, input, forced), (ours, theirs))) in steps
            .iter()
            .zip(answers.iter().zip(&ess_answers))
            .enumerate()
        {
            assert_eq!(
                ours, theirs,
                "{what}: step {step} {command} {input} forced {forced:?}: the kernel's answer \
                 (left) is the interpreter's (right)"
            );
        }
        assert_eq!(rows, ess_rows, "{what}: the stored rows agree");
    }
}

#[test]
fn an_external_refusal_before_the_row_reads_no_row() {
    let ir = ir();
    let (answers, rows, seen) = spindled(
        &ir,
        &[
            open(ONE),
            sign_in(ONE, "x", Some(BAD_CREDENTIAL)),
            sign_in(NONE, "x", Some(BAD_CREDENTIAL)),
            sign_in(ONE, "x", None),
        ],
    );
    for step in [1, 2] {
        assert_eq!(outcome(&answers[step]), Some(BAD_CREDENTIAL), "step {step}");
        assert_eq!(
            seen[step],
            vec![format!("ask {BAD_CREDENTIAL}")],
            "step {step}: the cause is asked and no row is read"
        );
    }
    let Ok((_, Some((error, fields)), events)) = &answers[2] else {
        panic!("the refusal reports its error: {:?}", answers[2]);
    };
    assert_eq!(error, "lobby.desk.BadCredential");
    assert_eq!(fields, &object(json!({"seat_id": NONE})));
    assert!(events.is_empty(), "a refusal emits nothing: {events:?}");
    assert_eq!(outcome(&answers[3]), Some("taken"));
    assert_eq!(
        seen[3].first().map(String::as_str),
        Some(format!("ask {BAD_CREDENTIAL}").as_str()),
        "the cause is asked before the seat is read: {:?}",
        seen[3]
    );
    assert!(
        seen[3].contains(&format!("load {SEAT}")),
        "an accepted cause reads the seat: {:?}",
        seen[3]
    );
    assert_eq!(
        rows,
        vec![object(
            json!({"seat_id": ONE, "label": "s", "state": "Busy"})
        )],
        "only the accepted sign-in moved the seat"
    );
}
