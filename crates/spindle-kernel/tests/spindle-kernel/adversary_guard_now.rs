//! Adversary probes of `story:kernel-guard-reads-now`: a command guard ordering against `now`,
//! compared step by step with ESS 0.90.0's interpreter at the same decision instant, on the
//! `tests/fixtures/adversary-guard-now` fixture.
//!
//! Each case runs its steps on a fresh store on both sides and compares every step's answer
//! (outcome and error, or that the step was refused) and the task rows after the last step.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, SystemTime};

use ess_compiler::refs::{CommandRef, ViewRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::occurrence_clock::{CommandClock, DecisionInstant};
use ess_conformance::target::{
    ConformanceTarget, Deadline, ScenarioContext, SemanticCommandRequest, SemanticViewRequest,
};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Map, Value, json};
use spindle_kernel::{ExecuteError, MemoryStore, NoExternals, Store, UuidV4, execute_at};

use super::related_guards::object;

const TYPED: &str = "probe.board.Typed";
const MAYBE: &str = "probe.board.Maybe";
const FAR: &str = "probe.board.Far";
const RECORD: &str = "probe.board.Record";
const CLOSE: &str = "probe.board.Close";
const CLOSE_TYPED: &str = "probe.board.CloseTyped";
const RESCHEDULE: &str = "probe.board.Reschedule";
const SWEEP: &str = "probe.board.Sweep";
const ANNOTATE: &str = "probe.board.Annotate";
const FINISH: &str = "probe.board.Finish";

const TASK: &str = "5a5b5c5d-0000-4000-8000-000000000001";
const MARK: &str = "6a6b6c6d-0000-4000-8000-000000000001";
const NOTE: &str = "7a7b7c7d-0000-4000-8000-000000000001";

/// 2026-03-01T00:00:00Z.
const NOW_SECONDS: u64 = 1_772_323_200;
const NOW: &str = "2026-03-01T00:00:00Z";
const BEFORE: &str = "2026-02-28T23:59:59Z";
const AFTER: &str = "2026-03-01T00:00:01Z";

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary-guard-now"),
    )
    .expect("the adversary-guard-now fixture compiles")
    .ir
}

/// One step's answer: the outcome and error, or `refused` with the error's kind.
type Answer = Result<(String, Option<String>), String>;

/// The interpreter's command clock at one instant, or none.
struct At(Option<DecisionInstant>);

impl CommandClock for At {
    fn read(&self) -> Option<DecisionInstant> {
        self.0
    }
}

fn node_of(value: &Value) -> Node {
    serde_json::from_value(value.clone()).expect("a JSON scalar is a node")
}

fn present(row: Map<String, Value>) -> Map<String, Value> {
    row.into_iter()
        .filter(|(_, value)| !value.is_null())
        .collect()
}

fn interpreted(
    ir: &spindle_spec::EssIr,
    instant: Option<&str>,
    steps: &[(&str, Value)],
) -> (Vec<Answer>, Vec<Map<String, Value>>) {
    let target = Interpreted::for_model(ir.clone()).with_command_clock(At(
        instant.map(|text| DecisionInstant::parse(text).expect("a decision instant")),
    ));
    let context = ScenarioContext::new(
        "probe.board/authored/guard-now".parse().expect("a scenario id"),
        CorrelationId::new("guard-now").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let mut answers = Vec::new();
    for (command, input) in steps {
        let input = object(input.clone())
            .iter()
            .map(|(field, value)| (field.clone(), node_of(value)))
            .collect();
        let answer = target
            .execute_command(SemanticCommandRequest {
                command: CommandRef::new(command.parse().expect("a command name")),
                actor: None,
                caller: None,
                input,
                correlation: context.correlation.clone(),
            })
            .map(|executed| {
                (
                    executed
                        .outcome
                        .map(|outcome| outcome.outcome.as_str().to_owned())
                        .unwrap_or_default(),
                    executed.error.map(|error| error.error.to_string()),
                )
            })
            .map_err(|error| format!("refused ({error})"));
        answers.push(answer);
    }
    let rows = target
        .query_view(SemanticViewRequest {
            view: ViewRef::new("probe.board.TaskCard".parse().expect("a view name")),
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
                    .map(|(field, node)| {
                        (
                            field.clone(),
                            serde_json::to_value(node).expect("a node is JSON"),
                        )
                    })
                    .collect(),
            )
        })
        .collect();
    (answers, rows)
}

