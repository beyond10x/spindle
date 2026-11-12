//! The PostgreSQL store (story postgres-store): the store contract, and components of one
//! database kept apart by their table prefix.
//!
//! The cases that reach a database are ignored without `SPINDLE_TEST_DATABASE_URL`, a loopback
//! PostgreSQL database whose role may create tables; `task check:postgres` runs them. Each case
//! writes under component names no earlier run used, so its tables start empty.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use spindle_kernel::{
    Change, Database, PostgresStore, Removal, Row, Scan, Store, Stored, Write, contract,
    postgres_prefix,
};

const NEEDS: &str = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres";

fn database() -> Database {
    let url = std::env::var("SPINDLE_TEST_DATABASE_URL")
        .expect("SPINDLE_TEST_DATABASE_URL names a loopback PostgreSQL (task check:postgres)");
    Database::new(url)
}

/// `stem` and letters no earlier run drew: a component name whose tables do not exist yet.
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

fn created(side: &str) -> Change {
    Change {
        entity: "apart.match.Match".to_owned(),
        identity: json!("one"),
        expected: None,
        row: Row {
            state: "open".to_owned(),
            fields: BTreeMap::from([("side".to_owned(), json!(side))]),
        },
        events: Vec::new(),
        scanned: Vec::new(),
    }
}

fn side(stored: &Stored) -> &Value {
    &stored.row.fields["side"]
}

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn the_postgres_store_passes_the_contract() {
    let _ = NEEDS;
    let database = database();
    assert!(!contract::CASES.is_empty());
    for case in contract::CASES {
        let mut store = open(&database, &fresh("contract"));
        (case.run)(&mut store);
    }
}

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn two_components_share_a_database_apart() {
    let database = database();
    let (west, east) = (fresh("west"), fresh("east"));
    let mut first = open(&database, &west);
    let mut second = open(&database, &east);
    first
        .commit(&created("west"))
        .expect("west creates the match");
    second
        .commit(&created("east"))
        .expect("east creates a match of the same identity: it holds none");
    for (store, expected) in [(&mut first, "west"), (&mut second, "east")] {
        let loaded = store
            .load("apart.match.Match", &json!("one"))
            .expect("the store reads")
            .expect("the match is held");
        assert_eq!(side(&loaded), &json!(expected));
        let scan = store.scan("apart.match.Match").expect("the store scans");
        assert_eq!(scan.rows.len(), 1, "{expected} holds its own match only");
        assert_eq!(side(&scan.rows[0].1), &json!(expected));
    }
    drop(first);
    let mut reopened = open(&database, &west);
    let loaded = reopened
        .load("apart.match.Match", &json!("one"))
        .expect("the store reads")
        .expect("the match outlives the handle");
    assert_eq!(side(&loaded), &json!("west"));
}

/// A write transaction older than a commit, still open on the same database, does not hide the
/// commit from a scan: the scan right after it holds the row its version counts, without waiting.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_scan_holds_a_commit_while_an_older_transaction_is_open() {
    let database = database();
    let mut store = open(&database, &fresh("lagging"));
    let holder = older_transaction(Duration::from_millis(1500));
    store
        .commit(&created("west"))
        .expect("the match is created");
    let started = std::time::Instant::now();
    let scan = store.scan("apart.match.Match").expect("the store scans");
    let took = started.elapsed();
    assert_eq!(
        scan.rows.len(),
        1,
        "the scan holds the row its version counts"
    );
    assert_eq!(scan.version, 1, "the version counts the one write");
    assert!(
        took < std::time::Duration::from_millis(1000),
        "the scan waited {took:?} for the older transaction"
    );
    holder.join().expect("the older transaction ends");
}

/// Opens a write transaction on the test database from a connection of its own, and holds it
/// open for `held` once it took a transaction id; answers once it has, and the thread that ends
/// it.
fn older_transaction(held: Duration) -> std::thread::JoinHandle<()> {
    let url = database().url;
    let (opened, older) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        runtime.block_on(async {
            let (mut client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls)
                .await
                .expect("a second connection");
            let driver = tokio::spawn(connection);
            let transaction = client.transaction().await.expect("a transaction");
            transaction
                .simple_query("SELECT pg_current_xact_id()")
                .await
                .expect("the transaction takes an id");
            opened.send(()).expect("the test waits");
            tokio::time::sleep(held).await;
            transaction.commit().await.expect("the transaction ends");
            drop(client);
            let _ = driver.await;
        });
    });
    older.recv().expect("the older transaction is open");
    holder
}

