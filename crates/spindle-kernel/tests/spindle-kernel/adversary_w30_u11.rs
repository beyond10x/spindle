//! Adversary cases of wave 30 U11 (`story:affects-over-input-list`): an `affects:` entry over an
//! input list (`each:`) read against ESS's interpreter at 0.99.2 and on the memory and SQLite
//! stores, where the unit's own cases leave it out.
//!
//! The fixture `tests/fixtures/adversary-w30-u11` holds books, their entries, and cells and
//! counters named by a `Decimal` and an `Integer` identity. `Post` writes one entry per line and
//! then posts every draft among them (two `each:` entries over one list: a create or update, then a
//! move); `PostFirst` posts first and writes after. `Note` writes only the lines tagged `keep`, a
//! filter that a line without a tag leaves unknown. `Stamp` mints a token per entry. `Label` and
//! `Count` label cells and counters, and `Tie` labels every cell it lists but its own subject.
//! Every entry holds a non-negative `qty` (an entity invariant).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ess_compiler::refs::{CommandRef, ViewRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::target::{
    ConformanceTarget, Deadline, ScenarioContext, SemanticCommandRequest, SemanticViewRequest,
};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use ess_primitives::time::Timestamp;
use serde_json::{Map, Value, json};
use spindle_kernel::{
    Change, ExecuteError, Executed, MemoryStore, NoExternals, Removal, Row, Scan, SqliteStore,
    Store, StoreError, Stored, Write, execute,
};

const BOOK: &str = "ledger.book.Book";
const ENTRY: &str = "ledger.book.Entry";
const CELL: &str = "ledger.book.Cell";
const COUNTER: &str = "ledger.book.Counter";

const OPEN: &str = "ledger.book.OpenBook";
const ADD_ENTRY: &str = "ledger.book.AddEntry";
const ADD_CELL: &str = "ledger.book.AddCell";
const ADD_COUNTER: &str = "ledger.book.AddCounter";
const POST: &str = "ledger.book.Post";
const POST_FIRST: &str = "ledger.book.PostFirst";
const NOTE: &str = "ledger.book.Note";
const STAMP: &str = "ledger.book.Stamp";
const LABEL: &str = "ledger.book.Label";
const COUNT: &str = "ledger.book.Count";
const TIE: &str = "ledger.book.Tie";

/// Each view, the entity it projects and that entity's identity field.
const VIEWS: [(&str, &str, &str); 4] = [
    ("ledger.book.BookCard", BOOK, "book_id"),
    ("ledger.book.EntryCard", ENTRY, "line_id"),
    ("ledger.book.CellCard", CELL, "slot"),
    ("ledger.book.CounterCard", COUNTER, "n"),
];

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/adversary-w30-u11")
}

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(&fixture())
        .expect("the adversary-w30-u11 fixture compiles")
        .ir
}

fn object(value: Value) -> Map<String, Value> {
    let Value::Object(object) = value else {
        panic!("an object, not {value}")
    };
    object
}

fn send(
    ir: &spindle_spec::EssIr,
    store: &mut dyn Store,
    command: &str,
    input: &Value,
) -> Result<Executed, ExecuteError> {
    execute(ir, store, &mut NoExternals, command, &object(input.clone()))
}

fn run(ir: &spindle_spec::EssIr, store: &mut dyn Store, command: &str, input: &Value) -> Executed {
    send(ir, store, command, input)
        .unwrap_or_else(|error| panic!("{command} {input} runs: {error}"))
}

fn node(value: &Value) -> Node {
    serde_json::from_value(value.clone()).unwrap_or_else(|_| panic!("a node, not {value}"))
}

fn json_of(node: &Node) -> Value {
    serde_json::to_value(node).expect("a node is JSON")
}

/// What one step answered, every value read as ESS's [`Node`]: the outcome, the declared error
/// and the events by name and payload; `Err` where the command did not run.
type Answer = Result<
    (
        String,
        Option<String>,
        Vec<(String, BTreeMap<String, Node>)>,
    ),
    (),
>;

