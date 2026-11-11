//! Reads beside the host's thread on the PostgreSQL store (`story:concurrent-reads`, acceptance
//! 5): the cases of `concurrent_reads` and `adversary_concurrent_reads` that hold for any store
//! with a reading handle, on a host whose component `desk` keeps its state in PostgreSQL, the
//! store `run` supplies for a component bound to `postgres`. Its reading handle reads through the
//! store's own pool, beside the writer; a reset empties it in place, so its reads wait for the
//! reset rather than read it half done.
//!
//! The cases are ignored without `SPINDLE_TEST_DATABASE_URL`, a loopback PostgreSQL database
//! whose role may create tables; `task check:postgres` runs them. Each host keeps `desk` under a
//! component name no earlier run used, so its tables start empty.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use spindle_host::{Host, ManualClock, Stores};
use spindle_kernel::{Database, PostgresStore, Store};

use crate::adversary_concurrent_reads::{
    purge_counted_whole, reads_each_command_once_it_returns, reset_counted_whole,
    small_view_not_paid_for,
};
use crate::concurrent_reads::{environment, prefixes_read, rows_fixture};

/// A component name no earlier run used: `desk-` and letters drawn from the time, the process
/// and a count.
fn fresh() -> String {
    static DRAWN: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let mut seed = u128::from(std::process::id())
        ^ nanos
        ^ (u128::from(DRAWN.fetch_add(1, Ordering::SeqCst)) << 64);
    let mut letters = String::from("desk-");
    for _ in 0..14 {
        letters.push(char::from(b'a' + u8::try_from(seed % 26).unwrap_or(0)));
        seed /= 26;
    }
    letters
}

/// A host of the rows fixture whose component `desk` keeps its state in a PostgreSQL store of
/// its own, opened as `run` opens it, with `pool` connections.
fn host(pool: usize) -> Host {
    host_on(pool, &fresh())
}

/// [`host`], keeping `desk` under the component name `component`.
fn host_on(pool: usize, component: &str) -> Host {
    let url = std::env::var("SPINDLE_TEST_DATABASE_URL")
        .expect("SPINDLE_TEST_DATABASE_URL names a loopback PostgreSQL (task check:postgres)");
    let mut database = Database::new(url);
    database.pool = pool;
    let store = PostgresStore::open(&database, component, 1)
        .unwrap_or_else(|error| panic!("the store of {component} opens: {error}"));
    let mut stores = Stores::new();
    stores.insert("desk".to_owned(), Box::new(store) as Box<dyn Store>);
    Host::supplied(
        rows_fixture(),
        &environment("postgres"),
        Arc::new(ManualClock::new()),
        None,
        stores,
    )
    .unwrap_or_else(|error| panic!("the postgres host opens: {error}"))
}

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn every_read_beside_the_host_is_the_state_after_a_prefix_of_the_commands() {
    let between = prefixes_read("postgres", &|_: &Path| host(4));
    assert!(
        between > 0,
        "no read landed between the first and the last step: the reads did not run beside the \
         commands"
    );
}

/// The same with one connection in the pool, which the reads and the writer take in turn.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn every_read_beside_the_host_is_the_state_after_a_prefix_of_the_commands_on_one_connection() {
    prefixes_read("postgres", &|_: &Path| host(1));
}

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_reader_counts_the_rows_before_a_reset_or_after_it_never_between() {
    reset_counted_whole(host(4));
}

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_reader_counts_a_set_removal_whole() {
    purge_counted_whole(host(4), 10);
}

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn reading_a_small_view_does_not_make_the_writer_pay() {
    small_view_not_paid_for(host(4), 2_000);
}

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_reader_reads_a_command_once_it_returns() {
    reads_each_command_once_it_returns(host(4));
}

fn submit(host: &mut Host, command: &str, input: Value) {
    let Value::Object(input) = input else {
        panic!("{command}: the input is an object");
    };
    let report = host
        .submit(command, input)
        .unwrap_or_else(|error| panic!("{command}: {error}"));
    assert!(report.failed.is_empty(), "{command}: {:?}", report.failed);
}

/// The events a host lists (`story:events-read-from-store`), by command and event.
fn listed(host: &mut Host) -> Vec<(String, String)> {
    host.published()
        .expect("the events read")
        .into_iter()
        .map(|published| (published.command, published.event.name))
        .collect()
}

/// A reset of the PostgreSQL store, which it empties in place, is recorded in the store: a host
/// opened on the same tables later lists only the events published after it. An event of a set
/// outcome that selected no row is listed too.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_reset_is_kept_across_a_restart() {
    let component = fresh();
    let mut first = host_on(2, &component);
    submit(
        &mut first,
        "lending.loans.Lend",
        json!({"loan_id": "l1", "borrower": "ada", "due": "2026-01-05T00:00:00Z"}),
    );
    first.reset().expect("the reset empties the store");
    submit(
        &mut first,
        "lending.loans.Lend",
        json!({"loan_id": "l2", "borrower": "ada", "due": "2026-01-05T00:00:00Z"}),
    );
    submit(
        &mut first,
        "lending.loans.MarkOverdue",
        json!({"as_of": "2025-01-01T00:00:00Z"}),
    );
    let since = vec![
        (
            "lending.loans.Lend".to_owned(),
            "lending.loans.Lent".to_owned(),
        ),
        (
            "lending.loans.MarkOverdue".to_owned(),
            "lending.loans.LoansMarked".to_owned(),
        ),
    ];
    assert_eq!(listed(&mut first), since, "before the restart");
    drop(first);

    let mut again = host_on(2, &component);
    assert_eq!(
        listed(&mut again),
        since,
        "after the restart, only what was published after the reset"
    );
}
