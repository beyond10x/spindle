//! Adversarial cases for the entity version a scan answers and a change is conditioned on
//! (`story:kernel-row-set-commit-conflict`): two SQLite handles on one file, a database written
//! before the version stream existed, a refused group, and the kernel raced on the second entity
//! of `Reserve`, on a removal, on an unscanned entity, on `Lend` and on the SQLite store.

use std::path::Path;
use std::time::{Duration, SystemTime};

use eventlog_core::{
    CommandMeta, EventStore, Expected, NewEvent, StreamId, TenantId, new_event_id, request_hash,
};
use eventlog_sqlite::SqliteEventStore;
use serde_json::{Value, json};
use spindle_kernel::{
    Change, EVERY_INSTANCE, ExecuteError, Executed, MemoryStore, NoExternals, Removal, Row, Scan,
    Scanned, SqliteStore, Store, StoreError, Stored, UuidV4, Write, execute_at,
};

use super::related_guards::object;

const TASK: &str = "probe.tasks.Task";

fn row(state: &str, title: &str) -> Row {
    Row {
        state: state.to_owned(),
        fields: [("title".to_owned(), json!(title))].into_iter().collect(),
    }
}

fn create(entity: &str, identity: &str, row: Row) -> Change {
    Change {
        entity: entity.to_owned(),
        identity: json!(identity),
        expected: None,
        row,
        events: Vec::new(),
        scanned: Vec::new(),
    }
}

fn conditioned(change: Change, entity: &str, version: u64) -> Change {
    Change {
        scanned: vec![Scanned {
            entity: entity.to_owned(),
            version,
        }],
        ..change
    }
}

fn is_entity_conflict(result: &Result<(), StoreError>, entity: &str) -> bool {
    matches!(result, Err(StoreError::Conflict { entity: named, identity, .. })
        if named == entity && identity == EVERY_INSTANCE)
}

/// Two handles on one SQLite file, as two processes hold it: the first scans, the second commits
/// a row of the entity, and the first's change conditioned on its scan is refused.
#[test]
fn two_sqlite_handles_on_one_file_refuse_a_change_decided_on_a_stale_scan() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = directory.path().join("two.sqlite");
    let mut first = SqliteStore::open(&path).expect("the first handle opens");
    let mut second = SqliteStore::open(&path).expect("the second handle opens");
    first
        .commit(&create(TASK, "t1", row("Open", "1")))
        .expect("t1 commits");
    let scan = first.scan(TASK).expect("the first handle scans");
    second
        .commit(&create(TASK, "t2", row("Open", "2")))
        .expect("the second handle commits t2");
    let result = first.commit(&conditioned(
        create(TASK, "t3", row("Open", "3")),
        TASK,
        scan.version,
    ));
    assert!(
        is_entity_conflict(&result, TASK),
        "the first handle's stale change is refused as an entity conflict, not {result:?}"
    );
    assert!(
        first.load(TASK, &json!("t3")).expect("t3 loads").is_none(),
        "t3 did not land"
    );
}

/// What the SQLite store wrote before the entity version stream existed: one `spindle.row` event
/// on the instance's stream, nothing else.
fn write_as_before(path: &Path, identity: &str, row: &Row) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a runtime");
    let log = runtime
        .block_on(SqliteEventStore::open(
            path.to_str().expect("a UTF-8 path"),
            "spindle",
        ))
        .expect("the log opens");
    let stream = StreamId::new(
        TenantId::new("local").expect("a tenant"),
        TASK,
        json!(identity).to_string(),
    )
    .expect("a stream");
    let events = vec![
        NewEvent::new(
            "spindle.row",
            1,
            serde_json::to_value(row).expect("a row value"),
        )
        .expect("an event"),
    ];
    let meta = CommandMeta {
        idempotency_key: new_event_id(),
        request_hash: request_hash(&events).expect("a hash"),
        subject: "spindle-kernel".to_owned(),
        actor: "spindle-kernel".to_owned(),
        request_id: new_event_id(),
        trace_id: new_event_id(),
        causation_id: None,
        causation_depth: 0,
        occurred_at: time::OffsetDateTime::now_utc(),
        claim: None,
    };
    runtime
        .block_on(log.append(&stream, Expected::NoStream, &events, &meta))
        .expect("the old-format row is appended");
}

