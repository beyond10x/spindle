//! Who may send what (story:actor-command-grants): a command sent as an actor whose `may:` does not
//! name it is refused before anything is read, as ESS's interpreter refuses it
//! (`interpret::command::command_grant`), and a command sent as no actor is not checked.
//!
//! The fixture `tests/fixtures/grants` declares `Reader`, whose `may:` lists only `ReadBook`, and
//! `Librarian`, who may add, read and delete a book.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, HostError, ManualClock};
use spindle_kernel::ExecuteError;

const ADD: &str = "shelf.books.AddBook";
const READ: &str = "shelf.books.ReadBook";
const DELETE: &str = "shelf.books.DeleteBook";
const READER: &str = "shelf.books.Reader";
const LIBRARIAN: &str = "shelf.books.Librarian";
const BOOKS: &str = "shelf.books.Books";
const BOOK: &str = "5b2d8f14-3c6a-4e7b-9d1f-2a3b4c5d6e7f";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("laptop.toml reads");
    Environment::parse(&text).expect("laptop.toml is an environment")
}

/// A host over the fixture, holding one shelved book the librarian added.
fn shelved(state: &Path) -> Host {
    let loaded =
        spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/grants"))
            .expect("the grants fixture compiles");
    let mut host = Host::new(loaded.ir, &laptop(), Arc::new(ManualClock::new()), state)
        .expect("the laptop admits the grants fixture");
    host.submit_as(
        ADD,
        object(json!({"book_id": BOOK, "title": "field notes"})),
        Some(LIBRARIAN),
        None,
    )
    .expect("the librarian adds the book");
    host
}

fn rows(host: &mut Host) -> Vec<Value> {
    host.list(BOOKS)
        .expect("the view answers")
        .into_iter()
        .map(Value::Object)
        .collect()
}

fn events(host: &mut Host) -> Vec<String> {
    host.published()
        .expect("the events read")
        .iter()
        .map(|published| published.event.name.clone())
        .collect()
}

#[test]
fn a_command_the_actor_may_not_send_is_refused() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = shelved(state.path());
    let before_rows = rows(&mut host);
    let before_events = events(&mut host);

    let refused = host.submit_as(DELETE, object(json!({"book_id": BOOK})), Some(READER), None);

    let Err(HostError::Execute { error, .. }) = refused else {
        panic!("the reader's delete is refused before it runs: {refused:?}");
    };
    assert!(
        matches!(&*error, ExecuteError::NotGranted { actor, .. } if actor == READER),
        "refused as not granted to the reader: {error:?}"
    );
    assert_eq!(error.kind(), "not_granted");
    assert!(
        error.to_string().contains("not granted"),
        "the refusal says it: {error}"
    );
    assert_eq!(rows(&mut host), before_rows, "no row is written");
    assert_eq!(events(&mut host), before_events, "no event is written");
}

#[test]
fn a_command_the_actor_may_send_runs() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = shelved(state.path());

    let report = host
        .submit_as(
            READ,
            object(json!({"book_id": BOOK, "note": "worn spine"})),
            Some(READER),
            None,
        )
        .expect("the reader reads the book");

    assert_eq!(
        report.ran.first().map(|ran| ran.executed.outcome.as_str()),
        Some("read")
    );
    assert_eq!(
        rows(&mut host),
        vec![
            json!({"book_id": BOOK, "title": "field notes", "note": "worn spine", "state": "Shelved"})
        ]
    );
    assert_eq!(
        events(&mut host),
        vec![
            "shelf.books.BookAdded".to_owned(),
            "shelf.books.BookRead".to_owned()
        ]
    );
}

/// An actor the specification does not declare holds no grant: ESS's interpreter refuses it as not
/// granted, naming it.
#[test]
fn an_undeclared_actor_is_not_granted() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = shelved(state.path());
    let refused = host.submit_as(
        READ,
        object(json!({"book_id": BOOK, "note": "worn spine"})),
        Some("shelf.books.Visitor"),
        None,
    );
    let Err(HostError::Execute { error, .. }) = refused else {
        panic!("an undeclared actor is refused: {refused:?}");
    };
    assert!(
        matches!(&*error, ExecuteError::NotGranted { actor, .. } if actor == "shelf.books.Visitor"),
        "{error:?}"
    );
}

/// A command sent as no actor is sent as the runtime's own authority, as ESS's interpreter sends
/// it: no grant is checked. Bindings, ticks and `Host::submit` send that way.
#[test]
fn a_command_sent_as_no_actor_is_not_checked() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = shelved(state.path());
    host.submit(DELETE, object(json!({"book_id": BOOK})))
        .expect("a delete sent as no actor runs");
    assert_eq!(
        rows(&mut host),
        vec![json!({"book_id": BOOK, "title": "field notes", "state": "Removed"})]
    );
}
