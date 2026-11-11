//! `state move` in the kernel (story state-moves-to-postgres): a SQLite file of an earlier
//! release, copied into PostgreSQL, scans there as it scanned in the file, and by the entity's
//! own stream wherever its writes say which instances they left; the check that the tables are
//! empty, and the lock one move holds on its schema.
//!
//! The cases are ignored without `SPINDLE_TEST_DATABASE_URL`, a loopback PostgreSQL URL of a
//! role that may create schemas and tables; `task check:postgres` runs them. Each case moves
//! into a schema of its own.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

use eventlog_core::{
    AppendGroup, AtomicEventStore, CommandMeta, Expected, NewEvent, StreamAppend, StreamId,
    TenantId, new_event_id, request_hash,
};
use eventlog_sqlite::SqliteEventStore;
use serde_json::json;
use spindle_kernel::{
    Change, Database, Moved, PostgresStore, Row, SqliteStore, StateLock, StateSource, StateTarget,
    Store, postgres_prefix,
};

const TASK: &str = "moved.task.Task";
const OTHER: &str = "moved.task.Other";

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

/// Runs `statements` as the test database's owner; answers the first column of the last
/// statement's rows.
fn sql(statements: &[String]) -> Vec<String> {
    let runtime = runtime();
    runtime.block_on(async {
        let (client, connection) = tokio_postgres::connect(&url(), tokio_postgres::NoTls)
            .await
            .expect("the test database accepts its owner");
        let driver = tokio::spawn(connection);
        let mut answered = Vec::new();
        for statement in statements {
            answered = client
                .simple_query(statement)
                .await
                .unwrap_or_else(|error| panic!("{statement}: {error}"))
                .into_iter()
                .filter_map(|message| match message {
                    tokio_postgres::SimpleQueryMessage::Row(row) => row.get(0).map(str::to_owned),
                    _ => None,
                })
                .collect();
        }
        drop(client);
        let _ = driver.await;
        answered
    })
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
}

/// The test database, in a schema of its own.
fn database() -> Database {
    let schema = format!("spindle_kmove_{}", letters());
    sql(&[format!("CREATE SCHEMA {schema}")]);
    let mut database = Database::new(url());
    database.schema = schema;
    database
}

/// The first column of `query` on `component`'s events table in `database`'s schema, as a
/// number.
fn count(database: &Database, component: &str, filter: &str) -> u64 {
    sql(&[format!(
        "SELECT count(*) FROM {}.{}_events WHERE {filter}",
        database.schema,
        postgres_prefix(component)
    )])[0]
        .parse()
        .expect("a count")
}

/// A SQLite file as a release before this one wrote it: its writes appended event by event.
struct Earlier {
    log: SqliteEventStore,
    tenant: TenantId,
    runtime: tokio::runtime::Runtime,
}

/// What one write leaves an instance as: a row in `state`, or removed.
enum Left<'a> {
    Row(&'a str),
    Removed,
}

impl Earlier {
    /// The store `run --state` keeps at `path`, created as the store creates it.
    fn open(path: &Path) -> Self {
        drop(SqliteStore::open(path).expect("the file is created"));
        let runtime = runtime();
        let log = runtime
            .block_on(SqliteEventStore::open(
                path.to_str().expect("a UTF-8 path"),
                "spindle",
            ))
            .expect("the file opens");
        Self {
            log,
            tenant: TenantId::new("local").expect("a tenant"),
            runtime,
        }
    }

