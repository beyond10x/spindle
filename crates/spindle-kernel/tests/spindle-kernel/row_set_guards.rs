//! `when_related:` over a row set (ess/22): every row of the selector's entity is a candidate, a
//! row is selected where `where:` holds of it (its fields, identity and held state, the input
//! under `input.`, the addressed subject under `subject.` and the decision's `now`), and the test
//! is decided over the selected rows, all read from the store as it was before selection, as
//! ESS's interpreter reads them (`interpret::execute::row_set`, `Reader::decide`, `decisions`).
//!
//! The fixture `tests/fixtures/library` lists catalogue entries by shelfmark. `Reserve` is
//! refused with `already-reserved` where the reader holds an `Active` reservation of the book that
//! has not run out by `now`, and with `unknown-shelfmark` where no entry's code is among the
//! input list `shelfmarks`. `Lend` is refused with `already-lent` where another reservation of the
//! subject's book is `Lent`.

use std::time::{Duration, SystemTime};

use ess_conformance::interpret::Interpreted;
use ess_conformance::occurrence_clock::{CommandClock, DecisionInstant};
use serde_json::{Value, json};
use spindle_kernel::{Executed, MemoryStore, NoExternals, Row, Store, UuidV4, execute_at};

use super::related_guards::{interpreted_on, object, spindled_with};

const ENTRY: &str = "stacks.library.Entry";
const RESERVATION: &str = "stacks.library.Reservation";
const LIST: &str = "stacks.library.ListEntry";
const RESERVE: &str = "stacks.library.Reserve";
const LEND: &str = "stacks.library.Lend";
const CANCEL: &str = "stacks.library.Cancel";

const ATLAS: &str = "0b0c0d0e-0000-4000-8000-000000000001";
const PRIMER: &str = "0b0c0d0e-0000-4000-8000-000000000002";
const ADA: &str = "a1a2a3a4-0000-4000-8000-000000000001";
const BEA: &str = "a1a2a3a4-0000-4000-8000-000000000002";
const FIRST: &str = "e1e2e3e4-0000-4000-8000-000000000001";
const SECOND: &str = "e1e2e3e4-0000-4000-8000-000000000002";
const THIRD: &str = "e1e2e3e4-0000-4000-8000-000000000003";
const FOURTH: &str = "e1e2e3e4-0000-4000-8000-000000000004";
const FIFTH: &str = "e1e2e3e4-0000-4000-8000-000000000005";
const SIXTH: &str = "e1e2e3e4-0000-4000-8000-000000000006";
const SHELF_ONE: &str = "c1c2c3c4-0000-4000-8000-000000000001";
const SHELF_TWO: &str = "c1c2c3c4-0000-4000-8000-000000000002";

/// The decision instant every step is read at: 2026-03-01T00:00:00Z.
const NOW: &str = "2026-03-01T00:00:00Z";
const NOW_SECONDS: u64 = 1_772_323_200;
/// A time after [`NOW`], so a reservation until it has not run out.
const LATER: &str = "2026-06-01T00:00:00Z";
/// A time before [`NOW`], so a reservation until it has run out.
const EARLIER: &str = "2026-01-01T00:00:00Z";

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/library"),
    )
    .expect("the library fixture compiles")
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

fn error(executed: &Executed) -> Option<&str> {
    executed.error.as_ref().map(|error| error.name.as_str())
}

fn row(store: &mut MemoryStore, entity: &str, identity: &str) -> Option<Row> {
    store
        .load(entity, &json!(identity))
        .expect("the store reads")
        .map(|stored| stored.row)
}

fn list(ir: &spindle_spec::EssIr, store: &mut MemoryStore, entry: &str, code: &str) {
    let listed = run(ir, store, LIST, json!({"entry_id": entry, "code": code}));
    assert_eq!(listed.outcome, "listed");
}

