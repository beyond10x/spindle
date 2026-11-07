//! Adversarial cases for `when_related:` over a row set, driven against ESS's interpreter
//! (`interpret::execute::row_set`, `Reader::decide`, `decisions`, and `addressed_row` in
//! `interpret::execute`) on the same steps.
//!
//! - `tests/fixtures/yard`: an owner adds weighed tasks. `AddTask` is refused with `full` where the
//!   owner holds two `Open` tasks (`count:`); `Finish` is refused with `not-heaviest` where another
//!   open task of the subject's owner is heavier (identity, held state and `subject.`); `Sweep` is
//!   refused with `all-kept` where every open task of the owner weighs five or more (`forall:`,
//!   true of no rows).
//! - `tests/fixtures/shelf`: ESS's own upsert fixture (`row-set-upsert.yaml`, beyond10x/ess#462).
//!   `Shelve` creates a book that is absent or replaces one that is held, and is refused while a
//!   library of the curator is open.
//! - `tests/fixtures/library`: the unit's own fixture, under a store another writer commits to.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, SystemTime};

use ess_compiler::refs::CommandRef;
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{ConformanceTarget, ScenarioContext, SemanticCommandRequest};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use serde_json::{Value, json};
use spindle_kernel::{
    Change, MemoryStore, NoExternals, Removal, Row, Store, StoreError, Stored, UuidV4, execute,
    execute_at,
};

use super::related_guards::object;

fn load(fixture: &str) -> spindle_spec::EssIr {
    spindle_spec::load(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(fixture),
    )
    .unwrap_or_else(|error| panic!("the {fixture} fixture compiles: {error}"))
    .ir
}

/// What the interpreter answers for each step: the outcome, or `undetermined` where it decides
/// none.
fn interpreted(ir: &spindle_spec::EssIr, steps: &[(&str, Value)]) -> Vec<String> {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "adversary.rowset/authored/rowset"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("rowset").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    steps
        .iter()
        .map(|(command, input)| {
            let input: BTreeMap<String, Node> = object(input.clone())
                .iter()
                .map(|(field, value)| {
                    (
                        field.clone(),
                        serde_json::from_value(value.clone()).expect("a node"),
                    )
                })
                .collect();
            match target.execute_command(SemanticCommandRequest {
                command: CommandRef::new(command.parse().expect("a command name")),
                actor: None,
                caller: None,
                input,
                correlation: context.correlation.clone(),
            }) {
                Ok(executed) => executed
                    .outcome
                    .map(|outcome| outcome.outcome.as_str().to_owned())
                    .unwrap_or_else(|| "no outcome".to_owned()),
                Err(error) => format!("undetermined: {error}"),
            }
        })
        .collect()
}

/// What the kernel answers for each step: the outcome, or `undetermined` where it decides none.
fn spindled(ir: &spindle_spec::EssIr, steps: &[(&str, Value)]) -> Vec<String> {
    let mut store = MemoryStore::default();
    steps
        .iter()
        .map(|(command, input)| {
            match execute(
                ir,
                &mut store,
                &mut NoExternals,
                command,
                &object(input.clone()),
            ) {
                Ok(executed) => executed.outcome,
                Err(error) => format!("undetermined: {error}"),
            }
        })
        .collect()
}

const ADD: &str = "yard.tasks.AddTask";
const FINISH: &str = "yard.tasks.Finish";
const SWEEP: &str = "yard.tasks.Sweep";

const ANN: &str = "a0a0a0a0-0000-4000-8000-000000000001";
const BOB: &str = "a0a0a0a0-0000-4000-8000-000000000002";
const CY: &str = "a0a0a0a0-0000-4000-8000-000000000003";
const T1: &str = "b0b0b0b0-0000-4000-8000-000000000001";
const T2: &str = "b0b0b0b0-0000-4000-8000-000000000002";
const T3: &str = "b0b0b0b0-0000-4000-8000-000000000003";
const T4: &str = "b0b0b0b0-0000-4000-8000-000000000004";
const T9: &str = "b0b0b0b0-0000-4000-8000-000000000009";
const S1: &str = "c0c0c0c0-0000-4000-8000-000000000001";
const S2: &str = "c0c0c0c0-0000-4000-8000-000000000002";

fn add(task: &str, owner: &str, weight: i64) -> Value {
    json!({"task_id": task, "owner_id": owner, "weight": weight})
}

/// `count:`, `forall:` (and its truth over no rows), a selector over the candidate's identity and
/// held state against `subject.`, and the addressed row's held state answering before the row set,
/// each against the interpreter. Green when written: the attack found no divergence here.
#[test]
fn counts_foralls_and_identity_agree_with_the_interpreter() {
    let ir = load("yard");
    let steps: Vec<(&str, Value)> = vec![
        (ADD, add(T1, ANN, 5)),
        (ADD, add(T2, ANN, 9)),
        (ADD, add(T3, ANN, 1)),
        (SWEEP, json!({"sweep_id": S1, "owner_id": ANN})),
        (SWEEP, json!({"sweep_id": S2, "owner_id": CY})),
        (FINISH, json!({"task_id": T1})),
        (ADD, add(T3, BOB, 1)),
        (SWEEP, json!({"sweep_id": S1, "owner_id": BOB})),
        (FINISH, json!({"task_id": T2})),
        (FINISH, json!({"task_id": T2})),
        (ADD, add(T4, ANN, 3)),
        (FINISH, json!({"task_id": T1})),
        (FINISH, json!({"task_id": T9})),
    ];
    let ess = interpreted(&ir, &steps);
    assert_eq!(
        ess,
        [
            "added",
            "added",
            "full",
            "all-kept",
            "all-kept",
            "not-heaviest",
            "added",
            "swept",
            "finished",
            "wrong-state",
            "added",
            "finished",
            "wrong-state",
        ],
        "the steps reach every branch through a count, a forall over some and over no rows, an \
         identity and the subject"
    );
    assert_eq!(spindled(&ir, &steps), ess);
}