/// The rows of each view after the last step, `null` members left out, every value a [`Node`],
/// each view's rows in one order.
type Rows = Vec<Vec<BTreeMap<String, Node>>>;

fn sorted(mut rows: Vec<BTreeMap<String, Node>>) -> Vec<BTreeMap<String, Node>> {
    rows.sort_by_key(|row| format!("{row:?}"));
    rows
}

/// What ESS's interpreter answers for each step, and the rows of every view after the last.
fn interpreted(ir: &spindle_spec::EssIr, steps: &[(&str, Value)]) -> (Vec<Answer>, Rows) {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "ledger.book/authored/adversary"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("adversary").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let mut answers = Vec::new();
    for (command, input) in steps {
        let input = object(input.clone())
            .iter()
            .map(|(field, value)| (field.clone(), node(value)))
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
                                .map(|(field, value)| (field.clone(), node(&json_of(value))))
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
            sorted(
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
                        row.iter()
                            .filter(|(_, value)| json_of(value) != Value::Null)
                            .map(|(field, value)| (field.clone(), node(&json_of(value))))
                            .collect()
                    })
                    .collect(),
            )
        })
        .collect();
    (answers, rows)
}

/// What the kernel answers for each step on `store`, and each stored row of every view's entity
/// as the view projects it (the entry's `token`, which no view shows, left out).
fn spindled(
    ir: &spindle_spec::EssIr,
    store: &mut dyn Store,
    steps: &[(&str, Value)],
) -> (Vec<Answer>, Rows) {
    let answers = steps
        .iter()
        .map(|(command, input)| {
            send(ir, store, command, input)
                .map_err(|_| ())
                .map(|executed| {
                    (
                        executed.outcome,
                        executed.error.map(|error| error.name),
                        executed
                            .events
                            .into_iter()
                            .map(|event| {
                                (
                                    event.name,
                                    event
                                        .payload
                                        .iter()
                                        .map(|(field, value)| (field.clone(), node(value)))
                                        .collect(),
                                )
                            })
                            .collect(),
                    )
                })
        })
        .collect();
    let rows = VIEWS
        .iter()
        .map(|(_, entity, identity_field)| {
            sorted(
                store
                    .scan(entity)
                    .expect("the store scans")
                    .into_iter()
                    .map(|(identity, stored)| {
                        let mut row: BTreeMap<String, Node> = stored
                            .row
                            .fields
                            .iter()
                            .filter(|(field, value)| *field != "token" && !value.is_null())
                            .map(|(field, value)| (field.clone(), node(value)))
                            .collect();
                        row.insert((*identity_field).to_owned(), node(&identity));
                        row.insert("state".to_owned(), Node::Text(stored.row.state.clone()));
                        row
                    })
                    .collect(),
            )
        })
        .collect();
    (answers, rows)
}

/// Each store a case runs on, by name: memory, and SQLite in a directory of its own.
fn stores() -> Vec<(&'static str, Box<dyn Store>, Option<tempfile::TempDir>)> {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    let sqlite = SqliteStore::open(&dir.path().join("ledger.sqlite")).expect("SQLite opens");
    vec![
        ("memory", Box::new(MemoryStore::default()), None),
        ("sqlite", Box::new(sqlite), Some(dir)),
    ]
}

/// Runs `steps` through the interpreter and through the kernel on every store, and asserts each
/// store answers and holds what the interpreter does; returns the interpreter's answers.
fn agree(steps: &[(&str, Value)]) -> Vec<Answer> {
    let ir = ir();
    let (ess_answers, ess_rows) = interpreted(&ir, steps);
    for (name, mut store, _dir) in stores() {
        let (answers, rows) = spindled(&ir, store.as_mut(), steps);
        for (index, ((command, input), (ours, theirs))) in steps
            .iter()
            .zip(answers.iter().zip(ess_answers.iter()))
            .enumerate()
        {
            assert_eq!(
                ours, theirs,
                "{name}: step {index} {command} {input} answers as the interpreter does"
            );
        }
        assert_eq!(
            rows, ess_rows,
            "{name}: every row agrees with the interpreter's views"
        );
    }
    ess_answers
}

