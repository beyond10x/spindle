//! Adversary pass 1, wave 19 (story postgres-store): the PostgreSQL store against what the SQLite
//! store answers, on a database other work shares.
//!
//! Ignored without `SPINDLE_TEST_DATABASE_URL` (a loopback PostgreSQL whose role may create
//! schemas and tables, and may connect to the database `postgres` of the same server) and, for
//! the TLS cases, `SPINDLE_TEST_DATABASE_CA`. Every case writes in the schema `adv1_w19`, under
//! component names no earlier run used.

use std::collections::BTreeMap;
use std::sync::{Arc, Barrier, mpsc};
use std::time::{Duration, Instant};

use serde_json::json;
use spindle_kernel::{Change, Database, PostgresStore, Row, Store, StoreError};

const SCHEMA: &str = "adv1_w19";
/// A password no message may print, plain or URL-encoded.
const PASSWORD: &str = "adv@pass:w/rd%x";
const ENCODED: &str = "adv%40pass%3Aw%2Frd%25x";
const ENTITY: &str = "adv.match.Match";

fn url() -> String {
    std::env::var("SPINDLE_TEST_DATABASE_URL")
        .expect("SPINDLE_TEST_DATABASE_URL names a loopback PostgreSQL (task check:postgres)")
}

fn ca() -> std::path::PathBuf {
    std::env::var("SPINDLE_TEST_DATABASE_CA")
        .expect("SPINDLE_TEST_DATABASE_CA names the PEM CA of the test server")
        .into()
}

/// `url` on the database `name` of the same server.
fn on_database(url: &str, name: &str) -> String {
    let (head, _) = url.rsplit_once('/').expect("a URL with a database");
    format!("{head}/{name}")
}

/// `url` with `user` and `password` in place of its own.
fn as_role(url: &str, user: &str, password: &str) -> String {
    let (scheme, rest) = url.split_once("://").expect("a URL with a scheme");
    let (_, location) = rest.split_once('@').expect("a URL with user and password");
    format!("{scheme}://{user}:{password}@{location}")
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

/// Holds a write transaction open on `url` until the returned sender is dropped or sent to.
fn hold_a_write_transaction(url: String) -> (mpsc::Sender<()>, std::thread::JoinHandle<()>) {
    let (opened, ready) = mpsc::channel();
    let (release, released) = mpsc::channel::<()>();
    let holder = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        runtime.block_on(async {
            let (mut client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls)
                .await
                .expect("a connection to the other database");
            let driver = tokio::spawn(connection);
            let transaction = client.transaction().await.expect("a transaction");
            transaction
                .simple_query("SELECT pg_current_xact_id()")
                .await
                .expect("the transaction takes an id");
            opened.send(()).expect("the test waits");
            let _ = tokio::task::spawn_blocking(move || {
                let _ = released.recv_timeout(Duration::from_secs(30));
            })
            .await;
            transaction.rollback().await.expect("the transaction ends");
            drop(client);
            let _ = driver.await;
        });
    });
    ready.recv().expect("the write transaction is open");
    (release, holder)
}

/// A write transaction left open in another database of the same server (another application,
/// a migration, a session idle in transaction) holds no row of Spindle's: a scan of a row
/// Spindle committed after it began still answers that row, as the SQLite store does.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv_a_scan_answers_while_another_database_holds_a_write_transaction_open() {
    let database = database();
    let mut store = open(&database, &fresh("elsewhere"));
    let (release, holder) = hold_a_write_transaction(on_database(&url(), "postgres"));
    store.commit(&created("one")).expect("the match is created");
    let started = Instant::now();
    let scanned = store.scan(ENTITY);
    let took = started.elapsed();
    let _ = release.send(());
    holder.join().expect("the other transaction ends");
    let scan = scanned.unwrap_or_else(|error| {
        panic!("the scan of a committed row fails after {took:?}: {error}")
    });
    assert_eq!(scan.rows.len(), 1, "the committed row is scanned");
    assert!(
        took < Duration::from_secs(2),
        "the scan waited {took:?} on a transaction of another database"
    );
}