    /// One write of instances of [`TASK`], each `(identity, version before, left)`: an event of
    /// the domain and its row, or its removal. `named` appends the `spindle.written` event of
    /// 0.7.0 to 0.10.0, which names no instances (schema 1, `{}`); without it, the write is one
    /// of a release before 0.7.0, which recorded no entity's version.
    fn write(&self, named: bool, instances: &[(&str, u64, Left<'_>)]) {
        let mut appends = Vec::new();
        if named {
            appends.push(StreamAppend {
                stream: StreamId::new(self.tenant.clone(), "spindle:entity", TASK)
                    .expect("a stream"),
                expected: Expected::Any,
                events: vec![NewEvent::new("spindle.written", 1, json!({})).expect("an event")],
            });
        }
        for (identity, before, left) in instances {
            let end = match left {
                Left::Row(state) => NewEvent::new(
                    "spindle.row",
                    1,
                    json!({"state": state, "fields": {"id": identity}}),
                ),
                Left::Removed => NewEvent::new("spindle.removed", 1, json!({})),
            }
            .expect("an event");
            appends.push(StreamAppend {
                stream: StreamId::new(self.tenant.clone(), TASK, json!(identity).to_string())
                    .expect("a stream"),
                expected: match before {
                    0 => Expected::NoStream,
                    version => Expected::Exact(*version),
                },
                events: vec![
                    NewEvent::new("moved.task.Touched", 1, json!({"id": identity}))
                        .expect("an event"),
                    end,
                ],
            });
        }
        let entries: Vec<&Vec<NewEvent>> = appends.iter().map(|append| &append.events).collect();
        let group = AppendGroup {
            tenant: self.tenant.clone(),
            meta: CommandMeta {
                idempotency_key: new_event_id(),
                request_hash: request_hash(&entries).expect("a hash"),
                subject: "earlier".to_owned(),
                actor: "spindle-kernel".to_owned(),
                request_id: new_event_id(),
                trace_id: new_event_id(),
                causation_id: None,
                causation_depth: 0,
                occurred_at: time::OffsetDateTime::now_utc(),
                claim: None,
            },
            appends,
        };
        self.runtime
            .block_on(self.log.append_group(&group))
            .expect("the write is appended");
    }
}

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

/// Moves the file at `path` into `component`'s tables in `database`, as `state move` does: the
/// file checked, the schema's lock taken, the tables found empty, the file copied.
fn moved(path: &Path, database: &Database, component: &str) -> Moved {
    let source = StateSource::open(path).expect("the file opens");
    let checked = source.check().expect("the file can move");
    let lock = StateLock::take(database).expect("the schema's lock is taken");
    let target = StateTarget::open(database, component, 1).expect("the tables open");
    assert!(
        target
            .is_empty(&[TASK.to_owned()], &source)
            .expect("the tables are read"),
        "the tables are empty"
    );
    let moved = target.copy(&source).expect("the file is copied");
    lock.release().expect("the lock is released");
    assert_eq!(moved, checked);
    moved
}

/// Holds `component`'s publication gate shared, as a write in flight holds it, for `held`, from
/// a thread of its own; answers once it is held. A scan that reads the whole log waits on the
/// gate; one that reads the entity's stream does not.
fn gate_held(database: &Database, component: &str, held: Duration) -> std::thread::JoinHandle<()> {
    let gate = serde_json::to_string(&[postgres_prefix(component), "publication".to_owned()])
        .expect("the gate's coordinate");
    let schema = database.schema.clone();
    let (taken, waiting) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || {
        runtime().block_on(async {
            let (mut client, connection) = tokio_postgres::connect(&url(), tokio_postgres::NoTls)
                .await
                .expect("the test database accepts its owner");
            let driver = tokio::spawn(connection);
            client
                .batch_execute(&format!("SET search_path TO {schema}"))
                .await
                .expect("the schema is set");
            let transaction = client.transaction().await.expect("a transaction");
            transaction
                .execute(
                    "SELECT pg_advisory_xact_lock_shared(hashtextextended(current_database() || \
                     ':' || current_schema() || $1, 0))",
                    &[&gate],
                )
                .await
                .expect("the gate is held shared");
            taken.send(()).expect("the case waits");
            tokio::time::sleep(held).await;
            transaction.commit().await.expect("the gate is released");
            drop(client);
            let _ = driver.await;
        });
    });
    waiting.recv().expect("the gate is held");
    holder
}

/// A file 0.7.0 to 0.10.0 wrote, whose `spindle.written` events name no instances (schema 1),
/// moves so that each names the instances its write left (schema 2): the moved log is not read
/// whole, so its scan does not wait on a write in flight, and holds what the file's scan holds.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_file_whose_writes_name_no_instances_moves_to_be_scanned_by_its_stream() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = directory.path().join("moved.sqlite");
    let earlier = Earlier::open(&path);
    earlier.write(
        true,
        &[("a", 0, Left::Row("open")), ("b", 0, Left::Row("open"))],
    );
    earlier.write(true, &[("a", 2, Left::Row("closed"))]);
    earlier.write(
        true,
        &[("b", 2, Left::Removed), ("c", 0, Left::Row("open"))],
    );
    earlier.write(true, &[]);
    earlier.write(true, &[("d", 0, Left::Row("open"))]);
    drop(earlier);
    let before = SqliteStore::open(&path)
        .expect("the file opens")
        .scan(TASK)
        .expect("the file scans");
    assert_eq!(before.version, 5);
    assert_eq!(before.rows.len(), 3);
    let database = database();
    let component = format!("named-{}", letters());
    moved(&path, &database, &component);
    assert_eq!(
        count(
            &database,
            &component,
            "event_name = 'spindle.written' AND (event_schema_version <> 2 OR NOT data ? 'rows')"
        ),
        0,
        "every spindle.written event names its instances"
    );
    assert_eq!(
        count(&database, &component, "stream_type = 'spindle:log'"),
        0,
        "a file whose every write recorded its entity's version is not marked"
    );
    let mut store = PostgresStore::open(&database, &component, 1).expect("the store opens");
    let holder = gate_held(&database, &component, Duration::from_secs(4));
    let started = Instant::now();
    let scan = store.scan(TASK);
    let took = started.elapsed();
    holder.join().expect("the gate is released");
    assert_eq!(scan.expect("the moved log scans"), before);
    assert!(
        took < Duration::from_millis(1500),
        "the scan waited {took:?} on a write in flight: it read the whole log"
    );
}