fn stored(store: &mut dyn Store, entity: &str, identity: &Value) -> Option<Stored> {
    store.load(entity, identity).expect("the store reads")
}

fn books_and_drafts() -> Vec<(&'static str, Value)> {
    vec![
        (OPEN, json!({"book_id": "b1"})),
        (
            ADD_ENTRY,
            json!({"line_id": "d1", "qty": 1, "memo": "first"}),
        ),
        (
            ADD_ENTRY,
            json!({"line_id": "d2", "qty": 2, "memo": "second"}),
        ),
        (ADD_ENTRY, json!({"line_id": "d3", "qty": 3})),
    ]
}

fn outcomes(answers: &[Answer]) -> Vec<String> {
    answers
        .iter()
        .map(|answer| match answer {
            Ok((outcome, _, _)) => outcome.clone(),
            Err(()) => "<refused>".to_owned(),
        })
        .collect()
}

/// An element that leaves out an `Optional` member, or holds it `null`, clears the field the
/// member fills in the held row it names (ESS `elements`: `after.fields.remove`), on both stores.
#[test]
fn an_optional_member_left_out_clears_the_held_field_as_the_interpreter_does() {
    let mut steps = books_and_drafts();
    steps.push((
        POST,
        json!({"book_id": "b1", "lines": [
            {"line_id": "d1", "qty": 5},
            {"line_id": "d2", "qty": 6, "memo": null},
        ]}),
    ));
    let answers = agree(&steps);
    assert_eq!(outcomes(&answers)[4], "posted");

    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        for (command, input) in &steps {
            run(&ir, store, command, input);
        }
        for line in ["d1", "d2"] {
            let row = stored(store, ENTRY, &json!(line))
                .unwrap_or_else(|| panic!("{name}: {line} is held"))
                .row;
            assert_eq!(
                row.fields.get("memo"),
                None,
                "{name}: {line}'s memo is cleared by an element without one"
            );
            assert_eq!(row.state, "Posted", "{name}: {line} is posted");
        }
    }
}

/// Two `each:` entries over one list, one creating or updating and one moving: a line naming a
/// held draft updates and posts it, one naming a posted entry updates it and skips the move, one
/// naming no entry creates it and the move posts what was just created. Posting first moves only
/// held drafts, and the write after it creates the rest as drafts.
#[test]
fn creates_and_moves_over_one_list_agree_with_the_interpreter() {
    let mut steps = books_and_drafts();
    steps.extend([
        (
            POST,
            json!({"book_id": "b1", "lines": [{"line_id": "d2", "qty": 2}]}),
        ),
        (
            POST,
            json!({"book_id": "b1", "lines": [
                {"line_id": "d1", "qty": 10},
                {"line_id": "d2", "qty": 20, "memo": "again"},
                {"line_id": "n1", "qty": 30, "memo": "new"},
            ]}),
        ),
        (
            POST_FIRST,
            json!({"book_id": "b1", "lines": [
                {"line_id": "d3", "qty": 40},
                {"line_id": "n2", "qty": 50},
            ]}),
        ),
    ]);
    let answers = agree(&steps);
    assert_eq!(outcomes(&answers)[4..], ["posted", "posted", "posted"]);

    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        for (command, input) in &steps {
            run(&ir, store, command, input);
        }
        let state = |store: &mut dyn Store, line: &str| {
            stored(store, ENTRY, &json!(line))
                .unwrap_or_else(|| panic!("{name}: {line} is held"))
                .row
        };
        let n1 = state(store, "n1");
        assert_eq!(n1.state, "Posted", "{name}: the row created is moved too");
        assert_eq!(n1.fields.get("qty"), Some(&json!(30)), "{name}");
        assert_eq!(state(store, "d1").state, "Posted", "{name}");
        assert_eq!(
            state(store, "d2").fields.get("qty"),
            Some(&json!(20)),
            "{name}: a posted entry is updated, its move skipped"
        );
        assert_eq!(state(store, "d3").state, "Posted", "{name}");
        assert_eq!(
            state(store, "d3").fields.get("qty"),
            Some(&json!(40)),
            "{name}: posted, then written"
        );
        let n2 = state(store, "n2");
        assert_eq!(
            n2.state, "Draft",
            "{name}: posting first skips a line naming no entry; the write after creates it"
        );
    }
}

