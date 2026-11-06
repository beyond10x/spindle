//! `deletes:` (ess/15): the outcome removes the row its `instance:` names, as ESS's interpreter
//! does (`interpret::execute::take`, `Acted::Removed`). The row leaves every view of its entity, a
//! later command on the identity answers as an unknown instance, and the outcome's events are
//! published.
//!
//! The fixture `tests/fixtures/deletes` shelves books, lends a shelved one, and removes a book
//! that is not lent. Each case runs on the in-memory store and on SQLite.

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
use spindle_kernel::{Executed, MemoryStore, NoExternals, SqliteStore, Store, execute};

const BOOK: &str = "library.books.Book";
const SHELVE: &str = "library.books.ShelveBook";
const LEND: &str = "library.books.LendBook";
const REMOVE: &str = "library.books.RemoveBook";
const CATALOGUE: &str = "library.books.Catalogue";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/deletes")
}

fn ir() -> spindle_spec::EssIr {
    spindle_spec::load(&fixture())
        .expect("the deletes fixture compiles")
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

/// Each store a case runs on, by name: memory, and SQLite in a directory of its own.
fn stores() -> Vec<(&'static str, Box<dyn Store>, Option<tempfile::TempDir>)> {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    let sqlite = SqliteStore::open(&dir.path().join("books.sqlite")).expect("SQLite opens");
    vec![
        ("memory", Box::new(MemoryStore::default()), None),
        ("sqlite", Box::new(sqlite), Some(dir)),
    ]
}

/// The catalogue as the store holds it: each book's identity, title and state, in the store's
/// order.
fn catalogue(store: &mut dyn Store) -> Vec<Map<String, Value>> {
    store
        .scan(BOOK)
        .expect("the store scans")
        .into_iter()
        .map(|(identity, stored)| {
            let mut row: Map<String, Value> = stored.row.fields.into_iter().collect();
            row.insert("book_id".to_owned(), identity);
            row.insert("state".to_owned(), json!(stored.row.state));
            row
        })
        .collect()
}

#[test]
fn a_deleting_outcome_removes_its_subject() {
    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        for (id, title) in [("b1", "Atlas of Rivers"), ("b2", "Field Notes")] {
            let shelved = run(&ir, store, SHELVE, json!({"book_id": id, "title": title}));
            assert_eq!(shelved.outcome, "shelved", "{name}");
        }

        let removed = run(&ir, store, REMOVE, json!({"book_id": "b1"}));
        assert_eq!(removed.outcome, "removed", "{name}");
        assert_eq!(removed.error, None, "{name}");
        let published: Vec<(&str, &Map<String, Value>)> = removed
            .events
            .iter()
            .map(|event| (event.name.as_str(), &event.payload))
            .collect();
        assert_eq!(
            published,
            [(
                "library.books.BookRemoved",
                &object(json!({"book_id": "b1"}))
            )],
            "{name}: the outcome's event is published"
        );
        assert_eq!(
            store.load(BOOK, &json!("b1")).expect("the store reads"),
            None,
            "{name}: no row carries the removed identity"
        );
        assert_eq!(
            catalogue(store),
            [object(
                json!({"book_id": "b2", "title": "Field Notes", "state": "Shelved"})
            )],
            "{name}: the removed book leaves the view"
        );

        let again = run(&ir, store, REMOVE, json!({"book_id": "b1"}));
        assert_eq!(again.outcome, "no-such-book", "{name}");
        assert_eq!(
            again.error.as_ref().map(|error| error.name.as_str()),
            Some("library.books.BookNotFound"),
            "{name}: a second removal answers the unknown-instance refusal"
        );
        assert!(again.events.is_empty(), "{name}");
        let lend = run(&ir, store, LEND, json!({"book_id": "b1"}));
        assert_eq!(
            lend.outcome, "no-such-book",
            "{name}: any later command on the identity answers as an unknown instance"
        );
        assert_eq!(
            catalogue(store).len(),
            1,
            "{name}: a refusal writes nothing"
        );
    }
}

#[test]
fn removing_a_lent_book_is_refused_and_keeps_it() {
    let ir = ir();
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        run(
            &ir,
            store,
            SHELVE,
            json!({"book_id": "b1", "title": "Tides"}),
        );
        run(&ir, store, LEND, json!({"book_id": "b1"}));
        let refused = run(&ir, store, REMOVE, json!({"book_id": "b1"}));
        assert_eq!(refused.outcome, "on-loan", "{name}");
        assert!(refused.events.is_empty(), "{name}");
        assert_eq!(
            catalogue(store),
            [object(
                json!({"book_id": "b1", "title": "Tides", "state": "Lent"})
            )],
            "{name}: a refused removal keeps the row"
        );
    }
}

