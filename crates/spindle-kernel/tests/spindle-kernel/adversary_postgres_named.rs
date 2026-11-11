//! Adversary pass 3, wave 19 (story postgres-store): the PostgreSQL scan that reads the entity's
//! own stream and one `spindle.row` per instance its schema-2 `spindle.written` events name,
//! against the SQLite store on the same command sequences, beside concurrent writes, with
//! identities that need escaping, and at many instances.
//!
//! Ignored without `SPINDLE_TEST_DATABASE_URL` (a loopback PostgreSQL whose role may create
//! schemas and tables), except the `database_url` cases, which connect nowhere they can reach.
//! Every case writes in the schema `adv3_w19`, under component names no earlier run used. The
//! measuring cases are meant for a release build (`cargo test --release`), one at a time.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use spindle_kernel::{
    Change, Database, Event, PostgresStore, Removal, Row, Scan, Scanned, SqliteStore, Store, Write,
};

const SCHEMA: &str = "adv3_w19";
const ALPHA: &str = "adv3.a.Alpha";
const BETA: &str = "adv3.b.Beta";

fn url() -> String {
    std::env::var("SPINDLE_TEST_DATABASE_URL")
        .expect("SPINDLE_TEST_DATABASE_URL names a loopback PostgreSQL (task check:postgres)")
}

fn sql(statement: &str) {
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
            .batch_execute(statement)
            .await
            .unwrap_or_else(|error| panic!("{statement}: {error}"));
        drop(client);
        let _ = driver.await;
    });
}

fn database() -> Database {
    sql(&format!("CREATE SCHEMA IF NOT EXISTS {SCHEMA}"));
    let mut database = Database::new(url());
    database.schema = SCHEMA.to_owned();
    database
}

fn fresh(stem: &str) -> String {
    let letters: String = uuid::Uuid::new_v4()
        .simple()
        .to_string()
        .bytes()
        .take(12)
        .map(|byte| char::from(b'a' + byte % 26))
        .collect();
    format!("{stem}-{letters}")
}

fn open(database: &Database, component: &str) -> PostgresStore {
    PostgresStore::open(database, component, 1)
        .unwrap_or_else(|error| panic!("the store of {component} opens: {error}"))
}

fn table(component: &str) -> String {
    format!(
        "{SCHEMA}.{}_events",
        spindle_kernel::postgres_prefix(component)
    )
}

fn sqlite() -> (tempfile::TempDir, SqliteStore) {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let store = SqliteStore::open(&directory.path().join("adv3.sqlite")).expect("SQLite opens");
    (directory, store)
}

/// A fixed-seed xorshift: the sequences are the same on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
}

/// Identities whose stream id, a JSON text, needs escaping inside the JSON object a schema-2
/// `spindle.written` event is, or is no string at all.
fn identities() -> Vec<Value> {
    vec![
        json!("plain"),
        json!("quo\"te"),
        json!("back\\slash"),
        json!("new\nline\ttab\r"),
        json!("\u{1}\u{1f}\u{8}\u{c}"),
        json!(" spaced  out "),
        json!("a:b/c?d#e@f%40"),
        json!("{\"rows\":{}}"),
        json!(""),
        json!(7),
        json!(-0.5),
        json!(12_345_678_901_234_567_890_u64),
        json!(1e300),
        json!({"k": "v", "n": [1, 2]}),
        json!(["x", 1, null]),
        json!(true),
        json!("x".repeat(300)),
    ]
}

fn row(state: &str, step: usize, identity: &Value) -> Row {
    Row {
        state: state.to_owned(),
        fields: BTreeMap::from([
            ("step".to_owned(), json!(step)),
            ("id".to_owned(), identity.clone()),
        ]),
    }
}

fn events(count: u64) -> Vec<Event> {
    (0..count)
        .map(|index| Event {
            name: "adv3.Happened".to_owned(),
            payload: Map::from_iter([("index".to_owned(), json!(index))]),
        })
        .collect()
}

/// Where an instance's stream stands: its version and whether its row rests.
#[derive(Clone, Copy, Default)]
struct Standing {
    version: u64,
    resting: bool,
}