/// A write transaction older than a commit, still open anywhere on the server, does not hide the
/// commit's events from the store that committed them: eventlog's feed withholds them until that
/// transaction ends, and the store's read of them, and of its head, waits for it. The read of
/// its reading handle too.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_writer_reads_its_own_events_while_an_older_transaction_is_open() {
    let database = database();
    let mut store = open(&database, &fresh("own"));
    let reading = store
        .reading()
        .expect("a PostgreSQL store reads beside its writer");
    let holder = older_transaction(Duration::from_millis(1500));
    let mut change = created("west");
    change.events = vec![spindle_kernel::Event {
        name: "apart.match.Opened".to_owned(),
        payload: serde_json::Map::new(),
    }];
    store.commit(&change).expect("the match is created");
    let names = |committed: Vec<spindle_kernel::Committed>| {
        committed
            .into_iter()
            .map(|committed| committed.event.name)
            .collect::<Vec<_>>()
    };
    let own = store.events_after(0, usize::MAX).expect("the events read");
    let position = own.first().map(|committed| committed.position);
    assert_eq!(
        names(own),
        ["apart.match.Opened"],
        "the writer reads the event it committed"
    );
    assert_eq!(
        names(
            reading
                .events_after(0, usize::MAX)
                .expect("the events read")
        ),
        ["apart.match.Opened"],
        "the reading handle reads it too"
    );
    let head = store.events_head().expect("the head reads");
    assert!(
        position.is_some_and(|position| head >= position),
        "the head {head} is at or past the event's position {position:?}"
    );
    assert_eq!(
        reading.events_head().expect("the head reads"),
        head,
        "the reading handle's head"
    );
    holder.join().expect("the older transaction ends");
    assert_eq!(
        store
            .events_after(head, usize::MAX)
            .expect("the events read"),
        [],
        "no event committed before the head is after it"
    );
}

#[test]
fn a_component_keeps_its_tables_under_spindle_and_its_name() {
    assert_eq!(postgres_prefix("rally-table"), "spindle_rally_table");
    assert_eq!(
        postgres_prefix("rally-player-west"),
        "spindle_rally_player_west"
    );
    let names = [
        "api-v2",
        "api-v-c",
        "apiv2",
        "api2",
        "a-very-long-component-name-for-billing",
        "a-very-long-component-name-for-billing-two",
    ];
    let mut prefixes = std::collections::BTreeSet::new();
    for name in names {
        let prefix = postgres_prefix(name);
        assert!(
            prefix.len() <= 32
                && prefix.starts_with("spindle_")
                && prefix
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'_'),
            "{name} -> {prefix} breaks eventlog's prefix rule"
        );
        assert!(
            prefixes.insert(prefix.clone()),
            "{name} -> {prefix} is taken"
        );
    }
}

