//! The events published, read as a range after a cursor (`story:events-range-read`): each page
//! is at most its limit, in the order the whole list has them, and its cursor resumes after its
//! last event, on the host's thread and beside it, across a reset and a restart. With the cursor
//! at the end, a read reads no event from any store, however long the log is.
//!
//! The console fixture declares two components: `board`, which a request reaches, and `ledger`,
//! which only a binding reaches.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Cursor, Host, ManualClock, Published, Reader, Stores};
use spindle_kernel::{
    Change, Committed, Event, MemoryStore, Origin, Position, Reading, Removal, Scan, Scanned,
    Store, StoreError, Stored, Write,
};

/// How many events the long log holds.
const LONG: usize = 10_000;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn tasks() -> ess_compiler::EssIr {
    spindle_spec::load(&root().join("crates/spindle-cli/tests/fixtures/ingress"))
        .expect("the console fixture compiles")
        .ir
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

fn bound(store: &str) -> Environment {
    Environment::parse(&format!(
        concat!(
            "format = \"spindle-environment/1\"\n",
            "name = \"range-{store}\"\n",
            "os = \"linux\"\n",
            "arch = \"x86_64\"\n",
            "\n",
            "[provides]\n",
            "\"store.durable\" = [\"{store}\"]\n",
            "\"store.ephemeral\" = [\"{store}\"]\n",
            "clock = [\"system\"]\n",
            "timer = [\"tokio\"]\n",
            "channel = [\"in-process\"]\n",
            "projection = [\"in-process\"]\n",
            "\"ingress.network\" = [\"http-loopback\"]\n",
        ),
        store = store
    ))
    .expect("the environment parses")
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("laptop.toml reads");
    Environment::parse(&text).expect("laptop.toml is an environment")
}

fn submit(host: &mut Host, command: &str, input: Value) {
    let report = host
        .submit(command, object(input))
        .unwrap_or_else(|error| panic!("{command}: {error}"));
    assert!(report.failed.is_empty(), "{command}: {:?}", report.failed);
}

/// Two tasks opened and the first closed, which the ledger records: four events over both
/// components.
fn script(host: &mut Host) {
    submit(
        host,
        "tasks.board.OpenTask",
        json!({"task_id": "t1", "title": "First"}),
    );
    submit(
        host,
        "tasks.board.OpenTask",
        json!({"task_id": "t2", "title": "Second"}),
    );
    submit(host, "tasks.board.CloseTask", json!({"task_id": "t1"}));
}

/// A memory store that counts every event its `events_after` reads answer, on the writer and
/// through its reading handle.
struct Counting {
    store: MemoryStore,
    read: Arc<AtomicUsize>,
}

struct CountingReading {
    reading: Arc<dyn Reading>,
    read: Arc<AtomicUsize>,
}

impl Store for Counting {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.store.load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.store.commit(change)
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.store.remove(removal)
    }

    fn commit_all(&mut self, writes: &[Write]) -> Result<(), StoreError> {
        self.store.commit_all(writes)
    }

    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError> {
        self.store.scan(entity)
    }

    fn counts(&mut self, entities: &[String]) -> Result<BTreeMap<String, usize>, StoreError> {
        self.store.counts(entities)
    }

    fn reading(&mut self) -> Option<Arc<dyn Reading>> {
        let reading = self.store.reading()?;
        Some(Arc::new(CountingReading {
            reading,
            read: Arc::clone(&self.read),
        }))
    }

    fn attribute(&mut self, origin: Option<Origin>) {
        self.store.attribute(origin);
    }

    fn commit_events(&mut self, events: &[Event], scanned: &[Scanned]) -> Result<(), StoreError> {
        self.store.commit_events(events, scanned)
    }

    fn forget_events(&mut self) -> Result<(), StoreError> {
        self.store.forget_events()
    }

    fn events_after(&mut self, position: u64, limit: usize) -> Result<Vec<Committed>, StoreError> {
        let committed = self.store.events_after(position, limit)?;
        self.read.fetch_add(committed.len(), Ordering::SeqCst);
        Ok(committed)
    }
}

impl Reading for CountingReading {
    fn scan(&self, entity: &str) -> Result<Scan, StoreError> {
        self.reading.scan(entity)
    }

    fn counts(&self, entities: &[String]) -> Result<BTreeMap<String, usize>, StoreError> {
        self.reading.counts(entities)
    }

    fn events_after(&self, position: u64, limit: usize) -> Result<Vec<Committed>, StoreError> {
        let committed = self.reading.events_after(position, limit)?;
        self.read.fetch_add(committed.len(), Ordering::SeqCst);
        Ok(committed)
    }