/// An element filter that reads an `Optional` member the element leaves out is `Unknown`, which
/// selects nothing (ESS `ElementFacts::selects`: only `Truth::True`); a tag other than `keep` is
/// skipped as well.
#[test]
fn an_element_filter_left_unknown_skips_the_element_as_the_interpreter_does() {
    let mut steps = books_and_drafts();
    steps.push((
        NOTE,
        json!({"book_id": "b1", "lines": [
            {"line_id": "d1", "qty": 11, "memo": "kept", "tag": "keep"},
            {"line_id": "d2", "qty": 12, "memo": "untagged"},
            {"line_id": "n3", "qty": 13, "memo": "dropped", "tag": "drop"},
            {"line_id": "n4", "qty": 14, "tag": "keep"},
        ]}),
    ));
    let answers = agree(&steps);
    assert_eq!(outcomes(&answers)[4], "noted");

    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        for (command, input) in &steps[..4] {
            run(&ir, store, command, input);
        }
        let d2 = stored(store, ENTRY, &json!("d2"));
        let (command, input) = &steps[4];
        run(&ir, store, command, input);
        assert_eq!(
            stored(store, ENTRY, &json!("d2")),
            d2,
            "{name}: an element without a tag is not selected"
        );
        assert_eq!(stored(store, ENTRY, &json!("n3")), None, "{name}");
        assert_eq!(
            stored(store, ENTRY, &json!("n4")).map(|held| held.row.fields.get("qty").cloned()),
            Some(Some(json!(14))),
            "{name}: a tagged element naming no entry creates it"
        );
    }
}

/// `{generated: true}` in an `each:` entry's `sets:` mints a value for each row it writes: two
/// entries created by one list hold two different tokens, and a held one comes to hold a third.
#[test]
fn each_row_an_element_writes_mints_its_own_token() {
    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        for (command, input) in books_and_drafts() {
            run(&ir, store, command, &input);
        }
        let stamped = run(
            &ir,
            store,
            STAMP,
            &json!({"book_id": "b1", "lines": [
                {"line_id": "n1", "qty": 1},
                {"line_id": "n2", "qty": 2},
                {"line_id": "d1", "qty": 3},
            ]}),
        );
        assert_eq!(stamped.outcome, "stamped", "{name}");
        let tokens: Vec<Value> = ["n1", "n2", "d1"]
            .iter()
            .map(|line| {
                stored(store, ENTRY, &json!(line))
                    .and_then(|held| held.row.fields.get("token").cloned())
                    .unwrap_or_else(|| panic!("{name}: {line} holds a token"))
            })
            .collect();
        assert_ne!(tokens[0], tokens[1], "{name}: {tokens:?}");
        assert_ne!(tokens[0], tokens[2], "{name}: {tokens:?}");
        assert_ne!(tokens[1], tokens[2], "{name}: {tokens:?}");
    }
}

/// One element breaking an entity invariant (`qty >= 0`) refuses the whole command, as ESS's
/// interpreter does: nothing of the group is written, not the subject's own effect, not the held
/// row an earlier element updated and not the row an earlier element created.
#[test]
fn one_element_breaking_an_invariant_writes_nothing_of_the_group() {
    let mut steps = books_and_drafts();
    steps.push((
        POST,
        json!({"book_id": "b1", "lines": [
            {"line_id": "d1", "qty": 7},
            {"line_id": "n1", "qty": 8},
            {"line_id": "n2", "qty": -1},
        ]}),
    ));
    let answers = agree(&steps);
    assert_eq!(answers[4], Err(()), "the interpreter refuses the command");

    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        for (command, input) in &steps[..4] {
            run(&ir, store, command, input);
        }
        let book = stored(store, BOOK, &json!("b1"));
        let d1 = stored(store, ENTRY, &json!("d1"));
        let (command, input) = &steps[4];
        let refused = send(&ir, store, command, input);
        assert!(
            matches!(refused, Err(ExecuteError::BrokenInvariant { .. })),
            "{name}: {refused:?}"
        );
        assert_eq!(stored(store, BOOK, &json!("b1")), book, "{name}: subject");
        assert_eq!(stored(store, ENTRY, &json!("d1")), d1, "{name}: d1");
        assert_eq!(stored(store, ENTRY, &json!("n1")), None, "{name}: n1");
        assert_eq!(stored(store, ENTRY, &json!("n2")), None, "{name}: n2");
    }
}

