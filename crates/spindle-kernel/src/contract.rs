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
use crate::store::{
    Change, EVERY_INSTANCE, Removal, Row, Scan, Scanned, Store, StoreError, Stored, Write,
};

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
    Case {
        name: "writes_committed_together_all_land",
        run: writes_committed_together_all_land,
    },
    Case {
        name: "writes_committed_together_one_refused_write_none",
        run: writes_committed_together_one_refused_write_none,
    },
    Case {
        name: "a_commit_conditioned_on_a_scanned_entity_is_refused_after_another_write",
        run: a_commit_conditioned_on_a_scanned_entity_is_refused_after_another_write,
    },
    Case {
        name: "the_counts_are_the_rows_each_scan_answers",
        run: the_counts_are_the_rows_each_scan_answers,
    },
    Case {
        name: "a_reading_handle_reads_each_write_whole_as_it_lands",
        run: a_reading_handle_reads_each_write_whole_as_it_lands,
    },
    Case {
        name: "a_position_scans_and_loads_one_state_of_the_store",
        run: a_position_scans_and_loads_one_state_of_the_store,
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
        scanned: Vec::new(),
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
    scanned(case, store, entity).rows
}

fn scanned(case: &str, store: &mut dyn Store, entity: &str) -> Scan {
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

/// What every entity of [`ENTITIES`] holds.
fn everything(case: &str, store: &mut dyn Store) -> Vec<Vec<(Value, Stored)>> {
    ENTITIES
        .iter()
        .map(|entity| scan(case, store, entity))
        .collect()
}

fn writes_committed_together_all_land(store: &mut dyn Store) {
    let case = "writes_committed_together_all_land";
    let (moved, gone, made, other) = (json!("t1"), json!("t2"), json!("t3"), json!("o1"));
    commit(
        case,
        store,
        &change(TASK, &moved, None, row("Open", "moved")),
    );
    commit(case, store, &change(TASK, &gone, None, row("Open", "gone")));
    let (before_moved, before_gone) = (held(case, store, &moved), held(case, store, &gone));
    store
        .commit_all(&[
            Write::Commit(change(
                TASK,
                &moved,
                Some(before_moved.version),
                row("Done", "moved"),
            )),
            Write::Remove(removal(&gone, before_gone.version)),
            Write::Commit(change(TASK, &made, None, row("Open", "made"))),
            Write::Commit(change(OTHER, &other, None, row("Open", "other"))),
        ])
        .unwrap_or_else(|error| panic!("{case}: the writes commit together: {error}"));
    let after = held(case, store, &moved);
    assert_eq!(after.row, row("Done", "moved"), "{case}");
    assert!(after.version > before_moved.version, "{case}");
    assert_eq!(load(case, store, TASK, &gone), None, "{case}");
    assert_eq!(held(case, store, &made).row, row("Open", "made"), "{case}");
    assert_eq!(
        load(case, store, OTHER, &other).map(|stored| stored.row),
        Some(row("Open", "other")),
        "{case}"
    );
    store
        .commit_all(&[])
        .unwrap_or_else(|error| panic!("{case}: no writes commit: {error}"));
    commit(
        case,
        store,
        &change(TASK, &moved, Some(after.version), row("Open", "again")),
    );
}

/// Writes committed together that the store refuses: what is refused, the writes, the version
/// the refused write names and the version its instance is held at.
type Refusal<'a> = (&'a str, Vec<Write>, Option<u64>, Option<u64>);

