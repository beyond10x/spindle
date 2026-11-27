//! Adversary cases of wave 31 U8 (story:dead-letters-kept-and-replayed): the dead-letter stream
//! (`spindle:log` `dead-letters`) beside the rows and events of a store. Scans, counts and the
//! resting rows a SQLite file folds when it opens skip it; a feed page of nothing but dead
//! letters does not end a read of the events; `state move` copies it into PostgreSQL with its ids,
//! and counts tables holding it as not empty.
//!
//! The `_on_postgres` cases are ignored without `SPINDLE_TEST_DATABASE_URL`, a loopback
//! PostgreSQL URL of a role that may create schemas and tables; `task check:postgres` runs them.

use std::collections::BTreeMap;

use serde_json::{Map, json};
use spindle_kernel::{
    Change, Database, DeadLetter, Event, MemoryStore, PostgresStore, Row, SqliteStore, StateLock,
    StateSource, StateTarget, Store,
};

const TASK: &str = "adv.task.Task";

fn task(identity: &str, expected: Option<u64>, state: &str) -> Change {
    Change {
        entity: TASK.to_owned(),
        identity: json!(identity),
        expected,
        row: Row {
            state: state.to_owned(),
            fields: BTreeMap::from([("id".to_owned(), json!(identity))]),
        },
        events: Vec::new(),
        scanned: Vec::new(),
    }
}

fn letter(order: &str) -> DeadLetter {
    DeadLetter {
        binding: "ship-on-order".to_owned(),
        event: Some("depot.orders.OrderPlaced".to_owned()),
        command: "depot.shipping.Ship".to_owned(),
        input: Map::from_iter([("order_id".to_owned(), json!(order))]),
        outcome: Some("busy".to_owned()),
        error: "depot.shipping.Busy".to_owned(),
        attempts: 5,
    }
}

fn named(name: &str) -> Event {
    Event {
        name: name.to_owned(),
        payload: Map::from_iter([("n".to_owned(), json!(name))]),
    }
}

/// Two rows, one removed and one kept, written around dead letters kept and taken.
fn written_around_letters(store: &mut dyn Store) {
    store.commit(&task("a", None, "open")).expect("row a");
    store.keep_dead_letter(&letter("x")).expect("a letter");
    store.commit(&task("b", None, "open")).expect("row b");
    let taken = store.keep_dead_letter(&letter("y")).expect("a letter");
    store.take_dead_letter(taken).expect("a take");
    store
        .commit(&task("a", Some(1), "done"))
        .expect("row a again");
    store.keep_dead_letter(&letter("z")).expect("a letter");
}

/// The same rows, written with no dead letter around them.
fn written_alone(store: &mut dyn Store) {
    store.commit(&task("a", None, "open")).expect("row a");
    store.commit(&task("b", None, "open")).expect("row b");
    store
        .commit(&task("a", Some(1), "done"))
        .expect("row a again");
}

/// A scan, the counts and a reading handle's scan of a store holding dead letters answer what a
/// store holding none answers; so does a SQLite file opened again, whose resting rows are folded
/// from its feed.
#[test]
fn adv_w31_u8_scans_counts_and_a_reopened_fold_skip_dead_letters() {
    let entities = [TASK.to_owned()];
    let mut plain = MemoryStore::default();
    written_alone(&mut plain);
    let expected = plain.scan(TASK).expect("the plain scan");
    assert_eq!(expected.rows.len(), 2, "{expected:?}");

    let mut memory = MemoryStore::default();
    written_around_letters(&mut memory);
    assert_eq!(memory.scan(TASK).expect("the memory scan"), expected);

    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = directory.path().join("letters.sqlite");
    {
        let mut sqlite = SqliteStore::open(&path).expect("SQLite opens");
        written_around_letters(&mut sqlite);
        assert_eq!(sqlite.scan(TASK).expect("the SQLite scan"), expected);
        assert_eq!(
            sqlite.counts(&entities).expect("the counts"),
            BTreeMap::from([(TASK.to_owned(), 2)])
        );
    }
    let mut reopened = SqliteStore::open(&path).expect("SQLite opens again");
    assert_eq!(reopened.scan(TASK).expect("the reopened scan"), expected);
    assert_eq!(
        reopened.counts(&entities).expect("the reopened counts"),
        BTreeMap::from([(TASK.to_owned(), 2)])
    );
    let reading = reopened.reading().expect("SQLite gives a reading handle");
    assert_eq!(reading.scan(TASK).expect("the handle's scan"), expected);
    assert_eq!(
        reading.counts(&entities).expect("the handle's counts"),
        BTreeMap::from([(TASK.to_owned(), 2)])
    );
    assert_eq!(
        reopened
            .dead_letters()
            .expect("the letters read")
            .into_iter()
            .map(|(_, letter)| letter.input["order_id"].clone())
            .collect::<Vec<_>>(),
        [json!("x"), json!("z")]
    );
}

