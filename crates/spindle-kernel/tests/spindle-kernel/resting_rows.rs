//! A scan of a SQLite store costs the resting rows of its entity, not the whole log
//! (`story:view-read-without-log-scan`).
//!
//! A downstream consumer on 0.12.1 measured two view reads over entities of tens of rows at about
//! 480 ms each on a 142 MB state: every `scan` read the tenant's whole feed and kept the entity's
//! events. Each case here holds one part of the fix against the whole-feed scan itself, which
//! [`whole_feed`] reads on a connection of its own exactly as `SqliteStore::scan` read it before:
//!
//! - a view read of 40 rows beside more than 100,000 events answers in under 20 ms (median of 5,
//!   in a debug build), the whole-feed time printed beside it;
//! - every scan, on the store, its reading handle and a second handle on the same file, answers
//!   what the whole feed answers after any drawn sequence of writes, refused groups, writes by the
//!   other handle, restarts and resets, on a new file and on one the 0.10.0 release wrote;
//! - opening a log of 1,300 rows of 1 MiB holds the rows that rest, not their history.
//!
//! `tests/fixtures/released-0.10.0/books.sqlite` was written by the 0.10.0 release binary
//! (`b10x-spindle-0.10.0-x86_64-unknown-linux-gnu.tar.gz`, SHA-256 `df4d1b2c…dd3f2`, checked
//! against the release's `SHA256SUMS`): `run tests/fixtures/deletes --environment
//! examples/environments/laptop.toml --state books.sqlite` on `first.jsonl`, then again on
//! `second.jsonl` beside it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
use eventlog_core::{CommandMeta, Expected, NewEvent, new_event_id, request_hash};
use eventlog_core::{EventStore, StreamId, TenantId};
use eventlog_sqlite::SqliteEventStore;
use serde_json::{Map, Value, json};
use spindle_kernel::{
    Change, Event, Reading, Removal, Row, Scan, Scanned, SqliteStore, Store, StoreError, Stored,
    Write,
};

/// What the whole feed answers for every entity it holds a row event of, and how many events it
/// read.
struct Feed {
    scans: BTreeMap<String, Scan>,
    events: u64,
}

impl Feed {
    fn scan(&self, entity: &str) -> Scan {
        self.scans.get(entity).cloned().unwrap_or_default()
    }

    fn counts(&self, entities: &[String]) -> BTreeMap<String, usize> {
        entities
            .iter()
            .map(|entity| (entity.clone(), self.scan(entity).rows.len()))
            .collect()
    }
}

/// Reads the file at `path` on a connection of its own as `SqliteStore::scan` read it before this
/// story: the entity's version first (`spindle:entity` stream), then the tenant's whole feed in
/// pages of 500, keeping each stream's last `spindle.row` unless a `spindle.removed` follows it.
/// `entities` names those whose version is read where no row event names them.
fn whole_feed(path: &Path, entities: &[String]) -> Feed {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a runtime");
    let log = runtime
        .block_on(SqliteEventStore::open_existing(
            path.to_str().expect("a UTF-8 path"),
            "spindle",
        ))
        .expect("the log opens");
    let tenant = TenantId::new("local").expect("a tenant");
    let mut versions = BTreeMap::<String, u64>::new();
    let mut named = entities.to_vec();
    let mut latest = BTreeMap::<String, BTreeMap<String, Stored>>::new();
    let mut events = 0;
    let mut position = 0;
    // The versions are read before the feed, as the scan read them.
    let version_of = |entity: &str| {
        let stream = StreamId::new(tenant.clone(), "spindle:entity", entity).expect("a stream");
        runtime
            .block_on(log.stream_version(&stream))
            .expect("the entity version reads")
            .unwrap_or(0)
    };
    for entity in &named {
        versions.insert(entity.clone(), version_of(entity));
    }
    loop {
        let page = runtime
            .block_on(log.read_feed(&tenant, position, 500))
            .expect("the feed reads");
        for event in page.events {
            events += 1;
            let rows = latest.entry(event.stream_type.clone()).or_default();
            match event.name.as_str() {
                "spindle.removed" => {
                    rows.remove(&event.stream_id);
                }
                "spindle.row" => {
                    let row = serde_json::from_value::<Row>(event.data).expect("a stored row");
                    rows.insert(
                        event.stream_id,
                        Stored {
                            row,
                            version: event.version,
                        },
                    );
                }
                _ => {}
            }
        }
        if !page.has_more {
            break;
        }
        position = page.next_position;
    }
    for entity in latest.keys() {
        if !versions.contains_key(entity) {
            named.push(entity.clone());
            versions.insert(entity.clone(), version_of(entity));
        }
    }
    let scans = named
        .iter()
        .map(|entity| {
            let rows = latest
                .remove(entity)
                .unwrap_or_default()
                .into_iter()
                .map(|(identity, stored)| {
                    (
                        serde_json::from_str::<Value>(&identity).expect("an identity"),
                        stored,
                    )
                })
                .collect();
            (
                entity.clone(),
                Scan {
                    rows,
                    version: versions[entity],
                },
            )
        })
        .collect();
    Feed { scans, events }
}

