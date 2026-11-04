//! `Store::scan`: every instance of one entity, in one stable order, on every store.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use spindle_kernel::{Change, MemoryStore, Row, SqliteStore, Store, Stored};

fn row(state: &str, title: &str) -> Row {
    Row {
        state: state.to_owned(),
        fields: BTreeMap::from([("title".to_owned(), json!(title))]),
    }
}

fn commit(store: &mut dyn Store, entity: &str, identity: Value, expected: Option<u64>, row: Row) {
    store
        .commit(&Change {
            entity: entity.to_owned(),
            identity,
            expected,
            row,
            events: Vec::new(),
        })
        .expect("the change commits");
}

/// Writes three instances of `a.Task` out of identity order, moves one twice, and writes one
/// instance of another entity.
fn arrange(store: &mut dyn Store) {
    commit(store, "a.Task", json!("t3"), None, row("Open", "third"));
    commit(store, "a.Task", json!("t1"), None, row("Open", "first"));
    commit(store, "b.Other", json!("t2"), None, row("Open", "other"));
    commit(store, "a.Task", json!("t2"), None, row("Open", "second"));
    commit(store, "a.Task", json!("t2"), Some(1), row("Done", "second"));
    commit(
        store,
        "a.Task",
        json!("t2"),
        Some(2),
        row("Done", "second, again"),
    );
}

fn expected() -> Vec<(Value, Stored)> {
    vec![
        (
            json!("t1"),
            Stored {
                row: row("Open", "first"),
                version: 1,
            },
        ),
        (
            json!("t2"),
            Stored {
                row: row("Done", "second, again"),
                version: 3,
            },
        ),
        (
            json!("t3"),
            Stored {
                row: row("Open", "third"),
                version: 1,
            },
        ),
    ]
}

#[test]
fn a_memory_store_scans_every_instance_of_one_entity_by_identity() {
    let mut store = MemoryStore::default();
    assert_eq!(store.scan("a.Task").expect("an empty scan"), Vec::new());
    arrange(&mut store);
    assert_eq!(store.scan("a.Task").expect("the scan reads"), expected());
    assert_eq!(store.scan("a.Missing").expect("the scan reads"), Vec::new());
}

#[test]
fn a_sqlite_store_scans_every_instance_of_one_entity_by_identity() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = directory.path().join("scan.sqlite");
    let mut store = SqliteStore::open(&path).expect("the store opens");
    assert_eq!(store.scan("a.Task").expect("an empty scan"), Vec::new());
    arrange(&mut store);
    assert_eq!(store.scan("a.Task").expect("the scan reads"), expected());
    assert_eq!(store.scan("a.Missing").expect("the scan reads"), Vec::new());
    drop(store);
    let mut reopened = SqliteStore::open(&path).expect("the store reopens");
    assert_eq!(
        reopened
            .scan("a.Task")
            .expect("the scan reads after a reopen"),
        expected(),
        "a scan reads what the file holds, not what the process remembers"
    );
}

/// The scan's version is the one a later commit must name.
#[test]
fn a_scanned_version_is_the_one_a_commit_names() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut sqlite = SqliteStore::open(&directory.path().join("v.sqlite")).expect("opens");
    let mut memory = MemoryStore::default();
    for store in [&mut sqlite as &mut dyn Store, &mut memory] {
        arrange(store);
        let scanned = store.scan("a.Task").expect("the scan reads");
        let (identity, stored) = &scanned[1];
        commit(
            store,
            "a.Task",
            identity.clone(),
            Some(stored.version),
            row("Done", "third time"),
        );
        assert_eq!(
            store
                .load("a.Task", identity)
                .expect("loads")
                .map(|s| s.version),
            Some(stored.version + 1)
        );
    }
}
