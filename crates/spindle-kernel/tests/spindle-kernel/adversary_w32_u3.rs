//! Adversary of wave 32 U3 (`story:leader-exits-promptly-on-sigterm`): the stop flag a read of the
//! feed now takes (`Reading::events_placed_after_unless_stopped`, D1 A) and the short wait of a
//! lease's release at exit (`Lease::release`, D3 A), read against the story and the documents the
//! unit wrote.
//!
//! - A SQLite store's reading handle answers the same page with the flag down as without it, and
//!   no event with it up: the flag changes nothing on SQLite but a read asked after a stop.
//! - A release that waited out the lease row's lock for its 1 s sets the connection's timeouts
//!   back, so the renewal after it waits its third of the lease again ("the connection's
//!   timeouts are set back after it", `Lease::release`).

use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;

use serde_json::json;
use spindle_kernel::{Change, Event, Row, SqliteStore, Store};

fn opened(identity: &str) -> Change {
    Change {
        entity: "apart.match.Match".to_owned(),
        identity: json!(identity),
        expected: None,
        row: Row {
            state: "open".to_owned(),
            fields: BTreeMap::from([("side".to_owned(), json!("west"))]),
        },
        events: vec![Event {
            name: "apart.match.Opened".to_owned(),
            payload: serde_json::Map::new(),
        }],
        scanned: Vec::new(),
    }
}

/// On SQLite the read with its stop flag down answers what the read without one answers, at
/// every position; with the flag up it answers no event, as the documentation of the trait's
/// default says.
#[cfg(feature = "sqlite")]
#[test]
fn a_sqlite_read_with_its_stop_flag_down_answers_as_the_read_without_one() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    let mut store = SqliteStore::open(&dir.path().join("stop.sqlite")).expect("the store opens");
    for identity in ["one", "two", "three"] {
        store
            .commit(&opened(identity))
            .expect("the match is created");
    }
    let reading = store
        .reading()
        .expect("a SQLite store reads beside its writer");
    let down = AtomicBool::new(false);
    let all = reading
        .events_placed_after(0, 64)
        .expect("the feed is read");
    assert_eq!(all.len(), 3, "three events were committed: {all:?}");
    let positions: Vec<u64> = std::iter::once(0)
        .chain(all.iter().map(|(placed, _)| *placed))
        .collect();
    for position in positions {
        for limit in [0, 1, 2, 64] {
            let plain = reading
                .events_placed_after(position, limit)
                .expect("the feed is read");
            let flagged = reading
                .events_placed_after_unless_stopped(position, limit, &down)
                .expect("the feed is read with the flag down");
            let shown = |page: &[(u64, spindle_kernel::Committed)]| -> Vec<(u64, String, u64)> {
                page.iter()
                    .map(|(placed, committed)| {
                        (*placed, committed.event.name.clone(), committed.position)
                    })
                    .collect()
            };
            assert_eq!(
                shown(&flagged),
                shown(&plain),
                "after {position}, at most {limit}: the flag down changes the page"
            );
        }
    }
    let up = AtomicBool::new(true);
    let stopped = reading
        .events_placed_after_unless_stopped(0, 64, &up)
        .expect("a stopped read answers");
    assert!(
        stopped.is_empty(),
        "a stopped read answers no event: {stopped:?}"
    );
}

/// The PostgreSQL lane (`task check:postgres`).
#[cfg(feature = "postgres")]
mod postgres {
    use std::time::{Duration, Instant};

    use spindle_kernel::{Database, Lease, postgres_prefix};

    fn database() -> Database {
        let url = std::env::var("SPINDLE_TEST_DATABASE_URL")
            .expect("SPINDLE_TEST_DATABASE_URL names a loopback PostgreSQL (task check:postgres)");
        Database::new(url)
    }

    /// `stem` and letters no earlier run drew.
    fn fresh(stem: &str) -> String {
        let mut value = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("a clock after 1970")
            .as_nanos()
            ^ u128::from(std::process::id()) << 64;
        let mut letters = String::new();
        for _ in 0..12 {
            letters.push(char::from(
                b'a' + u8::try_from(value % 26).expect("below 26"),
            ));
            value /= 26;
        }
        format!("{stem}-{letters}")
    }

    /// Another session holding the lease row of `table` `FOR UPDATE` from the call until
    /// `held` has passed.
    fn locked_for(url: String, table: String, held: Duration) -> std::thread::JoinHandle<()> {
        let (locked, row) = std::sync::mpsc::channel();
        let holder = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a runtime");
            let (mut client, connection) = runtime
                .block_on(tokio_postgres::connect(&url, tokio_postgres::NoTls))
                .expect("a second connection");
            let driver = runtime.spawn(connection);
            let transaction = runtime
                .block_on(client.transaction())
                .expect("a transaction");
            runtime
                .block_on(transaction.simple_query(&format!("SELECT 1 FROM {table} FOR UPDATE")))
                .expect("the lease row is locked");
            let began = Instant::now();
            locked.send(()).expect("the test waits");
            std::thread::sleep(held.saturating_sub(began.elapsed()));
            runtime
                .block_on(transaction.commit())
                .expect("the lock is let go");
            drop(client);
            let _ = runtime.block_on(driver);
        });
        row.recv().expect("the lease row is locked");
        holder
    }

    /// `Lease::release` sets the statement and lock timeouts to 1 s for its statement and back
    /// to a third of the lease after it, also where its statement waited out the lock: the
    /// renewal that follows on the same connection, under a lease of an hour, waits for a lock
    /// held 2.5 s past the release's start and lands, where a connection left at 1 s refuses it.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_renewal_after_a_release_that_waited_out_its_lock_waits_a_third_of_the_lease_again() {
        let database = database();
        let component = fresh("restored");
        let mut lease = Lease::connect(&database, Duration::from_secs(3600))
            .expect("a run connects for the lease");
        let token = lease
            .take(&component)
            .expect("the lease is read")
            .expect("no process holds the lease: the run takes it");
        let table = format!(
            "{}.{}_p_lease",
            database.schema,
            postgres_prefix(&component)
        );
        let holder = locked_for(database.url.clone(), table, Duration::from_millis(2500));
        let started = Instant::now();
        let released = lease.release(&component, token);
        let took = started.elapsed();
        assert!(
            released.is_err(),
            "the release waited out the locked row: {released:?}"
        );
        assert!(
            took < Duration::from_millis(1500),
            "the release waited {took:?}"
        );
        let renewing = Instant::now();
        let renewed = lease.renew(&component, token);
        let waited = renewing.elapsed();
        holder.join().expect("the other session ends");
        assert_eq!(
            renewed.as_ref().ok(),
            Some(&true),
            "the renewal after the failed release waited {waited:?} and answered {renewed:?}: \
             its connection kept the release's 1 s timeouts"
        );
        assert!(
            waited >= Duration::from_millis(1000),
            "the renewal waited {waited:?} for the lock held 2.5 s from the release's start"
        );
    }
}
