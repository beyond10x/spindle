//! The store contract: what every [`Store`] answers alike, whatever holds its rows.
//!
//! Each [`Case`] takes an empty store and panics, naming itself, where the store answers other
//! than [`MemoryStore`](crate::MemoryStore) does. A store's own tests run every case in
//! [`CASES`], each on a fresh store: the memory and SQLite stores in this crate's tests, the
//! IndexedDB store in `spindle-store-browser`'s. Versions are compared, never spelled: a version
//! grows with every change, and the one a read answers is the one a later write names. How far it
//! grows is the store's own (SQLite counts the events of a change too). Every refusal a case
//! expects is a conflict naming, exactly, the version the write named and the version the store
//! held just before it, read back from the store (`None` when it held nothing), and it leaves the
//! instance as it was.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::Event;
use crate::store::{Change, Removal, Row, Store, StoreError, Stored};

/// The entity most cases write.
pub const TASK: &str = "contract.tasks.Task";
/// A second entity, which a scan of [`TASK`] leaves out.
pub const OTHER: &str = "contract.tasks.Other";
/// Every entity a case writes, so a caller can compare what a store holds before and after.
pub const ENTITIES: &[&str] = &[TASK, OTHER];

/// One case of the contract.
pub struct Case {
    /// The case's name, which every failure carries.
    pub name: &'static str,
    /// Runs the case on an empty store.
    pub run: fn(&mut dyn Store),
}

/// Every case, in the order a store's tests run them.
pub const CASES: &[Case] = &[
    Case {
        name: "an_empty_store_holds_nothing",
        run: an_empty_store_holds_nothing,
    },
    Case {
        name: "a_creation_is_read_back",
        run: a_creation_is_read_back,
    },
    Case {
        name: "a_second_creation_of_one_identity_is_a_conflict",
        run: a_second_creation_of_one_identity_is_a_conflict,
    },
    Case {
        name: "a_change_names_the_version_it_read",
        run: a_change_names_the_version_it_read,
    },
    Case {
        name: "a_change_to_an_absent_instance_is_a_conflict",
        run: a_change_to_an_absent_instance_is_a_conflict,
    },
    Case {
        name: "a_scan_orders_by_identity_text_and_keeps_entities_apart",
        run: a_scan_orders_by_identity_text_and_keeps_entities_apart,
    },
    Case {
        name: "a_scanned_version_is_the_one_a_change_names",
        run: a_scanned_version_is_the_one_a_change_names,
    },
    Case {
        name: "a_removed_instance_is_neither_loaded_nor_scanned",
        run: a_removed_instance_is_neither_loaded_nor_scanned,
    },
    Case {
        name: "a_removed_instance_takes_no_second_removal_and_no_change",
        run: a_removed_instance_takes_no_second_removal_and_no_change,
    },
    Case {
        name: "a_removal_names_the_version_it_read",
        run: a_removal_names_the_version_it_read,
    },
    Case {
        name: "a_removed_identity_is_created_again_past_its_removed_version",
        run: a_removed_identity_is_created_again_past_its_removed_version,
    },
];

fn row(state: &str, title: &str) -> Row {
    Row {
        state: state.to_owned(),
        fields: BTreeMap::from([("title".to_owned(), json!(title))]),
    }
}

fn events(count: usize) -> Vec<Event> {
    (0..count)
        .map(|index| Event {
            name: "contract.tasks.Touched".to_owned(),
            payload: Map::from_iter([("index".to_owned(), json!(index))]),
        })
        .collect()
}

fn change(entity: &str, identity: &Value, expected: Option<u64>, row: Row) -> Change {
    Change {
        entity: entity.to_owned(),
        identity: identity.clone(),
        expected,
        row,
        events: events(2),
    }
}

fn removal(identity: &Value, expected: u64) -> Removal {
    Removal {
        entity: TASK.to_owned(),
        identity: identity.clone(),
        expected,
        events: events(1),
    }
}

fn commit(case: &str, store: &mut dyn Store, change: &Change) {
    store.commit(change).unwrap_or_else(|error| {
        panic!(
            "{case}: {} {} commits: {error}",
            change.entity, change.identity
        )
    });
}