fn writes_committed_together_one_refused_write_none(store: &mut dyn Store) {
    let case = "writes_committed_together_one_refused_write_none";
    let (first, second, third) = (json!("t1"), json!("t2"), json!("t3"));
    for identity in [&first, &second, &third] {
        commit(
            case,
            store,
            &change(TASK, identity, None, row("Open", "held")),
        );
    }
    let stale = held(case, store, &second).version;
    commit(
        case,
        store,
        &change(TASK, &second, Some(stale), row("Busy", "moved on")),
    );
    let (one, two, three) = (
        held(case, store, &first),
        held(case, store, &second),
        held(case, store, &third),
    );
    let before = everything(case, store);
    let refusals: [Refusal<'_>; 3] = [
        (
            "a later change naming a stale version",
            vec![
                Write::Commit(change(TASK, &first, Some(one.version), row("Done", "1"))),
                Write::Remove(removal(&third, three.version)),
                Write::Commit(change(TASK, &second, Some(stale), row("Done", "2"))),
            ],
            Some(stale),
            Some(two.version),
        ),
        (
            "a later removal naming a stale version",
            vec![
                Write::Commit(change(TASK, &first, Some(one.version), row("Done", "1"))),
                Write::Commit(change(OTHER, &json!("o1"), None, row("Open", "o"))),
                Write::Remove(removal(&second, stale)),
            ],
            Some(stale),
            Some(two.version),
        ),
        (
            "a later creation of a held identity",
            vec![
                Write::Remove(removal(&first, one.version)),
                Write::Commit(change(TASK, &json!("t4"), None, row("Open", "4"))),
                Write::Commit(change(TASK, &third, None, row("Open", "3"))),
            ],
            None,
            Some(three.version),
        ),
    ];
    for (what, writes, named, held_version) in refusals {
        conflict(case, what, store.commit_all(&writes), named, held_version);
        assert_eq!(
            everything(case, store),
            before,
            "{case}: {what} writes none of the writes"
        );
    }
    commit(
        case,
        store,
        &change(TASK, &first, Some(one.version), row("Done", "after")),
    );
}

/// Commits `change`, conditioned on `scanned`, by itself and as the one write of a group; the
/// store must refuse both as a conflict naming the entity, the version the scan answered and the
/// one the store holds now, and must leave every instance as it was.
fn refused_scanned(case: &str, store: &mut dyn Store, change: &Change, condition: &Scanned) {
    let before = everything(case, store);
    let now = scanned(case, store, &condition.entity).version;
    for grouped in [false, true] {
        let (what, result) = if grouped {
            (
                "the commit as a group",
                store.commit_all(&[Write::Commit(change.clone())]),
            )
        } else {
            ("the commit", store.commit(change))
        };
        if let Err(StoreError::Conflict {
            entity, identity, ..
        }) = &result
        {
            assert_eq!(
                (entity.as_str(), identity.as_str()),
                (condition.entity.as_str(), EVERY_INSTANCE),
                "{case}: {what}: the conflict names the scanned entity"
            );
        }
        conflict(case, what, result, Some(condition.version), Some(now));
        assert_eq!(
            everything(case, store),
            before,
            "{case}: {what} writes nothing"
        );
    }
}

fn a_commit_conditioned_on_a_scanned_entity_is_refused_after_another_write(store: &mut dyn Store) {
    let case = "a_commit_conditioned_on_a_scanned_entity_is_refused_after_another_write";
    commit(
        case,
        store,
        &change(TASK, &json!("t1"), None, row("Open", "1")),
    );
    let tasks = scanned(case, store, TASK);
    let others = scanned(case, store, OTHER);
    assert_eq!(
        scanned(case, store, TASK).version,
        tasks.version,
        "{case}: a scan moves no version"
    );
    // A second writer creates a row of the scanned entity.
    commit(
        case,
        store,
        &change(TASK, &json!("t2"), None, row("Open", "2")),
    );
    let left = scan(case, store, TASK);
    let at_scan = Scanned {
        entity: TASK.to_owned(),
        version: tasks.version,
    };
    let created = Change {
        scanned: vec![at_scan.clone()],
        ..change(TASK, &json!("t3"), None, row("Open", "3"))
    };
    refused_scanned(case, store, &created, &at_scan);
    let t1 = held(case, store, &json!("t1"));
    let moved = Change {
        scanned: vec![at_scan.clone()],
        ..change(TASK, &json!("t1"), Some(t1.version), row("Done", "1"))
    };
    refused_scanned(case, store, &moved, &at_scan);
    assert_eq!(
        scan(case, store, TASK),
        left,
        "{case}: the rows are as the second writer left them"
    );
    // A removal moves the entity too.
    let fresh = scanned(case, store, TASK);
    assert!(
        fresh.version > tasks.version,
        "{case}: a write moves the version"
    );
    let t2 = held(case, store, &json!("t2"));
    let removed = Removal {
        events: Vec::new(),
        ..removal(&json!("t2"), t2.version)
    };
    commit_removal(case, store, &removed);
    let at_fresh = Scanned {
        entity: TASK.to_owned(),
        version: fresh.version,
    };
    let after_removal = Change {
        scanned: vec![at_fresh.clone()],
        ..change(TASK, &json!("t3"), None, row("Open", "3"))
    };
    refused_scanned(case, store, &after_removal, &at_fresh);
    // A write of another entity does not move the scanned one, and a condition that still
    // holds lets the change land.
    let current = scanned(case, store, TASK);
    commit(
        case,
        store,
        &change(OTHER, &json!("o1"), None, row("Open", "o")),
    );
    let lands = Change {
        scanned: vec![Scanned {
            entity: TASK.to_owned(),
            version: current.version,
        }],
        ..change(TASK, &json!("t3"), None, row("Open", "3"))
    };
    commit(case, store, &lands);
    assert!(
        scanned(case, store, TASK).version > current.version,
        "{case}: a conditioned change moves the version"
    );
    let at_others = Scanned {
        entity: OTHER.to_owned(),
        version: others.version,
    };
    let stale_other = Change {
        scanned: vec![at_others.clone()],
        ..change(TASK, &json!("t4"), None, row("Open", "4"))
    };
    refused_scanned(case, store, &stale_other, &at_others);
}