    fn at(&self, entities: &[String]) -> Result<Box<dyn Position + '_>, StoreError> {
        self.reading.at(entities)
    }
}

/// A host on counting memory stores, the board's holding `events` events already, one write and
/// one command each, and how many events its stores' reads have answered.
fn counted(events: usize) -> (Host, Arc<AtomicUsize>) {
    let read = Arc::new(AtomicUsize::new(0));
    let mut board = MemoryStore::default();
    for index in 0..events {
        let sequence = u64::try_from(index).expect("an index fits") + 1;
        board.attribute(Some(Origin {
            command: "tasks.board.OpenTask".to_owned(),
            sequence,
        }));
        board
            .commit_events(
                &[Event {
                    name: "tasks.board.TaskOpened".to_owned(),
                    payload: object(json!({"task_id": format!("t{index}"), "title": "Old"})),
                }],
                &[],
            )
            .expect("the board records the event");
    }
    board.attribute(None);
    let given: Stores = [
        (
            "board".to_owned(),
            Box::new(Counting {
                store: board,
                read: Arc::clone(&read),
            }) as Box<dyn Store>,
        ),
        (
            "ledger".to_owned(),
            Box::new(Counting {
                store: MemoryStore::default(),
                read: Arc::clone(&read),
            }) as Box<dyn Store>,
        ),
    ]
    .into_iter()
    .collect();
    let host = Host::supplied(
        tasks(),
        &bound("elsewhere"),
        Arc::new(ManualClock::new()),
        None,
        given,
    )
    .expect("the host on counting stores opens");
    (host, read)
}

/// A host on counting memory stores holding `events` events between them, one write and one
/// command each, every third the ledger's and the rest the board's, so their sequences
/// interleave; and how many events each store's reads have answered, the board's first.
fn counted_apart(events: usize) -> (Host, [Arc<AtomicUsize>; 2]) {
    let reads = [Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0))];
    let (mut board, mut ledger) = (MemoryStore::default(), MemoryStore::default());
    for index in 0..events {
        let sequence = u64::try_from(index).expect("an index fits") + 1;
        let store = if index % 3 == 0 {
            &mut ledger
        } else {
            &mut board
        };
        store.attribute(Some(Origin {
            command: "tasks.board.OpenTask".to_owned(),
            sequence,
        }));
        store
            .commit_events(
                &[Event {
                    name: "tasks.board.TaskOpened".to_owned(),
                    payload: object(json!({"task_id": format!("t{index}"), "title": "Old"})),
                }],
                &[],
            )
            .expect("the store records the event");
    }
    board.attribute(None);
    ledger.attribute(None);
    let given: Stores = [("board", board, &reads[0]), ("ledger", ledger, &reads[1])]
        .into_iter()
        .map(|(name, store, read)| {
            (
                name.to_owned(),
                Box::new(Counting {
                    store,
                    read: Arc::clone(read),
                }) as Box<dyn Store>,
            )
        })
        .collect();
    let host = Host::supplied(
        tasks(),
        &bound("elsewhere"),
        Arc::new(ManualClock::new()),
        None,
        given,
    )
    .expect("the host on counting stores opens");
    (host, reads)
}

/// `story:events-read-bounded` acceptance 2: paging through 10,000 events at `limit=100`, each
/// page reads at most 100 events from each store, on the host's thread and beside it, and the
/// pages list the whole list's events in its order, though each store answers only a part of
/// what it holds after its mark and the two stores' events interleave.
#[test]
fn a_page_reads_at_most_its_limit_from_each_store() {
    const LIMIT: usize = 100;
    let (mut host, reads) = counted_apart(LONG);
    let whole = host.published().expect("the whole list reads");
    assert_eq!(whole.len(), LONG);
    let reader = host.reader();
    for beside in [false, true] {
        let mut events = Vec::new();
        let mut cursor = Cursor::start();
        let mut pages = 0;
        loop {
            for read in &reads {
                read.store(0, Ordering::SeqCst);
            }
            let page = if beside {
                reader.published_after(&cursor, LIMIT)
            } else {
                host.published_after(&cursor, LIMIT)
            }
            .unwrap_or_else(|error| panic!("page {pages} reads: {error}"));
            let read = reads
                .iter()
                .map(|read| read.load(Ordering::SeqCst))
                .collect::<Vec<_>>();
            assert!(
                read.iter().all(|read| *read <= LIMIT),
                "page {pages} (beside: {beside}) read {read:?} events from the board and the \
                 ledger, more than {LIMIT} from one"
            );
            if page.events.is_empty() {
                break;
            }
            assert!(page.events.len() <= LIMIT);
            events.extend(page.events);
            cursor = page.next;
            pages += 1;
        }
        assert_eq!(pages, LONG / LIMIT, "beside: {beside}");
        assert!(
            events == whole,
            "beside: {beside}: the pages list the whole list in order"
        );
    }
}