fn load(case: &str, store: &mut dyn Store, entity: &str, identity: &Value) -> Option<Stored> {
    store
        .load(entity, identity)
        .unwrap_or_else(|error| panic!("{case}: {entity} {identity} loads: {error}"))
}

fn held(case: &str, store: &mut dyn Store, identity: &Value) -> Stored {
    load(case, store, TASK, identity).unwrap_or_else(|| panic!("{case}: {TASK} {identity} is held"))
}

fn scan(case: &str, store: &mut dyn Store, entity: &str) -> Vec<(Value, Stored)> {
    store
        .scan(entity)
        .unwrap_or_else(|error| panic!("{case}: {entity} scans: {error}"))
}

/// Asserts `result` is a conflict naming `named`, the version the write named (`None` for a
/// creation), and `held`, the version the store held just before it (`None` when nothing was
/// held).
fn conflict(
    case: &str,
    what: &str,
    result: Result<(), StoreError>,
    named: Option<u64>,
    held: Option<u64>,
) {
    match result {
        Err(StoreError::Conflict {
            expected, actual, ..
        }) => {
            assert_eq!(
                expected, named,
                "{case}: {what}: the conflict names the version the write named"
            );
            assert_eq!(
                actual, held,
                "{case}: {what}: the conflict names the version the store held"
            );
        }
        other => panic!("{case}: {what} is a conflict, got {other:?}"),
    }
}

/// Commits `change`, which the store must refuse as a [`conflict`] naming the version it held
/// before, read back from it, and must leave the instance as it was.
fn refused_change(case: &str, what: &str, store: &mut dyn Store, change: &Change) {
    let before = load(case, store, &change.entity, &change.identity);
    let held = before.as_ref().map(|stored| stored.version);
    conflict(case, what, store.commit(change), change.expected, held);
    assert_eq!(
        load(case, store, &change.entity, &change.identity),
        before,
        "{case}: {what} writes nothing"
    );
}

/// Removes as `removal` says, which the store must refuse as a [`conflict`] naming the version it
/// held before, read back from it, and must leave the instance as it was.
fn refused_removal(case: &str, what: &str, store: &mut dyn Store, removal: &Removal) {
    let before = load(case, store, &removal.entity, &removal.identity);
    let held = before.as_ref().map(|stored| stored.version);
    conflict(
        case,
        what,
        store.remove(removal),
        Some(removal.expected),
        held,
    );
    assert_eq!(
        load(case, store, &removal.entity, &removal.identity),
        before,
        "{case}: {what} removes nothing"
    );
}

fn an_empty_store_holds_nothing(store: &mut dyn Store) {
    let case = "an_empty_store_holds_nothing";
    for entity in ENTITIES {
        assert_eq!(load(case, store, entity, &json!("t1")), None, "{case}");
        assert_eq!(scan(case, store, entity), Vec::new(), "{case}");
    }
}

fn a_creation_is_read_back(store: &mut dyn Store) {
    let case = "a_creation_is_read_back";
    let identity = json!("t1");
    commit(
        case,
        store,
        &change(TASK, &identity, None, row("Open", "first")),
    );
    let stored = held(case, store, &identity);
    assert_eq!(stored.row, row("Open", "first"), "{case}");
    assert!(stored.version >= 1, "{case}: a held row has a version");
    assert_eq!(scan(case, store, TASK), vec![(identity, stored)], "{case}");
    assert_eq!(scan(case, store, OTHER), Vec::new(), "{case}");
}

fn a_second_creation_of_one_identity_is_a_conflict(store: &mut dyn Store) {
    let case = "a_second_creation_of_one_identity_is_a_conflict";
    let identity = json!("t1");
    commit(
        case,
        store,
        &change(TASK, &identity, None, row("Open", "first")),
    );
    let before = held(case, store, &identity);
    refused_change(
        case,
        "a second creation",
        store,
        &change(TASK, &identity, None, row("Open", "usurper")),
    );
    assert_eq!(
        held(case, store, &identity),
        before,
        "{case}: the first row stays"
    );
}