/// Runs `statement` on the test database as its owner.
fn sql(statement: &str) {
    let url = database().url;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    runtime.block_on(async {
        let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls)
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

const LISTED: &str = "long.match.Match";

fn listed(identity: &str) -> Change {
    Change {
        entity: LISTED.to_owned(),
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

/// The instances and writes of the long log [`a_long_scan_lets_another_process_commit`] scans.
const INSTANCES: u64 = 1_000;
const WRITES: u64 = 150_000;

/// Two processes on one component's tables, as a rolling deploy runs them: one scans a log of
/// 300,000 events back to back, as a metrics scrape does, the other takes commands. Every command
/// lands: a scan holds no lock a commit waits on for longer than one bounded read. The log is
/// the entity's own: 150,000 writes of 1,000 instances, each write's `spindle.row` event and the
/// `spindle.written` event naming it, appended as the store appends them.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_long_scan_lets_another_process_commit() {
    let database = database();
    let component = fresh("scraped");
    let mut scanner = open(&database, &component);
    let mut writer = open(&database, &component);
    let table = format!("{}_events", postgres_prefix(&component));
    let columns = "tenant_id, stream_type, stream_id, version, event_id, event_name, \
                   event_schema_version, occurred_at, recorded_at, subject, actor, request_id, \
                   trace_id, data";
    // A transaction of every chunk of writes, in turn: one of all 300,000 rows would be the
    // oldest open on the server for seconds, which every other case's feed reads wait out.
    let chunks = (0..WRITES).step_by(10_000);
    let mut statements = String::new();
    for from in chunks.clone() {
        let (from, to) = (from + 1, (from + 10_000).min(WRITES));
        statements.push_str(&format!(
            "BEGIN; INSERT INTO {table} ({columns}) SELECT 'local', 'spindle:entity', \
             '{LISTED}', g, gen_random_uuid(), 'spindle.written', 2, now(), now(), 'a', 'a', \
             'r' || g, 'r' || g, jsonb_build_object('rows', jsonb_build_object('\"i' || \
             ((g - 1) % {INSTANCES}) || '\"', (g - 1) / {INSTANCES} + 1), 'removed', \
             '{{}}'::jsonb) FROM generate_series({from}, {to}) g ORDER BY g; COMMIT; "
        ));
    }
    for from in chunks {
        let (from, to) = (from + 1, (from + 10_000).min(WRITES));
        statements.push_str(&format!(
            "BEGIN; INSERT INTO {table} ({columns}) SELECT 'local', '{LISTED}', \
             '\"i' || ((g - 1) % {INSTANCES}) || '\"', (g - 1) / {INSTANCES} + 1, \
             gen_random_uuid(), 'spindle.row', 1, now(), now(), 'a', 'a', 'r' || g, 'r' || g, \
             jsonb_build_object('state', 'open', 'fields', jsonb_build_object('id', \
             'i' || ((g - 1) % {INSTANCES}))) FROM generate_series({from}, {to}) g ORDER BY g; \
             COMMIT; "
        ));
    }
    statements.push_str(&format!("ANALYZE {table}"));
    sql(&statements);
    let first = scanner.scan(LISTED).expect("the long log scans");
    assert_eq!(
        (first.rows.len() as u64, first.version),
        (INSTANCES, WRITES),
        "the scan holds every instance at the log's version"
    );
    assert!(
        first
            .rows
            .iter()
            .all(|(_, stored)| stored.version == WRITES / INSTANCES),
        "each instance at its last write"
    );
    let stop = Arc::new(AtomicBool::new(false));
    let (scans, written) = std::thread::scope(|scope| {
        let stopping = Arc::clone(&stop);
        let scanning = scope.spawn(move || {
            let mut scans: Vec<(Duration, Result<Scan, String>)> = Vec::new();
            for _ in 0..3 {
                let started = Instant::now();
                let scan = scanner.scan(LISTED).map_err(|error| error.to_string());
                scans.push((started.elapsed(), scan));
            }
            stopping.store(true, Ordering::SeqCst);
            scans
        });
        let writing = scope.spawn(move || {
            let mut written = Vec::new();
            while !stop.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(50));
                let started = Instant::now();
                let outcome = writer.commit(&listed(&format!("w{}", written.len())));
                written.push((
                    started.elapsed(),
                    outcome.map_err(|error| error.to_string()),
                ));
            }
            written
        });
        (
            scanning.join().expect("the scanner"),
            writing.join().expect("the writer"),
        )
    });
    let slowest = written.iter().map(|(took, _)| *took).max();
    let timed: Vec<_> = scans.iter().map(|(took, _)| *took).collect();
    eprintln!(
        "scans {timed:?}; {} commits, slowest {slowest:?}",
        written.len()
    );
    let failed: Vec<_> = written
        .iter()
        .filter(|(_, outcome)| outcome.is_err())
        .collect();
    assert!(
        failed.is_empty(),
        "commits failed during scans: {failed:#?}"
    );
    assert!(
        written.len() >= 3,
        "{} commits landed while three scans ran",
        written.len()
    );
    for (_, scan) in &scans {
        let scan = scan.as_ref().expect("every scan answers");
        assert_eq!(
            scan.rows.len() as u64,
            INSTANCES + scan.version - WRITES,
            "the scan holds exactly the rows of the writes its version counts"
        );
    }
}

/// An entity of 5,000 instances, each created once, scans every row at its version: the scan
/// reads one event per instance.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_scan_of_many_instances_holds_each_row() {
    const MANY: u64 = 5_000;
    let database = database();
    let component = fresh("many");
    let mut store = open(&database, &component);
    let table = format!("{}_events", postgres_prefix(&component));
    let columns = "tenant_id, stream_type, stream_id, version, event_id, event_name, \
                   event_schema_version, occurred_at, recorded_at, subject, actor, request_id, \
                   trace_id, data";
    sql(&format!(
        "INSERT INTO {table} ({columns}) SELECT 'local', 'spindle:entity', '{LISTED}', g, \
         gen_random_uuid(), 'spindle.written', 2, now(), now(), 'a', 'a', 'r' || g, 'r' || g, \
         jsonb_build_object('rows', jsonb_build_object('\"m' || g || '\"', 1), 'removed', \
         '{{}}'::jsonb) FROM generate_series(1, {MANY}) g; \
         INSERT INTO {table} ({columns}) SELECT 'local', '{LISTED}', '\"m' || g || '\"', 1, \
         gen_random_uuid(), 'spindle.row', 1, now(), now(), 'a', 'a', 'r' || g, 'r' || g, \
         jsonb_build_object('state', 'open', 'fields', jsonb_build_object('id', 'm' || g)) \
         FROM generate_series(1, {MANY}) g; ANALYZE {table}"
    ));
    let started = Instant::now();
    let scan = store.scan(LISTED).expect("the entity scans");
    eprintln!("a scan of {MANY} instances: {:?}", started.elapsed());
    assert_eq!(scan.version, MANY);
    assert_eq!(scan.rows.len() as u64, MANY);
    for (identity, stored) in &scan.rows {
        assert_eq!(stored.version, 1);
        assert_eq!(&stored.row.fields["id"], identity);
    }
}