/// A group of one to four writes over `ALPHA` and `BETA`, drawn from `rng` against `model`:
/// creations, updates, removals, re-creations of removed identities, an instance written more
/// than once in the group, now and then a stale version or a condition on a scan, valid or
/// stale. Returns the writes and the standing they leave, should they land.
fn group(
    rng: &mut Rng,
    step: usize,
    pool: &[Value],
    model: &BTreeMap<(String, String), Standing>,
    versions: &BTreeMap<String, u64>,
) -> (Vec<Write>, BTreeMap<(String, String), Standing>) {
    let mut after = BTreeMap::<(String, String), Standing>::new();
    let mut writes = Vec::new();
    for _ in 0..=rng.below(4) {
        let entity = if rng.below(3) == 0 { BETA } else { ALPHA };
        let identity = pool[rng.below(pool.len() as u64) as usize].clone();
        let key = (entity.to_owned(), identity.to_string());
        let standing = after
            .get(&key)
            .or_else(|| model.get(&key))
            .copied()
            .unwrap_or_default();
        let added = rng.below(3);
        let landed = standing.version + added + 1;
        if standing.resting {
            let mut expected = standing.version;
            if rng.below(25) == 0 {
                expected = if expected > 1 {
                    expected - 1
                } else {
                    expected + 1
                };
            }
            if rng.below(3) == 0 {
                writes.push(Write::Remove(Removal {
                    entity: entity.to_owned(),
                    identity: identity.clone(),
                    expected,
                    events: events(added),
                }));
                after.insert(
                    key,
                    Standing {
                        version: landed,
                        resting: false,
                    },
                );
            } else {
                writes.push(Write::Commit(Change {
                    entity: entity.to_owned(),
                    identity: identity.clone(),
                    expected: Some(expected),
                    row: row(
                        ["open", "won", "lost"][rng.below(3) as usize],
                        step,
                        &identity,
                    ),
                    events: events(added),
                    scanned: Vec::new(),
                }));
                after.insert(
                    key,
                    Standing {
                        version: landed,
                        resting: true,
                    },
                );
            }
        } else {
            writes.push(Write::Commit(Change {
                entity: entity.to_owned(),
                identity: identity.clone(),
                expected: None,
                row: row("open", step, &identity),
                events: events(added),
                scanned: Vec::new(),
            }));
            after.insert(
                key,
                Standing {
                    version: landed,
                    resting: true,
                },
            );
        }
    }
    if rng.below(5) == 0
        && let Some(Write::Commit(change)) = writes
            .iter_mut()
            .find(|write| matches!(write, Write::Commit(_)))
    {
        let entity = if rng.below(2) == 0 { ALPHA } else { BETA };
        let mut version = versions.get(entity).copied().unwrap_or(0);
        if rng.below(4) == 0 {
            version += 1;
        }
        change.scanned = vec![Scanned {
            entity: entity.to_owned(),
            version,
        }];
    }
    (writes, after)
}

/// What a store answers for `ALPHA` and `BETA`: each scan, or its error.
fn scans(store: &mut dyn Store) -> Vec<Result<Scan, String>> {
    [ALPHA, BETA]
        .iter()
        .map(|entity| store.scan(entity).map_err(|error| error.to_string()))
        .collect()
}