fn row(state: &str, fields: Map<String, Value>) -> Row {
    Row {
        state: state.to_owned(),
        fields: fields.into_iter().collect(),
    }
}

fn titled(state: &str, title: &str) -> Row {
    row(state, Map::from_iter([("title".to_owned(), json!(title))]))
}

fn events(count: usize, from: usize) -> Vec<Event> {
    (0..count)
        .map(|index| Event {
            name: "probe.rest.Happened".to_owned(),
            payload: Map::from_iter([("n".to_owned(), json!(from + index))]),
        })
        .collect()
}

fn create(entity: &str, identity: Value, row: Row, emitted: Vec<Event>) -> Write {
    Write::Commit(Change {
        entity: entity.to_owned(),
        identity,
        expected: None,
        row,
        events: emitted,
        scanned: Vec::new(),
    })
}

fn median(mut durations: Vec<Duration>) -> Duration {
    durations.sort();
    durations[durations.len() / 2]
}

fn timed<T>(each: impl FnOnce() -> T) -> (T, Duration) {
    let started = Instant::now();
    let answer = each();
    (answer, started.elapsed())
}

const VIEWED: &str = "probe.rest.Item";
const NOISE: &str = "probe.rest.Tick";

/// A view read of the 40 rows of one entity, beside more than 100,000 events of another entity,
/// answers in under 20 ms at the median of 5 on the store and on its reading handle, as the
/// host's view read reads it. The whole-feed scan the store answered before is timed beside it
/// and printed: the cost the read no longer pays.
#[test]
fn a_view_read_of_forty_rows_beside_a_hundred_thousand_events_answers_in_under_20_ms() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = directory.path().join("large.sqlite");
    {
        let mut store = SqliteStore::open(&path).expect("SQLite opens");
        // 10 groups of 50 rows, each row rewritten with 199 events: about 100,500 events.
        for group in 0..10 {
            let writes = (0..50)
                .map(|index| {
                    create(
                        NOISE,
                        json!(format!("tick-{group}-{index}")),
                        titled("Open", "noise"),
                        events(199, index),
                    )
                })
                .collect::<Vec<_>>();
            store.commit_all(&writes).expect("the noise lands");
        }
        let viewed = (0..40)
            .map(|index| {
                create(
                    VIEWED,
                    json!(format!("item-{index:02}")),
                    titled("Open", "viewed"),
                    events(1, index),
                )
            })
            .collect::<Vec<_>>();
        store.commit_all(&viewed).expect("the viewed rows land");
    }
    let named = [VIEWED.to_owned(), NOISE.to_owned()];
    let (feed, whole) = timed(|| whole_feed(&path, &named));
    assert!(
        feed.events >= 100_000,
        "the log holds {} events, fewer than 100,000",
        feed.events
    );
    let expected = feed.scan(VIEWED);
    assert_eq!(expected.rows.len(), 40, "the viewed entity rests 40 rows");
    let mut wholes = vec![whole];
    for _ in 0..2 {
        wholes.push(timed(|| whole_feed(&path, &named)).1);
    }

    let (opened, open) = timed(|| SqliteStore::open(&path).expect("SQLite reopens"));
    let mut store = opened;
    let reading = store.reading().expect("a reading handle");
    let mut on_store = Vec::new();
    let mut on_reading = Vec::new();
    for _ in 0..5 {
        let (scan, took) = timed(|| store.scan(VIEWED).expect("the store scans"));
        assert_eq!(
            scan, expected,
            "the store scans what the whole feed answers"
        );
        on_store.push(took);
        let (scan, took) = timed(|| reading.scan(VIEWED).expect("the handle scans"));
        assert_eq!(
            scan, expected,
            "the handle scans what the whole feed answers"
        );
        on_reading.push(took);
    }
    let (whole, on_store, on_reading) = (median(wholes), median(on_store), median(on_reading));
    eprintln!(
        "{} events, {} rows read: whole-feed scan (the read before) {whole:?} median of 3; \
         open {open:?}; view read on the store {on_store:?}, on its reading handle \
         {on_reading:?}, median of 5",
        feed.events,
        expected.rows.len()
    );
    let cap = Duration::from_millis(20);
    assert!(
        on_store < cap && on_reading < cap,
        "a view read of {} rows took {on_store:?} on the store and {on_reading:?} on its \
         reading handle at the median, not under {cap:?}; the whole feed of {} events reads in \
         {whole:?}",
        expected.rows.len(),
        feed.events
    );
}

