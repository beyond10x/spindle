//! The PostgreSQL store (story postgres-store): the store contract, and components of one
//! database kept apart by their table prefix.
//!
//! The cases that reach a database are ignored without `SPINDLE_TEST_DATABASE_URL`, a loopback
//! PostgreSQL database whose role may create tables; `task check:postgres` runs them. Each case
//! writes under component names no earlier run used, so its tables start empty.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use spindle_kernel::{
    Change, Database, PostgresStore, Row, Store, Stored, contract, postgres_prefix,
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

/// PostgreSQL's feed shows a commit only once every older write transaction has ended; a scan
/// right after a commit waits for it rather than answer the entity's version without its row.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_scan_waits_for_a_commit_an_older_transaction_holds_back() {
    let database = database();
    let mut store = open(&database, &fresh("lagging"));
    let url = database.url.clone();
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
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            transaction.commit().await.expect("the transaction ends");
            drop(client);
            let _ = driver.await;
        });
    });
    older.recv().expect("the older transaction is open");
    store
        .commit(&created("west"))
        .expect("the match is created");
    let scan = store.scan("apart.match.Match").expect("the store scans");
    assert_eq!(
        scan.rows.len(),
        1,
        "the scan holds the row its version counts"
    );
    holder.join().expect("the older transaction ends");
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
