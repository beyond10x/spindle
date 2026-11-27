//! The reserved `spindle:log` streams of a store's log, which no read of its events answers: a
//! dead letter (story:dead-letters-kept-and-replayed) is kept beside the events, never one of
//! them.

use serde_json::{Map, Value, json};
use spindle_kernel::{DeadLetter, Event, MemoryStore, SqliteStore, Store, contract};

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