fn commit_removal(case: &str, store: &mut dyn Store, removal: &Removal) {
    store.remove(removal).unwrap_or_else(|error| {
        panic!(
            "{case}: {} {} is removed: {error}",
            removal.entity, removal.identity
        )
    });
}

/// [`Store::counts`] for every entity of [`ENTITIES`] and one never written: as many as a scan of
/// each answers rows, through creations, a change, a removal and a creation again.
fn the_counts_are_the_rows_each_scan_answers(store: &mut dyn Store) {
    let case = "the_counts_are_the_rows_each_scan_answers";
    let never = "contract.tasks.Never".to_owned();
    let mut entities = ENTITIES
        .iter()
        .map(|entity| (*entity).to_owned())
        .collect::<Vec<_>>();
    entities.push(never.clone());
    let counted = |store: &mut dyn Store| {
        let counts = store
            .counts(&entities)
            .unwrap_or_else(|error| panic!("{case}: the store counts: {error}"));
        let scanned = entities
            .iter()
            .map(|entity| (entity.clone(), scan(case, store, entity).len()))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(counts, scanned, "{case}");
        counts
    };
    assert_eq!(
        counted(store),
        BTreeMap::from([
            (TASK.to_owned(), 0),
            (OTHER.to_owned(), 0),
            (never.clone(), 0)
        ]),
        "{case}"
    );
    let (kept, gone, other) = (json!("t1"), json!("t2"), json!("o1"));
    commit(case, store, &change(TASK, &kept, None, row("Open", "kept")));
    commit(case, store, &change(TASK, &gone, None, row("Open", "gone")));
    commit(
        case,
        store,
        &change(OTHER, &other, None, row("Open", "other")),
    );
    let version = held(case, store, &kept).version;
    commit(
        case,
        store,
        &change(TASK, &kept, Some(version), row("Done", "kept")),
    );
    let version = held(case, store, &gone).version;
    commit_removal(case, store, &removal(&gone, version));
    assert_eq!(
        counted(store),
        BTreeMap::from([
            (TASK.to_owned(), 1),
            (OTHER.to_owned(), 1),
            (never.clone(), 0)
        ]),
        "{case}"
    );
    commit(
        case,
        store,
        &change(TASK, &gone, None, row("Open", "again")),
    );
    assert_eq!(counted(store)[TASK], 2, "{case}");
}

/// Where the store gives a reading handle ([`Store::reading`]), it reads what the store holds as
/// each write lands, a group of writes whole, and nothing of a group the store refused; it counts
/// as the store counts. A store that gives none passes as it is.
fn a_reading_handle_reads_each_write_whole_as_it_lands(store: &mut dyn Store) {
    let case = "a_reading_handle_reads_each_write_whole_as_it_lands";
    let Some(reading) = store.reading() else {
        return;
    };
    let entities = ENTITIES
        .iter()
        .map(|entity| (*entity).to_owned())
        .collect::<Vec<_>>();
    let same = |store: &mut dyn Store, what: &str| {
        for entity in ENTITIES {
            let read = reading
                .scan(entity)
                .unwrap_or_else(|error| panic!("{case}: {what}: the handle reads: {error}"));
            assert_eq!(
                read,
                scanned(case, store, entity),
                "{case}: {what}: the handle reads {entity} as the store holds it"
            );
        }
        let counts = reading
            .counts(&entities)
            .unwrap_or_else(|error| panic!("{case}: {what}: the handle counts: {error}"));
        let held = store
            .counts(&entities)
            .unwrap_or_else(|error| panic!("{case}: {what}: the store counts: {error}"));
        assert_eq!(
            counts, held,
            "{case}: {what}: the handle counts as the store"
        );
    };
    same(store, "empty");
    let (first, second, other) = (json!("t1"), json!("t2"), json!("o1"));
    commit(case, store, &change(TASK, &first, None, row("Open", "1")));
    same(store, "a creation");
    store
        .commit_all(&[
            Write::Commit(change(TASK, &second, None, row("Open", "2"))),
            Write::Commit(change(OTHER, &other, None, row("Open", "o"))),
        ])
        .unwrap_or_else(|error| panic!("{case}: the writes commit together: {error}"));
    same(store, "writes committed together");
    let one = held(case, store, &first);
    let refused = store.commit_all(&[
        Write::Commit(change(TASK, &first, Some(one.version), row("Done", "1"))),
        Write::Commit(change(TASK, &second, None, row("Open", "again"))),
    ]);
    assert!(refused.is_err(), "{case}: a creation of a held identity");
    same(store, "a refused group");
    commit_removal(case, store, &removal(&first, one.version));
    same(store, "a removal");
}