fn spindled(
    ir: &spindle_spec::EssIr,
    clock: &dyn Fn() -> SystemTime,
    steps: &[(&str, Value)],
) -> (Vec<Answer>, Vec<Map<String, Value>>) {
    let mut store = MemoryStore::default();
    let mut answers = Vec::new();
    for (command, input) in steps {
        let answer = execute_at(
            ir,
            &mut store,
            &mut NoExternals,
            &mut UuidV4,
            clock,
            command,
            &object(input.clone()),
        )
        .map(|executed| (executed.outcome, executed.error.map(|error| error.name)))
        .map_err(|error: ExecuteError| format!("refused ({error})"));
        answers.push(answer);
    }
    let rows = store
        .scan("probe.board.Task")
        .expect("the store scans")
        .into_iter()
        .map(|(identity, stored)| {
            let mut row: Map<String, Value> = stored.row.fields.into_iter().collect();
            row.insert("task_id".to_owned(), identity);
            row.insert("state".to_owned(), json!(stored.row.state));
            present(row)
        })
        .collect();
    (answers, rows)
}

/// Whether two answers agree: the same outcome and error, or both refused.
fn same(left: &Answer, right: &Answer) -> bool {
    match (left, right) {
        (Ok(left), Ok(right)) => left == right,
        (Err(_), Err(_)) => true,
        _ => false,
    }
}

/// Runs every case at `instant` (the interpreter's spelling) and `clock` (the kernel's), and
/// returns every disagreement.
fn disagreements(
    instant: Option<&str>,
    clock: &dyn Fn() -> SystemTime,
    cases: &[Vec<(&str, Value)>],
) -> Vec<String> {
    let ir = ir();
    let mut found = Vec::new();
    for steps in cases {
        let (ess, ess_rows) = interpreted(&ir, instant, steps);
        let (kernel, kernel_rows) = spindled(&ir, clock, steps);
        if std::env::var_os("ADVERSARY_GUARD_NOW_TRACE").is_some() {
            eprintln!("{:?}\n  ess    {ess:?}\n  kernel {kernel:?}", steps.last());
        }
        let agree = ess.len() == kernel.len()
            && ess.iter().zip(&kernel).all(|(left, right)| same(left, right))
            && (ess.last().is_some_and(Result::is_err) || ess_rows == kernel_rows);
        if !agree {
            found.push(format!(
                "steps {steps:?}\n    interpreter: {ess:?} rows {ess_rows:?}\n    kernel:      {kernel:?} rows {kernel_rows:?}"
            ));
        }
    }
    found
}

fn at_now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(NOW_SECONDS)
}

fn report(found: &[String]) -> String {
    found.join("\n")
}

/// A guard over a newtype of `Timestamp`, in `when:` and `when_subject:`, before, at and after
/// the instant.
#[test]
fn a_now_guard_over_a_timestamp_newtype_agrees_with_the_interpreter() {
    let mut cases = Vec::new();
    for due in [BEFORE, NOW, AFTER] {
        cases.push(vec![(TYPED, json!({"task_id": TASK, "due_at": due}))]);
        cases.push(vec![
            (RECORD, json!({"task_id": TASK, "typed_at": due})),
            (CLOSE_TYPED, json!({"task_id": TASK})),
        ]);
    }
    let found = disagreements(Some(NOW), &at_now, &cases);
    assert!(found.is_empty(), "{}", report(&found));
}

/// A guard over an `Optional<Timestamp>` left absent or `null`, in `when:` and over a stored row.
#[test]
fn a_now_guard_over_an_absent_timestamp_agrees_with_the_interpreter() {
    let cases = vec![
        vec![(MAYBE, json!({"task_id": TASK}))],
        vec![(MAYBE, json!({"task_id": TASK, "due_at": null}))],
        vec![(MAYBE, json!({"task_id": TASK, "due_at": BEFORE}))],
        vec![
            (RECORD, json!({"task_id": TASK})),
            (CLOSE, json!({"task_id": TASK})),
        ],
        vec![
            (RECORD, json!({"task_id": TASK, "due_at": BEFORE})),
            (CLOSE, json!({"task_id": TASK})),
        ],
    ];
    let found = disagreements(Some(NOW), &at_now, &cases);
    assert!(found.is_empty(), "{}", report(&found));
}