/// Opening a store reads its log once, and nothing after that reads it again: counts, scans of
/// every entity on the store and its reading handle read no event, a write's own events are read
/// once after it lands, and another handle's write is read once by the next scan.
#[test]
fn opening_reads_the_log_once_and_each_read_after_only_what_was_appended() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = directory.path().join("once.sqlite");
    {
        let mut store = SqliteStore::open(&path).expect("SQLite opens");
        for group in 0..4 {
            let writes = (0..25)
                .map(|index| {
                    create(
                        if index % 2 == 0 { VIEWED } else { NOISE },
                        json!(format!("r-{group}-{index}")),
                        titled("Open", "once"),
                        events(20, index),
                    )
                })
                .collect::<Vec<_>>();
            store.commit_all(&writes).expect("the rows land");
        }
    }
    let entities = [VIEWED.to_owned(), NOISE.to_owned()];
    let logged = whole_feed(&path, &entities).events;
    let mut store = SqliteStore::open(&path).expect("SQLite reopens");
    assert_eq!(
        store.events_read(),
        logged,
        "opening reads each of the log's {logged} events once"
    );
    let reading = store.reading().expect("a reading handle");
    for _ in 0..3 {
        store.counts(&entities).expect("the store counts");
        reading.counts(&entities).expect("the handle counts");
        for entity in &entities {
            store.scan(entity).expect("the store scans");
            reading.scan(entity).expect("the handle scans");
        }
    }
    assert_eq!(
        store.events_read(),
        logged,
        "counts and scans after opening read no event of the log again"
    );
    store
        .commit_all(&[create(
            VIEWED,
            json!("late"),
            titled("Open", "late"),
            events(2, 0),
        )])
        .expect("a late row lands");
    // Two events, the row and the entity's `spindle.written`.
    assert_eq!(
        store.events_read(),
        logged + 4,
        "a write's own four events are read once"
    );
    let mut other = SqliteStore::open(&path).expect("a second handle opens");
    other
        .commit_all(&[create(
            NOISE,
            json!("elsewhere"),
            titled("Open", "elsewhere"),
            events(1, 0),
        )])
        .expect("the second handle's row lands");
    let scan = reading.scan(NOISE).expect("the handle scans");
    assert!(
        scan.rows
            .iter()
            .any(|(identity, _)| identity == &json!("elsewhere")),
        "the reading handle reads the second handle's row"
    );
    store.scan(NOISE).expect("the store scans");
    assert_eq!(
        store.events_read(),
        logged + 4 + 3,
        "the second handle's three events are read once, by the first read after them"
    );
}