/// A `Decimal` identity written `1.0`, `2.50` or `3e0` names the cell held as `1`, `2.5` or `3`,
/// on both stores: no second cell is created.
#[test]
fn decimal_identities_spelled_otherwise_name_the_held_cells() {
    let steps = vec![
        (OPEN, json!({"book_id": "b1"})),
        (ADD_CELL, json!({"slot": 1, "label": "one"})),
        (ADD_CELL, json!({"slot": 2.5, "label": "two"})),
        (ADD_CELL, json!({"slot": 3, "label": "three"})),
        (
            LABEL,
            json!({"book_id": "b1", "cells": [
                {"slot": 1.0, "label": "a"},
                {"slot": 2.50, "label": "b"},
                {"slot": 3e0, "label": "c"},
                {"slot": 4.0, "label": "d"},
            ]}),
        ),
    ];
    let answers = agree(&steps);
    assert_eq!(outcomes(&answers)[4], "labelled");

    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        for (command, input) in &steps {
            run(&ir, store, command, input);
        }
        let held: Vec<(Value, Value)> = store
            .scan(CELL)
            .expect("the store scans")
            .into_iter()
            .map(|(identity, stored)| {
                (
                    identity,
                    stored.row.fields.get("label").cloned().unwrap_or_default(),
                )
            })
            .collect();
        assert_eq!(
            held,
            [
                (json!(1), json!("a")),
                (json!(2.5), json!("b")),
                (json!(3), json!("c")),
                (json!(4), json!("d")),
            ],
            "{name}: one cell per number"
        );
    }
}

/// A `Decimal` identity written `-0.0` names the cell held as `0`, as ESS's interpreter reads it.
#[test]
fn a_negative_zero_identity_names_the_cell_held_as_zero() {
    let steps = vec![
        (OPEN, json!({"book_id": "b1"})),
        (ADD_CELL, json!({"slot": 0, "label": "zero"})),
        (
            LABEL,
            json!({"book_id": "b1", "cells": [{"slot": -0.0, "label": "signed"}]}),
        ),
    ];
    let answers = agree(&steps);
    assert_eq!(outcomes(&answers)[2], "labelled");
}

/// An `Integer` identity written `1.0` in the list names the counter held as `1`, as ESS's
/// interpreter reads it.
#[test]
fn an_integer_identity_written_with_a_fraction_agrees_with_the_interpreter() {
    let steps = vec![
        (OPEN, json!({"book_id": "b1"})),
        (ADD_COUNTER, json!({"n": 1, "label": "one"})),
        (
            COUNT,
            json!({"book_id": "b1", "counters": [
                {"n": 1.0, "label": "x"},
                {"n": 2, "label": "y"},
            ]}),
        ),
    ];
    agree(&steps);
}

/// A list naming one cell as `1` and as `1.0` names it twice: the `distinct:` refusal declared
/// beside the entry answers it, as ESS's interpreter does, and nothing is written.
#[test]
fn one_cell_named_in_two_spellings_is_not_distinct() {
    let steps = vec![
        (OPEN, json!({"book_id": "b1"})),
        (ADD_CELL, json!({"slot": 1, "label": "one"})),
        (
            LABEL,
            json!({"book_id": "b1", "cells": [
                {"slot": 1, "label": "a"},
                {"slot": 1.0, "label": "b"},
            ]}),
        ),
    ];
    let answers = agree(&steps);
    assert_eq!(outcomes(&answers)[2], "duplicated");
}