/// Every page from `after` on, `limit` at most each, until one answers nothing: the events in
/// order, and the cursor the last page answered.
fn paged(host: &mut Host, after: &Cursor, limit: usize) -> (Vec<Published>, Cursor) {
    let mut events = Vec::new();
    let mut cursor = after.clone();
    loop {
        let page = host
            .published_after(&cursor, limit)
            .unwrap_or_else(|error| panic!("a page reads: {error}"));
        assert!(page.events.len() <= limit, "{} events", page.events.len());
        if page.events.is_empty() {
            assert_eq!(
                page.next, cursor,
                "an empty page answers the cursor it was given"
            );
            return (events, cursor);
        }
        events.extend(page.events);
        cursor = page.next;
    }
}

/// [`paged`] through `reader`, beside the host's thread.
fn paged_beside(reader: &Reader, after: &Cursor, limit: usize) -> (Vec<Published>, Cursor) {
    let mut events = Vec::new();
    let mut cursor = after.clone();
    loop {
        let page = reader
            .published_after(&cursor, limit)
            .unwrap_or_else(|error| panic!("a page reads beside: {error}"));
        assert!(page.events.len() <= limit, "{} events", page.events.len());
        if page.events.is_empty() {
            assert_eq!(
                page.next, cursor,
                "an empty page answers the cursor it was given"
            );
            return (events, cursor);
        }
        events.extend(page.events);
        cursor = page.next;
    }
}

/// Each page of every limit, read from the start, lists the whole list's events in its order,
/// and so does every page read beside the host's thread; a cursor from one is read by the other.
#[test]
fn pages_list_the_events_in_the_order_of_the_whole_list() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = Host::new(
        tasks(),
        &laptop(),
        Arc::new(ManualClock::new()),
        state.path(),
    )
    .expect("the SQLite host opens");
    script(&mut host);
    let whole = host.published().expect("the whole list reads");
    assert_eq!(whole.len(), 4, "{whole:?}");
    for limit in 1..=5 {
        let (events, _) = paged(&mut host, &Cursor::start(), limit);
        assert_eq!(events, whole, "limit {limit}");
        let reader = host.reader();
        let (beside, _) = paged_beside(&reader, &Cursor::start(), limit);
        assert_eq!(beside, whole, "limit {limit}, beside the host's thread");
    }
    let first = host
        .published_after(&Cursor::start(), 2)
        .expect("the first page reads");
    assert_eq!(first.events, whole[..2]);
    let reader = host.reader();
    let rest = reader
        .published_after(&first.next, 10)
        .expect("the rest reads beside");
    assert_eq!(
        rest.events,
        whole[2..],
        "the reader resumes the host's cursor"
    );
}

/// A cursor is text that reads back as itself; text that is not one is refused.
#[test]
fn a_cursor_is_opaque_text_that_reads_back_as_itself() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = Host::new(
        tasks(),
        &laptop(),
        Arc::new(ManualClock::new()),
        state.path(),
    )
    .expect("the SQLite host opens");
    script(&mut host);
    let page = host
        .published_after(&Cursor::start(), 3)
        .expect("a page reads");
    let text = page.next.to_string();
    assert!(
        text.bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'),
        "the cursor needs no escaping in a query string: {text}"
    );
    assert_eq!(Cursor::parse(&text), Ok(page.next.clone()));
    assert_eq!(
        Cursor::parse(&Cursor::start().to_string()),
        Ok(Cursor::start())
    );
    for malformed in ["", "x", "!!!!", "e30", &format!("{text}x"), &text[1..]] {
        assert!(
            Cursor::parse(malformed).is_err(),
            "{malformed:?} is not a cursor"
        );
    }
}

