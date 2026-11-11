//! Adversary pass 3 of `state move` (story state-moves-to-postgres): a log written as 0.7.0 to
//! 0.10.0 wrote it, with several entities and instances per write, an instance written twice in
//! one write, removals, re-creations, an entity a write was only conditioned on and identities of
//! two JSON kinds, moved into PostgreSQL and scanned there by the entity's stream, answers every
//! scan and every load the SQLite file answers, before and after writes of this release on both.
//! The same for a file whose first writes are of a release before 0.7.0, which arrives marked.
//!
//! The writes of 0.10.0 are rebuilt from its `SqliteStore::commit_all` (tag 0.10.0,
//! `crates/spindle-kernel/src/sqlite.rs`): one append group, first one `spindle.written` event
//! (schema 1, `{}`) per entity written (expected any version) or conditioned on (expected the
//! version the scan answered), then each write's events and its `spindle.row` or
//! `spindle.removed`, a second write of an instance as a second append of its stream. A write
//! before 0.7.0 is one append of one stream, no `spindle.written`.
//!
//! Ignored without `SPINDLE_TEST_DATABASE_URL`; `task check:postgres` runs it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use eventlog_core::{
    AppendGroup, AtomicEventStore, CommandMeta, Expected, NewEvent, StreamAppend, StreamId,
    TenantId, new_event_id, request_hash,
};
use eventlog_sqlite::SqliteEventStore;
use serde_json::{Value, json};
use spindle_kernel::{
    Change, Database, Event, PostgresStore, Removal, Row, SqliteStore, StateLock, StateSource,
    StateTarget, Store, Write, postgres_prefix,
};

const BOOK: &str = "adv.lib.Book";
const SHELF: &str = "adv.lib.Shelf";
const TAG: &str = "adv.lib.Tag";
const ENTITIES: [&str; 3] = [BOOK, SHELF, TAG];

fn url() -> String {
    std::env::var("SPINDLE_TEST_DATABASE_URL")
        .expect("SPINDLE_TEST_DATABASE_URL names a loopback PostgreSQL (task check:postgres)")
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
}

