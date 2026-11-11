//! Adversary cases for `Store::remove` (story kernel-delete-outcomes): the trait's contract read
//! against both stores, and a removal across a restart and two handles on one SQLite file.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use spindle_kernel::{Change, MemoryStore, Removal, Row, SqliteStore, Store, StoreError};

const BOOK: &str = "library.books.Book";

fn row(title: &str) -> Row {
    Row {
        state: "Shelved".to_owned(),
        fields: BTreeMap::from([("title".to_owned(), json!(title))]),
    }
}

fn create(store: &mut dyn Store, identity: &Value, title: &str) {
    store
        .commit(&Change {
            entity: BOOK.to_owned(),
            identity: identity.clone(),
            expected: None,
            row: row(title),
            events: Vec::new(),
            scanned: Vec::new(),
        })
        .expect("the creation commits");
}

fn removal(identity: &Value, expected: u64) -> Removal {
    Removal {
        entity: BOOK.to_owned(),
        identity: identity.clone(),
        expected,
        events: Vec::new(),
        scanned: Vec::new(),
    }
}

fn version(store: &mut dyn Store, identity: &Value) -> u64 {
    store
        .load(BOOK, identity)
        .expect("the store reads")
        .expect("the row is held")
        .version
}

/// `Store::remove` promises `StoreError::Conflict` "when the instance is absent". Once a row is
/// removed nothing holds it, so a second removal at any version is a conflict, and so is a
/// change that names a version of it. The memory store refuses both; this asks SQLite the same,
/// naming the version its stream stands at after the removal (the row's version plus the
/// removal's one `spindle.removed` event).
#[test]
fn a_removed_instance_takes_no_second_removal_and_no_update_on_either_store() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    let sqlite = SqliteStore::open(&dir.path().join("books.sqlite")).expect("SQLite opens");
    let stores: Vec<(&str, Box<dyn Store>)> = vec![
        ("memory", Box::new(MemoryStore::default())),
        ("sqlite", Box::new(sqlite)),
    ];
    for (name, mut store) in stores {
        let store = store.as_mut();
        let identity = json!("b1");
        create(store, &identity, "Tides");
        let held = version(store, &identity);
        store
            .remove(&removal(&identity, held))
            .unwrap_or_else(|error| panic!("{name}: the first removal: {error}"));
        assert_eq!(store.load(BOOK, &identity).expect("reads"), None, "{name}");
        for expected in [held, held + 1] {
            let again = store.remove(&removal(&identity, expected));
            assert!(
                matches!(again, Err(StoreError::Conflict { .. })),
                "{name}: a removal at {expected} of an instance nothing holds is a conflict, \
                 got {again:?}"
            );
            let update = store.commit(&Change {
                entity: BOOK.to_owned(),
                identity: identity.clone(),
                expected: Some(expected),
                row: row("Tides, revived"),
                events: Vec::new(),
                scanned: Vec::new(),
            });
            assert!(
                matches!(update, Err(StoreError::Conflict { .. })),
                "{name}: an update at {expected} of a removed instance is a conflict, got \
                 {update:?}"
            );
            assert_eq!(
                store.load(BOOK, &identity).expect("reads"),
                None,
                "{name}: nothing brings the removed row back but a creation"
            );
        }
    }
}

/// A removal written to SQLite holds across a reopen; the identity is created again after it,
/// that row holds across a second reopen, and a stale removal naming the first row's version is
/// refused. Two handles on one file: the second removal of the same read loses.
#[test]
fn a_removal_survives_a_restart_and_a_stale_or_concurrent_removal_loses() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    let path = dir.path().join("books.sqlite");
    let identity = json!("b1");
    let first = {
        let mut store = SqliteStore::open(&path).expect("SQLite opens");
        create(&mut store, &identity, "Tides");
        create(&mut store, &json!("b2"), "Field Notes");
        let first = version(&mut store, &identity);
        store
            .remove(&removal(&identity, first))
            .expect("the removal commits");
        first
    };
    {
        let mut store = SqliteStore::open(&path).expect("SQLite reopens");
        assert_eq!(store.load(BOOK, &identity).expect("reads"), None);
        let scanned: Vec<Value> = store
            .scan(BOOK)
            .expect("scans")
            .into_iter()
            .map(|(identity, _)| identity)
            .collect();
        assert_eq!(scanned, [json!("b2")]);
        create(&mut store, &identity, "Tides, second edition");
    }
    let mut a = SqliteStore::open(&path).expect("SQLite reopens");
    let mut b = SqliteStore::open(&path).expect("a second handle opens");
    let again = a.load(BOOK, &identity).expect("reads").expect("held again");
    assert_eq!(again.row, row("Tides, second edition"));
    assert!(
        again.version > first,
        "the version goes on past the removal"
    );
    assert!(matches!(
        a.remove(&removal(&identity, first)),
        Err(StoreError::Conflict { .. })
    ));
    let read = b.load(BOOK, &identity).expect("reads").expect("held");
    a.remove(&removal(&identity, again.version))
        .expect("the first of two removals commits");
    assert!(matches!(
        b.remove(&removal(&identity, read.version)),
        Err(StoreError::Conflict { .. })
    ));
    assert_eq!(b.load(BOOK, &identity).expect("reads"), None);
    let scanned: Vec<Value> = b
        .scan(BOOK)
        .expect("scans")
        .into_iter()
        .map(|(identity, _)| identity)
        .collect();
    assert_eq!(scanned, [json!("b2")]);
}