/// Reads on the reading handle beside the writer see each write group whole, a group longer
/// than one page of the feed included: the writer rewrites two rows together, each with 300
/// events, and every read finds both at the same round, its entity version never going back.
#[test]
fn a_read_beside_the_writer_sees_each_group_whole_across_pages_of_the_feed() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut store = SqliteStore::open(&directory.path().join("pairs.sqlite")).expect("opens");
    let reading = store.reading().expect("a reading handle");
    let pair = [json!("pair-0"), json!("pair-1")];
    let done = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|scope| {
        let readers = (0..3)
            .map(|_| {
                let (reading, pair, done) = (&reading, &pair, &done);
                scope.spawn(move || {
                    let (mut reads, mut version) = (0, 0);
                    while !done.load(std::sync::atomic::Ordering::SeqCst) {
                        let scan = reading.scan(VIEWED).expect("the handle scans");
                        assert!(scan.version >= version, "the entity's version went back");
                        version = scan.version;
                        let titles = scan
                            .rows
                            .iter()
                            .filter(|(identity, _)| pair.contains(identity))
                            .map(|(_, stored)| stored.row.fields["title"].clone())
                            .collect::<Vec<_>>();
                        assert!(
                            titles.is_empty() || (titles.len() == 2 && titles[0] == titles[1]),
                            "a read saw part of a group: {titles:?}"
                        );
                        reads += 1;
                    }
                    reads
                })
            })
            .collect::<Vec<_>>();
        for round in 0..20 {
            let writes = pair
                .iter()
                .map(|identity| {
                    let expected = store
                        .load(VIEWED, identity)
                        .expect("the store loads")
                        .map(|stored| stored.version);
                    Write::Commit(Change {
                        entity: VIEWED.to_owned(),
                        identity: identity.clone(),
                        expected,
                        row: titled("Open", &format!("round {round}")),
                        events: events(300, round),
                        scanned: Vec::new(),
                    })
                })
                .collect::<Vec<_>>();
            store.commit_all(&writes).expect("the pair lands");
        }
        done.store(true, std::sync::atomic::Ordering::SeqCst);
        let reads = readers
            .into_iter()
            .map(|reader| reader.join().expect("a reader"))
            .sum::<usize>();
        assert!(reads > 0, "the readers read");
    });
}

/// A small deterministic generator, so a failing draw is replayed from its seed.
struct Draw(u64);

impl Draw {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.next() % bound as u64).expect("a small number")
    }

    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    fn pick<'a, T>(&mut self, from: &'a [T]) -> &'a T {
        &from[self.below(from.len())]
    }
}

const DRAWN: [&str; 3] = ["probe.rest.Task", "probe.rest.Note", "probe.rest.Tag"];

fn identities() -> Vec<Value> {
    vec![
        json!("t1"),
        json!("t2"),
        json!("t10"),
        json!("a b"),
        json!(7),
        json!(-1),
        json!({"k": 1}),
        json!(["a", 2]),
    ]
}

/// The store under test, its reading handle and a second handle on the same file, as another
/// process holds it.
struct Handles {
    path: PathBuf,
    store: SqliteStore,
    reading: Arc<dyn Reading>,
    other: SqliteStore,
}

impl Handles {
    fn open(path: &Path) -> Self {
        let mut store = SqliteStore::open(path).expect("SQLite opens");
        let reading = store.reading().expect("a reading handle");
        let other = SqliteStore::open(path).expect("a second handle opens");
        Self {
            path: path.to_owned(),
            store,
            reading,
            other,
        }
    }
}

/// One write drawn for `store`: a creation, a change or a removal of an instance, at the version
/// it holds or, now and then, at one it does not; with events, and now and then conditioned on a
/// scan of an entity, fresh or stale.
fn drawn_write(draw: &mut Draw, store: &mut SqliteStore, entity: &str, identity: &Value) -> Write {
    let held = store.load(entity, identity).expect("the store loads");
    let wrong = draw.chance(8);
    let emitted = events(draw.below(3), draw.below(1_000));
    let mut scanned = Vec::new();
    if draw.chance(25) {
        let on = *draw.pick(&DRAWN);
        let version = store.scan(on).expect("the store scans").version;
        let version = if draw.chance(30) {
            version + 1
        } else {
            version
        };
        scanned.push(Scanned {
            entity: on.to_owned(),
            version,
        });
    }
    match held {
        Some(stored) if draw.chance(30) => Write::Remove(Removal {
            entity: entity.to_owned(),
            identity: identity.clone(),
            expected: if wrong {
                stored.version + 3
            } else {
                stored.version
            },
            events: emitted,
            scanned: Vec::new(),
        }),
        held => {
            let expected = match (held, wrong) {
                (Some(stored), false) => Some(stored.version),
                (Some(_), true) => None,
                (None, false) => None,
                (None, true) => Some(1),
            };
            let state = *draw.pick(&["Open", "Done", "Held"]);
            Write::Commit(Change {
                entity: entity.to_owned(),
                identity: identity.clone(),
                expected,
                row: titled(state, &format!("title {}", draw.below(100))),
                events: emitted,
                scanned,
            })
        }
    }
}

