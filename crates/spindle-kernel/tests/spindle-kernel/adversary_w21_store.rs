//! Adversary cases on wave 21's PostgreSQL reading handle (`story:postgres-store`): the handle
//! reads through the writer's own pool ([`spindle_kernel::Store::reading`]), and the listener runs
//! each view read on a blocking thread of its own (`spindle-ingress` `listener.rs`, `handed`), up
//! to tokio's 512. The pool queues at most 32 acquisitions (eventlog-postgres `PoolOptions`,
//! `max_waiters`) and refuses the next one `Overloaded` at once, so a burst of view reads larger
//! than the pool and its queue refuses reads, and the writer's own acquisitions, that a host
//! reading on its own thread queued.
//!
//! Ignored without `SPINDLE_TEST_DATABASE_URL`, as `postgres` is; `task check:postgres` runs the
//! `postgres::` filter, so these run with `-- --include-ignored adversary_w21_store::`.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::json;
use spindle_kernel::{Change, Database, PostgresStore, Reading, Row, Store, StoreError};

const ITEM: &str = "burst.desk.Item";

/// View reads in flight at once: fewer than the listener's 512 blocking threads.
const READERS: usize = 48;

fn database() -> Database {
    let url = std::env::var("SPINDLE_TEST_DATABASE_URL")
        .expect("SPINDLE_TEST_DATABASE_URL names a loopback PostgreSQL (task check:postgres)");
    Database::new(url)
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

fn created(identity: &str) -> Change {
    Change {
        entity: ITEM.to_owned(),
        identity: json!(identity),
        expected: None,
        row: Row {
            state: "Open".to_owned(),
            fields: BTreeMap::from([("title".to_owned(), json!(identity))]),
        },
        events: Vec::new(),
        scanned: Vec::new(),
    }
}

/// A store with the default pool (4) holding 20 items, `READERS` threads scanning it through its
/// reading handle while it commits 40 more: what each side was refused.
fn burst() -> (Vec<String>, Vec<String>) {
    burst_of(|reading| reading.scan(ITEM).map(drop))
}

/// As [`burst`], each reader reading through a position of the handle ([`Reading::at`]): a scan
/// of the items, a load of one of them and whether the position held, as a view reading fields
/// through a reference reads.
fn burst_of_positions() -> (Vec<String>, Vec<String>) {
    burst_of(|reading| {
        let position = reading.at(&[ITEM.to_owned()])?;
        position.scan(ITEM)?;
        position.load(ITEM, &json!("seed-0"))?;
        position.holds().map(drop)
    })
}

fn burst_of(read: fn(&dyn Reading) -> Result<(), StoreError>) -> (Vec<String>, Vec<String>) {
    let database = database();
    assert_eq!(database.pool, 4, "the default pool");
    let component = fresh("burst");
    let mut store = PostgresStore::open(&database, &component, 1)
        .unwrap_or_else(|error| panic!("the store of {component} opens: {error}"));
    for index in 0..20 {
        store
            .commit(&created(&format!("seed-{index}")))
            .expect("a seed row lands");
    }
    let reading: Arc<dyn Reading> = store.reading().expect("a reading handle");
    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let readers = (0..READERS)
            .map(|_| {
                let (reading, stop) = (&reading, &stop);
                scope.spawn(move || {
                    let mut refused = Vec::new();
                    while !stop.load(Ordering::SeqCst) {
                        if let Err(error) = read(reading.as_ref()) {
                            refused.push(error.to_string());
                        }
                    }
                    refused
                })
            })
            .collect::<Vec<_>>();
        std::thread::sleep(std::time::Duration::from_millis(200));
        let mut writer = Vec::new();
        for index in 0..40 {
            if let Err(error) = store.commit(&created(&format!("burst-{index}"))) {
                writer.push(error.to_string());
            }
        }
        stop.store(true, Ordering::SeqCst);
        let mut reads = Vec::new();
        for reader in readers {
            reads.extend(reader.join().expect("a reader"));
        }
        (writer, reads)
    })
}

/// A command's write lands while view reads beyond the pool's queue run beside it: the reads
/// share the writer's pool, so none may take the writer's place in it.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_burst_of_view_reads_never_refuses_the_writer() {
    let (writer, _) = burst();
    assert!(
        writer.is_empty(),
        "{} of 40 commits were refused while {READERS} reads ran beside them, first: {:?}",
        writer.len(),
        writer.first()
    );
}

/// Every view read of a burst answers: a read beyond the pool waits for a connection, as a
/// SQLite reading handle's read waits for one, rather than fail.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_burst_of_view_reads_answers_every_read() {
    let (_, reads) = burst();
    assert!(
        reads.is_empty(),
        "{} reads were refused while {READERS} ran at once, first: {:?}",
        reads.len(),
        reads.first()
    );
}

/// A view reading fields through a reference reads a position of the handle, which loads rows
/// through the same pool: those reads count against the same cap as scans, so neither the writer
/// nor any of them is refused.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_burst_of_reads_through_positions_refuses_neither_side() {
    let (writer, reads) = burst_of_positions();
    assert!(
        writer.is_empty() && reads.is_empty(),
        "{} of 40 commits and {} position reads were refused while {READERS} ran at once, \
         first: {:?}",
        writer.len(),
        reads.len(),
        writer.first().or(reads.first())
    );
}
