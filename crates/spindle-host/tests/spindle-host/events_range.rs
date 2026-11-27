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
use std::time::{Duration, SystemTime};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Cursor, Host, ManualClock, Published, Reader, Stores};
use spindle_kernel::{
    Change, Committed, Event, MemoryStore, Origin, Position, Reading, Removal, Scan, Scanned,
    SqliteStore, Store, StoreError, Stored, Write,
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

/// A store that counts every event its `events_after` reads answer, on the writer and
/// through its reading handle.
struct Counting<S> {
    store: S,
    read: Arc<AtomicUsize>,
}

struct CountingReading {
    reading: Arc<dyn Reading>,
    read: Arc<AtomicUsize>,
}

impl<S: Store> Store for Counting<S> {
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

    fn events_head(&mut self) -> Result<u64, StoreError> {
        self.store.events_head()
    }

    fn events_id(&mut self) -> Result<u64, StoreError> {
        self.store.events_id()
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

    fn events_head(&self) -> Result<u64, StoreError> {
        self.reading.events_head()
    }

    fn events_id(&self) -> Result<u64, StoreError> {
        self.reading.events_id()
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
            cause: None,
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
            cause: None,
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

/// A fixed civil time, as `run --clock <at>` gives every process: the commands of a restarted
/// host take the sequences the commands before the restart took.
fn fixed() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000)
}

/// A host of the console fixture on SQLite under `state`, on a clock fixed at [`fixed`].
fn fixed_sqlite(state: &Path) -> Host {
    Host::new(
        tasks(),
        &laptop(),
        Arc::new(ManualClock::at(fixed())),
        state,
    )
    .expect("the SQLite host opens")
}

/// `whole` without the events of `answered`, each taken out once.
fn without(whole: &[Published], answered: &[Published]) -> Vec<Published> {
    let mut left = whole.to_vec();
    for event in answered {
        let at = left
            .iter()
            .position(|held| held == event)
            .unwrap_or_else(|| panic!("{event:?} is in the whole list"));
        left.remove(at);
    }
    left
}

/// `story:events-cursor-across-restart` acceptance 1: on SQLite with a fixed clock, a cursor
/// taken before a restart reads, after it, every event the restarted host commits, once each and
/// in the order of the whole list; so does every cursor taken part of the way, which also reads
/// the events before the restart it did not answer. A command after that read is answered once
/// more, by the cursor that read answered.
#[test]
fn an_events_cursor_from_before_a_restart_reads_the_events_after_it() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = fixed_sqlite(state.path());
    script(&mut host);
    let before = host.published().expect("the whole list reads");
    assert_eq!(before.len(), 4, "{before:?}");
    let cursors = (1..=4)
        .map(|taken| {
            let page = host
                .published_after(&Cursor::start(), taken)
                .expect("a page reads");
            assert_eq!(page.events, before[..taken]);
            (taken, page.next.to_string())
        })
        .collect::<Vec<_>>();
    drop(host);

    let mut again = fixed_sqlite(state.path());
    submit(
        &mut again,
        "tasks.board.CloseTask",
        json!({"task_id": "t2"}),
    );
    let whole = again
        .published()
        .expect("the whole list reads after the restart");
    let since = without(&whole, &before);
    assert_eq!(
        since
            .iter()
            .map(|event| event.component.as_str())
            .collect::<Vec<_>>(),
        ["board", "ledger"],
        "the command after the restart published on both components: {since:?}"
    );
    let reader = again.reader();
    for (taken, text) in &cursors {
        let cursor = Cursor::parse(text).expect("the cursor reads after the restart");
        let expected = without(&whole, &before[..*taken]);
        for limit in [1, 2, 100] {
            let (events, _) = paged(&mut again, &cursor, limit);
            assert_eq!(
                events, expected,
                "the cursor after {taken} events, limit {limit}"
            );
            let (beside, _) = paged_beside(&reader, &cursor, limit);
            assert_eq!(
                beside, expected,
                "the cursor after {taken} events, limit {limit}, beside the host's thread"
            );
        }
    }

    let end = Cursor::parse(&cursors[3].1).expect("the cursor reads");
    let (events, next) = paged(&mut again, &end, 1);
    assert_eq!(events, since);
    submit(
        &mut again,
        "tasks.board.OpenTask",
        json!({"task_id": "t3", "title": "Third"}),
    );
    let (later, _) = paged(&mut again, &next, 1);
    let ids = later
        .iter()
        .map(|published| published.event.payload["task_id"].clone())
        .collect::<Vec<_>>();
    assert_eq!(ids, [json!("t3")], "the command after the read: {later:?}");
}

/// `cursor` as an earlier release wrote it: each mark without the id of its store's log.
fn without_logs(cursor: &Cursor) -> String {
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.to_string())
        .expect("a cursor is base64");
    let mut value = serde_json::from_slice::<Value>(&bytes).expect("a cursor is JSON");
    let marks = value[3].as_array_mut().expect("the marks are a list");
    for mark in marks {
        let fields = mark.as_array_mut().expect("a mark is a list");
        assert_eq!(fields.len(), 6, "a mark names its log: {fields:?}");
        fields.truncate(5);
    }
    URL_SAFE_NO_PAD.encode(value.to_string())
}