/// More dead letters than one page of the feed holds (500), kept between two events, do not end
/// a read of the events: each read, whole or one event at a time, answers both events, and the
/// read after the second answers nothing.
#[test]
fn adv_w31_u8_a_feed_page_of_dead_letters_between_two_events_reads_on() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut store =
        SqliteStore::open(&directory.path().join("paged.sqlite")).expect("SQLite opens");
    store
        .commit_events(&[named("First")], &[])
        .expect("the first event");
    for at in 0..520 {
        store
            .keep_dead_letter(&letter(&format!("o-{at}")))
            .expect("a letter");
    }
    store
        .commit_events(&[named("Second")], &[])
        .expect("the second event");
    let names = |read: &[spindle_kernel::Committed]| {
        read.iter()
            .map(|committed| committed.event.name.clone())
            .collect::<Vec<_>>()
    };
    let whole = store.events_after(0, usize::MAX).expect("the events read");
    assert_eq!(names(&whole), ["First", "Second"]);
    let first = store.events_after(0, 1).expect("the first page");
    assert_eq!(names(&first), ["First"]);
    let second = store
        .events_after(first[0].position, 1)
        .expect("the second page");
    assert_eq!(names(&second), ["Second"]);
    assert!(
        store
            .events_after(second[0].position, usize::MAX)
            .expect("the read after the last")
            .is_empty()
    );
    let reading = store.reading().expect("SQLite gives a reading handle");
    assert_eq!(
        names(
            &reading
                .events_after(0, usize::MAX)
                .expect("the handle reads")
        ),
        ["First", "Second"]
    );
    assert_eq!(store.dead_letters().expect("the letters read").len(), 520);
}

/// The version a take appended at in the dead-letter stream is no dead letter's id: it is neither
/// listed nor taken, and the next letter kept is listed under an id of its own.
#[test]
fn adv_w31_u8_the_version_of_a_take_is_no_dead_letter() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut store =
        SqliteStore::open(&directory.path().join("takes.sqlite")).expect("SQLite opens");
    let first = store.keep_dead_letter(&letter("a")).expect("a letter");
    assert_eq!(
        store.take_dead_letter(first).expect("the take"),
        Some(letter("a"))
    );
    let next = store.keep_dead_letter(&letter("b")).expect("a letter");
    for id in (first + 1)..next {
        assert_eq!(
            store.take_dead_letter(id).expect("a take of no letter"),
            None,
            "id {id} is a take's version"
        );
    }
    assert_eq!(
        store.dead_letters().expect("the letters read"),
        vec![(next, letter("b"))]
    );
}

fn url() -> String {
    std::env::var("SPINDLE_TEST_DATABASE_URL")
        .expect("SPINDLE_TEST_DATABASE_URL names a loopback PostgreSQL (task check:postgres)")
}

fn letters() -> String {
    uuid::Uuid::new_v4()
        .simple()
        .to_string()
        .bytes()
        .take(12)
        .map(|byte| char::from(b'a' + byte % 26))
        .collect()
}

/// The test database, in a schema of its own.
fn database() -> Database {
    let schema = format!("spindle_advdl_{}", letters());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    runtime.block_on(async {
        let (client, connection) = tokio_postgres::connect(&url(), tokio_postgres::NoTls)
            .await
            .expect("the test database accepts its owner");
        let driver = tokio::spawn(connection);
        client
            .simple_query(&format!("CREATE SCHEMA {schema}"))
            .await
            .expect("the schema is created");
        drop(client);
        let _ = driver.await;
    });
    let mut database = Database::new(url());
    database.schema = schema;
    database
}

