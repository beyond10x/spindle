//! A command guard ordering a timestamp against `now` is decided on the kernel's clock: the
//! decision's one instant, read as ESS's interpreter reads it in `when:`, `when_subject:` and a
//! `when_related:` over one row (`interpret::execute`, `caller::Facts::new(.., context.now)`).
//!
//! The fixture `tests/fixtures/deadlines` declares tasks that fall due at `due_at`. `Schedule`
//! refuses a due time before `now` with `overdue`, `ScheduleWithGrace` one before `now - 3600s`;
//! `Close` refuses an addressed task due before `now`, and `Annotate` a note on a named task due
//! before `now`. `Record` stores a task with any due time, unguarded.

use std::path::Path;
use std::time::{Duration, SystemTime};

use ess_conformance::interpret::Interpreted;
use ess_conformance::occurrence_clock::{CommandClock, DecisionInstant};
use serde_json::{Value, json};
use spindle_kernel::{Executed, MemoryStore, NoExternals, UuidV4, execute_at};

use super::related_guards::{interpreted_on, object, spindled_with};

const SCHEDULE: &str = "chores.board.Schedule";
const GRACE: &str = "chores.board.ScheduleWithGrace";
const RECORD: &str = "chores.board.Record";
const CLOSE: &str = "chores.board.Close";
const ANNOTATE: &str = "chores.board.Annotate";

const TASK: &str = "5a5b5c5d-0000-4000-8000-000000000001";
const NOTE: &str = "6a6b6c6d-0000-4000-8000-000000000001";

/// The decision instant every step is read at.
const NOW: &str = "2026-03-01T00:00:00Z";
const NOW_SECONDS: u64 = 1_772_323_200;
/// One second before [`NOW`].
const BEFORE: &str = "2026-02-28T23:59:59Z";
/// One second after [`NOW`].
const AFTER: &str = "2026-03-01T00:00:01Z";
/// 3601 s before [`NOW`]: before `now - 3600s`.
const GRACE_BEFORE: &str = "2026-02-28T22:59:59Z";
/// 3600 s before [`NOW`]: `now - 3600s` itself.
const GRACE_AT: &str = "2026-02-28T23:00:00Z";
/// 3599 s before [`NOW`]: after `now - 3600s`.
const GRACE_AFTER: &str = "2026-02-28T23:00:01Z";

const VIEWS: [(&str, &str, &str); 2] = [
    ("chores.board.TaskCard", "chores.board.Task", "task_id"),
    ("chores.board.NoteCard", "chores.board.Note", "note_id"),
];

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/deadlines"))
        .expect("the deadlines fixture compiles")
        .ir
}

fn now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(NOW_SECONDS)
}

fn run(ir: &spindle_spec::EssIr, store: &mut MemoryStore, command: &str, input: Value) -> Executed {
    execute_at(
        ir,
        store,
        &mut NoExternals,
        &mut UuidV4,
        &now,
        command,
        &object(input.clone()),
    )
    .unwrap_or_else(|error| panic!("{command} {input} runs: {error}"))
}

/// The outcome and error of the last of `steps`, each run on one fresh store at [`NOW`].
fn last(ir: &spindle_spec::EssIr, steps: &[(&str, Value)]) -> (String, Option<String>) {
    let mut store = MemoryStore::default();
    let mut answered = None;
    for (command, input) in steps {
        answered = Some(run(ir, &mut store, command, input.clone()));
    }
    let executed = answered.expect("at least one step");
    (executed.outcome, executed.error.map(|error| error.name))
}

fn schedule(command: &'static str, due_at: &str) -> Vec<(&'static str, Value)> {
    vec![(command, json!({"task_id": TASK, "due_at": due_at}))]
}

fn close(due_at: &str) -> Vec<(&'static str, Value)> {
    vec![
        (RECORD, json!({"task_id": TASK, "due_at": due_at})),
        (CLOSE, json!({"task_id": TASK})),
    ]
}

fn annotate(due_at: &str) -> Vec<(&'static str, Value)> {
    vec![
        (RECORD, json!({"task_id": TASK, "due_at": due_at})),
        (ANNOTATE, json!({"note_id": NOTE, "task_id": TASK})),
    ]
}