/// A group of one to three writes of distinct instances, committed by `store`: it lands whole or
/// is refused whole, and any other failure fails the case.
fn drawn_group(draw: &mut Draw, store: &mut SqliteStore, entities: &[String]) {
    let identities = identities();
    let mut picked = Vec::<(String, Value)>::new();
    for _ in 0..=draw.below(3) {
        let entity = draw.pick(entities).clone();
        let identity = draw.pick(&identities).clone();
        if !picked.contains(&(entity.clone(), identity.clone())) {
            picked.push((entity, identity));
        }
    }
    let writes = picked
        .iter()
        .map(|(entity, identity)| drawn_write(draw, store, entity, identity))
        .collect::<Vec<_>>();
    match store.commit_all(&writes) {
        Ok(()) | Err(StoreError::Conflict { .. }) => {}
        Err(error) => panic!("a drawn group failed other than as a conflict: {error}"),
    }
}

/// Every scan and count of the store, its reading handle and the second handle, against the
/// whole feed of the file.
fn answers_the_whole_feed(handles: &mut Handles, entities: &[String], what: &str) -> Feed {
    let feed = whole_feed(&handles.path, entities);
    for entity in entities {
        let expected = feed.scan(entity);
        assert_eq!(
            handles.store.scan(entity).expect("the store scans"),
            expected,
            "{what}: the store scans {entity} as the whole feed does"
        );
        assert_eq!(
            handles.reading.scan(entity).expect("the handle scans"),
            expected,
            "{what}: the reading handle scans {entity} as the whole feed does"
        );
        assert_eq!(
            handles.other.scan(entity).expect("the second handle scans"),
            expected,
            "{what}: the second handle scans {entity} as the whole feed does"
        );
    }
    let counts = feed.counts(entities);
    assert_eq!(
        handles.store.counts(entities).expect("the store counts"),
        counts,
        "{what}: the store counts as the whole feed does"
    );
    assert_eq!(
        handles.reading.counts(entities).expect("the handle counts"),
        counts,
        "{what}: the reading handle counts as the whole feed does"
    );
    feed
}

/// Moves the file at `path` and its side files aside, as a reset does, so a new store opens empty
/// at the same path.
fn moved_aside(path: &Path, round: usize) {
    for suffix in ["", "-wal", "-shm"] {
        let from = PathBuf::from(format!("{}{suffix}", path.display()));
        if from.exists() {
            let to = PathBuf::from(format!("{}.aside-{round}{suffix}", path.display()));
            std::fs::rename(&from, &to).expect("a file moves aside");
        }
    }
}

/// Draws `steps` steps on the file at `path`, comparing every scan with the whole feed after each.
fn drawn_steps(seed: u64, path: &Path, entities: &[String], steps: usize) -> (usize, usize) {
    let mut draw = Draw(seed);
    let mut handles = Handles::open(path);
    answers_the_whole_feed(&mut handles, entities, &format!("seed {seed}: opened"));
    let (mut restarts, mut resets) = (0, 0);
    for step in 0..steps {
        let what = format!("seed {seed} step {step}");
        match draw.below(10) {
            0..=5 => drawn_group(&mut draw, &mut handles.store, entities),
            6 | 7 => drawn_group(&mut draw, &mut handles.other, entities),
            8 => {
                restarts += 1;
                let path = handles.path.clone();
                drop(handles);
                handles = Handles::open(&path);
            }
            _ => {
                resets += 1;
                let before = whole_feed(path, entities);
                let Handles {
                    path,
                    store,
                    reading,
                    other,
                } = handles;
                drop(other);
                drop(store);
                moved_aside(&path, resets);
                for entity in entities {
                    assert_eq!(
                        reading.scan(entity).expect("a retired handle reads"),
                        before.scan(entity),
                        "{what}: a reading handle of the store a reset closed reads it as it \
                         was closed"
                    );
                }
                handles = Handles::open(&path);
            }
        }
        answers_the_whole_feed(&mut handles, entities, &what);
    }
    (restarts, resets)
}

