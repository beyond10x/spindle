//! Adversary pass 2, wave 19 (story postgres-store): the PostgreSQL scan through eventlog's
//! consistent tenant capture, against what the SQLite store answers at the same log sizes, with
//! a second process on the same tables, and the refusals that name a database.
//!
//! Ignored without `SPINDLE_TEST_DATABASE_URL` (a loopback PostgreSQL whose role may create
//! schemas, tables and roles) and, for the TLS cases, `SPINDLE_TEST_DATABASE_CA`. Every case
//! writes in the schema `adv2_w19`, under component names no earlier run used. The long-log
//! cases are meant for a release build (`cargo test --release`).

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

use serde_json::json;
use spindle_kernel::{Change, Database, PostgresStore, Row, SqliteStore, Store, Write};

const SCHEMA: &str = "adv2_w19";
const ENTITY: &str = "adv.match.Match";
/// The log sizes a scan is measured at, in events.
const SIZES: [usize; 3] = [100_000, 200_000, 500_000];

fn url() -> String {
    std::env::var("SPINDLE_TEST_DATABASE_URL")
        .expect("SPINDLE_TEST_DATABASE_URL names a loopback PostgreSQL (task check:postgres)")
}

fn ca() -> std::path::PathBuf {
    std::env::var("SPINDLE_TEST_DATABASE_CA")
        .expect("SPINDLE_TEST_DATABASE_CA names the PEM CA of the test server")
        .into()
}

