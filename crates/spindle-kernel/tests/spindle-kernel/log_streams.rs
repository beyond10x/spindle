//! The reserved `spindle:log` streams of a component's log that are no published event: what
//! `Store::events_after` and `GET /api/events` read leaves them out.
//!
//! The egress cursor (`story:event-egress-webhook`): the store position up to which a served run
//! posted a component's events to one `[http.<name>]` call, saved in the component's own log and
//! read back after a restart.
//!
//! A dead letter (`story:dead-letters-kept-and-replayed`) is kept beside the events, never one of
//! them.

use serde_json::{Map, Value, json};
use spindle_kernel::{Change, DeadLetter, Event, MemoryStore, Row, SqliteStore, Store, contract};

const TASK: &str = "tasks.board.Task";

fn opened(identity: &str) -> Change {
    let mut payload = Map::new();
    payload.insert("task_id".to_owned(), json!(identity));
    Change {
        entity: TASK.to_owned(),
        identity: json!(identity),
        expected: None,
        row: Row {
            state: "Open".to_owned(),
            fields: std::collections::BTreeMap::from([("title".to_owned(), json!("t"))]),
        },
        events: vec![Event {
            name: "tasks.board.TaskOpened".to_owned(),
            payload,
        }],
        scanned: Vec::new(),
    }
}

fn names(store: &mut dyn Store) -> Vec<(String, u64)> {
    store
        .events_after(0, usize::MAX)
        .expect("the events read")
        .into_iter()
        .map(|committed| (committed.event.name, committed.position))
        .collect()
}

fn sqlite() -> (SqliteStore, tempfile::TempDir) {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let store = SqliteStore::open(&directory.path().join("log.sqlite")).expect("SQLite opens");
    (store, directory)
}

fn letter() -> DeadLetter {
    DeadLetter {
        binding: "ship-on-order".to_owned(),
        event: Some("depot.orders.OrderPlaced".to_owned()),
        command: "depot.shipping.Ship".to_owned(),
        input: match json!({"order_id": "o-1"}) {
            Value::Object(fields) => fields,
            _ => Map::new(),
        },
        outcome: Some("busy".to_owned()),
        error: "depot.shipping.Busy".to_owned(),
        attempts: 5,
    }
}

#[test]
fn an_egress_cursor_is_no_published_event() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = directory.path().join("board.sqlite");
    let first = {
        let mut store = SqliteStore::open(&path).expect("the store opens");
        assert_eq!(
            store.egress_position("hook").expect("a cursor reads"),
            0,
            "no event was posted yet"
        );
        store.commit(&opened("t1")).expect("t1 commits");
        let committed = names(&mut store);
        assert_eq!(committed.len(), 1, "{committed:?}");
        let (_, first) = committed[0].clone();
        store
            .egress_posted("hook", first)
            .expect("the cursor is saved");
        store.commit(&opened("t2")).expect("t2 commits");
        let after = names(&mut store);
        assert_eq!(
            after
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            ["tasks.board.TaskOpened", "tasks.board.TaskOpened"],
            "the cursor is no event of the log: {after:?}"
        );
        assert!(after[1].1 > first, "{after:?}");
        let reading = store.reading().expect("a reading handle");
        let read = reading
            .events_after(first, usize::MAX)
            .expect("the reading handle reads");
        assert_eq!(read.len(), 1, "only t2 is after the cursor: {read:?}");
        assert_eq!(read[0].event.payload["task_id"], json!("t2"));
        first
    };

    let mut again = SqliteStore::open(&path).expect("the store opens again");
    assert_eq!(
        again.egress_position("hook").expect("the cursor reads"),
        first,
        "the cursor survives a restart"
    );
    assert_eq!(
        again
            .egress_position("other")
            .expect("another cursor reads"),
        0,
        "each call has a cursor of its own"
    );
    assert_eq!(names(&mut again).len(), 2);
}

#[test]
fn the_memory_store_keeps_dead_letters_as_the_contract_says() {
    assert!(!contract::DEAD_LETTER_CASES.is_empty());
    for case in contract::DEAD_LETTER_CASES {
        (case.run)(&mut MemoryStore::default());
    }
}

#[test]
fn the_sqlite_store_keeps_dead_letters_as_the_contract_says() {
    for case in contract::DEAD_LETTER_CASES {
        let (mut store, _directory) = sqlite();
        (case.run)(&mut store);
    }
}

#[test]
fn a_dead_letter_is_no_published_event() {
    let (mut store, _directory) = sqlite();
    store
        .commit_events(
            &[Event {
                name: "depot.orders.OrderPlaced".to_owned(),
                payload: Map::new(),
            }],
            &[],
        )
        .expect("an event is committed");
    let head = store.events_head().expect("the head reads");
    let id = store
        .keep_dead_letter(&letter())
        .expect("the letter is kept");
    let events = store
        .events_after(0, usize::MAX)
        .expect("the events read")
        .into_iter()
        .map(|committed| committed.event.name)
        .collect::<Vec<_>>();
    assert_eq!(events, ["depot.orders.OrderPlaced"]);
    assert!(
        store
            .events_after(head, usize::MAX)
            .expect("the events after the head read")
            .is_empty(),
        "a dead letter kept after the head is no event after it"
    );
    let reading = store.reading().expect("SQLite gives a reading handle");
    assert_eq!(
        reading
            .events_after(0, usize::MAX)
            .expect("the events read beside the writer")
            .len(),
        1
    );
    assert_eq!(
        store
            .take_dead_letter(id)
            .expect("the letter is taken")
            .as_ref(),
        Some(&letter())
    );
    assert_eq!(
        store
            .events_after(0, usize::MAX)
            .expect("the events read")
            .len(),
        1,
        "a replay mark is no event either"
    );
}

#[test]
fn a_dead_letter_survives_a_reopened_sqlite_file() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = directory.path().join("log.sqlite");
    let id = {
        let mut store = SqliteStore::open(&path).expect("SQLite opens");
        store
            .keep_dead_letter(&letter())
            .expect("the letter is kept")
    };
    let mut reopened = SqliteStore::open(&path).expect("SQLite opens again");
    assert_eq!(
        reopened.dead_letters().expect("the letters read"),
        vec![(id, letter())]
    );
}