/// Every scan answers what the whole feed answers, on the store, its reading handle and a second
/// handle on the same file, after each step of a drawn sequence of groups of writes (creations,
/// changes, removals, creations again, refused groups, groups conditioned on fresh and stale
/// scans), writes by the second handle, restarts and resets.
#[test]
fn every_scan_answers_what_the_whole_feed_answers_through_drawn_writes_restarts_and_resets() {
    let entities = DRAWN.map(str::to_owned).to_vec();
    let (mut restarts, mut resets) = (0, 0);
    for seed in 1..=8 {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let (r, s) = drawn_steps(seed, &directory.path().join("drawn.sqlite"), &entities, 40);
        restarts += r;
        resets += s;
    }
    assert!(
        restarts > 0 && resets > 0,
        "the draws restart ({restarts}) and reset ({resets})"
    );
}

const BOOK: &str = "library.books.Book";

/// The same on a state file the 0.10.0 release wrote: its books (shelved, lent, removed, shelved
/// again, over two runs) scan as the whole feed answers them when it opens, and after drawn steps
/// on top of them.
#[test]
fn every_scan_answers_what_the_whole_feed_answers_on_a_state_the_0_10_0_release_wrote() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/released-0.10.0/books.sqlite");
    let mut entities = vec![BOOK.to_owned()];
    entities.extend(DRAWN.map(str::to_owned));
    for seed in 11..=14 {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let path = directory.path().join("books.sqlite");
        std::fs::copy(&fixture, &path).expect("the released state copies");
        let feed = whole_feed(&path, &entities);
        let books = feed.scan(BOOK);
        assert_eq!(
            books.rows.len(),
            17,
            "the release left 17 books resting: {:?}",
            books.rows
        );
        assert!(books.version > 0, "the release wrote the entity's version");
        drawn_steps(seed, &path, &entities, 30);
    }
}

/// The resident set of this process, in bytes, from `/proc/self/status`: `VmRSS` and `VmHWM`.
#[cfg(target_os = "linux")]
fn resident() -> (u64, u64) {
    let status = std::fs::read_to_string("/proc/self/status").expect("the status reads");
    let field = |name: &str| {
        status
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|rest| rest.trim().strip_suffix("kB"))
            .and_then(|kilobytes| kilobytes.trim().parse::<u64>().ok())
            .map(|kilobytes| kilobytes * 1024)
            .unwrap_or_else(|| panic!("{name} in /proc/self/status"))
    };
    (field("VmRSS:"), field("VmHWM:"))
}

#[cfg(target_os = "linux")]
const LARGE: &str = "probe.rest.Page";
/// How many rows of 1 MiB the large state writes, and how many of them rest at 1 MiB once the
/// rest are written again small.
#[cfg(target_os = "linux")]
const LARGE_ROWS: usize = 1_300;
#[cfg(target_os = "linux")]
const LARGE_RESTING: usize = 100;
#[cfg(target_os = "linux")]
const MIB: u64 = 1 << 20;