/// A cursor an earlier release wrote, whose marks name no log, is read as that release read it
/// after a restart on SQLite, on a clock that moved forward between the runs: every store from
/// its start, the events it answered skipped by their sequences, and nothing else.
#[test]
fn a_cursor_of_an_earlier_release_reads_on_after_a_restart() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = fixed_sqlite(state.path());
    script(&mut host);
    let before = host.published().expect("the whole list reads");
    let (_, end) = paged(&mut host, &Cursor::start(), 100);
    let text = without_logs(&end);
    drop(host);

    let mut again = Host::new(
        tasks(),
        &laptop(),
        Arc::new(ManualClock::at(fixed() + Duration::from_secs(3_600))),
        state.path(),
    )
    .expect("the SQLite host opens an hour later");
    submit(
        &mut again,
        "tasks.board.CloseTask",
        json!({"task_id": "t2"}),
    );
    let whole = again
        .published()
        .expect("the whole list reads after the restart");
    let cursor = Cursor::parse(&text).expect("a cursor of an earlier release reads");
    assert_eq!(cursor.to_string(), text, "it reads back as itself");
    for limit in [1, 100] {
        let (events, _) = paged(&mut again, &cursor, limit);
        assert_eq!(events, without(&whole, &before), "limit {limit}");
    }
}

/// `story:events-cursor-across-restart` acceptance 2: on memory stores with a fixed clock, the
/// cursor of the first host reads every event of the second host, whose stores start empty, each
/// with a log of its own.
#[test]
fn a_cursor_from_another_run_reads_a_new_memory_store_from_its_start() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let open = || {
        Host::new(
            tasks(),
            &bound("memory"),
            Arc::new(ManualClock::at(fixed())),
            state.path(),
        )
        .expect("the memory host opens")
    };
    let mut first = open();
    script(&mut first);
    let (before, cursor) = paged(&mut first, &Cursor::start(), 100);
    assert_eq!(before.len(), 4, "{before:?}");
    let text = cursor.to_string();
    drop(first);

    let mut second = open();
    for (task, title) in [("t3", "Third"), ("t4", "Fourth")] {
        submit(
            &mut second,
            "tasks.board.OpenTask",
            json!({"task_id": task, "title": title}),
        );
    }
    let whole = second.published().expect("the second host's list reads");
    assert_eq!(whole.len(), 2, "{whole:?}");
    let cursor = Cursor::parse(&text).expect("the cursor reads in the second host");
    for limit in [1, 100] {
        let (events, _) = paged(&mut second, &cursor, limit);
        assert_eq!(events, whole, "limit {limit}");
        let reader = second.reader();
        let (beside, _) = paged_beside(&reader, &cursor, limit);
        assert_eq!(beside, whole, "limit {limit}, beside the host's thread");
    }
}

/// `story:events-cursor-across-restart` acceptance 3: on SQLite with a fixed clock, the cursor
/// [`Host::published_head`] gives before a restart reads, after it, every event the restarted host
/// commits, once each and in order, and none from before the restart.
#[test]
fn a_head_cursor_from_before_a_restart_reads_the_events_after_it() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = fixed_sqlite(state.path());
    script(&mut host);
    let before = host.published().expect("the whole list reads");
    let head = host.published_head().expect("the head reads").to_string();
    let beside = host
        .reader()
        .published_head()
        .expect("the head reads beside")
        .to_string();
    drop(host);

    let mut again = fixed_sqlite(state.path());
    submit(
        &mut again,
        "tasks.board.CloseTask",
        json!({"task_id": "t2"}),
    );
    let whole = again
        .published()
        .expect("the whole list reads after the restart");
    let since = without(&whole, &before);
    assert_eq!(since.len(), 2, "{since:?}");
    let reader = again.reader();
    for (side, text) in [("on the host's thread", &head), ("beside it", &beside)] {
        let cursor = Cursor::parse(text).expect("the head reads after the restart");
        for limit in [1, 100] {
            let (events, _) = paged(&mut again, &cursor, limit);
            assert_eq!(events, since, "the head taken {side}, limit {limit}");
            let (read_beside, _) = paged_beside(&reader, &cursor, limit);
            assert_eq!(
                read_beside, since,
                "the head taken {side}, limit {limit}, read beside the host's thread"
            );
        }
    }
}

