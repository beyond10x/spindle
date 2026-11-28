//! Adversary, wave 31 U1 (story postgres-lane-kills-slow-tests): `task check:postgres` now runs
//! its tests under cargo-nextest, one process per test, where the seven `cargo test` commands ran
//! one process per binary.
//!
//! `adversary_postgres`, `adversary_postgres_scan` and `adversary_postgres_named` each create
//! their schema behind a `std::sync::Once`, whose own comment says why: "cases running in parallel
//! on a fresh database race their `CREATE SCHEMA IF NOT EXISTS` into a unique violation". A
//! `Once` holds within one process. Under nextest every case is a process of its own, so on a
//! fresh database, as CI's PostgreSQL job starts one for every run, the cases of one module start
//! side by side and race again.
//!
//! The case here runs cases of those three modules as nextest does: each one a process of this
//! test binary, started side by side on a database created for the case, while a sibling holds
//! the three schemas created and not yet committed. Its name holds `on_postgres`, so the lane's
//! filter selects it.

use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::postgres_reads::sql_value;

/// One case of each module that creates its schema behind a `Once`, each short. The case of
/// `adversary_postgres_named` was `adv3_a_scan_beside_removals_and_recreations_holds_the_state_at_its_version`,
/// 400 drawn groups and a scanner beside them: 24 processes of it made this case 43 to 60 s of
/// the lane on a 4-CPU runner (`story:postgres-lane-margin`). The race is in the schema's
/// creation, which every case of the module goes through first.
const CASES: [&str; 3] = [
    "adversary_postgres::adv_two_stores_on_one_prefix_create_one_identity_exactly_once",
    "adversary_postgres_scan::adv2_a_migrate_beside_a_serving_store_delays_nothing",
    "adversary_postgres_named::adv3_each_escaped_identity_scans_as_on_sqlite",
];

/// The schemas of the three modules, which the sibling holds created.
const SCHEMAS: [&str; 3] = ["adv1_w19", "adv2_w19", "adv3_w19"];

/// The advisory lock each module takes around its `CREATE SCHEMA IF NOT EXISTS`.
const SCHEMA_LOCK: i64 = 5_312_021_918;

/// Processes of each case started side by side.
const SIDE_BY_SIDE: usize = 2;

/// Holds `statement`'s transaction open on `url` until the returned sender is sent to, then
/// commits it; the join handle answers once it has committed.
fn held(url: String, statement: String) -> (mpsc::Sender<()>, std::thread::JoinHandle<()>) {
    let (opened, ready) = mpsc::channel();
    let (commit, committed) = mpsc::channel::<()>();
    let holder = std::thread::spawn(move || {
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
                .batch_execute(&statement)
                .await
                .unwrap_or_else(|error| panic!("{statement}: {error}"));
            opened.send(()).expect("the case waits for the holder");
            let _ = committed.recv_timeout(Duration::from_secs(60));
            client
                .batch_execute("COMMIT")
                .await
                .expect("the sibling commits");
            drop(client);
            let _ = driver.await;
        });
    });
    ready.recv().expect("the holder opened its transaction");
    (commit, holder)
}

fn url() -> String {
    std::env::var("SPINDLE_TEST_DATABASE_URL")
        .expect("SPINDLE_TEST_DATABASE_URL names a loopback PostgreSQL (task check:postgres)")
}

/// `url` on the database `name` of the same server.
fn on_database(url: &str, name: &str) -> String {
    let (head, _) = url.rsplit_once('/').expect("a URL with a database");
    format!("{head}/{name}")
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

fn fresh_name() -> String {
    let letters: String = uuid::Uuid::new_v4()
        .simple()
        .to_string()
        .bytes()
        .take(12)
        .map(|byte| char::from(b'a' + byte % 26))
        .collect();
    format!("adv_w31_u1_{letters}")
}

/// The lines of a child's output that say why it failed.
fn why(output: &Output) -> String {
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    text.lines()
        .filter(|line| {
            line.contains("panicked")
                || line.contains("CREATE SCHEMA")
                || line.contains("db error")
                || line.contains("duplicate key")
        })
        .collect::<Vec<_>>()
        .join("\n      ")
}

/// Each case of the three `Once` modules passes as one process among its module's siblings on a
/// fresh database, which is how the lane runs it since this unit.
///
/// A sibling holds the three schemas created under the modules' lock, uncommitted, until every
/// case process waits on the server, then commits. A case that takes the lock waits for it and
/// finds its schema; one that does not passes `IF NOT EXISTS`, waits on the sibling's row and
/// fails with a unique violation once the sibling commits. The case started 4 rounds of 6
/// processes of each case on fresh databases instead, 72 processes in 11 to 33 s of the lane;
/// against modules without the lock all 72 passed, and 2 rounds of 4 failed 0, 3 and 1 of 24 in
/// three runs. With the sibling, all 6 processes fail at their `CREATE SCHEMA` on those modules
/// (`story:postgres-lane-margin`).
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv_w31_the_once_modules_pass_one_process_per_case_on_postgres() {
    let owner = url();
    let binary = std::env::current_exe().expect("this test binary");
    let name = fresh_name();
    sql(
        &on_database(&owner, "postgres"),
        &format!("CREATE DATABASE {name}"),
    );
    let database = on_database(&owner, &name);
    let creations: String = SCHEMAS
        .iter()
        .map(|schema| format!("CREATE SCHEMA {schema}; "))
        .collect();
    let (commit, holder) = held(
        database.clone(),
        format!("BEGIN; SELECT pg_advisory_xact_lock({SCHEMA_LOCK}); {creations}"),
    );
    let children: Vec<(&str, std::process::Child)> = CASES
        .iter()
        .flat_map(|case| std::iter::repeat_n(*case, SIDE_BY_SIDE))
        .map(|case| {
            let child = Command::new(&binary)
                .args(["--exact", "--include-ignored", "--test-threads", "1", case])
                .env("SPINDLE_TEST_DATABASE_URL", &database)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("a case starts as a process of this binary");
            (case, child)
        })
        .collect();
    let deadline = Instant::now() + Duration::from_secs(20);
    let waiting = loop {
        let waiting: usize = sql_value(
            &owner,
            &format!(
                "SELECT count(*) FROM pg_stat_activity WHERE datname = '{name}' \
                 AND wait_event_type = 'Lock'"
            ),
        )
        .parse()
        .expect("a count");
        if waiting >= children.len() || Instant::now() >= deadline {
            break waiting;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    commit.send(()).expect("the sibling holds its transaction");
    holder.join().expect("the sibling committed");
    let mut failed = Vec::new();
    for (case, child) in children {
        let output = child.wait_with_output().expect("the case ends");
        if !output.status.success() {
            failed.push(format!("{case}: {}\n      {}", output.status, why(&output)));
        } else if !String::from_utf8_lossy(&output.stdout).contains("1 passed") {
            failed.push(format!("{case}: the filter selected no case"));
        }
    }
    sql(
        &on_database(&owner, "postgres"),
        &format!("DROP DATABASE {name} WITH (FORCE)"),
    );
    assert_eq!(
        waiting,
        CASES.len() * SIDE_BY_SIDE,
        "case processes waiting on the sibling's schemas within 20 s"
    );
    assert!(
        failed.is_empty(),
        "{} of {} case processes failed on a fresh database whose schemas a sibling was \
         creating, each a process of its own as under nextest:\n{}",
        failed.len(),
        CASES.len() * SIDE_BY_SIDE,
        failed.join("\n")
    );
}