/// What one step answered: the outcome, the declared error, and the events by name and payload.
type Answer = (String, Option<String>, Vec<(String, Map<String, Value>)>);

fn json_of(node: &Node) -> Value {
    serde_json::to_value(node).expect("a node is JSON")
}

fn node_of(value: &Value) -> Node {
    serde_json::from_value(value.clone()).expect("a JSON scalar is a node")
}

/// The steps the agreement is read over: every branch of `RemoveBook`, a later command on a
/// removed identity, and the identity shelved again once removed.
fn steps() -> Vec<(&'static str, Value)> {
    vec![
        (SHELVE, json!({"book_id": "b1", "title": "Atlas of Rivers"})),
        (SHELVE, json!({"book_id": "b2", "title": "Field Notes"})),
        (SHELVE, json!({"book_id": "b3", "title": "Tides"})),
        (LEND, json!({"book_id": "b2"})),
        (REMOVE, json!({"book_id": "b2"})),
        (REMOVE, json!({"book_id": "b1"})),
        (REMOVE, json!({"book_id": "b1"})),
        (LEND, json!({"book_id": "b1"})),
        (REMOVE, json!({"book_id": "nobody"})),
        (
            SHELVE,
            json!({"book_id": "b1", "title": "Atlas of Rivers, second edition"}),
        ),
        (LEND, json!({"book_id": "b1"})),
        (REMOVE, json!({"book_id": "b3"})),
    ]
}

/// What ESS's interpreter answers for each step, and the catalogue's rows after the last.
fn interpreted(
    ir: &spindle_spec::EssIr,
    steps: &[(&str, Value)],
) -> (Vec<Answer>, Vec<Map<String, Value>>) {
    let target = Interpreted::for_model(ir.clone());
    let context = ScenarioContext::new(
        "library.books/authored/deletes"
            .parse()
            .expect("a scenario id"),
        CorrelationId::new("deletes").expect("a correlation id"),
    );
    target.begin_scenario(&context).expect("the scenario opens");
    let mut answers = Vec::new();
    for (command, input) in steps {
        let input = object(input.clone())
            .iter()
            .map(|(field, value)| (field.clone(), node_of(value)))
            .collect();
        let executed = target
            .execute_command(SemanticCommandRequest {
                command: CommandRef::new(command.parse().expect("a command name")),
                actor: None,
                caller: None,
                input,
                correlation: context.correlation.clone(),
            })
            .unwrap_or_else(|error| panic!("the interpreter runs {command}: {error}"));
        answers.push((
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
        ));
    }
    let rows = target
        .query_view(SemanticViewRequest {
            view: ViewRef::new(CATALOGUE.parse().expect("a view name")),
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

#[test]
fn deletes_agree_with_the_interpreter() {
    let ir = ir();
    let steps = steps();
    let (ess_answers, ess_rows) = interpreted(&ir, &steps);
    let outcomes: Vec<&str> = ess_answers
        .iter()
        .map(|(outcome, _, _)| outcome.as_str())
        .collect();
    assert_eq!(
        outcomes,
        [
            "shelved",
            "shelved",
            "shelved",
            "lent",
            "on-loan",
            "removed",
            "no-such-book",
            "no-such-book",
            "no-such-book",
            "shelved",
            "lent",
            "removed",
        ],
        "the steps reach every branch of RemoveBook and shelve a removed identity again"
    );
    for (name, mut store, _dir) in stores() {
        let store = store.as_mut();
        let answers: Vec<Answer> = steps
            .iter()
            .map(|(command, input)| {
                let executed = run(&ir, store, command, input.clone());
                (
                    executed.outcome,
                    executed.error.map(|error| error.name),
                    executed
                        .events
                        .into_iter()
                        .map(|event| (event.name, event.payload))
                        .collect(),
                )
            })
            .collect();
        assert_eq!(
            answers, ess_answers,
            "{name}: outcomes, errors and events agree"
        );
        assert_eq!(
            catalogue(store),
            ess_rows,
            "{name}: the stored books agree with the interpreter's view rows"
        );
    }
}