/// Story: "The store passes the kernel's store contract exactly as the SQLite store does." 2,400
/// groups drawn at a fixed seed, each committed to a SQLite store and to a PostgreSQL store:
/// both land or both are refused, and after each, both scan `ALPHA` and `BETA` alike (rows,
/// versions, the entity's version), as does a PostgreSQL store opened anew every 200 groups.
/// `ALPHA`'s stream outgrows the 1,000 events one page of its read holds.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv3_drawn_command_sequences_scan_as_sqlite_does() {
    let database = database();
    let component = fresh("drawn");
    let mut postgres = open(&database, &component);
    let (_directory, mut reference) = sqlite();
    let pool = identities();
    let mut rng = Rng(0x5eed_0019_ad73_0003);
    let mut model = BTreeMap::<(String, String), Standing>::new();
    let mut versions = BTreeMap::<String, u64>::new();
    let mut diverged = Vec::new();
    let mut landed = 0;
    for step in 0..2_400 {
        let (writes, after) = group(&mut rng, step, &pool, &model, &versions);
        let on_sqlite = reference.commit_all(&writes);
        let on_postgres = postgres.commit_all(&writes);
        if on_sqlite.is_ok() != on_postgres.is_ok() {
            diverged.push(format!(
                "step {step}: SQLite {on_sqlite:?}, PostgreSQL {on_postgres:?}, writes {writes:?}"
            ));
            break;
        }
        if on_sqlite.is_ok() {
            landed += 1;
            model.extend(after);
        }
        let expected = scans(&mut reference);
        for (index, scan) in expected.iter().enumerate() {
            let scan = scan.as_ref().expect("SQLite scans");
            versions.insert([ALPHA, BETA][index].to_owned(), scan.version);
        }
        let answered = scans(&mut postgres);
        if answered != expected {
            diverged.push(format!(
                "step {step}: PostgreSQL {answered:?}\nSQLite {expected:?}"
            ));
            break;
        }
        if step % 200 == 199 {
            let reopened = scans(&mut open(&database, &component));
            if reopened != expected {
                diverged.push(format!("step {step}, reopened: {reopened:?}"));
                break;
            }
        }
    }
    eprintln!(
        "{landed} of 2400 groups landed; ALPHA at version {:?}",
        versions.get(ALPHA)
    );
    assert!(diverged.is_empty(), "{}", diverged.join("\n"));
    assert!(landed > 1_500, "only {landed} groups landed");
    assert!(versions[ALPHA] > 1_000, "ALPHA's stream stays in one page");
}

/// A scan while another process commits a fixed script of groups (creations, updates,
/// removals, re-creations, instances of both entities in one group, one instance twice in a
/// group): every scan of `ALPHA` answers exactly what SQLite answered, after the same script, at
/// the version the scan names.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv3_a_scan_beside_removals_and_recreations_holds_the_state_at_its_version() {
    let pool: Vec<Value> = identities().into_iter().take(9).collect();
    let mut rng = Rng(0x0dd5_eed0_0019_0331);
    let (_directory, mut reference) = sqlite();
    let mut model = BTreeMap::<(String, String), Standing>::new();
    let mut versions = BTreeMap::<String, u64>::new();
    let mut script = Vec::new();
    let mut at = BTreeMap::<u64, Scan>::new();
    at.insert(0, reference.scan(ALPHA).expect("scans"));
    for step in 0..400 {
        let (writes, after) = group(&mut rng, step, &pool, &model, &versions);
        if reference.commit_all(&writes).is_err() {
            continue;
        }
        model.extend(after);
        for entity in [ALPHA, BETA] {
            let scan = reference.scan(entity).expect("scans");
            versions.insert(entity.to_owned(), scan.version);
            if entity == ALPHA {
                at.insert(scan.version, scan);
            }
        }
        script.push(writes);
    }
    let database = database();
    let component = fresh("beside");
    let mut writer = open(&database, &component);
    let mut scanner = open(&database, &component);
    let stop = Arc::new(AtomicBool::new(false));
    let (written, observed) = std::thread::scope(|scope| {
        let stopping = Arc::clone(&stop);
        let script = &script;
        let writing = scope.spawn(move || {
            let mut failed = Vec::new();
            for (index, writes) in script.iter().enumerate() {
                if let Err(error) = writer.commit_all(writes) {
                    failed.push(format!("group {index}: {error}"));
                }
            }
            stopping.store(true, Ordering::SeqCst);
            failed
        });
        let scanning = scope.spawn(move || {
            let mut observed = Vec::new();
            loop {
                let done = stop.load(Ordering::SeqCst);
                observed.push(scanner.scan(ALPHA).map_err(|error| error.to_string()));
                if done {
                    break;
                }
            }
            observed
        });
        (
            writing.join().expect("the writer"),
            scanning.join().expect("the scanner"),
        )
    });
    assert_eq!(written, Vec::<String>::new());
    let mut wrong = Vec::new();
    let mut versions_seen = std::collections::BTreeSet::new();
    for outcome in &observed {
        match outcome {
            Err(error) => wrong.push(format!("a scan failed: {error}")),
            Ok(scan) => {
                versions_seen.insert(scan.version);
                match at.get(&scan.version) {
                    Some(expected) if expected == scan => {}
                    Some(expected) => wrong.push(format!(
                        "at version {}: {:?}\nSQLite: {:?}",
                        scan.version, scan.rows, expected.rows
                    )),
                    None => wrong.push(format!("version {} SQLite never had", scan.version)),
                }
            }
        }
    }
    eprintln!(
        "{} groups, {} scans at {} distinct versions",
        script.len(),
        observed.len(),
        versions_seen.len()
    );
    assert!(
        wrong.is_empty(),
        "{}",
        wrong[..wrong.len().min(3)].join("\n")
    );
    assert!(versions_seen.len() > 20, "too few scans landed mid-run");
}