fn reserve(reservation: &str, reader: &str, book: &str, until: &str, marks: &[&str]) -> Value {
    json!({
        "reservation_id": reservation,
        "reader_id": reader,
        "book_id": book,
        "until": until,
        "shelfmarks": marks,
    })
}

#[test]
fn an_existing_matching_row_takes_the_row_set_branch() {
    let ir = ir();
    let mut store = MemoryStore::default();
    list(&ir, &mut store, SHELF_ONE, "QA-76");
    let first = run(
        &ir,
        &mut store,
        RESERVE,
        reserve(FIRST, ADA, ATLAS, LATER, &["QA-76"]),
    );
    assert_eq!(first.outcome, "reserved");
    assert_eq!(error(&first), None);
    assert_eq!(first.events.len(), 1);
    assert_eq!(first.events[0].name, "stacks.library.Reserved");
    let events = store.events().len();

    let second = run(
        &ir,
        &mut store,
        RESERVE,
        reserve(SECOND, ADA, ATLAS, LATER, &["QA-76"]),
    );
    assert_eq!(second.outcome, "already-reserved");
    assert_eq!(error(&second), Some("stacks.library.AlreadyReserved"));
    assert!(second.events.is_empty());
    assert_eq!(
        row(&mut store, RESERVATION, SECOND),
        None,
        "nothing is created"
    );
    assert_eq!(store.events().len(), events, "nothing is written");

    let other_reader = run(
        &ir,
        &mut store,
        RESERVE,
        reserve(SECOND, BEA, ATLAS, LATER, &["QA-76"]),
    );
    assert_eq!(
        other_reader.outcome, "reserved",
        "a row of another reader is not selected"
    );
    let other_book = run(
        &ir,
        &mut store,
        RESERVE,
        reserve(THIRD, ADA, PRIMER, LATER, &["QA-76"]),
    );
    assert_eq!(
        other_book.outcome, "reserved",
        "a row of another book is not selected"
    );
}

#[test]
fn a_row_in_another_state_does_not_match() {
    let ir = ir();
    let mut store = MemoryStore::default();
    list(&ir, &mut store, SHELF_ONE, "QA-76");
    run(
        &ir,
        &mut store,
        RESERVE,
        reserve(FIRST, ADA, ATLAS, LATER, &["QA-76"]),
    );
    let cancelled = run(&ir, &mut store, CANCEL, json!({"reservation_id": FIRST}));
    assert_eq!(cancelled.outcome, "cancelled");

    let again = run(
        &ir,
        &mut store,
        RESERVE,
        reserve(SECOND, ADA, ATLAS, LATER, &["QA-76"]),
    );
    assert_eq!(
        again.outcome, "reserved",
        "a cancelled reservation is not selected"
    );
    assert_eq!(
        row(&mut store, RESERVATION, SECOND).map(|row| row.state),
        Some("Active".to_owned())
    );

    let lapsed = run(
        &ir,
        &mut store,
        RESERVE,
        reserve(THIRD, BEA, PRIMER, EARLIER, &["QA-76"]),
    );
    assert_eq!(lapsed.outcome, "reserved");
    let after_lapse = run(
        &ir,
        &mut store,
        RESERVE,
        reserve(FOURTH, BEA, PRIMER, LATER, &["QA-76"]),
    );
    assert_eq!(
        after_lapse.outcome, "reserved",
        "an active reservation that ran out before `now` is not selected"
    );
}

