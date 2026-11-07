//! Set outcomes (`instances:`, ess/16): the outcome selects every row of its entity whose filter
//! holds, read from the store as it was before the outcome, and moves, updates or removes each in
//! ascending order of identity. A row resting outside a move's `from` states is skipped, and zero
//! rows selected is a successful outcome. `{count: changed}` reads how many rows changed, and an
//! event declared once per changed row (ess/25) is published once for each, its `{changed: …}`
//! fields read from that row as it was before the outcome, as ESS's interpreter does
//! (`interpret::execute::set_effects`).
//!
//! The fixture `tests/fixtures/loans` lends books until a due time. `MarkOverdue` moves every loan
//! with `due <= input.as_of` from `Out` to `Overdue`, publishing `LoansMarked` with the count and
//! one `LoanMarked` per changed loan; `Renew` updates every loan of a borrower, and `Purge`
//! removes a borrower's returned loans.

use std::collections::BTreeMap;

use ess_primitives::node::Node;
use serde_json::{Map, Value, json};
use spindle_kernel::{Executed, MemoryStore, NoExternals, Row, SqliteStore, Store, execute};

use super::related_guards::{Answer, interpreted, spindled_with};

const LOAN: &str = "lending.loans.Loan";
const LEND: &str = "lending.loans.Lend";
const RETURN: &str = "lending.loans.Return";
const MARK: &str = "lending.loans.MarkOverdue";
const RENEW: &str = "lending.loans.Renew";
const PURGE: &str = "lending.loans.Purge";

const MARKED: &str = "lending.loans.LoansMarked";
const LOAN_MARKED: &str = "lending.loans.LoanMarked";

const JANUARY: &str = "2026-01-15T00:00:00Z";
const FEBRUARY: &str = "2026-02-15T00:00:00Z";
const MARCH: &str = "2026-03-01T00:00:00Z";
const JUNE: &str = "2026-06-01T00:00:00Z";

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/loans"),
    )
    .expect("the loans fixture compiles")
    .ir
}

fn object(value: Value) -> Map<String, Value> {
    let Value::Object(object) = value else {
        panic!("an object, not {value}")
    };
    object
}

fn run(ir: &spindle_spec::EssIr, store: &mut dyn Store, command: &str, input: Value) -> Executed {
    execute(ir, store, &mut NoExternals, command, &object(input.clone()))
        .unwrap_or_else(|error| panic!("{command} {input} runs: {error}"))
}

fn lend(ir: &spindle_spec::EssIr, store: &mut dyn Store, loan: &str, borrower: &str, due: &str) {
    let lent = run(
        ir,
        store,
        LEND,
        json!({"loan_id": loan, "borrower": borrower, "due": due}),
    );
    assert_eq!(lent.outcome, "lent");
}

fn row(store: &mut dyn Store, loan: &str) -> Option<Row> {
    store
        .load(LOAN, &json!(loan))
        .expect("the store reads")
        .map(|stored| stored.row)
}

fn state(store: &mut dyn Store, loan: &str) -> String {
    row(store, loan)
        .unwrap_or_else(|| panic!("{loan} is held"))
        .state
}

/// The events of `executed`, by name and payload.
fn events(executed: &Executed) -> Vec<(&str, Value)> {
    executed
        .events
        .iter()
        .map(|event| (event.name.as_str(), Value::Object(event.payload.clone())))
        .collect()
}