/// A database a release before this change wrote holds rows and no entity version stream. The
/// new store reads it, a change conditioned on its scan lands while nothing moved, and a stale one
/// is refused once a row is written after the scan.
#[test]
fn a_sqlite_database_written_before_the_entity_stream_refuses_a_stale_scan() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = directory.path().join("old.sqlite");
    write_as_before(&path, "t1", &row("Open", "old"));
    let mut store = SqliteStore::open(&path).expect("the store opens an old database");
    let scan = store.scan(TASK).expect("the old database scans");
    assert_eq!(scan.rows.len(), 1, "the old row is read: {:?}", scan.rows);
    store
        .commit(&conditioned(
            create(TASK, "t2", row("Open", "2")),
            TASK,
            scan.version,
        ))
        .expect("a change conditioned on an unmoved old database lands");
    let stale = store.commit(&conditioned(
        create(TASK, "t3", row("Open", "3")),
        TASK,
        scan.version,
    ));
    assert!(
        is_entity_conflict(&stale, TASK),
        "a change conditioned on the scan before t2 is refused, not {stale:?}"
    );
}

/// A group one of whose writes is refused lands nothing, so it moves no entity version: a change
/// conditioned on the scan before it still lands.
fn a_refused_group_moves_no_entity_version(store: &mut dyn Store) {
    store
        .commit(&create(TASK, "t1", row("Open", "1")))
        .expect("t1 commits");
    let before = store.scan(TASK).expect("the store scans");
    let refused = store.commit_all(&[
        Write::Commit(create(TASK, "t2", row("Open", "2"))),
        Write::Commit(Change {
            expected: Some(99),
            ..create(TASK, "t1", row("Done", "1"))
        }),
    ]);
    assert!(
        matches!(refused, Err(StoreError::Conflict { .. })),
        "the group is refused: {refused:?}"
    );
    assert_eq!(
        store.scan(TASK).expect("the store scans").version,
        before.version,
        "a refused group moves no version"
    );
    store
        .commit(&conditioned(
            create(TASK, "t3", row("Open", "3")),
            TASK,
            before.version,
        ))
        .expect("a change conditioned on the scan before the refused group lands");
}

#[test]
fn a_refused_group_moves_no_entity_version_on_the_memory_store() {
    a_refused_group_moves_no_entity_version(&mut MemoryStore::default());
}

#[test]
fn a_refused_group_moves_no_entity_version_on_the_sqlite_store() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut store =
        SqliteStore::open(&directory.path().join("group.sqlite")).expect("the store opens");
    a_refused_group_moves_no_entity_version(&mut store);
}

/// A removal of an instance after the scan moves the entity, and a group that lands after the
/// scan does too, even where it writes the entity twice: a change conditioned on either scan is
/// refused as an entity conflict and lands nothing.
fn a_removal_and_a_landed_group_move_the_entity_version(store: &mut dyn Store) {
    store
        .commit(&create(TASK, "t1", row("Open", "1")))
        .expect("t1 commits");
    let before_removal = store.scan(TASK).expect("the store scans");
    let held = store
        .load(TASK, &json!("t1"))
        .expect("t1 loads")
        .expect("t1 is held");
    store
        .remove(&Removal {
            entity: TASK.to_owned(),
            identity: json!("t1"),
            expected: held.version,
            events: Vec::new(),
        })
        .expect("t1 is removed");
    let after_removal = store.commit(&conditioned(
        create(TASK, "t2", row("Open", "2")),
        TASK,
        before_removal.version,
    ));
    assert!(
        is_entity_conflict(&after_removal, TASK),
        "a change conditioned on the scan before the removal is refused, not {after_removal:?}"
    );
    assert!(
        store.load(TASK, &json!("t2")).expect("t2 loads").is_none(),
        "t2 did not land"
    );
    let before_group = store.scan(TASK).expect("the store scans");
    store
        .commit_all(&[
            Write::Commit(create(TASK, "t3", row("Open", "3"))),
            Write::Commit(create(TASK, "t4", row("Open", "4"))),
        ])
        .expect("the group lands");
    assert_ne!(
        store.scan(TASK).expect("the store scans").version,
        before_group.version,
        "a landed group moves the version"
    );
    let after_group = store.commit_all(&[Write::Commit(conditioned(
        create(TASK, "t5", row("Open", "5")),
        TASK,
        before_group.version,
    ))]);
    assert!(
        is_entity_conflict(&after_group, TASK),
        "a group conditioned on the scan before the landed group is refused, not {after_group:?}"
    );
    assert!(
        store.load(TASK, &json!("t5")).expect("t5 loads").is_none(),
        "t5 did not land"
    );
}

#[test]
fn a_removal_and_a_landed_group_move_the_entity_version_on_the_memory_store() {
    a_removal_and_a_landed_group_move_the_entity_version(&mut MemoryStore::default());
}

#[test]
fn a_removal_and_a_landed_group_move_the_entity_version_on_the_sqlite_store() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut store =
        SqliteStore::open(&directory.path().join("moves.sqlite")).expect("the store opens");
    a_removal_and_a_landed_group_move_the_entity_version(&mut store);
}

/// What another writer does to the store, once.
type Race<S> = Box<dyn FnOnce(&mut S)>;

/// A store another writer writes to right after the kernel's first scan of `entity`.
struct Raced<S: Store> {
    inner: S,
    entity: &'static str,
    race: Option<Race<S>>,
}

