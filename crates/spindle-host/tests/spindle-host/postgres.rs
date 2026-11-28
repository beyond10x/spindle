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

/// Two drawn sequences of 60 steps each, where the memory and the SQLite store run four: the
/// reader reads PostgreSQL back to back through each, so four took 14 to 32 s of a 4-CPU runner
/// beside the lane (`story:postgres-lane-margin`).
const SEQUENCES: u64 = 2;

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn every_read_beside_the_host_is_the_state_after_a_prefix_of_the_commands() {
    let between = prefixes_read("postgres", SEQUENCES, &|_: &Path| host(4));
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
    prefixes_read("postgres", SEQUENCES, &|_: &Path| host(1));
}

/// 4 resets from 5 loans to 50, where the memory store's case makes 20 to 300: on PostgreSQL a
/// reset writes each loan of its world in a commit of its own and empties each one the last
/// world left, about 3.3 ms a loan, so 20 resets each way over 300 loans took 27 of the case's
/// 29 s, and 10 over 50 loans 20 to 40 s of a 4-CPU runner beside the lane
/// (`story:postgres-lane-margin`). A count handed to the readers before the reset ends is one of
/// the 1 to 49 loans written so far, or the 0 of the emptied store, which a reset of 50 shows as
/// surely as one of 300: the sampler reads many counts during each of the 4 resets.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_reader_counts_the_rows_before_a_reset_or_after_it_never_between() {
    reset_counted_whole(host(4), 4, 50);
}

/// 3 purges of 30 loans, where the memory store's case makes up to 60 of 100: each round lends
/// and returns every loan, two commands a loan, so 10 rounds of 100 loans (2,000 commands) ran
/// into the 60 s kill on a 4-CPU runner (`story:postgres-lane-margin`). A purge of 30 removals
/// counted part way is a count from 1 to 29, which the sampler reads while each purge lands.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_reader_counts_a_set_removal_whole() {
    purge_counted_whole(host(4), 3, 30);
}

/// 500 loans held, where the memory store's case holds 20,000: a reset writes each loan in a
/// commit of its own, so 2,000 took most of the case's 33 to 58 s on a 4-CPU runner
/// (`story:postgres-lane-margin`). A read of the members that read or copied every loan would
/// still cost a write several times its own time.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn reading_a_small_view_does_not_make_the_writer_pay() {
    small_view_not_paid_for(host(4), 500);
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

/// `story:events-head-cursor` acceptances 1 and 2 on PostgreSQL
/// ([`crate::events_range::the_head_reads_no_event_of_a_long_log_on`]): the cursor after the last
/// of 10,000 events is answered reading no event of either component's store.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn the_head_reads_no_event_of_a_long_log() {
    let url = std::env::var("SPINDLE_TEST_DATABASE_URL")
        .expect("SPINDLE_TEST_DATABASE_URL names a loopback PostgreSQL (task check:postgres)");
    let database = Database::new(url);
    let open = |component: &str| {
        PostgresStore::open(&database, component, 1)
            .unwrap_or_else(|error| panic!("the store of {component} opens: {error}"))
    };
    crate::events_range::the_head_reads_no_event_of_a_long_log_on(
        "postgres",
        open(&fresh()),
        open(&fresh()),
    );
}

/// `story:view-filter-text-contains` on PostgreSQL ([`crate::text_filters::agree_on`]): every
/// read of the text-filters fixture answers what ESS's interpreter answers, and is refused where
/// it refuses.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn text_filters_agree_with_the_interpreter() {
    let url = std::env::var("SPINDLE_TEST_DATABASE_URL")
        .expect("SPINDLE_TEST_DATABASE_URL names a loopback PostgreSQL (task check:postgres)");
    let database = Database::new(url);
    let component = fresh();
    let store = PostgresStore::open(&database, &component, 1)
        .unwrap_or_else(|error| panic!("the store of {component} opens: {error}"));
    crate::text_filters::agree_on("postgres", Box::new(store));
}