fn sql(url: &str, statement: &str) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    runtime.block_on(async {
        let (client, connection) = tokio_postgres::connect(url, tokio_postgres::NoTls)
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
    sql(&url(), &format!("CREATE SCHEMA IF NOT EXISTS {SCHEMA}"));
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

fn created(identity: &str) -> Change {
    Change {
        entity: ENTITY.to_owned(),
        identity: json!(identity),
        expected: None,
        row: Row {
            state: "open".to_owned(),
            fields: BTreeMap::from([("id".to_owned(), json!(identity))]),
        },
        events: Vec::new(),
        scanned: Vec::new(),
    }
}

/// Appends events `from + 1 ..= to` of another entity to `component`'s log, as its other
/// instances would.
fn noise(component: &str, from: usize, to: usize) {
    let table = format!(
        "{SCHEMA}.{}_events",
        spindle_kernel::postgres_prefix(component)
    );
    sql(
        &url(),
        &format!(
            "INSERT INTO {table} (tenant_id, stream_type, stream_id, version, event_id, \
             event_name, event_schema_version, occurred_at, recorded_at, subject, actor, \
             request_id, trace_id, data) SELECT 'local', 'adv.noise.Noise', 'n' || g, 1, \
             gen_random_uuid(), 'spindle.row', 1, now(), now(), 'a', 'a', 'a', 'a', \
             '{{\"state\":\"open\",\"fields\":{{\"id\":\"noise\"}}}}'::jsonb \
             FROM generate_series({}, {to}) g; ANALYZE {table}",
            from + 1,
        ),
    );
}

/// The same events `from + 1 ..= to` in the SQLite store's file at `path`.
fn sqlite_noise(path: &std::path::Path, from: usize, to: usize) {
    let statement = format!(
        "WITH RECURSIVE g(n) AS (SELECT {} UNION ALL SELECT n + 1 FROM g WHERE n < {to}) \
         INSERT INTO spindle_events (tenant_id, stream_type, stream_id, version, event_id, \
         event_name, event_schema_version, occurred_at, recorded_at, subject, actor, \
         request_id, trace_id, data) SELECT e.tenant_id, 'adv.noise.Noise', 'n' || g.n, 1, \
         printf('%08x-0000-4000-8000-%012x', g.n, g.n), 'spindle.row', 1, e.occurred_at, \
         e.recorded_at, e.subject, e.actor, e.request_id, e.trace_id, \
         '{{\"state\":\"open\",\"fields\":{{\"id\":\"noise\"}}}}' FROM g, \
         (SELECT * FROM spindle_events WHERE event_name = 'spindle.row' LIMIT 1) e;",
        from + 1
    );
    let output = std::process::Command::new("sqlite3")
        .arg(path)
        .arg(&statement)
        .output()
        .expect("sqlite3 runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The peak resident set of this process, in MiB.
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

/// Story: "The store passes the kernel's store contract exactly as the SQLite store does." A
/// SQLite store scans a log of 500,000 events (`adv2_sqlite_scans_the_same_long_logs`); the
/// PostgreSQL store scans the same logs, rather than fail once the log outgrows the time one
/// store transaction is allowed.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv2_a_scan_answers_over_a_long_log_as_sqlite_does() {
    let database = database();
    let component = fresh("long");
    let mut store = open(&database, &component);
    store.commit(&created("one")).expect("the match is created");
    let mut failures = Vec::new();
    let mut from = 0;
    for size in SIZES {
        noise(&component, from, size);
        from = size;
        let started = Instant::now();
        let scanned = store.scan(ENTITY);
        let took = started.elapsed();
        eprintln!(
            "postgres scan over {size} events: {took:?}, {} rows, peak {} MiB",
            scanned.as_ref().map_or(0, |scan| scan.rows.len()),
            peak_mib()
        );
        match scanned {
            Ok(scan) => assert_eq!(scan.rows.len(), 1, "{size} events"),
            Err(error) => failures.push(format!("{size} events, after {took:?}: {error}")),
        }
    }
    assert!(
        failures.is_empty(),
        "the scans SQLite answers fail on PostgreSQL: {failures:#?}"
    );
}

/// A log of a million events: the SQLite store scans it page by page; the PostgreSQL store's scan
/// is one store transaction, which eventlog bounds at ten seconds. The scan answers, as
/// SQLite's does, rather than fail every list view, reset and set-deciding command from then on.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv2_a_scan_answers_over_a_million_events() {
    let database = database();
    let component = fresh("million");
    let mut store = open(&database, &component);
    store.commit(&created("one")).expect("the match is created");
    noise(&component, 0, 500_000);
    noise(&component, 500_000, 1_000_000);
    let started = Instant::now();
    let scanned = store.scan(ENTITY);
    let took = started.elapsed();
    eprintln!(
        "postgres scan over 1000000 events: {took:?}, peak {} MiB, {:?}",
        peak_mib(),
        scanned.as_ref().map(|scan| scan.rows.len())
    );
    let scan = scanned.unwrap_or_else(|error| panic!("the scan fails after {took:?}: {error}"));
    assert_eq!(scan.rows.len(), 1);
    let started = Instant::now();
    store.commit(&created("two")).unwrap_or_else(|error| {
        panic!("a commit after the scan, {:?}: {error}", started.elapsed())
    });
}

/// A scan holds what it reads one page at a time, as SQLite's does (18 MiB peak at 500,000
/// events, 2 MiB more than at 100,000): the memory a scan takes does not grow with the log, so
/// a container's memory limit is not a limit on how long the log may grow.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv2_a_scans_memory_does_not_grow_with_the_log() {
    let database = database();
    let component = fresh("memory");
    let mut store = open(&database, &component);
    store.commit(&created("one")).expect("the match is created");
    noise(&component, 0, 100_000);
    store.scan(ENTITY).expect("the scan over 100,000 events");
    let small = peak_mib();
    noise(&component, 100_000, 500_000);
    store.scan(ENTITY).expect("the scan over 500,000 events");
    let large = peak_mib();
    eprintln!("peak after a scan of 100,000 events {small} MiB, of 500,000 {large} MiB");
    assert!(
        large.saturating_sub(small) < 64,
        "the scan of 400,000 more events took {} MiB more",
        large.saturating_sub(small)
    );
}

/// What the SQLite store answers at the same log sizes, measured.
#[test]
#[ignore = "measures logs of up to 500,000 events, with the sqlite3 command: run it by name"]
fn adv2_sqlite_scans_the_same_long_logs() {
    if std::process::Command::new("sqlite3")
        .arg("-version")
        .output()
        .is_err()
    {
        eprintln!("skipped: no sqlite3");
        return;
    }
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = directory.path().join("long.sqlite");
    {
        let mut store = SqliteStore::open(&path).expect("the store opens");
        store.commit(&created("one")).expect("the match is created");
    }
    let mut from = 0;
    for size in SIZES {
        sqlite_noise(&path, from, size);
        from = size;
        let mut store = SqliteStore::open(&path).expect("the store opens");
        let started = Instant::now();
        let scan = store.scan(ENTITY).expect("the SQLite store scans");
        eprintln!(
            "sqlite scan over {size} events: {:?}, peak {} MiB",
            started.elapsed(),
            peak_mib()
        );
        assert_eq!(scan.rows.len(), 1, "{size} events");
    }
}

/// A log long enough that one scan takes `at_least`, at most `cap` events: its size and one
/// scan's time.
fn grown(store: &mut PostgresStore, component: &str, at_least: Duration, cap: usize) -> usize {
    let mut size = 0;
    for next in [50_000, 100_000, 150_000, 200_000, 300_000, 400_000, 500_000] {
        if next > cap {
            break;
        }
        noise(component, size, next);
        size = next;
        let started = Instant::now();
        let scanned = store.scan(ENTITY);
        let took = started.elapsed();
        eprintln!("grown to {size} events: one scan {took:?}");
        if scanned.is_ok() && took >= at_least {
            return size;
        }
    }
    size
}

/// Two processes on one component's tables (a rolling deploy, or two replicas), one scanning
/// back to back as a metrics scrape does, the other taking commands: every command lands, as it
/// does on SQLite, where a reader never fails a writer.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv2_an_append_from_another_process_lands_during_a_long_scan() {
    let database = database();
    let component = fresh("scraped");
    let mut scanner = open(&database, &component);
    let mut writer = open(&database, &component);
    scanner
        .commit(&created("first"))
        .expect("the match is created");
    let size = grown(&mut scanner, &component, Duration::from_secs(3), 500_000);
    let stop = Arc::new(AtomicBool::new(false));
    let (scans, writes) = std::thread::scope(|scope| {
        let stopping = Arc::clone(&stop);
        let scanning = scope.spawn(move || {
            let mut scans = Vec::new();
            for _ in 0..3 {
                let started = Instant::now();
                let outcome = scanner.scan(ENTITY).map(|scan| scan.rows.len());
                scans.push((
                    started.elapsed(),
                    outcome.map_err(|error| error.to_string()),
                ));
            }
            stopping.store(true, Ordering::SeqCst);
            scans
        });
        let writing = scope.spawn(move || {
            let mut writes = Vec::new();
            let mut round = 0;
            std::thread::sleep(Duration::from_millis(200));
            while !stop.load(Ordering::SeqCst) {
                let started = Instant::now();
                let outcome = writer.commit(&created(&format!("w{round}")));
                writes.push((
                    started.elapsed(),
                    outcome.map_err(|error| error.to_string()),
                ));
                round += 1;
                std::thread::sleep(Duration::from_millis(100));
            }
            writes
        });
        (
            scanning.join().expect("the scanner"),
            writing.join().expect("the writer"),
        )
    });
    let slowest = writes
        .iter()
        .map(|(took, _)| *took)
        .max()
        .unwrap_or_default();
    let failed: Vec<_> = writes
        .iter()
        .filter_map(|(took, outcome)| outcome.as_ref().err().map(|error| (took, error)))
        .collect();
    eprintln!(
        "log {size} events; scans {scans:?}; {} commits, {} failed, slowest {slowest:?}",
        writes.len(),
        failed.len()
    );
    assert!(
        failed.is_empty(),
        "commits of another process fail while a scan runs: {failed:#?}"
    );
}