#[test]
fn a_quantifier_over_an_input_list_selects_rows() {
    let ir = ir();
    let mut store = MemoryStore::default();
    list(&ir, &mut store, SHELF_ONE, "QA-76");
    list(&ir, &mut store, SHELF_TWO, "PR-20");

    let refused = run(
        &ir,
        &mut store,
        RESERVE,
        reserve(FIRST, ADA, ATLAS, LATER, &["ZZ-1", "ZZ-2"]),
    );
    assert_eq!(refused.outcome, "unknown-shelfmark");
    assert_eq!(error(&refused), Some("stacks.library.UnknownShelfmark"));
    assert_eq!(row(&mut store, RESERVATION, FIRST), None);

    let empty = run(
        &ir,
        &mut store,
        RESERVE,
        reserve(FIRST, ADA, ATLAS, LATER, &[]),
    );
    assert_eq!(
        empty.outcome, "unknown-shelfmark",
        "an empty list selects no entry"
    );

    let matched = run(
        &ir,
        &mut store,
        RESERVE,
        reserve(FIRST, ADA, ATLAS, LATER, &["ZZ-1", "PR-20"]),
    );
    assert_eq!(
        matched.outcome, "reserved",
        "an entry whose code is one of the shelfmarks is selected"
    );
}

#[test]
fn a_selector_reads_the_addressed_subject() {
    let ir = ir();
    let mut store = MemoryStore::default();
    list(&ir, &mut store, SHELF_ONE, "QA-76");
    for (reservation, reader) in [(FIRST, ADA), (SECOND, BEA)] {
        run(
            &ir,
            &mut store,
            RESERVE,
            reserve(reservation, reader, ATLAS, LATER, &["QA-76"]),
        );
    }
    run(
        &ir,
        &mut store,
        RESERVE,
        reserve(THIRD, ADA, PRIMER, LATER, &["QA-76"]),
    );

    let lent = run(&ir, &mut store, LEND, json!({"reservation_id": FIRST}));
    assert_eq!(lent.outcome, "lent");
    let events = store.events().len();
    let refused = run(&ir, &mut store, LEND, json!({"reservation_id": SECOND}));
    assert_eq!(
        refused.outcome, "already-lent",
        "the atlas the subject reserves is out"
    );
    assert_eq!(error(&refused), Some("stacks.library.AlreadyLent"));
    assert_eq!(store.events().len(), events);
    assert_eq!(
        row(&mut store, RESERVATION, SECOND).map(|row| row.state),
        Some("Active".to_owned())
    );
    let other_book = run(&ir, &mut store, LEND, json!({"reservation_id": THIRD}));
    assert_eq!(other_book.outcome, "lent", "the primer is not out");

    let wrong = run(&ir, &mut store, LEND, json!({"reservation_id": FIRST}));
    assert_eq!(
        wrong.outcome, "wrong-state",
        "a lent reservation answers its held state before the row set is read"
    );
}

const VIEWS: [(&str, &str, &str); 2] = [
    ("stacks.library.EntryCard", ENTRY, "entry_id"),
    (
        "stacks.library.ReservationCard",
        RESERVATION,
        "reservation_id",
    ),
];

/// The interpreter's command clock: every decision reads [`NOW`].
struct Fixed;

impl CommandClock for Fixed {
    fn read(&self) -> Option<DecisionInstant> {
        Some(DecisionInstant::parse(NOW).expect("an instant"))
    }
}