/// A SQLite file holding rows, dead letters, a taken one and a reset mark moves into PostgreSQL
/// with them: the store opened on the tables lists the letters kept after the reset and not
/// taken, under the ids the file listed them with, takes them by those ids, and scans the rows
/// the file scanned.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv_w31_u8_a_moved_file_keeps_its_dead_letters_on_postgres() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = directory.path().join("moved.sqlite");
    let (listed, scanned) = {
        let mut file = SqliteStore::open(&path).expect("the file opens");
        file.keep_dead_letter(&letter("before-reset"))
            .expect("a letter");
        file.forget_events().expect("a reset mark");
        written_around_letters(&mut file);
        (
            file.dead_letters().expect("the file's letters"),
            file.scan(TASK).expect("the file's scan"),
        )
    };
    assert_eq!(listed.len(), 2, "{listed:?}");
    let database = database();
    let component = format!("letters-{}", letters());
    let source = StateSource::open(&path).expect("the file opens");
    let checked = source.check().expect("the file can move");
    let lock = StateLock::take(&database).expect("the schema's lock is taken");
    let target = StateTarget::open(&database, &component, 1).expect("the tables open");
    assert!(
        target
            .is_empty(&[TASK.to_owned()], &source)
            .expect("the tables are read"),
        "fresh tables are empty"
    );
    assert_eq!(target.copy(&source).expect("the file is copied"), checked);
    lock.release().expect("the lock is released");
    let mut moved = PostgresStore::open(&database, &component, 1).expect("the tables open");
    assert_eq!(moved.dead_letters().expect("the moved letters"), listed);
    assert_eq!(moved.scan(TASK).expect("the moved scan"), scanned);
    assert!(
        moved
            .events_after(0, usize::MAX)
            .expect("the events read")
            .is_empty(),
        "no dead letter moved as an event"
    );
    let (id, kept) = listed[0].clone();
    assert_eq!(moved.take_dead_letter(id).expect("the take"), Some(kept));
    let next = moved.keep_dead_letter(&letter("after")).expect("a letter");
    assert!(next > listed[1].0, "{next} after {listed:?}");
}

/// `StateTarget::is_empty` answers whether the tables hold a stream a move could land on. A
/// component on PostgreSQL that only ever kept dead letters (each command a binding sent it was
/// refused) holds the dead-letter stream, which a move of a file holding dead letters lands on:
/// its tables are not empty.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv_w31_u8_tables_holding_only_dead_letters_are_not_empty_on_postgres() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = directory.path().join("moved.sqlite");
    {
        let mut file = SqliteStore::open(&path).expect("the file opens");
        file.commit(&task("a", None, "open")).expect("row a");
        file.keep_dead_letter(&letter("in-the-file"))
            .expect("a letter");
    }
    let database = database();
    let component = format!("lettered-{}", letters());
    PostgresStore::open(&database, &component, 1)
        .expect("the tables open")
        .keep_dead_letter(&letter("on-postgres"))
        .expect("a letter kept on PostgreSQL");
    let source = StateSource::open(&path).expect("the file opens");
    source.check().expect("the file can move");
    let target = StateTarget::open(&database, &component, 1).expect("the tables open");
    assert!(
        !target
            .is_empty(&[TASK.to_owned()], &source)
            .expect("the tables are read"),
        "tables holding a dead letter are not empty"
    );
}

/// The ids of the store's dead letters, by id.
fn ids(store: &mut dyn Store) -> Vec<u64> {
    store
        .dead_letters()
        .expect("the letters read")
        .into_iter()
        .map(|(id, _)| id)
        .collect()
}

/// Round 2: two handles on one file. One keeps a letter, the other continues the ids past 41 and
/// keeps one, then the first keeps another: each handle answers the id both list, the ids grow,
/// and a file opened again lists the same ids and keeps past them.
#[test]
fn adv_w31_u8_r2_two_handles_and_a_reopened_file_agree_on_continued_ids() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = directory.path().join("shared.sqlite");
    let mut first = SqliteStore::open(&path).expect("SQLite opens");
    let mut second = SqliteStore::open(&path).expect("SQLite opens beside it");
    let one = first.keep_dead_letter(&letter("one")).expect("a letter");
    second
        .continue_dead_letter_ids(41)
        .expect("the ids continue");
    let two = second.keep_dead_letter(&letter("two")).expect("a letter");
    let three = first.keep_dead_letter(&letter("three")).expect("a letter");
    assert!(one < 41 && two > 41 && three > two, "{one} {two} {three}");
    assert_eq!(ids(&mut first), [one, two, three]);
    assert_eq!(ids(&mut second), [one, two, three]);
    assert!(first.last_dead_letter_id().expect("the last id") >= three);
    assert_eq!(
        second.take_dead_letter(three).expect("the take"),
        Some(letter("three"))
    );
    drop((first, second));
    let mut reopened = SqliteStore::open(&path).expect("SQLite opens again");
    assert_eq!(ids(&mut reopened), [one, two]);
    let four = reopened
        .keep_dead_letter(&letter("four"))
        .expect("a letter");
    assert!(four > three, "{four} after {three}");
    assert_eq!(ids(&mut reopened), [one, two, four]);
}