/// Writes the large state at `path` as the store writes a row, one `spindle.row` event on the
/// instance's stream, straight to the log: each request hashed by its instance and round rather
/// than its 1 MiB body, which a debug build hashes at a few MiB a second. 1,300 rows of 1 MiB,
/// then 1,200 of them again as small rows.
#[cfg(target_os = "linux")]
fn write_large_rows(path: &Path) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a runtime");
    let log = runtime
        .block_on(SqliteEventStore::open(
            path.to_str().expect("a UTF-8 path"),
            "spindle",
        ))
        .expect("the log opens");
    let tenant = TenantId::new("local").expect("a tenant");
    let page = "x".repeat(usize::try_from(MIB).expect("a size"));
    let large = row("Open", Map::from_iter([("page".to_owned(), json!(page))]));
    let small = titled("Done", "small");
    let append = |identity: usize, row: &Row, expected: Expected, round: &str| {
        let stream =
            StreamId::new(tenant.clone(), LARGE, json!(identity).to_string()).expect("a stream");
        let events = vec![
            NewEvent::new(
                "spindle.row",
                1,
                serde_json::to_value(row).expect("a row value"),
            )
            .expect("an event"),
        ];
        let meta = CommandMeta {
            idempotency_key: new_event_id(),
            request_hash: request_hash(&json!([identity, round])).expect("a hash"),
            subject: "spindle-kernel".to_owned(),
            actor: "spindle-kernel".to_owned(),
            request_id: new_event_id(),
            trace_id: new_event_id(),
            causation_id: None,
            causation_depth: 0,
            occurred_at: time::OffsetDateTime::now_utc(),
            claim: None,
        };
        runtime
            .block_on(log.append(&stream, expected, &events, &meta))
            .expect("the row is appended");
    };
    for identity in 0..LARGE_ROWS {
        append(identity, &large, Expected::NoStream, "large");
    }
    for identity in LARGE_RESTING..LARGE_ROWS {
        append(identity, &small, Expected::Exact(1), "small");
    }
}

/// Runs only when spawned by the case below, in a process of its own: opens the state its
/// environment names, scans and counts it, and prints how much the process holds.
#[cfg(target_os = "linux")]
#[test]
#[ignore = "spawned by opening_a_log_of_large_rows_holds_the_rows_that_rest_not_their_history"]
fn large_rows_child_opens_and_reads() {
    let path = std::env::var("SPINDLE_RESTING_ROWS_STATE").expect("the state to open");
    let (before, _) = resident();
    let mut store = SqliteStore::open(Path::new(&path)).expect("SQLite opens");
    let reading = store.reading().expect("a reading handle");
    let rows = store.scan(LARGE).expect("the store scans").rows.len();
    let read = reading.scan(LARGE).expect("the handle scans").rows.len();
    let counted = store.counts(&[LARGE.to_owned()]).expect("the store counts");
    assert_eq!(
        (rows, read, counted[LARGE]),
        (LARGE_ROWS, LARGE_ROWS, LARGE_ROWS)
    );
    let (held, peak) = resident();
    println!(
        "LARGE-ROWS-CHILD held {} peak {}",
        held.saturating_sub(before),
        peak.saturating_sub(before)
    );
}

/// A log of 1,300 rows of 1 MiB, 1,200 of them then written again small: more than 1.3 GiB of
/// history, 100 MiB resting. A process that opens it, scans it on the store and its reading
/// handle and counts it holds at most the resting rows and a margin afterwards, never the
/// history. Measured in a process of its own, with every allocation of 128 KiB or more mapped
/// and handed back to the system when freed (`MALLOC_MMAP_THRESHOLD_`), so the resident set
/// shows what is held.
#[cfg(target_os = "linux")]
#[test]
fn opening_a_log_of_large_rows_holds_the_rows_that_rest_not_their_history() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let path = directory.path().join("large-rows.sqlite");
    write_large_rows(&path);
    let output = std::process::Command::new(std::env::current_exe().expect("the test binary"))
        .args([
            "resting_rows::large_rows_child_opens_and_reads",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("SPINDLE_RESTING_ROWS_STATE", &path)
        .env("MALLOC_MMAP_THRESHOLD_", "131072")
        .output()
        .expect("the child runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "the child ran its case: {stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let line = stdout
        .lines()
        .find_map(|line| line.split_once("LARGE-ROWS-CHILD").map(|(_, rest)| rest))
        .expect("the child prints what it holds");
    let numbers = line
        .split_whitespace()
        .filter_map(|word| word.parse::<u64>().ok())
        .collect::<Vec<_>>();
    let (held, peak) = (numbers[0], numbers[1]);
    let history = LARGE_ROWS as u64 * MIB;
    let resting = LARGE_RESTING as u64 * MIB;
    eprintln!(
        "a log of {} MiB of row history, {} MiB resting: held {} MiB after open, scans and \
         counts, peak {} MiB",
        history / MIB,
        resting / MIB,
        held / MIB,
        peak / MIB
    );
    assert!(
        held < resting + 160 * MIB,
        "the process holds {} MiB after reading a log of {} MiB of history with {} MiB resting",
        held / MIB,
        history / MIB,
        resting / MIB
    );
}