#[test]
fn row_set_guards_agree_with_the_interpreter() {
    let ir = ir();
    let steps: Vec<(&str, Value)> = vec![
        (LIST, json!({"entry_id": SHELF_ONE, "code": "QA-76"})),
        (RESERVE, reserve(FIRST, ADA, ATLAS, LATER, &["ZZ-1"])),
        (RESERVE, reserve(FIRST, ADA, ATLAS, LATER, &[])),
        (
            RESERVE,
            reserve(FIRST, ADA, ATLAS, LATER, &["ZZ-1", "QA-76"]),
        ),
        (RESERVE, reserve(SECOND, ADA, ATLAS, LATER, &["QA-76"])),
        (RESERVE, reserve(SECOND, BEA, ATLAS, LATER, &["QA-76"])),
        (RESERVE, reserve(THIRD, ADA, PRIMER, EARLIER, &["QA-76"])),
        (RESERVE, reserve(FOURTH, ADA, PRIMER, LATER, &["QA-76"])),
        (LEND, json!({"reservation_id": FIRST})),
        (LEND, json!({"reservation_id": SECOND})),
        (LEND, json!({"reservation_id": FIRST})),
        (CANCEL, json!({"reservation_id": SECOND})),
        (LEND, json!({"reservation_id": SECOND})),
        (RESERVE, reserve(FIFTH, BEA, ATLAS, LATER, &["QA-76"])),
        (LEND, json!({"reservation_id": FOURTH})),
        (RESERVE, reserve(SIXTH, ADA, ATLAS, LATER, &["QA-76"])),
    ];
    let (ess_answers, ess_rows) = interpreted_on(
        Interpreted::for_model(ir.clone()).with_command_clock(Fixed),
        &VIEWS,
        &steps,
    );
    let outcomes: Vec<&str> = ess_answers
        .iter()
        .map(|(outcome, _, _)| outcome.as_str())
        .collect();
    assert_eq!(
        outcomes,
        [
            "listed",
            "unknown-shelfmark",
            "unknown-shelfmark",
            "reserved",
            "already-reserved",
            "reserved",
            "reserved",
            "reserved",
            "lent",
            "already-lent",
            "wrong-state",
            "cancelled",
            "wrong-state",
            "reserved",
            "lent",
            "reserved",
        ],
        "the steps reach every branch a row set decides, through a field, a state, `now`, an \
         input list and the subject"
    );
    let (answers, rows) = spindled_with(&VIEWS, &steps, &mut |store, command, input| {
        run(&ir, store, command, input)
    });
    assert_eq!(
        answers, ess_answers,
        "outcomes, errors and published payloads agree"
    );
    assert_eq!(
        rows, ess_rows,
        "the stored entries and reservations agree with ESS's view rows"
    );
}

/// The `unknown-shelfmark` selector over the input list.
const OVER_THE_LIST: &str = "exists: {in: input.shelfmarks, as: mark, that: code == mark}";

#[test]
fn a_rows_quantifier_in_a_selector_is_refused() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/library");
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    std::fs::create_dir(dir.path().join("domains")).expect("the domains directory");
    for file in ["system.yaml", "domains/library.yaml"] {
        let text = std::fs::read_to_string(fixture.join(file)).expect("the fixture reads");
        assert!(
            file == "system.yaml" || text.contains(OVER_THE_LIST),
            "the fixture selects entries over the input list"
        );
        let text = text.replace(
            OVER_THE_LIST,
            "exists: {in: {entity: stacks.library.Entry}, as: other, that: other.code == code}",
        );
        std::fs::write(dir.path().join(file), text).expect("the copy writes");
    }
    let ir = spindle_spec::load(dir.path())
        .unwrap_or_else(|error| panic!("the library with a rows quantifier compiles: {error}"))
        .ir;
    let mut store = MemoryStore::default();
    list(&ir, &mut store, SHELF_ONE, "QA-76");
    let events = store.events().len();
    let refused = execute_at(
        &ir,
        &mut store,
        &mut NoExternals,
        &mut UuidV4,
        &now,
        RESERVE,
        &object(reserve(FIRST, ADA, ATLAS, LATER, &["QA-76"])),
    );
    match refused {
        Err(spindle_kernel::ExecuteError::Unsupported { construct }) => assert_eq!(
            construct,
            "a rows quantifier in a `when_related:` row-set selector, in \
             `stacks.library.Reserve/unknown-shelfmark`"
        ),
        other => panic!("a rows quantifier is refused by name, not {other:?}"),
    }
    assert_eq!(row(&mut store, RESERVATION, FIRST), None);
    assert_eq!(store.events().len(), events, "nothing is written");
}