/// Two stores on one component's tables (two processes on one database and one prefix, as a
/// rolling deploy runs them): of two creations of one identity at once, exactly one lands and
/// the other is the conflict the contract names, never a backend error.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv_two_stores_on_one_prefix_create_one_identity_exactly_once() {
    let database = database();
    let component = fresh("twice");
    let mut stores = [open(&database, &component), open(&database, &component)];
    let rounds = 20;
    let barrier = Arc::new(Barrier::new(2));
    let outcomes: Vec<Vec<Result<(), StoreError>>> = std::thread::scope(|scope| {
        let handles: Vec<_> = stores
            .iter_mut()
            .map(|store| {
                let barrier = Arc::clone(&barrier);
                scope.spawn(move || {
                    (0..rounds)
                        .map(|round| {
                            barrier.wait();
                            store.commit(&created(&format!("r{round}")))
                        })
                        .collect()
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("a writer"))
            .collect()
    });
    for (round, (first, second)) in outcomes[0].iter().zip(&outcomes[1]).enumerate() {
        let pair = [first, second];
        let landed = pair.iter().filter(|outcome| outcome.is_ok()).count();
        assert_eq!(landed, 1, "round {round}: {pair:?}");
        for outcome in pair {
            if let Err(error) = outcome {
                assert!(
                    matches!(error, StoreError::Conflict { .. }),
                    "round {round}: the loser is a conflict, not {error}"
                );
            }
        }
    }
    let scan = stores[0].scan(ENTITY).expect("the store scans");
    assert_eq!(scan.rows.len(), rounds);
}

/// Two stores on one prefix writing different instances at once: every write lands, and the
/// entity's version counts each of them.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv_two_stores_on_one_prefix_write_different_instances_and_all_land() {
    let database = database();
    let component = fresh("apart");
    let mut stores = [open(&database, &component), open(&database, &component)];
    let rounds = 25;
    let barrier = Arc::new(Barrier::new(2));
    let failures: Vec<String> = std::thread::scope(|scope| {
        let handles: Vec<_> = stores
            .iter_mut()
            .enumerate()
            .map(|(writer, store)| {
                let barrier = Arc::clone(&barrier);
                scope.spawn(move || {
                    (0..rounds)
                        .filter_map(|round| {
                            barrier.wait();
                            store
                                .commit(&created(&format!("w{writer}-{round}")))
                                .err()
                                .map(|error| format!("w{writer}-{round}: {error}"))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("a writer"))
            .collect()
    });
    assert_eq!(failures, Vec::<String>::new());
    let scan = stores[1].scan(ENTITY).expect("the store scans");
    assert_eq!(scan.rows.len(), 2 * rounds);
    assert_eq!(scan.version, u64::try_from(2 * rounds).expect("small"));
}

/// `store migrate` run by two processes at once (two migrate jobs of one release) leaves the
/// tables once, both exiting 0.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv_concurrent_migrations_of_one_component_both_succeed() {
    let database = database();
    let component = fresh("migrated");
    let barrier = Arc::new(Barrier::new(4));
    let outcomes: Vec<Result<(), StoreError>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let barrier = Arc::clone(&barrier);
                let (database, component) = (&database, &component);
                scope.spawn(move || {
                    barrier.wait();
                    PostgresStore::migrate(database, component)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("a migration"))
            .collect()
    });
    for outcome in &outcomes {
        assert!(outcome.is_ok(), "{outcomes:?}");
    }
    let mut store = open(&database, &component);
    store
        .commit(&created("after"))
        .expect("the tables take a write");
}

/// Appends `count` events of another entity to `component`'s log, as its other instances would.
fn noise(component: &str, from: usize, count: usize) {
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
             '{{\"state\":\"open\",\"fields\":{{}}}}'::jsonb FROM generate_series({}, {}) g",
            from + 1,
            from + count
        ),
    );
}

/// A scan reads the component's whole log; it costs in proportion to the log, as the SQLite
/// store's does: twice the log, at most three times the time.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv_a_scan_costs_in_proportion_to_the_log() {
    let database = database();
    let component = fresh("long");
    let mut store = open(&database, &component);
    store.commit(&created("one")).expect("the match is created");
    let size = 40_000;
    noise(&component, 0, size);
    let started = Instant::now();
    assert_eq!(store.scan(ENTITY).expect("the scan").rows.len(), 1);
    let once = started.elapsed();
    noise(&component, size, size);
    let started = Instant::now();
    assert_eq!(store.scan(ENTITY).expect("the scan").rows.len(), 1);
    let twice = started.elapsed();
    eprintln!(
        "scan over {size} events: {once:?}; over {}: {twice:?}",
        2 * size
    );
    assert!(
        twice < once * 3,
        "a scan over {size} events took {once:?}, over {} {twice:?}",
        2 * size
    );
}

fn refusal(database: &Database) -> String {
    match PostgresStore::migrate(database, &fresh("tls")) {
        Ok(()) => panic!("the server was admitted: {database:?}"),
        Err(error) => error.to_string(),
    }
}

/// The server is verified against `database_ca`: a CA bundle that did not issue its
/// certificate is refused, and so is a host name its certificate does not name.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL and SPINDLE_TEST_DATABASE_CA: task check:postgres"]
fn adv_tls_refuses_a_foreign_ca_and_a_host_the_certificate_does_not_name() {
    let mut database = database();
    database.ca = Some(ca());
    PostgresStore::migrate(&database, &fresh("tls")).expect("the right CA is admitted");

    let mut foreign = database.clone();
    foreign.ca = Some("/etc/ssl/certs/ca-certificates.crt".into());
    let _ = refusal(&foreign);

    let (head, tail) = database
        .url
        .split_once("@")
        .expect("a URL with credentials");
    let (_, rest) = tail.split_once(':').expect("host and port");
    let mut named = database.clone();
    named.url = format!("{head}@localhost:{rest}?hostaddr=127.0.0.1");
    PostgresStore::migrate(&named, &fresh("tls")).expect("a host the certificate names");
    let mut misnamed = database.clone();
    misnamed.url = format!("{head}@not-the-server.example:{rest}?hostaddr=127.0.0.1");
    let _ = refusal(&misnamed);
}

/// An operator who gave the wrong CA bundle is told it is the server's certificate that failed,
/// not only that the database is unavailable: a refused certificate names `database_ca`.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL and SPINDLE_TEST_DATABASE_CA: task check:postgres"]
fn adv_a_refused_certificate_names_database_ca() {
    let mut database = database();
    database.ca = Some("/etc/ssl/certs/ca-certificates.crt".into());
    let message = refusal(&database);
    assert!(
        message.contains("database_ca") || message.contains("certificate"),
        "the refusal names the CA bundle or the certificate: {message}"
    );
}

/// No refusal prints the password of `database_url`, plain or URL-encoded.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL and SPINDLE_TEST_DATABASE_CA: task check:postgres"]
fn adv_no_refusal_prints_the_password() {
    let base = url();
    let wrong = as_role(&base, "adv_nobody", ENCODED);
    let mut messages = Vec::new();
    for (url, ca, schema) in [
        (wrong.clone(), None, "public"),
        (wrong.clone(), Some(ca()), "public"),
        (wrong.replace(":55", ":1"), None, "public"),
        (format!("{wrong}?sslmode=bogus"), None, "public"),
        (format!("{wrong}&&="), Some(ca()), "public"),
        (wrong.clone(), None, "Not A Schema"),
        (
            format!("postgres://adv:{PASSWORD}@db.example.invalid:5432/x"),
            None,
            "public",
        ),
    ] {
        let mut database = Database::new(url);
        database.ca = ca;
        database.schema = schema.to_owned();
        database.connections = Some(100);
        if let Err(error) = PostgresStore::open(&database, &fresh("secret"), 1) {
            messages.push(error.to_string());
        }
        if let Err(error) = PostgresStore::migrate(&database, &fresh("secret")) {
            messages.push(error.to_string());
        }
        messages.push(format!("{database:?}"));
    }
    for message in &messages {
        for secret in [PASSWORD, ENCODED, "pass:w", "pass%3A"] {
            assert!(!message.contains(secret), "{secret} printed in: {message}");
        }
    }
}