/// A board store at `path` whose one write, of `events` events and one command at the fixed
/// clock's sequence, opened `w1-0` to `w1-<events - 1>`.
fn board_with_one_write(path: &Path, events: usize) -> SqliteStore {
    let mut board = SqliteStore::open(path).expect("SQLite opens");
    let sequence = u64::try_from(
        fixed()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("after the epoch")
            .as_micros(),
    )
    .expect("the time fits");
    board.attribute(Some(Origin {
        command: "tasks.board.OpenTask".to_owned(),
        sequence,
        cause: None,
    }));
    let written = (0..events)
        .map(|index| Event {
            name: "tasks.board.TaskOpened".to_owned(),
            payload: object(json!({"task_id": format!("w1-{index}"), "title": "Old"})),
        })
        .collect::<Vec<_>>();
    board
        .commit_events(&written, &[])
        .expect("the board records the write");
    board.attribute(None);
    board
}

/// A host on the SQLite stores under `state`, `board` the board's, on a clock fixed at
/// [`fixed`].
fn supplied_sqlite(state: &Path, board: SqliteStore) -> Host {
    let ledger = SqliteStore::open(&state.join("ledger.sqlite")).expect("SQLite opens");
    let given: Stores = [
        ("board".to_owned(), Box::new(board) as Box<dyn Store>),
        ("ledger".to_owned(), Box::new(ledger) as Box<dyn Store>),
    ]
    .into_iter()
    .collect();
    Host::supplied(
        tasks(),
        &bound("elsewhere"),
        Arc::new(ManualClock::at(fixed())),
        None,
        given,
    )
    .expect("the host on SQLite stores opens")
}