/// An element naming the subject in another spelling (`1.0` for the cell addressed as `1`, and
/// `1` for one addressed as `1.0`) is skipped: the subject holds what its own effect writes.
#[test]
fn an_element_naming_the_subject_in_another_spelling_is_skipped() {
    let steps = vec![
        (ADD_CELL, json!({"slot": 1, "label": "one"})),
        (ADD_CELL, json!({"slot": 2, "label": "two"})),
        (
            TIE,
            json!({"slot": 1, "cells": [
                {"slot": 1.0, "label": "self"},
                {"slot": 2.5, "label": "other"},
            ]}),
        ),
        (
            TIE,
            json!({"slot": 2.0, "cells": [
                {"slot": 2, "label": "self"},
                {"slot": 3, "label": "third"},
            ]}),
        ),
    ];
    let answers = agree(&steps);
    assert_eq!(outcomes(&answers)[2..], ["tied", "tied"]);

    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        for (command, input) in &steps {
            run(&ir, store, command, input);
        }
        for slot in [json!(1), json!(2)] {
            assert_eq!(
                stored(store, CELL, &slot).map(|held| held.row.fields.get("label").cloned()),
                Some(Some(json!("tied"))),
                "{name}: cell {slot} holds its own effect"
            );
        }
    }
}

/// A store that lets another writer land one change just before the kernel commits its group:
/// the row an `each:` element would create, written by someone else between the scan and the
/// commit.
struct Racing {
    inner: Box<dyn Store>,
    rival: RefCell<Option<Change>>,
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

    fn commit_all(&mut self, writes: &[Write]) -> Result<(), StoreError> {
        if let Some(rival) = self.rival.borrow_mut().take() {
            self.inner.commit(&rival)?;
        }
        self.inner.commit_all(writes)
    }

    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError> {
        self.inner.scan(entity)
    }
}

/// A row an `each:` element would create that another writer creates after the scan refuses the
/// whole group: the subject, the held row an earlier element updates and the rival's row are as
/// they were, and nothing of the outcome lands.
#[test]
fn a_row_created_by_another_writer_after_the_scan_refuses_the_whole_group() {
    raced_by("n1");
}

/// A row of the entity an `each:` entry scanned that another writer creates after the scan, one no
/// element names, refuses the whole group as well: the group is conditioned on the scan.
#[test]
fn a_row_no_element_names_created_after_the_scan_refuses_the_whole_group() {
    raced_by("z9");
}

/// Posts `d1` and `n1` while a rival creates the entry `rival` between the scan and the commit.
fn raced_by(rival_line: &str) {
    let ir = ir();
    for (name, store, _dir) in stores() {
        let mut racing = Racing {
            inner: store,
            rival: RefCell::new(None),
        };
        for (command, input) in books_and_drafts() {
            run(&ir, &mut racing, command, &input);
        }
        let book = stored(&mut racing, BOOK, &json!("b1"));
        let d1 = stored(&mut racing, ENTRY, &json!("d1"));
        let rival = Row {
            state: "Draft".to_owned(),
            fields: BTreeMap::from([("qty".to_owned(), json!(99))]),
        };
        racing.rival.replace(Some(Change {
            entity: ENTRY.to_owned(),
            identity: json!(rival_line),
            expected: None,
            row: rival.clone(),
            events: Vec::new(),
            scanned: Vec::new(),
        }));

        let raced = send(
            &ir,
            &mut racing,
            POST,
            &json!({"book_id": "b1", "lines": [
                {"line_id": "d1", "qty": 5},
                {"line_id": "n1", "qty": 6},
            ]}),
        );

        assert!(
            matches!(raced, Err(ExecuteError::Store(StoreError::Conflict { .. }))),
            "{name}: {raced:?}"
        );
        assert_eq!(stored(&mut racing, BOOK, &json!("b1")), book, "{name}");
        assert_eq!(stored(&mut racing, ENTRY, &json!("d1")), d1, "{name}");
        assert_eq!(
            stored(&mut racing, ENTRY, &json!(rival_line)).map(|held| held.row),
            Some(rival),
            "{name}: the rival's row stands"
        );
        if rival_line != "n1" {
            assert_eq!(stored(&mut racing, ENTRY, &json!("n1")), None, "{name}");
        }
    }
}