/// How many positions [`a_position_scans_and_loads_one_state_of_the_store`] takes at most before
/// one holds.
const POSITIONS: usize = 4;

/// Where the store gives a reading handle, a position of it ([`crate::Reading::at`]) that holds
/// answers one state of the store: a scan of one entity and a load of an instance of another
/// agree, even where a group of writes changing both landed between them, and the position then
/// answers what the store held before the group (a store that takes its entities at once) or
/// does not hold, and a position taken next answers what it held after. A load of an identity
/// that rests nowhere answers `None`, and a read of an entity the position was not taken for
/// fails.
fn a_position_scans_and_loads_one_state_of_the_store(store: &mut dyn Store) {
    let case = "a_position_scans_and_loads_one_state_of_the_store";
    let Some(reading) = store.reading() else {
        return;
    };
    let (task, other) = (json!("t1"), json!("o1"));
    commit(
        case,
        store,
        &change(TASK, &task, None, row("Open", "before")),
    );
    commit(
        case,
        store,
        &change(OTHER, &other, None, row("Open", "before")),
    );
    let entities = [TASK.to_owned(), OTHER.to_owned()];
    let title = |stored: &Stored| stored.row.fields["title"].clone();
    let mut read = None;
    for attempt in 0..POSITIONS {
        let position = reading
            .at(&entities)
            .unwrap_or_else(|error| panic!("{case}: a position is taken: {error}"));
        let scanned = position
            .scan(TASK)
            .unwrap_or_else(|error| panic!("{case}: the position scans: {error}"));
        if attempt == 0 {
            let one = held(case, store, &task);
            let two =
                load(case, store, OTHER, &other).unwrap_or_else(|| panic!("{case}: {other} rests"));
            store
                .commit_all(&[
                    Write::Commit(change(TASK, &task, Some(one.version), row("Open", "after"))),
                    Write::Commit(change(
                        OTHER,
                        &other,
                        Some(two.version),
                        row("Open", "after"),
                    )),
                ])
                .unwrap_or_else(|error| panic!("{case}: the writes commit together: {error}"));
        }
        let loaded = position
            .load(OTHER, &other)
            .unwrap_or_else(|error| panic!("{case}: the position loads: {error}"))
            .unwrap_or_else(|| panic!("{case}: {other} rests"));
        assert_eq!(
            position
                .load(OTHER, &json!("nowhere"))
                .unwrap_or_else(|error| panic!("{case}: the position loads: {error}")),
            None,
            "{case}: an identity that rests nowhere"
        );
        assert!(
            position.load("contract.tasks.Untaken", &other).is_err(),
            "{case}: a load of an entity the position was not taken for fails"
        );
        if position
            .holds()
            .unwrap_or_else(|error| panic!("{case}: the position is checked: {error}"))
        {
            let [(identity, row)] = scanned.rows.as_slice() else {
                panic!("{case}: one task is scanned: {scanned:?}");
            };
            assert_eq!(identity, &task, "{case}");
            read = Some((attempt, title(row), title(&loaded)));
            break;
        }
    }
    let Some((attempt, scanned, loaded)) = read else {
        panic!(
            "{case}: no position of {POSITIONS} held, though nothing was written after the first"
        );
    };
    assert_eq!(
        scanned, loaded,
        "{case}: the scan and the load of one position answer one state of the store"
    );
    let expected = if attempt == 0 { "before" } else { "after" };
    assert_eq!(
        scanned,
        json!(expected),
        "{case}: a position that holds at once answers the store before the group, a later one \
         after it"
    );
}