/// One write of 4,000 instances whose identities are 200 characters with quotes and
/// backslashes (a `spindle.written` event of about a megabyte), and one group writing one
/// instance 40 times with events: both stores scan them alike, every instance at its version.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv3_one_write_of_many_instances_and_one_instance_written_many_times() {
    let database = database();
    let component = fresh("wide");
    let mut postgres = open(&database, &component);
    let (_directory, mut reference) = sqlite();
    let identities: Vec<Value> = (0..4_000)
        .map(|index| json!(format!("{index:05}\"\\{}", "q\\\"".repeat(40))))
        .collect();
    let wide: Vec<Write> = identities
        .iter()
        .map(|identity| {
            Write::Commit(Change {
                entity: ALPHA.to_owned(),
                identity: identity.clone(),
                expected: None,
                row: row("open", 0, identity),
                events: Vec::new(),
                scanned: Vec::new(),
            })
        })
        .collect();
    for store in [&mut postgres as &mut dyn Store, &mut reference] {
        store.commit_all(&wide).expect("one wide write lands");
    }
    let mut version = 0;
    let mut again = Vec::new();
    for index in 0..40_u64 {
        let identity = json!("again");
        again.push(Write::Commit(Change {
            entity: ALPHA.to_owned(),
            identity: identity.clone(),
            expected: (index > 0).then_some(version),
            row: row("open", index as usize, &identity),
            events: events(index % 3),
            scanned: Vec::new(),
        }));
        version += index % 3 + 1;
    }
    for store in [&mut postgres as &mut dyn Store, &mut reference] {
        store
            .commit_all(&again)
            .expect("one instance written 40 times lands");
    }
    let on_postgres = postgres.scan(ALPHA).expect("PostgreSQL scans");
    let on_sqlite = reference.scan(ALPHA).expect("SQLite scans");
    assert_eq!(on_postgres.version, 2);
    assert_eq!(on_postgres.rows.len(), 4_001);
    let last = on_postgres
        .rows
        .iter()
        .find(|(identity, _)| identity == &json!("again"))
        .expect("again rests");
    assert_eq!(last.1.version, version);
    assert_eq!(last.1.row.fields["step"], json!(39));
    assert!(on_postgres == on_sqlite, "the stores scan the same");
}

/// A command whose set outcome writes every instance of an entity commits them as one group
/// (`execute/set.rs`, no bound on its members). One group of 1,000, 5,000 and 20,000 writes
/// lands on SQLite; it lands on PostgreSQL too, rather than outlast the 10 s eventlog allows
/// one store transaction.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv3_one_group_of_many_writes_lands_as_on_sqlite() {
    let database = database();
    let mut refused = Vec::new();
    for count in [1_000, 5_000, 20_000] {
        let component = fresh("group");
        let mut postgres = open(&database, &component);
        let (_directory, mut reference) = sqlite();
        let writes: Vec<Write> = (0..count)
            .map(|index| {
                let identity = json!(format!("m{index}"));
                Write::Commit(Change {
                    entity: ALPHA.to_owned(),
                    identity: identity.clone(),
                    expected: None,
                    row: row("open", 0, &identity),
                    events: Vec::new(),
                    scanned: Vec::new(),
                })
            })
            .collect();
        let started = Instant::now();
        let on_sqlite = reference
            .commit_all(&writes)
            .map_err(|error| error.to_string());
        let local = started.elapsed();
        let started = Instant::now();
        let on_postgres = postgres
            .commit_all(&writes)
            .map_err(|error| error.to_string());
        let remote = started.elapsed();
        eprintln!(
            "{count} writes: SQLite {local:?} {on_sqlite:?}, PostgreSQL {remote:?} {on_postgres:?}"
        );
        assert_eq!(on_sqlite, Ok(()));
        if on_postgres.is_err() {
            refused.push(format!("{count}: {on_postgres:?} after {remote:?}"));
        }
    }
    assert!(refused.is_empty(), "{refused:#?}");
}