impl<S: Store> Store for Raced<S> {
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
        self.inner.commit_all(writes)
    }

    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError> {
        let scan = self.inner.scan(entity)?;
        if entity == self.entity
            && let Some(race) = self.race.take()
        {
            race(&mut self.inner);
        }
        Ok(scan)
    }
}

const ENTRY: &str = "stacks.library.Entry";
const RESERVATION: &str = "stacks.library.Reservation";
const ENTRY_ID: &str = "c1c2c3c4-0000-4000-8000-000000000001";
const READER: &str = "a1a2a3a4-0000-4000-8000-000000000001";
const OTHER_READER: &str = "a1a2a3a4-0000-4000-8000-000000000002";
const BOOK: &str = "0b0c0d0e-0000-4000-8000-000000000001";
const OURS: &str = "e1e2e3e4-0000-4000-8000-000000000001";
const THEIRS: &str = "e1e2e3e4-0000-4000-8000-000000000002";
const UNTIL: &str = "2026-06-01T00:00:00Z";

fn library() -> spindle_spec::EssIr {
    spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/library"))
        .expect("the library fixture compiles")
        .ir
}

/// 2026-03-01T00:00:00Z, before [`UNTIL`].
fn library_now() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_772_323_200)
}

fn run(
    ir: &spindle_spec::EssIr,
    store: &mut dyn Store,
    command: &str,
    input: Value,
) -> Result<Executed, ExecuteError> {
    execute_at(
        ir,
        store,
        &mut NoExternals,
        &mut UuidV4,
        &library_now,
        command,
        &object(input),
    )
}

fn reserve(reservation: &str, reader: &str) -> Value {
    json!({
        "reservation_id": reservation,
        "reader_id": reader,
        "book_id": BOOK,
        "until": UNTIL,
        "shelfmarks": ["QA-76"],
    })
}

fn listed<S: Store>(ir: &spindle_spec::EssIr, inner: S, entity: &'static str) -> Raced<S> {
    let mut store = Raced {
        inner,
        entity,
        race: None,
    };
    let listed = run(
        ir,
        &mut store,
        "stacks.library.ListEntry",
        json!({"entry_id": ENTRY_ID, "code": "QA-76"}),
    )
    .expect("the entry is listed");
    assert_eq!(listed.outcome, "listed");
    store
}

fn answered(result: &Result<Executed, ExecuteError>) -> String {
    match result {
        Ok(executed) => format!("Ok({})", executed.outcome),
        Err(error) => format!("Err({error:?})"),
    }
}

fn store_conflict(result: &Result<Executed, ExecuteError>) -> bool {
    matches!(
        result,
        Err(ExecuteError::Store(StoreError::Conflict { .. }))
    )
}

/// `Reserve` answers `reserved` only where an entry carries a shelfmark given (`unknown-shelfmark`,
/// a row set over `Entry`). Another writer moves the only entry to another shelfmark after the
/// kernel scanned the entries: the decision rests on a row set that no longer holds, and the
/// reservation is refused as a conflict.
#[test]
fn an_entry_changed_after_its_scan_refuses_the_reservation() {
    let ir = library();
    let mut store = listed(&ir, MemoryStore::default(), ENTRY);
    store.race = Some(Box::new(|inner: &mut MemoryStore| {
        let held = inner
            .load(ENTRY, &json!(ENTRY_ID))
            .expect("the entry loads")
            .expect("the entry is held");
        inner
            .commit(&Change {
                entity: ENTRY.to_owned(),
                identity: json!(ENTRY_ID),
                expected: Some(held.version),
                row: Row {
                    state: "Listed".to_owned(),
                    fields: [("code".to_owned(), json!("ZZ-99"))].into_iter().collect(),
                },
                events: Vec::new(),
                scanned: Vec::new(),
            })
            .expect("the other writer moves the entry");
    }));
    let ours = run(
        &ir,
        &mut store,
        "stacks.library.Reserve",
        reserve(OURS, READER),
    );
    assert!(
        store_conflict(&ours),
        "a reservation decided on a stale entry is refused, not {}",
        answered(&ours)
    );
    assert!(
        store
            .inner
            .load(RESERVATION, &json!(OURS))
            .expect("the reservation loads")
            .is_none(),
        "no reservation landed"
    );
}

fn reservation(reader: &str, state: &str) -> Row {
    Row {
        state: state.to_owned(),
        fields: [
            ("reader_id".to_owned(), json!(reader)),
            ("book_id".to_owned(), json!(BOOK)),
            ("until".to_owned(), json!(UNTIL)),
        ]
        .into_iter()
        .collect(),
    }
}