fn a_change_names_the_version_it_read(store: &mut dyn Store) {
    let case = "a_change_names_the_version_it_read";
    let identity = json!("t1");
    commit(
        case,
        store,
        &change(TASK, &identity, None, row("Open", "first")),
    );
    let first = held(case, store, &identity);
    commit(
        case,
        store,
        &change(TASK, &identity, Some(first.version), row("Done", "second")),
    );
    let second = held(case, store, &identity);
    assert_eq!(second.row, row("Done", "second"), "{case}");
    assert!(
        second.version > first.version,
        "{case}: the version grows ({} after {})",
        second.version,
        first.version
    );
    refused_change(
        case,
        "a change naming the version read before the last one",
        store,
        &change(TASK, &identity, Some(first.version), row("Open", "stale")),
    );
    assert_eq!(
        held(case, store, &identity),
        second,
        "{case}: a stale change writes nothing"
    );
    commit(
        case,
        store,
        &change(TASK, &identity, Some(second.version), row("Open", "third")),
    );
    let third = held(case, store, &identity);
    assert_eq!(third.row, row("Open", "third"), "{case}");
    assert!(third.version > second.version, "{case}");
}

fn a_change_to_an_absent_instance_is_a_conflict(store: &mut dyn Store) {
    let case = "a_change_to_an_absent_instance_is_a_conflict";
    let identity = json!("t1");
    refused_change(
        case,
        "a change to an instance never created",
        store,
        &change(TASK, &identity, Some(1), row("Open", "nobody")),
    );
    assert_eq!(load(case, store, TASK, &identity), None, "{case}");
    assert_eq!(scan(case, store, TASK), Vec::new(), "{case}");
}

fn a_scan_orders_by_identity_text_and_keeps_entities_apart(store: &mut dyn Store) {
    let case = "a_scan_orders_by_identity_text_and_keeps_entities_apart";
    for (identity, title) in [
        (json!(9), "nine"),
        (json!("t3"), "third"),
        (json!(10), "ten"),
        (json!("t1"), "first"),
        (json!("t2"), "second"),
    ] {
        commit(
            case,
            store,
            &change(TASK, &identity, None, row("Open", title)),
        );
    }
    commit(
        case,
        store,
        &change(OTHER, &json!("t0"), None, row("Open", "other")),
    );
    let t2 = held(case, store, &json!("t2"));
    commit(
        case,
        store,
        &change(TASK, &json!("t2"), Some(t2.version), row("Done", "second")),
    );
    let scanned = scan(case, store, TASK);
    let identities: Vec<Value> = scanned
        .iter()
        .map(|(identity, _)| identity.clone())
        .collect();
    assert_eq!(
        identities,
        [json!("t1"), json!("t2"), json!("t3"), json!(10), json!(9)],
        "{case}: ordered by the identity's canonical JSON text"
    );
    for (identity, stored) in &scanned {
        assert_eq!(
            Some(stored),
            load(case, store, TASK, identity).as_ref(),
            "{case}: a scan answers what a load answers for {identity}"
        );
    }
    assert_eq!(
        scan(case, store, OTHER),
        vec![(
            json!("t0"),
            load(case, store, OTHER, &json!("t0")).expect("held")
        )],
        "{case}"
    );
    assert_eq!(
        scan(case, store, "contract.tasks.Missing"),
        Vec::new(),
        "{case}"
    );
}

fn a_scanned_version_is_the_one_a_change_names(store: &mut dyn Store) {
    let case = "a_scanned_version_is_the_one_a_change_names";
    for identity in ["t1", "t2"] {
        commit(
            case,
            store,
            &change(TASK, &json!(identity), None, row("Open", identity)),
        );
    }
    let scanned = scan(case, store, TASK);
    let (identity, stored) = &scanned[1];
    commit(
        case,
        store,
        &change(TASK, identity, Some(stored.version), row("Done", "moved")),
    );
    let moved = held(case, store, identity);
    assert_eq!(moved.row, row("Done", "moved"), "{case}");
    assert!(moved.version > stored.version, "{case}");
}