/// A log whose `spindle.written` events name no instances, as the stores before this one wrote
/// them (and `state move` copied them), scans as it did: the same rows at the same version, and
/// with the writes after them.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_log_written_before_writes_named_their_instances_scans_the_same() {
    let database = database();
    let component = fresh("older");
    let mut store = open(&database, &component);
    for identity in ["a", "b", "c"] {
        store.commit(&listed(identity)).expect("created");
    }
    let mut moved = listed("a");
    moved.expected = Some(1);
    moved.row.state = "closed".to_owned();
    store.commit(&moved).expect("a moves");
    store
        .remove(&Removal {
            entity: LISTED.to_owned(),
            identity: json!("b"),
            expected: 1,
            events: Vec::new(),
            scanned: Vec::new(),
        })
        .expect("b is removed");
    store
        .commit_all(&[
            Write::Commit(listed("d")),
            Write::Commit(listed("e")),
            Write::Commit(created("other")),
        ])
        .expect("a group");
    let before = (
        store.scan(LISTED).expect("scans"),
        store.scan("apart.match.Match").expect("scans"),
    );
    assert_eq!(before.0.rows.len(), 4);
    assert_eq!(before.0.version, 6);
    sql(&format!(
        "UPDATE {}_events SET data = '{{}}'::jsonb, event_schema_version = 1 \
         WHERE event_name = 'spindle.written'",
        postgres_prefix(&component)
    ));
    let mut reopened = open(&database, &component);
    for store in [&mut store, &mut reopened] {
        let after = (
            store.scan(LISTED).expect("scans"),
            store.scan("apart.match.Match").expect("scans"),
        );
        assert_eq!(after, before, "the older log scans the same");
    }
    reopened.commit(&listed("f")).expect("a write after them");
    let mixed = store.scan(LISTED).expect("scans");
    assert_eq!(mixed.version, 7);
    let identities: Vec<_> = mixed.rows.iter().map(|(identity, _)| identity).collect();
    assert_eq!(
        identities,
        [json!("a"), json!("c"), json!("d"), json!("e"), json!("f")]
            .iter()
            .collect::<Vec<_>>()
    );
}