/// A reservation of another reader is removed after the kernel scanned the reservations: a
/// removal moves the entity, so the reservation decided on the scan is refused.
#[test]
fn a_reservation_removed_after_the_scan_refuses_the_reservation() {
    let ir = library();
    let mut store = listed(&ir, MemoryStore::default(), RESERVATION);
    store
        .inner
        .commit(&create(
            RESERVATION,
            THEIRS,
            reservation(OTHER_READER, "Active"),
        ))
        .expect("their reservation commits");
    store.race = Some(Box::new(|inner: &mut MemoryStore| {
        let held = inner
            .load(RESERVATION, &json!(THEIRS))
            .expect("their reservation loads")
            .expect("their reservation is held");
        inner
            .remove(&Removal {
                entity: RESERVATION.to_owned(),
                identity: json!(THEIRS),
                expected: held.version,
                events: Vec::new(),
            })
            .expect("the other writer removes their reservation");
    }));
    let ours = run(
        &ir,
        &mut store,
        "stacks.library.Reserve",
        reserve(OURS, READER),
    );
    assert!(
        store_conflict(&ours),
        "a reservation decided on a scan a removal moved is refused, not {}",
        answered(&ours)
    );
}

/// A write of an entity no guard of `Reserve` scans, landing between the scan and the commit,
/// does not refuse the reservation.
#[test]
fn a_write_of_an_unscanned_entity_after_the_scan_lets_the_reservation_land() {
    let ir = library();
    let mut store = listed(&ir, MemoryStore::default(), RESERVATION);
    store.race = Some(Box::new(|inner: &mut MemoryStore| {
        inner
            .commit(&create("elsewhere.things.Thing", "x1", row("Open", "x")))
            .expect("the other writer writes another entity");
    }));
    let ours = run(
        &ir,
        &mut store,
        "stacks.library.Reserve",
        reserve(OURS, READER),
    );
    assert_eq!(
        answered(&ours),
        "Ok(reserved)",
        "a write of another entity does not refuse the reservation"
    );
}

/// `Lend` moves its subject, guarded by a row set over the subject's own entity
/// (`already-lent`). Another writer lends the other reservation of the same book after the scan:
/// the lend decided on the scan is refused, and the book is out once.
#[test]
fn a_reservation_lent_after_the_scan_refuses_another_lend() {
    let ir = library();
    let mut store = listed(&ir, MemoryStore::default(), RESERVATION);
    for (id, reader) in [(OURS, READER), (THEIRS, OTHER_READER)] {
        let reserved = run(
            &ir,
            &mut store,
            "stacks.library.Reserve",
            reserve(id, reader),
        );
        assert_eq!(answered(&reserved), "Ok(reserved)");
    }
    store.race = Some(Box::new(|inner: &mut MemoryStore| {
        let held = inner
            .load(RESERVATION, &json!(THEIRS))
            .expect("their reservation loads")
            .expect("their reservation is held");
        inner
            .commit(&Change {
                expected: Some(held.version),
                ..create(RESERVATION, THEIRS, reservation(OTHER_READER, "Lent"))
            })
            .expect("the other writer lends their reservation");
    }));
    let ours = run(
        &ir,
        &mut store,
        "stacks.library.Lend",
        json!({"reservation_id": OURS}),
    );
    assert!(
        store_conflict(&ours),
        "a lend decided on a stale scan is refused, not {}",
        answered(&ours)
    );
    let lent = store
        .inner
        .scan(RESERVATION)
        .expect("the store scans")
        .rows
        .into_iter()
        .filter(|(_, stored)| stored.row.state == "Lent")
        .count();
    assert_eq!(lent, 1, "the book is out once");
}

/// The unit's race case on the SQLite store: another writer commits an active reservation of the
/// same reader and book after the kernel's scan, and the kernel's reservation is refused.
#[test]
fn the_sqlite_store_refuses_a_reservation_raced_after_its_scan() {
    let ir = library();
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let inner = SqliteStore::open(&directory.path().join("race.sqlite")).expect("the store opens");
    let mut store = listed(&ir, inner, RESERVATION);
    store.race = Some(Box::new(|inner: &mut SqliteStore| {
        inner
            .commit(&create(RESERVATION, THEIRS, reservation(READER, "Active")))
            .expect("the other writer reserves first");
    }));
    let ours = run(
        &ir,
        &mut store,
        "stacks.library.Reserve",
        reserve(OURS, READER),
    );
    assert!(
        store_conflict(&ours),
        "the losing reservation is refused as a store conflict, not {}",
        answered(&ours)
    );
    let active: Vec<Value> = store
        .inner
        .scan(RESERVATION)
        .expect("the store scans")
        .rows
        .into_iter()
        .filter(|(_, stored)| stored.row.state == "Active")
        .map(|(identity, _)| identity)
        .collect();
    assert_eq!(
        active,
        [json!(THEIRS)],
        "only the first reservation is active"
    );
}