fn a_removed_instance_is_neither_loaded_nor_scanned(store: &mut dyn Store) {
    let case = "a_removed_instance_is_neither_loaded_nor_scanned";
    let (gone, kept) = (json!("t1"), json!("t2"));
    commit(case, store, &change(TASK, &gone, None, row("Open", "gone")));
    commit(case, store, &change(TASK, &kept, None, row("Open", "kept")));
    let version = held(case, store, &gone).version;
    store
        .remove(&removal(&gone, version))
        .unwrap_or_else(|error| panic!("{case}: the removal commits: {error}"));
    assert_eq!(load(case, store, TASK, &gone), None, "{case}");
    let kept_stored = held(case, store, &kept);
    assert_eq!(scan(case, store, TASK), vec![(kept, kept_stored)], "{case}");
}

fn a_removed_instance_takes_no_second_removal_and_no_change(store: &mut dyn Store) {
    let case = "a_removed_instance_takes_no_second_removal_and_no_change";
    let identity = json!("t1");
    commit(
        case,
        store,
        &change(TASK, &identity, None, row("Open", "gone")),
    );
    let version = held(case, store, &identity).version;
    store
        .remove(&removal(&identity, version))
        .unwrap_or_else(|error| panic!("{case}: the first removal commits: {error}"));
    for expected in [version, version + 1, version + 2] {
        refused_removal(
            case,
            &format!("a removal at {expected} of a removed instance"),
            store,
            &removal(&identity, expected),
        );
        refused_change(
            case,
            &format!("a change at {expected} of a removed instance"),
            store,
            &change(TASK, &identity, Some(expected), row("Open", "revived")),
        );
        assert_eq!(
            load(case, store, TASK, &identity),
            None,
            "{case}: nothing brings the removed row back but a creation"
        );
    }
    refused_removal(
        case,
        "a removal of an identity never created",
        store,
        &removal(&json!("never"), 1),
    );
    assert_eq!(scan(case, store, TASK), Vec::new(), "{case}");
}

fn a_removal_names_the_version_it_read(store: &mut dyn Store) {
    let case = "a_removal_names_the_version_it_read";
    let identity = json!("t1");
    commit(
        case,
        store,
        &change(TASK, &identity, None, row("Open", "first")),
    );
    let first = held(case, store, &identity);
    commit(
        case,
        store,
        &change(TASK, &identity, Some(first.version), row("Done", "second")),
    );
    let second = held(case, store, &identity);
    refused_removal(
        case,
        "a removal naming the version read before the last change",
        store,
        &removal(&identity, first.version),
    );
    assert_eq!(
        held(case, store, &identity),
        second,
        "{case}: a stale removal removes nothing"
    );
    store
        .remove(&removal(&identity, second.version))
        .unwrap_or_else(|error| panic!("{case}: the removal at the held version commits: {error}"));
    assert_eq!(load(case, store, TASK, &identity), None, "{case}");
}

fn a_removed_identity_is_created_again_past_its_removed_version(store: &mut dyn Store) {
    let case = "a_removed_identity_is_created_again_past_its_removed_version";
    let identity = json!("t1");
    commit(
        case,
        store,
        &change(TASK, &identity, None, row("Open", "first")),
    );
    let first = held(case, store, &identity);
    commit(
        case,
        store,
        &change(TASK, &identity, Some(first.version), row("Done", "first")),
    );
    let removed = held(case, store, &identity).version;
    store
        .remove(&removal(&identity, removed))
        .unwrap_or_else(|error| panic!("{case}: the removal commits: {error}"));
    commit(
        case,
        store,
        &change(TASK, &identity, None, row("Open", "second life")),
    );
    let again = held(case, store, &identity);
    assert_eq!(again.row, row("Open", "second life"), "{case}");
    assert!(
        again.version > removed,
        "{case}: the version goes on past the removal ({} after {removed})",
        again.version
    );
    for stale in [first.version, removed] {
        refused_change(
            case,
            &format!("a change naming {stale}, a version of the removed row"),
            store,
            &change(TASK, &identity, Some(stale), row("Open", "stale")),
        );
        refused_removal(
            case,
            &format!("a removal naming {stale}, a version of the removed row"),
            store,
            &removal(&identity, stale),
        );
    }
    assert_eq!(held(case, store, &identity), again, "{case}");
    assert_eq!(scan(case, store, TASK), vec![(identity, again)], "{case}");
}