/// A cursor taken before a restart resumes after it: on SQLite, nothing is answered again, and
/// what was published after the restart is answered.
#[test]
fn a_cursor_resumes_after_a_restart() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let open = || {
        Host::new(
            tasks(),
            &laptop(),
            Arc::new(ManualClock::new()),
            state.path(),
        )
        .expect("the SQLite host opens")
    };
    let mut host = open();
    script(&mut host);
    let (before, cursor) = paged(&mut host, &Cursor::start(), 3);
    assert_eq!(before.len(), 4);
    let text = cursor.to_string();
    drop(host);

    let mut again = open();
    let cursor = Cursor::parse(&text).expect("the cursor reads after the restart");
    let idle = again
        .published_after(&cursor, 10)
        .expect("the idle page reads");
    assert_eq!(
        idle.events,
        Vec::<Published>::new(),
        "nothing is answered again"
    );
    submit(
        &mut again,
        "tasks.board.OpenTask",
        json!({"task_id": "t3", "title": "Third"}),
    );
    let page = again
        .published_after(&idle.next, 10)
        .expect("the page after the restart reads");
    let names = page
        .events
        .iter()
        .map(|published| published.event.payload["task_id"].clone())
        .collect::<Vec<_>>();
    assert_eq!(names, [json!("t3")], "only what was published since");
    let stale = again
        .published_after(&cursor, 10)
        .expect("the cursor from before the restart still reads");
    assert_eq!(stale.events, page.events);
}

/// A reset replaces a memory or SQLite store with an empty one, whose positions start over: a
/// cursor taken before it still answers every event published after it.
#[test]
fn a_cursor_resumes_after_a_reset() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let hosts = [
        (
            "sqlite",
            Host::new(
                tasks(),
                &laptop(),
                Arc::new(ManualClock::new()),
                state.path(),
            )
            .expect("the SQLite host opens"),
        ),
        (
            "memory",
            Host::new(
                tasks(),
                &bound("memory"),
                Arc::new(ManualClock::new()),
                state.path(),
            )
            .expect("the memory host opens"),
        ),
        ("supplied", counted(0).0),
    ];
    for (name, mut host) in hosts {
        script(&mut host);
        submit(
            &mut host,
            "tasks.board.OpenTask",
            json!({"task_id": "t3", "title": "Third"}),
        );
        let (before, cursor) = paged(&mut host, &Cursor::start(), 100);
        assert_eq!(before.len(), 5, "{name}");
        host.reset().expect("the reset empties every store");
        submit(
            &mut host,
            "tasks.board.OpenTask",
            json!({"task_id": "t9", "title": "After"}),
        );
        let page = host
            .published_after(&cursor, 100)
            .expect("the page after the reset reads");
        let ids = page
            .events
            .iter()
            .map(|published| published.event.payload["task_id"].clone())
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            [json!("t9")],
            "{name}: the event published after the reset"
        );
        let reader = host.reader();
        let beside = reader
            .published_after(&cursor, 100)
            .expect("the page after the reset reads beside");
        assert_eq!(
            beside.events, page.events,
            "{name}: beside the host's thread"
        );
    }
}

/// `story:events-range-read` acceptance 3: with the cursor at the end of a log of 10,000 events,
/// a read answers nothing and the same cursor, and reads no event from any store, on the host's
/// thread and beside it. The bound: an idle read reads 0 events whatever the log's length, and a
/// read after one more command reads that command's events and no other.
#[test]
fn an_idle_read_reads_no_event_of_a_long_log() {
    let (mut host, read) = counted(LONG);
    let (events, end) = paged(&mut host, &Cursor::start(), 1_000);
    assert_eq!(events.len(), LONG);

    read.store(0, Ordering::SeqCst);
    let idle = host
        .published_after(&end, 1_000)
        .expect("the idle read reads");
    assert_eq!(idle.events, Vec::<Published>::new());
    assert_eq!(idle.next, end, "the same cursor");
    assert_eq!(
        idle.next.to_string(),
        end.to_string(),
        "the same cursor text"
    );
    assert_eq!(
        read.load(Ordering::SeqCst),
        0,
        "an idle read on the host's thread reads no event of {LONG}"
    );
    let reader = host.reader();
    let beside = reader
        .published_after(&end, 1_000)
        .expect("the idle read beside reads");
    assert_eq!(beside.events, Vec::<Published>::new());
    assert_eq!(beside.next, end);
    assert_eq!(
        read.load(Ordering::SeqCst),
        0,
        "an idle read beside the host's thread reads no event of {LONG}"
    );

    submit(
        &mut host,
        "tasks.board.OpenTask",
        json!({"task_id": "new", "title": "New"}),
    );
    read.store(0, Ordering::SeqCst);
    let page = reader
        .published_after(&end, 1_000)
        .expect("the read after one command reads");
    assert_eq!(page.events.len(), 1, "{:?}", page.events);
    assert_eq!(page.events[0].event.payload["task_id"], json!("new"));
    assert_eq!(
        read.load(Ordering::SeqCst),
        1,
        "the read after one command reads its one event"
    );
}