/// A row written before 0.7.0, when no write recorded its entity's version, in a log marked as
/// holding writes that name no instances (stream `spindle:log` `unnamed-writes`, which `state
/// move` appends to before it copies such writes): the scan holds it beside the rows written
/// since, at the version the writes since count.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_marked_log_scans_the_rows_written_before_entity_versions() {
    let database = database();
    let component = fresh("marked");
    let mut store = open(&database, &component);
    let columns = "tenant_id, stream_type, stream_id, version, event_id, event_name, \
                   event_schema_version, occurred_at, recorded_at, subject, actor, request_id, \
                   trace_id, data";
    let table = format!("{}_events", postgres_prefix(&component));
    sql(&format!(
        "INSERT INTO {table} ({columns}) VALUES \
         ('local', '{LISTED}', '\"old\"', 1, gen_random_uuid(), 'spindle.row', 1, now(), now(), \
          'a', 'a', 'old', 'old', '{{\"state\":\"open\",\"fields\":{{\"id\":\"old\"}}}}'), \
         ('local', 'spindle:log', 'unnamed-writes', 1, gen_random_uuid(), 'spindle.unnamed', 1, \
          now(), now(), 'a', 'a', 'mark', 'mark', '{{}}')",
    ));
    store.commit(&listed("new")).expect("a write since");
    let scan = store.scan(LISTED).expect("scans");
    assert_eq!(scan.version, 1, "one write recorded the entity's version");
    let identities: Vec<_> = scan.rows.iter().map(|(identity, _)| identity).collect();
    assert_eq!(identities, [&json!("new"), &json!("old")]);
}

/// A `database_url` holding an `@` that is not percent-encoded, other than the one ending its
/// user info before the host, is refused before anything connects, naming `database_url` and
/// printing no part of it: the URL parser takes everything before the first `@` for the user
/// and password, wherever it is, so a password in the query string would otherwise be read as
/// the host. So is one that names no host.
#[test]
fn a_database_url_holding_an_unencoded_at_or_no_host_is_refused() {
    for url in [
        "postgres://adv:sec@retpart@db.example.invalid:5432/x",
        "postgres://adv:sec@retpart@127.0.0.1:1/x",
        "postgresql://adv@sec@retpart@127.0.0.1/x?sslmode=disable",
        "postgres://127.0.0.1:1/x?user=adv&password=sec@retpart",
        "postgres:///x?host=127.0.0.1&port=1&user=adv&password=sec@retpart",
        "postgres://db.example.invalid/x?user=adv&password=sec@retpart",
        "postgres://adv@127.0.0.1:1/x?password=sec@retpart",
        "postgres://adv:sec/ret@part@db.example.invalid/x",
        "postgres://adv:sec?ret@db.example.invalid/x",
        "postgres://adv:pw@/x?host=/run/sec@retpart",
    ] {
        for ca in [None, Some(std::path::PathBuf::from("absent-ca.pem"))] {
            let mut database = Database::new(url);
            database.ca = ca;
            database.connections = Some(100);
            let opened = PostgresStore::open(&database, "secret", 1).map(|_| ());
            let migrated = PostgresStore::migrate(&database, "secret");
            for refused in [opened, migrated] {
                let message = refused.expect_err("refused").to_string();
                assert!(
                    message.contains("database_url") && message.contains("percent-encode"),
                    "{message}"
                );
                for part in ["sec@", "retpart", "example", "127.0.0.1", "adv:", "adv@"] {
                    assert!(!message.contains(part), "{part} printed: {message}");
                }
            }
        }
    }
    for url in ["postgres://", "postgresql://", "postgres:///x", ""] {
        let mut database = Database::new(url);
        database.connections = Some(100);
        let message = PostgresStore::open(&database, "secret", 1)
            .map(|_| ())
            .expect_err("refused")
            .to_string();
        assert!(
            message.contains("database_url") && message.contains("no host"),
            "{url:?}: {message}"
        );
    }
}