#[test]
fn a_set_outcome_moves_every_selected_row() {
    let ir = ir();
    let mut store = MemoryStore::default();
    lend(&ir, &mut store, "loan-1", "ada", JANUARY);
    lend(&ir, &mut store, "loan-2", "bea", FEBRUARY);
    lend(&ir, &mut store, "loan-3", "ada", JUNE);

    let marked = run(&ir, &mut store, MARK, json!({"as_of": MARCH}));

    assert_eq!(marked.outcome, "marked");
    assert_eq!(marked.error, None);
    assert_eq!(state(&mut store, "loan-1"), "Overdue");
    assert_eq!(state(&mut store, "loan-2"), "Overdue");
    assert_eq!(
        state(&mut store, "loan-3"),
        "Out",
        "a loan not yet due stays out"
    );
    assert_eq!(
        events(&marked).first(),
        Some(&(MARKED, json!({"count": 2}))),
        "the count is the two rows changed"
    );
    assert_eq!(
        events(&marked)
            .iter()
            .filter(|(name, _)| *name == LOAN_MARKED)
            .count(),
        2
    );
}

#[test]
fn rows_outside_the_move_s_from_states_are_skipped() {
    let ir = ir();
    let mut store = MemoryStore::default();
    lend(&ir, &mut store, "loan-1", "ada", JANUARY);
    lend(&ir, &mut store, "loan-2", "bea", FEBRUARY);
    let returned = run(&ir, &mut store, RETURN, json!({"loan_id": "loan-2"}));
    assert_eq!(returned.outcome, "returned");

    let marked = run(&ir, &mut store, MARK, json!({"as_of": MARCH}));

    assert_eq!(marked.outcome, "marked");
    assert_eq!(state(&mut store, "loan-1"), "Overdue");
    assert_eq!(
        state(&mut store, "loan-2"),
        "Returned",
        "a returned loan, due, is not moved"
    );
    assert_eq!(
        events(&marked),
        [
            (MARKED, json!({"count": 1})),
            (
                LOAN_MARKED,
                json!({"loan_id": "loan-1", "borrower": "ada", "due": JANUARY, "was": "Out"})
            ),
        ],
        "the returned loan is neither counted nor published"
    );
}

#[test]
fn zero_selected_rows_is_a_successful_outcome() {
    let ir = ir();
    let mut store = MemoryStore::default();
    lend(&ir, &mut store, "loan-1", "ada", JUNE);
    let before = row(&mut store, "loan-1");

    let marked = run(&ir, &mut store, MARK, json!({"as_of": MARCH}));

    assert_eq!(marked.outcome, "marked");
    assert_eq!(marked.error, None);
    assert_eq!(
        events(&marked),
        [(MARKED, json!({"count": 0}))],
        "a count of 0 and no per-row event"
    );
    assert_eq!(row(&mut store, "loan-1"), before, "nothing changed");
}

#[test]
fn one_event_per_changed_row_in_identity_order() {
    let ir = ir();
    let mut store = MemoryStore::default();
    lend(&ir, &mut store, "loan-c", "cid", FEBRUARY);
    lend(&ir, &mut store, "loan-a", "ada", JANUARY);
    lend(&ir, &mut store, "loan-b", "bea", MARCH);

    let marked = run(&ir, &mut store, MARK, json!({"as_of": MARCH}));

    assert_eq!(
        events(&marked),
        [
            (MARKED, json!({"count": 3})),
            (
                LOAN_MARKED,
                json!({"loan_id": "loan-a", "borrower": "ada", "due": JANUARY, "was": "Out"})
            ),
            (
                LOAN_MARKED,
                json!({"loan_id": "loan-b", "borrower": "bea", "due": MARCH, "was": "Out"})
            ),
            (
                LOAN_MARKED,
                json!({"loan_id": "loan-c", "borrower": "cid", "due": FEBRUARY, "was": "Out"})
            ),
        ],
        "each per-row event carries its row as it was before, in ascending identity order"
    );
}

const VIEWS: [(&str, &str, &str); 1] = [("lending.loans.LoanCard", LOAN, "loan_id")];

/// What one step answered, each payload value read as ESS's [`Node`], as a conformance target's
/// answer is compared: the interpreter writes an integral `Integer` as `2.0` and the kernel as
/// `2`, one number either way.
type NodeAnswer = (
    String,
    Option<String>,
    Vec<(String, BTreeMap<String, Node>)>,
);