/// A cursor taken in the middle of a write, before a restart on a fixed clock, reads the rest of
/// that write once, and then what the restarted host committed, at every limit.
#[test]
fn a_cursor_from_before_a_restart_reads_the_rest_of_a_write_it_answered_in_part() {
    const EVENTS: usize = 4;
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let path = state.path().join("board.sqlite");
    let mut host = supplied_sqlite(state.path(), board_with_one_write(&path, EVENTS));
    let before = host.published().expect("the whole list reads");
    assert_eq!(before.len(), EVENTS, "{before:?}");
    let cursors = (1..EVENTS)
        .map(|taken| {
            let page = host
                .published_after(&Cursor::start(), taken)
                .expect("a page reads");
            assert_eq!(page.events, before[..taken]);
            (taken, page.next.to_string())
        })
        .collect::<Vec<_>>();
    drop(host);

    let board = SqliteStore::open(&path).expect("SQLite opens again");
    let mut again = supplied_sqlite(state.path(), board);
    submit(
        &mut again,
        "tasks.board.OpenTask",
        json!({"task_id": "t9", "title": "After"}),
    );
    let whole = again
        .published()
        .expect("the whole list reads after the restart");
    assert_eq!(whole.len(), EVENTS + 1, "{whole:?}");
    let reader = again.reader();
    for (taken, text) in &cursors {
        let cursor = Cursor::parse(text).expect("the cursor reads after the restart");
        let expected = without(&whole, &before[..*taken]);
        for limit in 1..=EVENTS + 1 {
            let (events, _) = paged(&mut again, &cursor, limit);
            assert_eq!(
                events, expected,
                "the cursor after {taken} events, limit {limit}"
            );
            let (beside, _) = paged_beside(&reader, &cursor, limit);
            assert_eq!(
                beside, expected,
                "the cursor after {taken} events, limit {limit}, beside the host's thread"
            );
        }
    }
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

/// How many events one write of [`the_head_reads_no_event_of_a_long_log_on`]'s log commits.
const WRITE: usize = 100;

/// `story:events-head-cursor` acceptances 1 and 2 on the stores `board` and `ledger`, of the
/// store `name`: with 10,000 events committed to the board, in writes of [`WRITE`] events and
/// one command each, the cursor after the last event published is answered on the host's thread
/// and beside it reading no event of any store; a page after it answers nothing and the same
/// cursor, reading nothing; once one more command ran, a page after it answers that command's
/// event alone, reading only it.
pub(crate) fn the_head_reads_no_event_of_a_long_log_on(
    name: &str,
    mut board: impl Store + 'static,
    ledger: impl Store + 'static,
) {
    for write in 0..LONG / WRITE {
        let sequence = u64::try_from(write).expect("a write fits") + 1;
        board.attribute(Some(Origin {
            command: "tasks.board.OpenTask".to_owned(),
            sequence,
            cause: None,
        }));
        let events = (0..WRITE)
            .map(|index| Event {
                name: "tasks.board.TaskOpened".to_owned(),
                payload: object(json!({"task_id": format!("t{write}-{index}"), "title": "Old"})),
            })
            .collect::<Vec<_>>();
        board
            .commit_events(&events, &[])
            .unwrap_or_else(|error| panic!("{name}: the board records write {write}: {error}"));
    }
    board.attribute(None);
    let read = Arc::new(AtomicUsize::new(0));
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
                store: ledger,
                read: Arc::clone(&read),
            }) as Box<dyn Store>,
        ),
    ]
    .into_iter()
    .collect();
    let mut host = Host::supplied(
        tasks(),
        &bound("elsewhere"),
        Arc::new(ManualClock::new()),
        None,
        given,
    )
    .unwrap_or_else(|error| panic!("{name}: the host opens: {error}"));
    assert_eq!(
        host.published().expect("the whole list reads").len(),
        LONG,
        "{name}"
    );

    read.store(0, Ordering::SeqCst);
    let head = host
        .published_head()
        .unwrap_or_else(|error| panic!("{name}: the head reads: {error}"));
    assert_eq!(
        read.load(Ordering::SeqCst),
        0,
        "{name}: the head on the host's thread reads no event of {LONG}"
    );
    let reader = host.reader();
    let beside = reader
        .published_head()
        .unwrap_or_else(|error| panic!("{name}: the head reads beside: {error}"));
    assert_eq!(
        read.load(Ordering::SeqCst),
        0,
        "{name}: the head beside the host's thread reads no event of {LONG}"
    );
    for (side, cursor) in [("on the host's thread", &head), ("beside it", &beside)] {
        let idle = host
            .published_after(cursor, 100)
            .unwrap_or_else(|error| panic!("{name}: the page after the head reads: {error}"));
        assert_eq!(
            idle.events,
            Vec::<Published>::new(),
            "{name}: nothing after the head taken {side}"
        );
        assert_eq!(&idle.next, cursor, "{name}: the same cursor, {side}");
    }
    assert_eq!(
        read.load(Ordering::SeqCst),
        0,
        "{name}: a page after the head reads no event"
    );

    submit(
        &mut host,
        "tasks.board.OpenTask",
        json!({"task_id": "new", "title": "New"}),
    );
    for (side, cursor) in [("on the host's thread", &head), ("beside it", &beside)] {
        read.store(0, Ordering::SeqCst);
        let page = reader
            .published_after(cursor, 100)
            .unwrap_or_else(|error| panic!("{name}: the page after one command reads: {error}"));
        let ids = page
            .events
            .iter()
            .map(|published| published.event.payload["task_id"].clone())
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            [json!("new")],
            "{name}: after the head taken {side}, the one event published since"
        );
        assert_eq!(
            read.load(Ordering::SeqCst),
            1,
            "{name}: after the head taken {side}, the page reads that event alone"
        );
    }
}

/// [`the_head_reads_no_event_of_a_long_log_on`] the memory store.
#[test]
fn the_head_reads_no_event_of_a_long_log_on_memory() {
    the_head_reads_no_event_of_a_long_log_on(
        "memory",
        MemoryStore::default(),
        MemoryStore::default(),
    );
}

/// [`the_head_reads_no_event_of_a_long_log_on`] SQLite.
#[test]
fn the_head_reads_no_event_of_a_long_log_on_sqlite() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let open = |name: &str| {
        SqliteStore::open(&state.path().join(format!("{name}.sqlite"))).expect("SQLite opens")
    };
    the_head_reads_no_event_of_a_long_log_on("sqlite", open("board"), open("ledger"));
}