fn overdue() -> (String, Option<String>) {
    (
        "overdue".to_owned(),
        Some("chores.board.Overdue".to_owned()),
    )
}

fn accepted(outcome: &str) -> (String, Option<String>) {
    (outcome.to_owned(), None)
}

#[test]
fn a_when_guard_ordering_against_now_is_decided_on_the_clock() {
    let ir = ir();
    assert_eq!(
        last(&ir, &schedule(SCHEDULE, BEFORE)),
        overdue(),
        "a due time before `now` is overdue"
    );
    assert_eq!(
        last(&ir, &schedule(SCHEDULE, AFTER)),
        accepted("scheduled"),
        "a due time after `now` is scheduled"
    );
}

#[test]
fn a_when_subject_guard_ordering_against_now_is_decided_on_the_clock() {
    let ir = ir();
    assert_eq!(
        last(&ir, &close(BEFORE)),
        overdue(),
        "a task due before `now` is overdue"
    );
    assert_eq!(
        last(&ir, &close(AFTER)),
        accepted("closed"),
        "a task due after `now` closes"
    );
}

#[test]
fn a_when_related_guard_ordering_against_now_is_decided_on_the_clock() {
    let ir = ir();
    assert_eq!(
        last(&ir, &annotate(BEFORE)),
        overdue(),
        "a note on a task due before `now` is refused"
    );
    assert_eq!(
        last(&ir, &annotate(AFTER)),
        accepted("noted"),
        "a note on a task due after `now` is written"
    );
}

#[test]
fn a_guard_ordering_against_now_minus_a_duration_is_decided_on_the_clock() {
    let ir = ir();
    assert_eq!(
        last(&ir, &schedule(GRACE, GRACE_BEFORE)),
        overdue(),
        "a due time 3601 s before `now` is past the hour's grace"
    );
    assert_eq!(
        last(&ir, &schedule(GRACE, GRACE_AFTER)),
        accepted("scheduled"),
        "a due time 3599 s before `now` is within the hour's grace"
    );
}

/// The interpreter's command clock: every decision reads [`NOW`].
struct Fixed;

impl CommandClock for Fixed {
    fn read(&self) -> Option<DecisionInstant> {
        Some(DecisionInstant::parse(NOW).expect("an instant"))
    }
}

#[test]
fn now_guards_agree_with_the_interpreter() {
    let ir = ir();
    let cases: Vec<(Vec<(&str, Value)>, &str)> = vec![
        (schedule(SCHEDULE, BEFORE), "overdue"),
        (schedule(SCHEDULE, NOW), "scheduled"),
        (schedule(SCHEDULE, AFTER), "scheduled"),
        (schedule(GRACE, GRACE_BEFORE), "overdue"),
        (schedule(GRACE, GRACE_AT), "scheduled"),
        (schedule(GRACE, GRACE_AFTER), "scheduled"),
        (close(BEFORE), "overdue"),
        (close(NOW), "closed"),
        (close(AFTER), "closed"),
        (annotate(BEFORE), "overdue"),
        (annotate(NOW), "noted"),
        (annotate(AFTER), "noted"),
    ];
    for (steps, expected) in cases {
        let (ess_answers, ess_rows) = interpreted_on(
            Interpreted::for_model(ir.clone()).with_command_clock(Fixed),
            &VIEWS,
            &steps,
        );
        let guarded = steps.last().map(|(command, input)| (command, input));
        assert_eq!(
            ess_answers.last().map(|(outcome, _, _)| outcome.as_str()),
            Some(expected),
            "the interpreter answers {guarded:?} with `{expected}`"
        );
        let (answers, rows) = spindled_with(&VIEWS, &steps, &mut |store, command, input| {
            run(&ir, store, command, input)
        });
        assert_eq!(
            answers, ess_answers,
            "outcomes, errors and published payloads of {guarded:?} agree"
        );
        assert_eq!(
            rows, ess_rows,
            "the stored tasks and notes after {guarded:?} agree with ESS's view rows"
        );
    }
}