/// ESS's upsert fixture (`tests/fixtures/shelf`) as ESS's own adversary pass varies it
/// (`adversary_row_set_upsert_462.rs`, `input_guarded_creation`): `Shelve` declares
/// `unknown_instance:` and creates a book only where the input asks for a new one.
fn shelf_by_mode(dir: &std::path::Path) -> spindle_spec::EssIr {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/shelf");
    std::fs::create_dir(dir.join("domains")).expect("the domains directory");
    std::fs::copy(fixture.join("system.yaml"), dir.join("system.yaml")).expect("the system copies");
    let text = std::fs::read_to_string(fixture.join("domains/shelf.yaml")).expect("the domain");
    let (head, tail) = text
        .split_once("  - name: demo.shelf.Shelve\n")
        .expect("the Shelve command");
    let (shelve, rest) = tail
        .split_once("  - name: demo.shelf.Restock\n")
        .expect("the next command");
    let changed = [
        (
            "      - {name: curator, type: String}\n",
            "      - {name: curator, type: String}\n      - {name: mode, type: String}\n",
        ),
        (
            "    outcomes:\n",
            "    outcomes:\n      - name: unknown-book\n        unknown_instance: true\n        \
             error: demo.shelf.UnknownBook\n",
        ),
        (
            "external: the book is not yet shelved",
            "when: mode == \"new\"",
        ),
    ]
    .into_iter()
    .fold(shelve.to_owned(), |shelve, (from, to)| {
        assert!(shelve.contains(from), "`{from}` is in Shelve");
        shelve.replacen(from, to, 1)
    });
    std::fs::write(
        dir.join("domains/shelf.yaml"),
        format!("{head}  - name: demo.shelf.Shelve\n{changed}  - name: demo.shelf.Restock\n{rest}"),
    )
    .expect("the variant writes");
    spindle_spec::load(dir)
        .unwrap_or_else(|error| panic!("the shelf by mode compiles: {error}"))
        .ir
}

/// On a row-set upsert (beyond10x/ess#462) an absent book the creation takes from the same input
/// field is left to the row sets, then to the branch selected after them: refused while a library
/// of the curator is open, created where the input asks for a new book, and the declared unknown
/// instance where the selected branch updates a book nobody holds (ESS `addressed_row`,
/// `creation_takes_absent`, `take`). `Restock`, with no creation, answers the absent book first.
#[test]
fn an_upsert_leaves_an_absent_row_to_the_row_sets_as_the_interpreter_does() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    let ir = shelf_by_mode(dir.path());
    let shelve = |book: &str, mode: &str| {
        (
            "demo.shelf.Shelve",
            json!({"book": book, "curator": "ann", "mode": mode}),
        )
    };
    let steps: Vec<(&str, Value)> = vec![
        (
            "demo.shelf.OpenLibrary",
            json!({"library": "decoy", "curator": "bob"}),
        ),
        shelve("b1", "old"),
        shelve("b1", "new"),
        shelve("b1", "old"),
        (
            "demo.shelf.Restock",
            json!({"book": "b9", "curator": "ann"}),
        ),
        (
            "demo.shelf.OpenLibrary",
            json!({"library": "mine", "curator": "ann"}),
        ),
        shelve("b2", "new"),
        shelve("b2", "old"),
        shelve("b1", "old"),
    ];
    let views = [("demo.shelf.Books", "demo.shelf.Book", "book")];
    let (ess_answers, ess_rows) =
        interpreted_on(Interpreted::for_model(ir.clone()), &views, &steps);
    let outcomes: Vec<&str> = ess_answers
        .iter()
        .map(|(outcome, _, _)| outcome.as_str())
        .collect();
    assert_eq!(
        outcomes,
        [
            "opened",
            "unknown-book",
            "added",
            "replaced",
            "unknown-book",
            "opened",
            "refused",
            "refused",
            "refused",
        ],
        "the steps reach the unknown instance after the row sets, the creation, the update and \
         the row set over an absent and a held book"
    );
    let (answers, rows) = spindled_with(&views, &steps, &mut |store, command, input| {
        run(&ir, store, command, input)
    });
    assert_eq!(
        answers, ess_answers,
        "outcomes, errors and published payloads agree"
    );
    assert_eq!(
        rows, ess_rows,
        "the stored books agree with ESS's view rows"
    );
}