/// Appends `count` creations of `entity` in `component`'s log as one write each would, each
/// write's schema-2 `spindle.written` naming its instance: a log of `2 x count` events.
fn created_by_sql(component: &str, entity: &str, count: usize) {
    let table = table(component);
    let columns = "tenant_id, stream_type, stream_id, version, event_id, event_name, \
                   event_schema_version, occurred_at, recorded_at, subject, actor, request_id, \
                   trace_id, data";
    sql(&format!(
        "INSERT INTO {table} ({columns}) SELECT 'local', '{entity}', '\"i' || g || '\"', 1, \
         gen_random_uuid(), 'spindle.row', 1, now(), now(), 'a', 'a', 'a', 'a', \
         jsonb_build_object('state', 'open', 'fields', jsonb_build_object('id', 'i' || g)) \
         FROM generate_series(1, {count}) g ORDER BY g; \
         INSERT INTO {table} ({columns}) SELECT 'local', 'spindle:entity', '{entity}', g, \
         gen_random_uuid(), 'spindle.written', 2, now(), now(), 'a', 'a', 'a', 'a', \
         jsonb_build_object('rows', jsonb_build_object('\"i' || g || '\"', 1), 'removed', \
         '{{}}'::jsonb) FROM generate_series(1, {count}) g ORDER BY g; ANALYZE {table}"
    ));
}

/// Appends events `from + 1 ..= to` of another entity to `component`'s log.
fn noise(component: &str, from: usize, to: usize) {
    let table = table(component);
    sql(&format!(
        "INSERT INTO {table} (tenant_id, stream_type, stream_id, version, event_id, \
         event_name, event_schema_version, occurred_at, recorded_at, subject, actor, \
         request_id, trace_id, data) SELECT 'local', 'adv3.noise.Noise', 'n' || g, 1, \
         gen_random_uuid(), 'spindle.row', 1, now(), now(), 'a', 'a', 'a', 'a', \
         '{{\"state\":\"open\",\"fields\":{{\"id\":\"noise\"}}}}'::jsonb \
         FROM generate_series({}, {to}) g; ANALYZE {table}",
        from + 1,
    ));
}

/// The mark `state move` is to write before it copies writes that name no instances.
fn mark(component: &str) {
    sql(&format!(
        "INSERT INTO {} (tenant_id, stream_type, stream_id, version, event_id, event_name, \
         event_schema_version, occurred_at, recorded_at, subject, actor, request_id, trace_id, \
         data) VALUES ('local', 'spindle:log', 'unnamed-writes', 1, gen_random_uuid(), \
         'spindle.unnamed', 1, now(), now(), 'a', 'a', 'mark', 'mark', '{{}}')",
        table(component)
    ));
}

fn timed(store: &mut dyn Store, entity: &str) -> (Duration, Scan) {
    let started = Instant::now();
    let scan = store.scan(entity).expect("scans");
    (started.elapsed(), scan)
}

