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
//! test binary, started side by side on a database created for the round. Its name holds
//! `on_postgres`, so the lane's filter selects it.

use std::process::{Command, Output, Stdio};

/// One case of each module that creates its schema behind a `Once`, each short.
const CASES: [&str; 3] = [
    "adversary_postgres::adv_two_stores_on_one_prefix_create_one_identity_exactly_once",
    "adversary_postgres_scan::adv2_a_migrate_beside_a_serving_store_delays_nothing",
    "adversary_postgres_named::adv3_a_scan_beside_removals_and_recreations_holds_the_state_at_its_version",
];

/// Processes of each case started side by side in one round.
const SIDE_BY_SIDE: usize = 6;

/// Rounds, each on a database of its own.
const ROUNDS: usize = 4;

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
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn adv_w31_the_once_modules_pass_one_process_per_case_on_postgres() {
    let owner = url();
    let binary = std::env::current_exe().expect("this test binary");
    let mut failed = Vec::new();
    for round in 0..ROUNDS {
        let name = fresh_name();
        sql(
            &on_database(&owner, "postgres"),
            &format!("CREATE DATABASE {name}"),
        );
        let database = on_database(&owner, &name);
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
        for (case, child) in children {
            let output = child.wait_with_output().expect("the case ends");
            if !output.status.success() {
                failed.push(format!(
                    "round {round}: {case}: {}\n      {}",
                    output.status,
                    why(&output)
                ));
            } else if !String::from_utf8_lossy(&output.stdout).contains("1 passed") {
                failed.push(format!(
                    "round {round}: {case}: the filter selected no case"
                ));
            }
        }
        sql(
            &on_database(&owner, "postgres"),
            &format!("DROP DATABASE {name} WITH (FORCE)"),
        );
    }
    assert!(
        failed.is_empty(),
        "{} of {} case processes failed on a fresh database, each a process of its own as under \
         nextest:\n{}",
        failed.len(),
        ROUNDS * CASES.len() * SIDE_BY_SIDE,
        failed.join("\n")
    );
}