/// Round 2: a file whose first event continues the ids, as a reset leaves it, reads no event,
/// row or count of that event; its log has an id; reopened, it scans the same.
#[test]
fn adv_w31_u8_r2_the_continue_event_is_no_event_row_or_count() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = directory.path().join("continued.sqlite");
    let mut plain = MemoryStore::default();
    written_alone(&mut plain);
    let expected = plain.scan(TASK).expect("the plain scan");
    {
        let mut store = SqliteStore::open(&path).expect("SQLite opens");
        store.continue_dead_letter_ids(9).expect("the ids continue");
        assert_ne!(store.events_id().expect("the log's id"), 0);
        assert_eq!(store.events_after(0, usize::MAX).expect("the events"), []);
        written_alone(&mut store);
        store
            .commit_events(&[named("After")], &[])
            .expect("an event");
        let read = store.events_after(0, usize::MAX).expect("the events");
        assert_eq!(read.len(), 1, "{read:?}");
        assert_eq!(read[0].event.name, "After");
        assert_eq!(store.scan(TASK).expect("the scan"), expected);
    }
    let mut reopened = SqliteStore::open(&path).expect("SQLite opens again");
    assert_eq!(reopened.scan(TASK).expect("the reopened scan"), expected);
    assert_eq!(
        reopened
            .counts(&[TASK.to_owned()])
            .expect("the reopened counts"),
        BTreeMap::from([(TASK.to_owned(), 2)])
    );
    let id = reopened.keep_dead_letter(&letter("a")).expect("a letter");
    assert!(id > 9, "{id}");
}

/// Round 2: a file a reset left, its ids continued and letters kept past them, moves into
/// PostgreSQL with those ids; tables holding only a continue event are not empty to a move.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv_w31_u8_r2_continued_ids_move_and_hold_their_tables_on_postgres() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = directory.path().join("continued.sqlite");
    let listed = {
        let mut file = SqliteStore::open(&path).expect("the file opens");
        file.continue_dead_letter_ids(30).expect("the ids continue");
        file.commit(&task("a", None, "open")).expect("row a");
        file.keep_dead_letter(&letter("x")).expect("a letter");
        file.keep_dead_letter(&letter("y")).expect("a letter");
        file.dead_letters().expect("the file's letters")
    };
    assert!(listed.iter().all(|(id, _)| *id > 30), "{listed:?}");
    let database = database();
    let component = format!("continued-{}", letters());
    let source = StateSource::open(&path).expect("the file opens");
    let checked = source.check().expect("the file can move");
    let lock = StateLock::take(&database).expect("the schema's lock is taken");
    let target = StateTarget::open(&database, &component, 1).expect("the tables open");
    assert_eq!(target.copy(&source).expect("the file is copied"), checked);
    lock.release().expect("the lock is released");
    let mut moved = PostgresStore::open(&database, &component, 1).expect("the tables open");
    assert_eq!(moved.dead_letters().expect("the moved letters"), listed);
    let next = moved.keep_dead_letter(&letter("z")).expect("a letter");
    assert!(next > listed[1].0, "{next} after {listed:?}");

    let bare = format!("bare-{}", letters());
    PostgresStore::open(&database, &bare, 1)
        .expect("the tables open")
        .continue_dead_letter_ids(41)
        .expect("the ids continue on PostgreSQL");
    let target = StateTarget::open(&database, &bare, 1).expect("the tables open");
    assert!(
        !target
            .is_empty(&[TASK.to_owned()], &source)
            .expect("the tables are read"),
        "tables holding only a continue event are not empty"
    );
}