/// What a scan of 1,000, 10,000 and 50,000 live instances costs, one write per instance: on
/// PostgreSQL by the entity's stream (this build), on the same log read whole (the scan before
/// fix pass 2, reached through the mark), and on SQLite. The entity's stream should not be
/// slower than reading the whole log when the log is nothing but that entity: then it reads
/// every event the whole-log read does, and more statements.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv3_a_scan_of_many_instances_costs_no_more_than_reading_the_log() {
    let database = database();
    let mut slower = Vec::new();
    for count in [1_000, 10_000, 50_000] {
        let component = fresh("many");
        let mut store = open(&database, &component);
        created_by_sql(&component, ALPHA, count);
        let (named, by_stream) = timed(&mut store, ALPHA);
        assert_eq!(by_stream.rows.len(), count);
        assert_eq!(by_stream.version, count as u64);
        let (_, warm) = timed(&mut store, ALPHA);
        assert_eq!(warm, by_stream);
        mark(&component);
        let (whole, by_log) = timed(&mut store, ALPHA);
        assert!(by_log == by_stream, "both reads answer the same");
        let (_directory, mut reference) = sqlite();
        let writes: Vec<Write> = by_stream
            .rows
            .iter()
            .map(|(identity, stored)| {
                Write::Commit(Change {
                    entity: ALPHA.to_owned(),
                    identity: identity.clone(),
                    expected: None,
                    row: stored.row.clone(),
                    events: Vec::new(),
                    scanned: Vec::new(),
                })
            })
            .collect();
        for chunk in writes.chunks(1) {
            reference.commit_all(chunk).expect("SQLite takes it");
        }
        let (local, on_sqlite) = timed(&mut reference, ALPHA);
        assert_eq!(on_sqlite.rows.len(), count);
        eprintln!(
            "{count} instances: PostgreSQL by stream {named:?} ({:.3} ms per row), whole log \
             {whole:?}, SQLite {local:?}",
            named.as_secs_f64() * 1000.0 / count as f64
        );
        if named > whole * 2 {
            slower.push(format!("{count}: by stream {named:?}, whole log {whole:?}"));
        }
    }
    assert!(
        slower.is_empty(),
        "the scan by the entity's stream is slower than reading the whole log: {slower:#?}"
    );
}

/// A log moved from a state directory 0.10.0 wrote: `state move` copies each event as it is,
/// so the entity's stream keeps a schema-1 `spindle.written` naming no instances, for good.
/// Every later scan of that entity then reads the whole log under eventlog's publication lock,
/// as before fix pass 2: once the log has grown to 400,000 events, a scan takes seconds, and
/// another process's commits on the same tables during it wait on that lock.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv3_an_entity_moved_from_0_10_0_scans_without_reading_the_whole_log() {
    let database = database();
    let component = fresh("moved");
    let mut store = open(&database, &component);
    let identity = json!("from-0.10.0");
    store
        .commit(&Change {
            entity: ALPHA.to_owned(),
            identity: identity.clone(),
            expected: None,
            row: row("open", 0, &identity),
            events: Vec::new(),
            scanned: Vec::new(),
        })
        .expect("written");
    sql(&format!(
        "UPDATE {} SET data = '{{}}'::jsonb, event_schema_version = 1 \
         WHERE event_name = 'spindle.written'",
        table(&component)
    ));
    for index in 0..50 {
        let identity = json!(format!("since-{index}"));
        store
            .commit(&Change {
                entity: ALPHA.to_owned(),
                identity: identity.clone(),
                expected: None,
                row: row("open", index, &identity),
                events: Vec::new(),
                scanned: Vec::new(),
            })
            .expect("written since the move");
    }
    noise(&component, 0, 400_000);
    let started = Instant::now();
    let scanned = store
        .scan(ALPHA)
        .map(|scan| scan.rows.len())
        .map_err(|error| error.to_string());
    let elapsed = started.elapsed();
    let mut scanner = open(&database, &component);
    let mut other = open(&database, &component);
    let stop = Arc::new(AtomicBool::new(false));
    let failed = std::thread::scope(|scope| {
        let stopping = Arc::clone(&stop);
        scope.spawn(move || {
            while !stopping.load(Ordering::SeqCst) {
                let _ = scanner.scan(ALPHA);
            }
        });
        std::thread::sleep(Duration::from_millis(500));
        let mut failed = Vec::new();
        for index in 0..12 {
            let identity = json!(format!("other-{index}"));
            let started = Instant::now();
            if let Err(error) = other.commit(&Change {
                entity: BETA.to_owned(),
                identity: identity.clone(),
                expected: None,
                row: row("open", index, &identity),
                events: Vec::new(),
                scanned: Vec::new(),
            }) {
                failed.push(format!("{index} after {:?}: {error}", started.elapsed()));
            }
        }
        stop.store(true, Ordering::SeqCst);
        failed
    });
    eprintln!("scan of the moved entity over 400,051 events: {elapsed:?}, {scanned:?}");
    assert!(
        scanned == Ok(51) && failed.is_empty() && elapsed < Duration::from_secs(1),
        "scan {scanned:?} after {elapsed:?}; commits of another process failed: {failed:#?}"
    );
}