/// `now + 24h` and `now` on the left of `>=`, at each side of both bounds.
#[test]
fn now_plus_a_duration_and_now_on_the_left_agree_with_the_interpreter() {
    let cases: Vec<_> = [
        "2026-03-02T00:00:01Z",
        "2026-03-02T00:00:00Z",
        "2026-03-01T23:59:59Z",
        AFTER,
        NOW,
        BEFORE,
    ]
    .into_iter()
    .map(|due| vec![(FAR, json!({"task_id": TASK, "due_at": due}))])
    .collect();
    let found = disagreements(Some(NOW), &at_now, &cases);
    assert!(found.is_empty(), "{}", report(&found));
}

/// An instant with a fraction of a second, and timestamps spelled with an offset other than `Z`.
#[test]
fn a_fractional_instant_and_offset_spellings_agree_with_the_interpreter() {
    let fractional = || SystemTime::UNIX_EPOCH + Duration::new(NOW_SECONDS, 500_000_000);
    let cases: Vec<_> = [
        "2026-03-01T00:00:00.4Z",
        "2026-03-01T00:00:00.5Z",
        "2026-03-01T00:00:00.500000001Z",
        "2026-03-01T00:00:00.6Z",
        "2026-03-01T01:00:00.4+01:00",
        "2026-03-01T01:00:00.6+01:00",
    ]
    .into_iter()
    .map(|due| vec![(MAYBE, json!({"task_id": TASK, "due_at": due}))])
    .collect();
    let found = disagreements(Some("2026-03-01T00:00:00.5Z"), &fractional, &cases);
    assert!(found.is_empty(), "{}", report(&found));
}

/// `input.<field>` ordered against `now` inside `when_subject:`.
#[test]
fn an_input_operand_against_now_in_when_subject_agrees_with_the_interpreter() {
    let cases: Vec<_> = [BEFORE, NOW, AFTER]
        .into_iter()
        .map(|due| {
            vec![
                (RECORD, json!({"task_id": TASK, "due_at": AFTER})),
                (RESCHEDULE, json!({"task_id": TASK, "due_at": due})),
            ]
        })
        .collect();
    let found = disagreements(Some(NOW), &at_now, &cases);
    assert!(found.is_empty(), "{}", report(&found));
}

/// A row-set `when_related:` and a `when:` guard in one command, both reading `now`.
#[test]
fn a_row_set_and_an_input_guard_in_one_command_agree_with_the_interpreter() {
    let mut cases = Vec::new();
    for stored in [BEFORE, NOW, AFTER] {
        for at in [BEFORE, NOW, AFTER] {
            cases.push(vec![
                (RECORD, json!({"task_id": TASK, "due_at": stored})),
                (SWEEP, json!({"mark_id": MARK, "at": at})),
            ]);
        }
    }
    cases.push(vec![
        (RECORD, json!({"task_id": TASK})),
        (SWEEP, json!({"mark_id": MARK, "at": AFTER})),
    ]);
    let found = disagreements(Some(NOW), &at_now, &cases);
    assert!(found.is_empty(), "{}", report(&found));
}

/// A `when:` guard against `now` ahead of a related row that is missing, present and overdue,
/// or present and not: the look-ahead reads the guard before the row.
#[test]
fn a_now_guard_beside_a_missing_related_row_agrees_with_the_interpreter() {
    let mut cases = Vec::new();
    for at in [BEFORE, AFTER] {
        cases.push(vec![(
            ANNOTATE,
            json!({"note_id": NOTE, "task_id": TASK, "at": at}),
        )]);
        for due in [BEFORE, AFTER] {
            cases.push(vec![
                (RECORD, json!({"task_id": TASK, "due_at": due})),
                (
                    ANNOTATE,
                    json!({"note_id": NOTE, "task_id": TASK, "at": at}),
                ),
            ]);
        }
    }
    let found = disagreements(Some(NOW), &at_now, &cases);
    assert!(found.is_empty(), "{}", report(&found));
}

