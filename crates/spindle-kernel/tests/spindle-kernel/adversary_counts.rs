//! Adversary cases for `Store::counts` on SQLite (`story:metrics-read-without-scan`).
//!
//! The host counts every store's rows when it opens one, so a restart on a large state file pays
//! for `counts` once per component. The SQLite store answers it in one read of the feed; the
//! trait's default scans each entity, which reads the whole feed once per entity: the read the
//! story took out of every metrics read (42 s on a model of 125 entities). The contract case
//! `the_counts_are_the_rows_each_scan_answers` holds the numbers and passes on the default too,
//! and the latency case of `spindle-ingress` opens its host on an empty store, so nothing held
//! the one read until this case.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use spindle_kernel::{Change, Removal, Row, Scan, SqliteStore, Store, StoreError, Stored, Write};

/// How many entities the state file holds, and the rows of each.
const ENTITIES: usize = 60;
const ROWS: usize = 25;

fn entity(index: usize) -> String {
    format!("adversary.counts.E{index}")
}

fn row(title: &str) -> Row {
    Row {
        state: "Open".to_owned(),
        fields: BTreeMap::from([("title".to_owned(), json!(title))]),
    }
}

/// The time `each` takes, the fastest of three runs.
fn fastest(mut each: impl FnMut()) -> Duration {
    (0..3)
        .map(|_| {
            let started = Instant::now();
            each();
            started.elapsed()
        })
        .min()
        .expect("three runs")
}

#[test]
fn adversary_counts_sqlite_counts_every_entity_in_about_one_read_of_the_feed() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut store =
        SqliteStore::open(&directory.path().join("counts.sqlite")).expect("SQLite opens");
    let entities = (0..ENTITIES).map(entity).collect::<Vec<_>>();
    for name in &entities {
        let writes = (0..ROWS)
            .map(|index| {
                Write::Commit(Change {
                    entity: name.clone(),
                    identity: json!(format!("r{index}")),
                    expected: None,
                    row: row("a row"),
                    events: Vec::new(),
                    scanned: Vec::new(),
                })
            })
            .collect::<Vec<_>>();
        store.commit_all(&writes).expect("the rows land");
    }

    let counts = store.counts(&entities).expect("the store counts");
    assert_eq!(
        counts,
        entities
            .iter()
            .map(|name| (name.clone(), ROWS))
            .collect::<BTreeMap<_, _>>()
    );
    let scan = fastest(|| {
        store.scan(&entities[0]).expect("a scan");
    });
    let counted = fastest(|| {
        store.counts(&entities).expect("the store counts");
    });
    eprintln!("one scan {scan:?}; counts of {ENTITIES} entities {counted:?}");
    assert!(
        counted < scan * 8,
        "counting {ENTITIES} entities took {counted:?}, more than eight scans of one ({scan:?}): \
         the feed is read once per entity"
    );
}

/// A SQLite store answering `counts` by the trait's default, a scan of each entity: the mutant the
/// case above catches. Not used by any case; kept so the mutation is one line to try.
#[allow(dead_code)]
struct ByDefault(SqliteStore);

impl Store for ByDefault {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.0.load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.0.commit(change)
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.0.remove(removal)
    }

    fn commit_all(&mut self, writes: &[Write]) -> Result<(), StoreError> {
        self.0.commit_all(writes)
    }

    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError> {
        self.0.scan(entity)
    }
}