/// Resets this process's peak resident set (Linux `clear_refs` 5).
fn reset_peak() {
    std::fs::write("/proc/self/clear_refs", "5").expect("the peak resets");
}

/// This process's peak resident set since the last reset, in MiB.
fn peak_mib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with("VmHWM:"))
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|kib| kib.parse::<u64>().ok())
        })
        .map_or(0, |kib| kib / 1024)
}

/// What a scan holds, small enough to compare: each identity, version, state, the text's length
/// and its first characters.
fn digest(scan: &Scan) -> Vec<(Value, u64, String, usize, String)> {
    scan.rows
        .iter()
        .map(|(identity, stored)| {
            let text = stored.row.fields["text"].as_str().unwrap_or_default();
            (
                identity.clone(),
                stored.version,
                stored.row.state.clone(),
                text.len(),
                text.chars().take(24).collect(),
            )
        })
        .collect()
}

/// A downstream deployment holds about 1,300 live instances of one entity whose rows carry
/// texts of up to 1 MiB, written by 0.8.0 and 0.10.0 on SQLite and to be moved into PostgreSQL.
/// 1,300 instances, each row a 1 MiB text: a scan on PostgreSQL by the entity's stream, the
/// same log as a move from 0.10.0 leaves it (schema-1 `spindle.written`, read whole), and
/// SQLite, each timed with this process's peak memory during the scan. All three answer the
/// same rows.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL with about 4 GB free"]
fn adv3_a_scan_of_1300_rows_of_a_mebibyte_each() {
    const COUNT: usize = 1_300;
    const TEXT: usize = 1 << 20;
    let database = database();
    let component = fresh("mebibyte");
    let mut postgres = open(&database, &component);
    let (_directory, mut reference) = sqlite();
    let compressible = std::env::var_os("ADV3_COMPRESSIBLE").is_some();
    let mut letters = Rng(0x1e77_e125_0019_0003);
    let started = Instant::now();
    for index in 0..COUNT {
        let identity = json!(format!("doc-{index:04}"));
        // Words of random letters, which PostgreSQL's TOAST compression cannot shrink as it
        // shrinks a repeated phrase (ADV3_COMPRESSIBLE=1 writes that instead).
        let mut text = format!("{index:04}:");
        while text.len() < TEXT {
            if compressible {
                text.push_str("lorem ipsum dolor sit amet, ");
            } else {
                for _ in 0..=letters.below(9) {
                    text.push(char::from(b'a' + letters.below(26) as u8));
                }
                text.push(' ');
            }
        }
        text.truncate(TEXT);
        let change = Change {
            entity: ALPHA.to_owned(),
            identity: identity.clone(),
            expected: None,
            row: Row {
                state: "open".to_owned(),
                fields: BTreeMap::from([
                    ("id".to_owned(), identity.clone()),
                    ("text".to_owned(), json!(text)),
                ]),
            },
            events: Vec::new(),
            scanned: Vec::new(),
        };
        postgres.commit(&change).expect("PostgreSQL takes the row");
        reference.commit(&change).expect("SQLite takes the row");
    }
    eprintln!("written in {:?}", started.elapsed());
    let mut measured = Vec::new();
    let mut digests = Vec::new();
    let mut failed = Vec::new();
    let mut measure = |what: &str, store: &mut dyn Store| {
        reset_peak();
        let before = peak_mib();
        let started = Instant::now();
        let outcome = store.scan(ALPHA);
        let elapsed = started.elapsed();
        let peak = peak_mib();
        match outcome {
            Ok(scan) => {
                measured.push(format!(
                    "{what}: {elapsed:?}, peak {peak} MiB (from {before} MiB), {} rows",
                    scan.rows.len()
                ));
                digests.push((what.to_owned(), digest(&scan)));
            }
            Err(error) => {
                measured.push(format!("{what}: failed after {elapsed:?}, peak {peak} MiB"));
                failed.push(format!("{what}: {error}"));
            }
        }
    };
    measure("PostgreSQL by the entity's stream", &mut postgres);
    measure("SQLite", &mut reference);
    sql(&format!(
        "UPDATE {} SET data = '{{}}'::jsonb, event_schema_version = 1 \
         WHERE event_name = 'spindle.written'",
        table(&component)
    ));
    measure("PostgreSQL moved from 0.10.0 (whole log)", &mut postgres);
    eprintln!("{}", measured.join("\n"));
    assert!(failed.is_empty(), "{failed:#?}");
    let (_, expected) = &digests[1];
    assert_eq!(expected.len(), COUNT);
    for (what, digest) in &digests {
        assert!(digest == expected, "{what} answers other rows than SQLite");
    }
}