/// ESS's upsert (beyond10x/ess#462, `docs/design/filtered-related-reads.md`, "Subject borrowing
/// and precedence"): on a command guarded by a row set, an absent addressed row that a creation
/// branch of the same entity takes from the same input field is that creation's, not an unknown
/// instance, and the row sets answer first. The interpreter answers `refused` (a library of the
/// curator is open) for a book nobody shelved yet. The kernel answers the absent book at the
/// existence step instead, as a missing instance the command declares no answer for.
#[test]
fn an_upsert_answers_its_row_set_for_an_absent_row() {
    let ir = load("shelf");
    let steps: Vec<(&str, Value)> = vec![
        (
            "demo.shelf.OpenLibrary",
            json!({"library": "l1", "curator": "ann"}),
        ),
        ("demo.shelf.Shelve", json!({"book": "b1", "curator": "ann"})),
    ];
    let ess = interpreted(&ir, &steps);
    assert_eq!(
        ess,
        ["opened", "refused"],
        "the interpreter reads the row set before the absent book"
    );
    assert_eq!(spindled(&ir, &steps), ess);
}

/// A store another writer commits to between the kernel's scan of `entity` and its commit: the
/// first scan of `entity` answers the rows as they were, and `raced` is committed right after.
struct Racing {
    inner: MemoryStore,
    entity: &'static str,
    raced: Option<Change>,
}

impl Store for Racing {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.inner.load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.inner.commit(change)
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.inner.remove(removal)
    }

    fn scan(&mut self, entity: &str) -> Result<Vec<(Value, Stored)>, StoreError> {
        let rows = self.inner.scan(entity)?;
        if entity == self.entity
            && let Some(raced) = self.raced.take()
        {
            self.inner.commit(&raced)?;
        }
        Ok(rows)
    }
}

const RESERVATION: &str = "stacks.library.Reservation";
const READER: &str = "a1a2a3a4-0000-4000-8000-000000000001";
const BOOK: &str = "0b0c0d0e-0000-4000-8000-000000000001";
const OURS: &str = "e1e2e3e4-0000-4000-8000-000000000001";
const THEIRS: &str = "e1e2e3e4-0000-4000-8000-000000000002";
const UNTIL: &str = "2026-06-01T00:00:00Z";

/// 2026-03-01T00:00:00Z, before [`UNTIL`].
fn library_now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_772_323_200)
}

/// `already-reserved` refuses a second active reservation of a book by one reader. Another writer
/// commits such a reservation after the kernel scanned the reservations and before it commits its
/// own. The kernel's decision rests on a row set that no longer holds, and its commit names only
/// the one instance it creates (`Change::expected: None`), so both commits land and the reader
/// holds two active reservations of the book: the state the guard exists to rule out.
#[test]
#[ignore = "a row committed by another writer between the scan and the commit is not detected; story:kernel-row-set-commit-conflict"]
fn a_row_committed_between_the_scan_and_the_commit_does_not_slip_past_the_guard() {
    let ir = load("library");
    let mut store = Racing {
        inner: MemoryStore::default(),
        entity: RESERVATION,
        raced: None,
    };
    let listed = execute_at(
        &ir,
        &mut store,
        &mut NoExternals,
        &mut UuidV4,
        &library_now,
        "stacks.library.ListEntry",
        &object(json!({"entry_id": "c1c2c3c4-0000-4000-8000-000000000001", "code": "QA-76"})),
    )
    .expect("the entry is listed");
    assert_eq!(listed.outcome, "listed");
    store.raced = Some(Change {
        entity: RESERVATION.to_owned(),
        identity: json!(THEIRS),
        expected: None,
        row: Row {
            state: "Active".to_owned(),
            fields: [
                ("reader_id".to_owned(), json!(READER)),
                ("book_id".to_owned(), json!(BOOK)),
                ("until".to_owned(), json!(UNTIL)),
            ]
            .into_iter()
            .collect(),
        },
        events: Vec::new(),
    });
    let ours = execute_at(
        &ir,
        &mut store,
        &mut NoExternals,
        &mut UuidV4,
        &library_now,
        "stacks.library.Reserve",
        &object(json!({
            "reservation_id": OURS,
            "reader_id": READER,
            "book_id": BOOK,
            "until": UNTIL,
            "shelfmarks": ["QA-76"],
        })),
    );
    let active: Vec<Value> = store
        .inner
        .scan(RESERVATION)
        .expect("the store scans")
        .into_iter()
        .filter(|(_, stored)| stored.row.state == "Active")
        .map(|(identity, _)| identity)
        .collect();
    assert_eq!(
        active,
        [json!(THEIRS)],
        "only the reservation committed first is active; the kernel answered {:?}",
        ours.map(|executed| executed.outcome)
    );
}