/// A file holding writes of a release before 0.7.0, which recorded no entity's version, moves
/// with the log marked as holding them (stream `spindle:log` `unnamed-writes`, before the first
/// copied write), so a scan reads the whole log and holds every row the file's scan holds.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_file_holding_writes_before_entity_versions_moves_marked_and_scans_every_row() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = directory.path().join("moved.sqlite");
    let earlier = Earlier::open(&path);
    earlier.write(
        false,
        &[
            ("old-a", 0, Left::Row("open")),
            ("old-b", 0, Left::Row("open")),
        ],
    );
    earlier.write(false, &[("old-a", 2, Left::Row("closed"))]);
    earlier.write(true, &[("mid", 0, Left::Row("open"))]);
    drop(earlier);
    let mut file = SqliteStore::open(&path).expect("the file opens");
    file.commit(&task("new", None, "open"))
        .expect("a write of this release");
    file.commit(&task("old-b", Some(2), "moved"))
        .expect("the older instance moves");
    let before = file.scan(TASK).expect("the file scans");
    drop(file);
    assert_eq!(before.version, 3);
    assert_eq!(before.rows.len(), 4);
    let database = database();
    let component = format!("marked-{}", letters());
    let moved = moved(&path, &database, &component);
    let marks = sql(&[format!(
        "SELECT stream_id || ' ' || version || ' ' || (global_seq = (SELECT min(global_seq) \
         FROM {schema}.{prefix}_events)) FROM {schema}.{prefix}_events WHERE stream_type = \
         'spindle:log'",
        schema = database.schema,
        prefix = postgres_prefix(&component)
    )]);
    assert_eq!(
        marks,
        ["unnamed-writes 1 true"],
        "one mark, appended before the first copied write"
    );
    assert_eq!(
        count(&database, &component, "true"),
        moved.events as u64 + 1,
        "the file's events and the mark"
    );
    let scan = PostgresStore::open(&database, &component, 1)
        .expect("the store opens")
        .scan(TASK)
        .expect("the moved log scans");
    assert_eq!(scan, before);
}