/// Two processes on one component's tables scan at once (two replicas scraped together): both
/// answer.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv2_two_processes_scan_one_long_log_at_once() {
    let database = database();
    let component = fresh("twoscans");
    let mut first = open(&database, &component);
    let mut second = open(&database, &component);
    first.commit(&created("one")).expect("the match is created");
    let size = grown(&mut first, &component, Duration::from_secs(3), 500_000);
    let barrier = Arc::new(Barrier::new(2));
    let outcomes: Vec<(Duration, Result<usize, String>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = [&mut first, &mut second]
            .into_iter()
            .map(|store| {
                let barrier = Arc::clone(&barrier);
                scope.spawn(move || {
                    barrier.wait();
                    let started = Instant::now();
                    let outcome = store
                        .scan(ENTITY)
                        .map(|scan| scan.rows.len())
                        .map_err(|error| error.to_string());
                    (started.elapsed(), outcome)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("a scanner"))
            .collect()
    });
    eprintln!("log {size} events; two scans at once: {outcomes:?}");
    for (took, outcome) in &outcomes {
        assert_eq!(outcome, &Ok(1), "a scan after {took:?}");
    }
}

/// A scan while another process appends groups of three creations: every scan holds whole
/// groups, and its version counts exactly the groups whose rows it holds.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv2_a_scan_during_grouped_writes_holds_whole_groups_at_their_version() {
    let database = database();
    let component = fresh("grouped");
    let mut scanner = open(&database, &component);
    let mut writer = open(&database, &component);
    noise(&component, 0, 2_000);
    let rounds = 150;
    let stop = Arc::new(AtomicBool::new(false));
    let (observed, written) = std::thread::scope(|scope| {
        let stopping = Arc::clone(&stop);
        let writing = scope.spawn(move || {
            let mut failed = Vec::new();
            for round in 0..rounds {
                let writes: Vec<Write> = (0..3)
                    .map(|member| Write::Commit(created(&format!("g{round}-{member}"))))
                    .collect();
                if let Err(error) = writer.commit_all(&writes) {
                    failed.push(format!("{round}: {error}"));
                }
            }
            stopping.store(true, Ordering::SeqCst);
            failed
        });
        let scanning = scope.spawn(move || {
            let mut observed = Vec::new();
            while !stop.load(Ordering::SeqCst) {
                observed.push(
                    scanner
                        .scan(ENTITY)
                        .map(|scan| (scan.rows.len(), scan.version))
                        .map_err(|error| error.to_string()),
                );
            }
            observed.push(
                scanner
                    .scan(ENTITY)
                    .map(|scan| (scan.rows.len(), scan.version))
                    .map_err(|error| error.to_string()),
            );
            observed
        });
        (
            scanning.join().expect("the scanner"),
            writing.join().expect("the writer"),
        )
    });
    assert_eq!(written, Vec::<String>::new());
    let mid: Vec<_> = observed
        .iter()
        .filter_map(|outcome| outcome.as_ref().ok())
        .filter(|(rows, _)| *rows > 0 && *rows < 3 * rounds)
        .collect();
    eprintln!("{} scans, {} mid-run", observed.len(), mid.len());
    let mut last = 0;
    for outcome in &observed {
        let (rows, version) = outcome.as_ref().expect("every scan answers");
        assert_eq!(
            *rows as u64,
            3 * version,
            "rows {rows} at version {version}"
        );
        assert!(
            *version >= last,
            "version went back from {last} to {version}"
        );
        last = *version;
    }
    assert_eq!(last, rounds as u64);
}