/// A `when:` guard against `now` on a command that moves the addressed row, with the row
/// missing, open and done.
#[test]
fn a_now_guard_beside_the_held_state_agrees_with_the_interpreter() {
    let mut cases = Vec::new();
    for at in [BEFORE, AFTER] {
        cases.push(vec![(FINISH, json!({"task_id": TASK, "at": at}))]);
        cases.push(vec![
            (RECORD, json!({"task_id": TASK, "due_at": AFTER})),
            (FINISH, json!({"task_id": TASK, "at": at})),
        ]);
        cases.push(vec![
            (RECORD, json!({"task_id": TASK, "due_at": AFTER})),
            (CLOSE, json!({"task_id": TASK})),
            (FINISH, json!({"task_id": TASK, "at": at})),
        ]);
    }
    let found = disagreements(Some(NOW), &at_now, &cases);
    assert!(found.is_empty(), "{}", report(&found));
}

/// A clock with no instant to give: the kernel's clock past year 9999, the interpreter with no
/// command clock. Every guard that needs the instant is `Unknown` on both sides.
#[test]
fn a_clock_with_no_instant_agrees_with_the_interpreter() {
    let unspelled = || SystemTime::UNIX_EPOCH + Duration::from_secs(253_402_300_800);
    let cases = vec![
        vec![(MAYBE, json!({"task_id": TASK, "due_at": BEFORE}))],
        vec![(FAR, json!({"task_id": TASK, "due_at": BEFORE}))],
        vec![
            (RECORD, json!({"task_id": TASK, "due_at": AFTER})),
            (CLOSE, json!({"task_id": TASK})),
        ],
        vec![
            (RECORD, json!({"task_id": TASK, "due_at": AFTER})),
            (SWEEP, json!({"mark_id": MARK, "at": AFTER})),
        ],
    ];
    let found = disagreements(None, &unspelled, &cases);
    assert!(found.is_empty(), "{}", report(&found));
}

/// One decision reads one instant: a clock that moves on by ten seconds on every read gives the
/// row-set selector and the input guard of one `Sweep` the same instant, and so `Annotate`'s input
/// guard and its related row.
#[test]
fn every_guard_of_one_decision_reads_the_clock_once() {
    let ir = ir();
    let start = SystemTime::UNIX_EPOCH + Duration::from_secs(NOW_SECONDS);
    let reads = Cell::new(0u32);
    let stepping = || {
        let read = reads.get();
        reads.set(read + 1);
        start + Duration::from_secs(10 * u64::from(read))
    };
    // T+5: after the first read, before the second.
    let between = "2026-03-01T00:00:05Z";
    let mut store = MemoryStore::default();
    store_task(&ir, &mut store, between);
    reads.set(0);
    let swept = execute_at(
        &ir,
        &mut store,
        &mut NoExternals,
        &mut UuidV4,
        &stepping,
        SWEEP,
        &object(json!({"mark_id": MARK, "at": between})),
    )
    .expect("the sweep runs");
    assert_eq!(
        (swept.outcome.as_str(), reads.get()),
        ("swept", 1),
        "one read: no open task is overdue at T and the sweep is not late"
    );
    reads.set(0);
    let noted = execute_at(
        &ir,
        &mut store,
        &mut NoExternals,
        &mut UuidV4,
        &stepping,
        ANNOTATE,
        &object(json!({"note_id": NOTE, "task_id": TASK, "at": between})),
    )
    .expect("the note runs");
    assert_eq!(
        (noted.outcome.as_str(), reads.get()),
        ("noted", 1),
        "one read: neither the note nor the task is overdue at T"
    );
}

fn store_task(ir: &spindle_spec::EssIr, store: &mut MemoryStore, due: &str) {
    execute_at(
        ir,
        store,
        &mut NoExternals,
        &mut UuidV4,
        &at_now,
        RECORD,
        &object(json!({"task_id": TASK, "due_at": due})),
    )
    .expect("the task is recorded");
}