fn as_nodes(answers: Vec<Answer>) -> Vec<NodeAnswer> {
    answers
        .into_iter()
        .map(|(outcome, error, events)| {
            let events = events
                .into_iter()
                .map(|(name, payload)| {
                    let payload = payload
                        .into_iter()
                        .map(|(field, value)| {
                            let node = serde_json::from_value(value.clone())
                                .unwrap_or_else(|_| panic!("{field} holds a node, not {value}"));
                            (field, node)
                        })
                        .collect();
                    (name, payload)
                })
                .collect();
            (outcome, error, events)
        })
        .collect()
}

#[test]
fn set_outcomes_agree_with_the_interpreter() {
    let ir = ir();
    let loan = |id: &str, borrower: &str, due: &str| {
        (
            LEND,
            json!({"loan_id": id, "borrower": borrower, "due": due}),
        )
    };
    let steps: Vec<(&str, Value)> = vec![
        (MARK, json!({"as_of": MARCH})),
        loan("loan-c", "ada", FEBRUARY),
        loan("loan-a", "ada", JANUARY),
        loan("loan-b", "bea", JUNE),
        loan("loan-d", "bea", JANUARY),
        (RETURN, json!({"loan_id": "loan-d"})),
        (MARK, json!({"as_of": MARCH})),
        (MARK, json!({"as_of": MARCH})),
        (RENEW, json!({"borrower": "ada", "until": JUNE})),
        (RENEW, json!({"borrower": "cid", "until": JUNE})),
        (MARK, json!({"as_of": JUNE})),
        (RETURN, json!({"loan_id": "loan-a"})),
        (PURGE, json!({"borrower": "ada"})),
        (PURGE, json!({"borrower": "bea"})),
        (PURGE, json!({"borrower": "bea"})),
    ];
    let (ess_answers, ess_rows) = interpreted(&ir, &VIEWS, &steps);
    let outcomes: Vec<&str> = ess_answers
        .iter()
        .map(|(outcome, _, _)| outcome.as_str())
        .collect();
    assert_eq!(
        outcomes,
        [
            "marked", "lent", "lent", "lent", "lent", "returned", "marked", "marked", "renewed",
            "renewed", "marked", "returned", "purged", "purged", "purged",
        ],
        "the steps move, update and remove rows, and select none"
    );
    let (answers, rows) = spindled_with(&VIEWS, &steps, &mut |store, command, input| {
        run(&ir, store, command, input)
    });
    assert_eq!(
        as_nodes(answers),
        as_nodes(ess_answers),
        "outcomes, errors and published payloads agree"
    );
    assert_eq!(
        rows, ess_rows,
        "the stored loans agree with ESS's view rows"
    );
}

#[test]
fn a_set_outcome_survives_a_restart_on_sqlite() {
    let ir = ir();
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    let path = dir.path().join("loans.sqlite");
    {
        let mut store = SqliteStore::open(&path).expect("SQLite opens");
        lend(&ir, &mut store, "loan-1", "ada", JANUARY);
        lend(&ir, &mut store, "loan-2", "bea", FEBRUARY);
        lend(&ir, &mut store, "loan-3", "ada", JUNE);
    }
    {
        let mut store = SqliteStore::open(&path).expect("SQLite reopens");
        let marked = run(&ir, &mut store, MARK, json!({"as_of": MARCH}));
        assert_eq!(
            events(&marked).first(),
            Some(&(MARKED, json!({"count": 2}))),
            "the selection reads the rows persisted before the restart"
        );
    }
    let mut store = SqliteStore::open(&path).expect("SQLite reopens again");
    assert_eq!(state(&mut store, "loan-1"), "Overdue");
    assert_eq!(state(&mut store, "loan-2"), "Overdue");
    assert_eq!(state(&mut store, "loan-3"), "Out");
}