/// The first scan of a log records its stream identity; two processes scanning a new log for
/// the first time at once both answer.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv2_two_first_scans_at_once_both_answer() {
    let database = database();
    for round in 0..10 {
        let component = fresh("firstscan");
        let mut stores = [open(&database, &component), open(&database, &component)];
        stores[0]
            .commit(&created("one"))
            .expect("the match is created");
        let barrier = Arc::new(Barrier::new(2));
        let outcomes: Vec<Result<usize, String>> = std::thread::scope(|scope| {
            let handles: Vec<_> = stores
                .iter_mut()
                .map(|store| {
                    let barrier = Arc::clone(&barrier);
                    scope.spawn(move || {
                        barrier.wait();
                        store
                            .scan(ENTITY)
                            .map(|scan| scan.rows.len())
                            .map_err(|error| error.to_string())
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().expect("a scanner"))
                .collect()
        });
        assert_eq!(outcomes, vec![Ok(1), Ok(1)], "round {round}");
    }
}

/// A serving role with exactly the rights the documentation lists scans a log it never scanned:
/// the first scan's stream identity is written with them. A role the open admits without INSERT
/// on the identity table would fail its first scan, so the open must refuse it.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL and SPINDLE_TEST_DATABASE_CA: task check:postgres"]
fn adv2_a_serving_role_scans_a_log_it_never_scanned() {
    let owner = url();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_nanos();
    let schema = format!("adv2_w19_role_{stamp}");
    sql(&owner, &format!("CREATE SCHEMA {schema}"));
    let mut hosted = Database::new(owner.clone());
    hosted.schema = schema.clone();
    hosted.ca = Some(ca());
    hosted.connections = Some(20);
    let mut outcomes = Vec::new();
    for (role, revoked) in [
        (format!("adv2_full_{stamp}"), false),
        (format!("adv2_noid_{stamp}"), true),
    ] {
        let component = fresh("served");
        PostgresStore::migrate(&hosted, &component).expect("the owner migrates");
        let prefix = spindle_kernel::postgres_prefix(&component);
        let revoke = revoked.then(|| format!("REVOKE INSERT ON {schema}.{prefix}_identity FROM "));
        sql(
            &owner,
            &format!(
                "CREATE ROLE {role} LOGIN PASSWORD 'adv2' CONNECTION LIMIT 4; GRANT USAGE ON \
                 SCHEMA {schema} TO {role}; GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES \
                 IN SCHEMA {schema} TO {role}; GRANT USAGE ON ALL SEQUENCES IN SCHEMA {schema} \
                 TO {role}; GRANT TEMPORARY ON DATABASE spindle TO {role}"
            ),
        );
        if let Some(revoke) = &revoke {
            sql(&owner, &format!("{revoke}{role}"));
        }
        let (scheme, rest) = owner.split_once("://").expect("a URL");
        let (_, location) = rest.split_once('@').expect("credentials");
        let mut serving = hosted.clone();
        serving.url = format!("{scheme}://{role}:adv2@{location}");
        let outcome = match PostgresStore::open(&serving, &component, 1) {
            Err(error) => Err(format!("open refused: {error}")),
            Ok(mut store) => {
                let committed = store.commit(&created("one"));
                let scanned = store.scan(ENTITY).map(|scan| scan.rows.len());
                Ok((
                    committed.map_err(|error| error.to_string()),
                    scanned.map_err(|error| error.to_string()),
                ))
            }
        };
        outcomes.push((revoke.is_some(), outcome));
    }
    for (role, _) in [
        (format!("adv2_full_{stamp}"), ()),
        (format!("adv2_noid_{stamp}"), ()),
    ] {
        sql(&owner, &format!("DROP OWNED BY {role}; DROP ROLE {role}"));
    }
    sql(&owner, &format!("DROP SCHEMA {schema} CASCADE"));
    eprintln!("{outcomes:#?}");
    let (_, full) = &outcomes[0];
    assert_eq!(
        full,
        &Ok((Ok(()), Ok(1))),
        "the documented serving role writes and scans"
    );
    let (_, without) = &outcomes[1];
    if let Ok((_, scanned)) = without {
        assert_eq!(
            scanned,
            &Ok(1),
            "a role the open admitted cannot run its first scan"
        );
    }
}

/// `store migrate` run again while a store serves the same tables (a release's migrate job
/// beside the running deployment): every command and scan of the serving store answers, none
/// waits for the migration's 60 s.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv2_a_migrate_beside_a_serving_store_delays_nothing() {
    let database = database();
    let component = fresh("remigrated");
    PostgresStore::migrate(&database, &component).expect("the first migrate");
    let mut store = open(&database, &component);
    let stop = Arc::new(AtomicBool::new(false));
    let (migrations, served) = std::thread::scope(|scope| {
        let stopping = Arc::clone(&stop);
        let (database, component) = (&database, &component);
        let migrating = scope.spawn(move || {
            let outcomes: Vec<_> = (0..5)
                .map(|_| PostgresStore::migrate(database, component).map_err(|e| e.to_string()))
                .collect();
            stopping.store(true, Ordering::SeqCst);
            outcomes
        });
        let serving = scope.spawn(move || {
            let mut served = Vec::new();
            let mut round = 0;
            while !stop.load(Ordering::SeqCst) {
                let started = Instant::now();
                let outcome = store
                    .commit(&created(&format!("m{round}")))
                    .and_then(|()| store.scan(ENTITY).map(|_| ()));
                served.push((started.elapsed(), outcome.map_err(|e| e.to_string())));
                round += 1;
            }
            served
        });
        (
            migrating.join().expect("the migrations"),
            serving.join().expect("the store"),
        )
    });
    let slowest = served
        .iter()
        .map(|(took, _)| *took)
        .max()
        .unwrap_or_default();
    eprintln!(
        "{} served, slowest {slowest:?}; migrations {migrations:?}",
        served.len()
    );
    assert!(migrations.iter().all(Result::is_ok), "{migrations:?}");
    let failed: Vec<_> = served
        .iter()
        .filter(|(_, outcome)| outcome.is_err())
        .collect();
    assert!(failed.is_empty(), "{failed:#?}");
    assert!(
        slowest < Duration::from_secs(2),
        "a command waited {slowest:?}"
    );
}

fn refusals(database: &Database) -> Vec<String> {
    let mut messages = Vec::new();
    if let Err(error) = PostgresStore::open(database, &fresh("secret"), 1) {
        messages.push(error.to_string());
    }
    if let Err(error) = PostgresStore::migrate(database, &fresh("secret")) {
        messages.push(error.to_string());
    }
    messages
}

/// A password holding `@` that the operator did not percent-encode: no refusal prints any part
/// of it, with or without `database_ca`.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL and SPINDLE_TEST_DATABASE_CA: task check:postgres"]
fn adv2_no_refusal_prints_a_password_holding_an_unencoded_at() {
    let mut printed = Vec::new();
    for (url, ca) in [
        (
            "postgres://adv:sec@retpart@db.example.invalid:5432/x",
            Some(ca()),
        ),
        ("postgres://adv:sec@retpart@db.example.invalid:5432/x", None),
        ("postgres://adv:sec@retpart@127.0.0.1:1/x", None),
        ("postgres://adv:sec@retpart@127.0.0.1:1/x", Some(ca())),
        ("postgres://adv@127.0.0.1:1/x?password=retpart", Some(ca())),
        ("postgres://adv@127.0.0.1:1/x?password=retpart", None),
    ] {
        let mut database = Database::new(url.to_owned());
        database.ca = ca;
        database.connections = Some(100);
        for message in refusals(&database) {
            if message.contains("retpart") {
                printed.push(message);
            }
        }
    }
    assert!(
        printed.is_empty(),
        "a part of the password is printed: {printed:#?}"
    );
}

/// A `database_url` that is no PostgreSQL URL is refused naming `database_url`.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv2_a_url_that_is_none_is_refused_naming_database_url() {
    let mut unnamed = Vec::new();
    for url in [
        "",
        "postgres://",
        "http://example.invalid/x",
        "not a url at all",
    ] {
        for ca in [None, Some(ca())] {
            let mut database = Database::new(url.to_owned());
            database.ca = ca.clone();
            database.connections = Some(100);
            for message in refusals(&database) {
                if !message.contains("database_url") {
                    unnamed.push(format!("{url:?} (ca {}): {message}", ca.is_some()));
                }
            }
        }
    }
    assert!(
        unnamed.is_empty(),
        "refusals not naming database_url: {unnamed:#?}"
    );
}