/// The tables are empty only where no entity's stream and no mark is there: an entity the file
/// does not hold counts as well as one it holds, and reading writes nothing, not even the
/// tenant's identity a scan of the whole log records.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn tables_holding_any_entitys_stream_or_a_mark_are_not_empty_and_reading_writes_nothing() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = directory.path().join("moved.sqlite");
    SqliteStore::open(&path)
        .expect("the file opens")
        .commit(&task("a", None, "open"))
        .expect("a row");
    let source = StateSource::open(&path).expect("the file opens");
    source.check().expect("the file can move");
    let database = database();
    let declared = [TASK.to_owned(), OTHER.to_owned()];
    let identities = |component: &str| -> u64 {
        sql(&[format!(
            "SELECT count(*) FROM {}.{}_identity",
            database.schema,
            postgres_prefix(component)
        )])[0]
            .parse()
            .expect("a count")
    };

    let other = format!("other-{}", letters());
    let mut written = PostgresStore::open(&database, &other, 1).expect("the store opens");
    let mut row = task("elsewhere", None, "open");
    row.entity = OTHER.to_owned();
    written.commit(&row).expect("a row of another entity");
    drop(written);
    let target = StateTarget::open(&database, &other, 1).expect("the tables open");
    let held = (count(&database, &other, "true"), identities(&other));
    assert!(
        !target
            .is_empty(&declared, &source)
            .expect("the tables are read"),
        "the stream of an entity the file does not hold is there"
    );
    assert_eq!(
        (count(&database, &other, "true"), identities(&other)),
        held,
        "reading writes nothing"
    );

    let marked = format!("mark-{}", letters());
    let target = StateTarget::open(&database, &marked, 1).expect("the tables open");
    assert!(
        target
            .is_empty(&declared, &source)
            .expect("the tables are read"),
        "fresh tables are empty"
    );
    sql(&[format!(
        "INSERT INTO {}.{}_events (tenant_id, stream_type, stream_id, version, event_id, \
         event_name, event_schema_version, occurred_at, recorded_at, subject, actor, request_id, \
         trace_id, data) VALUES ('local', 'spindle:log', 'unnamed-writes', 1, \
         gen_random_uuid(), 'spindle.unnamed', 1, now(), now(), 'a', 'a', 'm', 'm', '{{}}')",
        database.schema,
        postgres_prefix(&marked)
    )]);
    assert!(
        !target
            .is_empty(&declared, &source)
            .expect("the tables are read"),
        "the mark of a partial move is there"
    );
    assert_eq!(identities(&marked), 0, "reading writes nothing");
}

/// One move holds its schema: a second waits a little, then is refused naming the other move;
/// once the first ends, a move takes it. A move into another schema never waits.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_second_move_into_a_held_schema_is_refused_after_a_short_wait() {
    let database = database();
    let elsewhere = self::database();
    let first = StateLock::take(&database).expect("the first move takes the schema");
    let started = Instant::now();
    let second = StateLock::take(&database);
    let waited = started.elapsed();
    let refused = second.expect_err("a second move into the schema is refused");
    let message = refused.to_string();
    assert!(
        message.contains("another state move") && message.contains(&database.schema),
        "the refusal names the other move and the schema: {message}"
    );
    assert!(
        waited >= Duration::from_secs(1) && waited < Duration::from_secs(30),
        "the second move waited {waited:?}"
    );
    let started = Instant::now();
    let other = StateLock::take(&elsewhere).expect("another schema is not held");
    assert!(started.elapsed() < Duration::from_secs(1));
    other.release().expect("released");
    first.release().expect("released");
    StateLock::take(&database)
        .expect("a move after the first takes the schema")
        .release()
        .expect("released");
}