fn sql(statements: &[String]) -> Vec<String> {
    runtime().block_on(async {
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

/// A fixed-seed xorshift: the same log on every run.
struct Dice(u64);

impl Dice {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

/// A schema of its own, dropped when the case ends.
struct Schema(Database);

impl Schema {
    fn new(tag: &str) -> Self {
        let letters: String = uuid::Uuid::new_v4()
            .simple()
            .to_string()
            .bytes()
            .take(10)
            .map(|byte| char::from(b'a' + byte % 26))
            .collect();
        let name = format!("spindle_kadv_{tag}_{letters}");
        sql(&[format!("CREATE SCHEMA {name}")]);
        let mut database = Database::new(url());
        database.schema = name;
        Self(database)
    }
}

impl Drop for Schema {
    fn drop(&mut self) {
        let schema = self.0.schema.clone();
        let _ = std::panic::catch_unwind(|| sql(&[format!("DROP SCHEMA {schema} CASCADE")]));
    }
}

/// One write of an earlier release: the instance's row after it, or its removal.
struct Earlier {
    entity: &'static str,
    identity: Value,
    row: Option<Row>,
    events: usize,
}

/// A SQLite file written as earlier releases wrote it, with what each stream stands at.
struct Old {
    log: SqliteEventStore,
    tenant: TenantId,
    runtime: tokio::runtime::Runtime,
    /// Each instance's stream version and whether its last event is a removal.
    instances: BTreeMap<(String, String), (u64, bool)>,
    entities: BTreeMap<String, u64>,
}

impl Old {
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
            instances: BTreeMap::new(),
            entities: BTreeMap::new(),
        }
    }

    fn append(&self, appends: Vec<StreamAppend>) {
        let entries: Vec<&Vec<NewEvent>> = appends.iter().map(|append| &append.events).collect();
        let group = AppendGroup {
            tenant: self.tenant.clone(),
            meta: CommandMeta {
                idempotency_key: new_event_id(),
                request_hash: request_hash(&entries).expect("a hash"),
                subject: "spindle-kernel".to_owned(),
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
            .expect("the earlier write is appended");
    }

    /// The instance append of `write`, at the version its stream stands at.
    fn instance(&mut self, write: &Earlier) -> StreamAppend {
        let key = (write.entity.to_owned(), write.identity.to_string());
        let (version, _) = self.instances.get(&key).copied().unwrap_or((0, false));
        let mut events: Vec<NewEvent> = (0..write.events)
            .map(|index| {
                NewEvent::new(
                    format!("{}Touched", write.entity),
                    1,
                    json!({"n": index, "id": write.identity}),
                )
                .expect("an event")
            })
            .collect();
        events.push(
            match &write.row {
                Some(row) => NewEvent::new(
                    "spindle.row",
                    1,
                    serde_json::to_value(row).expect("a row as JSON"),
                ),
                None => NewEvent::new("spindle.removed", 1, json!({})),
            }
            .expect("an event"),
        );
        let after = version + events.len() as u64;
        self.instances.insert(key, (after, write.row.is_none()));
        StreamAppend {
            stream: StreamId::new(
                self.tenant.clone(),
                write.entity,
                write.identity.to_string(),
            )
            .expect("a stream"),
            expected: match version {
                0 => Expected::NoStream,
                version => Expected::Exact(version),
            },
            events,
        }
    }

    /// A write of 0.7.0 to 0.10.0 (`SqliteStore::commit_all` of 0.10.0).
    fn named(&mut self, writes: &[Earlier], scanned: &[&'static str]) {
        let mut marks = BTreeMap::<&str, Expected>::new();
        for entity in scanned {
            let version = self.entities.get(*entity).copied().unwrap_or(0);
            marks.insert(
                entity,
                match version {
                    0 => Expected::NoStream,
                    version => Expected::Exact(version),
                },
            );
        }
        for write in writes {
            marks.entry(write.entity).or_insert(Expected::Any);
        }
        let mut appends = Vec::new();
        for (entity, expected) in marks {
            *self.entities.entry(entity.to_owned()).or_default() += 1;
            appends.push(StreamAppend {
                stream: StreamId::new(self.tenant.clone(), "spindle:entity", entity)
                    .expect("a stream"),
                expected,
                events: vec![NewEvent::new("spindle.written", 1, json!({})).expect("an event")],
            });
        }
        for write in writes {
            appends.push(self.instance(write));
        }
        self.append(appends);
    }

    /// A write before 0.7.0: one stream's append, no entity's version.
    fn unnamed(&mut self, write: &Earlier) {
        let append = self.instance(write);
        self.append(vec![append]);
    }
}

fn row(state: &str, dice: &mut Dice, identity: &Value) -> Row {
    let mut fields = BTreeMap::new();
    fields.insert("id".to_owned(), identity.clone());
    fields.insert("n".to_owned(), json!(dice.below(1_000_000)));
    if dice.chance(30) {
        fields.insert(
            "text".to_owned(),
            json!(format!(
                "\u{e4}\"\\\n{}",
                "x".repeat(dice.below(3000) as usize)
            )),
        );
    }
    Row {
        state: state.to_owned(),
        fields,
    }
}

fn identity(entity: &str, index: u64) -> Value {
    if entity == SHELF {
        json!(index)
    } else {
        json!(format!("i-{index}"))
    }
}

/// `groups` writes of 0.10.0 onto `old`, from `dice`: one to three writes each, of a book or a
/// shelf among eight of each; a fifth write the instance a second time in the same group; one in
/// six conditioned on the tags or the books besides.
fn named_history(old: &mut Old, dice: &mut Dice, groups: usize) {
    for _ in 0..groups {
        let mut writes = Vec::new();
        let mut touched = BTreeSet::new();
        // Whether each instance rests after the group's writes so far.
        let mut resting = BTreeMap::<String, bool>::new();
        for _ in 0..=dice.below(3) {
            let entity = if dice.chance(60) { BOOK } else { SHELF };
            let id = identity(entity, dice.below(8));
            if !touched.insert((entity, id.to_string())) {
                continue;
            }
            let twice = dice.chance(20);
            for _ in 0..if twice { 2 } else { 1 } {
                let key = (entity.to_owned(), id.to_string());
                let alive = *resting.entry(format!("{entity} {id}")).or_insert_with(|| {
                    old.instances.get(&key).is_some_and(|(_, removed)| !removed)
                });
                let row = if alive && dice.chance(30) {
                    None
                } else {
                    Some(row(if alive { "Moved" } else { "New" }, dice, &id))
                };
                resting.insert(format!("{entity} {id}"), row.is_some());
                writes.push(Earlier {
                    entity,
                    identity: id.clone(),
                    row,
                    events: dice.below(3) as usize,
                });
            }
        }
        let scanned: Vec<&'static str> = if dice.chance(16) {
            vec![if dice.chance(50) { TAG } else { BOOK }]
        } else {
            Vec::new()
        };
        // A second write of an instance in the group is a second append of its stream, at the
        // version the first left.
        old.named(&writes, &scanned);
    }
}

/// Every scan, and every load of every instance the history touched.
fn answers(store: &mut dyn Store) -> (Vec<String>, Vec<String>) {
    let scans = ENTITIES
        .iter()
        .map(|entity| {
            format!(
                "{entity} {:?}",
                store.scan(entity).expect("the store scans")
            )
        })
        .collect();
    let mut loads = Vec::new();
    for entity in [BOOK, SHELF] {
        for index in 0..10 {
            let id = identity(entity, index);
            loads.push(format!(
                "{entity} {id} {:?}",
                store.load(entity, &id).expect("the store loads")
            ));
        }
    }
    (scans, loads)
}

/// Moves the file at `path` into `component`'s tables, as `state move` does.
fn moved(path: &Path, database: &Database, component: &str) {
    let source = StateSource::open(path).expect("the file opens");
    let checked = source.check().expect("the file can move");
    let lock = StateLock::take(database).expect("the schema's lock is taken");
    let target = StateTarget::open(database, component, 1).expect("the tables open");
    let declared: Vec<String> = ENTITIES.iter().map(|entity| (*entity).to_owned()).collect();
    assert!(
        target
            .is_empty(&declared, &source)
            .expect("the tables are read"),
        "the tables are empty"
    );
    let moved = target.copy(&source).expect("the file is copied");
    lock.release().expect("the lock is released");
    assert_eq!(moved, checked);
}

fn count(database: &Database, component: &str, filter: &str) -> u64 {
    sql(&[format!(
        "SELECT count(*) FROM {}.{}_events WHERE {filter}",
        database.schema,
        postgres_prefix(component)
    )])[0]
        .parse()
        .expect("a count")
}

/// Writes of this release on both stores: a change of a resting book at its version, a creation
/// of a book never written and the removal of a resting shelf, one group; then a change
/// conditioned on the books' scan.
fn continue_on(store: &mut dyn Store, round: u32) {
    let books = store.scan(BOOK).expect("the store scans");
    let shelves = store.scan(SHELF).expect("the store scans");
    let (book, stored) = books.rows.first().cloned().expect("a resting book");
    let (shelf, shelved) = shelves.rows.first().cloned().expect("a resting shelf");
    let mut writes = vec![
        Write::Commit(Change {
            entity: BOOK.to_owned(),
            identity: book,
            expected: Some(stored.version),
            row: Row {
                state: "Continued".to_owned(),
                fields: BTreeMap::new(),
            },
            events: vec![Event {
                name: "adv.lib.Continued".to_owned(),
                payload: serde_json::Map::new(),
            }],
            scanned: Vec::new(),
        }),
        Write::Remove(Removal {
            entity: SHELF.to_owned(),
            identity: shelf,
            expected: shelved.version,
            events: Vec::new(),
            scanned: Vec::new(),
        }),
    ];
    writes.push(Write::Commit(Change {
        entity: BOOK.to_owned(),
        identity: json!(format!("never-before-{round}")),
        expected: None,
        row: Row {
            state: "New".to_owned(),
            fields: BTreeMap::new(),
        },
        events: Vec::new(),
        scanned: Vec::new(),
    }));
    store.commit_all(&writes).expect("the writes land");
    let books = store.scan(BOOK).expect("the store scans");
    store
        .commit(&Change {
            entity: BOOK.to_owned(),
            identity: json!(format!("on-scan-{round}")),
            expected: None,
            row: Row {
                state: "New".to_owned(),
                fields: BTreeMap::new(),
            },
            events: Vec::new(),
            scanned: vec![spindle_kernel::Scanned {
                entity: BOOK.to_owned(),
                version: books.version,
            }],
        })
        .expect("the conditioned write lands");
}

/// A log of 0.7.0 to 0.10.0 moved into PostgreSQL arrives unmarked, every `spindle.written`
/// naming its instances, and answers every scan and load the file answers, before and after
/// writes of this release on both.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_moved_log_of_0_10_answers_every_scan_and_load_the_file_answers() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = directory.path().join("named.sqlite");
    let mut old = Old::open(&path);
    let mut dice = Dice(0x5350_4d56_0019_0003);
    named_history(&mut old, &mut dice, 600);
    drop(old);
    let mut file = SqliteStore::open(&path).expect("the file opens");
    let before = answers(&mut file);
    let schema = Schema::new("named");
    let component = "adv-named";
    moved(&path, &schema.0, component);
    assert_eq!(
        count(&schema.0, component, "stream_type = 'spindle:log'"),
        0,
        "a log of 0.7.0 to 0.10.0 is not marked"
    );
    assert_eq!(
        count(
            &schema.0,
            component,
            "event_name = 'spindle.written' AND event_schema_version <> 2"
        ),
        0,
        "every spindle.written names its instances"
    );
    let mut moved = PostgresStore::open(&schema.0, component, 1).expect("the store opens");
    let after = answers(&mut moved);
    assert_eq!(after.0, before.0, "every scan answers as the file's");
    assert_eq!(after.1, before.1, "every load answers as the file's");
    continue_on(&mut file, 1);
    continue_on(&mut moved, 1);
    assert_eq!(
        answers(&mut moved),
        answers(&mut file),
        "after writes of this release on both"
    );
}

/// A file whose first writes are of a release before 0.7.0, then of 0.10.0, then of this
/// release, arrives marked and answers every scan and load the file answers.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_moved_log_from_before_0_7_answers_every_scan_and_load_the_file_answers() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = directory.path().join("unnamed.sqlite");
    let mut old = Old::open(&path);
    let mut dice = Dice(0x5350_4d56_0019_0004);
    for _ in 0..60 {
        let entity = if dice.chance(60) { BOOK } else { SHELF };
        let id = identity(entity, dice.below(8));
        let key = (entity.to_owned(), id.to_string());
        let alive = old.instances.get(&key).is_some_and(|(_, removed)| !removed);
        let row = if alive && dice.chance(30) {
            None
        } else {
            Some(row("Old", &mut dice, &id))
        };
        old.unnamed(&Earlier {
            entity,
            identity: id,
            row,
            events: dice.below(2) as usize,
        });
    }
    named_history(&mut old, &mut dice, 200);
    drop(old);
    let mut file = SqliteStore::open(&path).expect("the file opens");
    continue_on(&mut file, 0);
    let before = answers(&mut file);
    let schema = Schema::new("unnamed");
    let component = "adv-unnamed";
    moved(&path, &schema.0, component);
    assert_eq!(
        count(&schema.0, component, "stream_type = 'spindle:log'"),
        1,
        "the log is marked"
    );
    let mut moved = PostgresStore::open(&schema.0, component, 1).expect("the store opens");
    let after = answers(&mut moved);
    assert_eq!(after.0, before.0, "every scan answers as the file's");
    assert_eq!(after.1, before.1, "every load answers as the file's");
    continue_on(&mut file, 1);
    continue_on(&mut moved, 1);
    assert_eq!(answers(&mut moved), answers(&mut file));
}