/// Every `@` placement, IPv6 and Unix-socket hosts, `?host=` and percent-encoded forms: no
/// refusal, with or without `database_ca`, prints a part of the password.
#[test]
fn adv3_no_refusal_prints_a_part_of_the_password() {
    let mut cas = vec![None, Some(std::path::PathBuf::from("absent-ca.pem"))];
    if let Some(ca) = std::env::var_os("SPINDLE_TEST_DATABASE_CA") {
        cas.push(Some(ca.into()));
    }
    let mut printed = Vec::new();
    for url in [
        "postgres://adv:pa/Zq7SECRET@db.example.invalid:5432/x",
        "postgres://adv:pa/Zq7SE@CRET@db.example.invalid:5432/x",
        "postgres://adv:pa?Zq7SE@CRET@db.example.invalid:5432/x",
        "postgres://adv:pa#Zq7SE@CRET@db.example.invalid:5432/x",
        "postgres://adv:pa/Zq7SE@CRET@127.0.0.1:1/x",
        "postgres://adv:pa?Zq7SE@CRET@127.0.0.1:1/x",
        "postgres://adv:Zq7SE@CRET@[::1]:1/x",
        "postgres://adv:pw@[Zq7SE@CRET::1]:1/x",
        "postgres://adv:Zq7SE@CRET@/x?host=/run/postgresql",
        "postgres://adv:pw@/x?host=Zq7SE@CRET.invalid",
        "postgres://adv:pw@/x?host=/run/Zq7SE@CRET",
        "postgres://adv:pw@%2Frun%2FZq7SE%40CRET/x",
        "postgres://adv:pw@db%40Zq7SECRET.invalid/x",
        "postgres://adv:Zq7SE%40CRET@db.example.invalid/x",
        "postgres://adv:Zq7SE%40CRET@127.0.0.1:1/x",
        "postgres://adv:pw@db.example.invalid,Zq7SE@CRET.invalid/x",
        "postgresql://adv:Zq7SE@CRET@127.0.0.1:1/x?sslmode=disable",
        "postgres://127.0.0.1:1/x?user=adv&password=Zq7SE@CRET",
        "postgres:///x?host=127.0.0.1&port=1&user=adv&password=Zq7SE@CRET",
        "postgres://db.example.invalid/x?user=adv&password=Zq7SE@CRET",
        "postgres://adv@127.0.0.1:1/x?password=Zq7SE@CRET",
        "host=db.example.invalid user=adv password=Zq7SE@CRET",
        "host=Zq7SE@CRET.invalid user=adv password=pw",
        "host=127.0.0.1 port=1 user=adv password=Zq7SECRET",
    ] {
        for ca in cas.clone() {
            let mut database = Database::new(url);
            database.ca = ca.clone();
            database.connections = Some(100);
            let mut messages = Vec::new();
            if let Err(error) = PostgresStore::open(&database, "secret", 1) {
                messages.push(error.to_string());
            }
            if let Err(error) = PostgresStore::migrate(&database, "secret") {
                messages.push(error.to_string());
            }
            for message in messages {
                if message.contains("Zq7") || message.contains("CRET") {
                    printed.push(format!("{url} (ca {}): {message}", ca.is_some()));
                }
            }
        }
    }
    assert!(
        printed.is_empty(),
        "a part of the password is printed: {printed:#?}"
    );
}
