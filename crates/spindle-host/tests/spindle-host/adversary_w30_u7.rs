//! Adversary, wave 30 unit U7 (`story:events-cursor-across-restart`): an events cursor of
//! another run against a new memory store that already reached the positions the cursor marks,
//! a reset on either side of a restart, a cursor from two runs back, an idle read after a
//! restart, and the "same cursor" an idle read answers. Round 2: the id of each store's log of
//! events (`Store::events_id`), a mark taken on an empty store, a log a state move copies, and a
//! cursor of an earlier release.
//!
//! The console fixture declares two components: `board`, which a request reaches, and `ledger`,
//! which only a binding reaches.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Cursor, Host, ManualClock, Published, Stores};
use spindle_kernel::{Event, MemoryStore, Origin, SqliteStore, Store};

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
            "name = \"adversary-{store}\"\n",
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

/// The civil time of the first run.
fn first_run() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000)
}

/// The civil time of a later run on a clock that moves forward (the system clock): an hour
/// after [`first_run`] for each run after the first.
fn later_run(runs: u64) -> SystemTime {
    first_run() + Duration::from_secs(3_600 * runs)
}

fn open(state: &Path, environment: &Environment, at: SystemTime) -> Host {
    Host::new(tasks(), environment, Arc::new(ManualClock::at(at)), state).expect("the host opens")
}

fn submit(host: &mut Host, command: &str, input: Value) {
    let report = host
        .submit(command, object(input))
        .unwrap_or_else(|error| panic!("{command}: {error}"));
    assert!(report.failed.is_empty(), "{command}: {:?}", report.failed);
}

/// Two tasks opened and the first closed, which the ledger records: four events over both
/// components (three writes on `board`, one on `ledger`).
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

/// More writes on both components than [`script`] made: every store of a host that ran it is
/// past the position a cursor taken after [`script`] marks.
fn longer_script(host: &mut Host) {
    script(host);
    submit(
        host,
        "tasks.board.OpenTask",
        json!({"task_id": "t3", "title": "Third"}),
    );
    submit(host, "tasks.board.CloseTask", json!({"task_id": "t2"}));
    submit(host, "tasks.board.CloseTask", json!({"task_id": "t3"}));
}

/// Every page from `after` on, `limit` at most each, until one answers nothing.
fn paged(host: &mut Host, after: &Cursor, limit: usize) -> (Vec<Published>, Cursor) {
    let mut events = Vec::new();
    let mut cursor = after.clone();
    loop {
        let page = host
            .published_after(&cursor, limit)
            .unwrap_or_else(|error| panic!("a page reads: {error}"));
        assert!(page.events.len() <= limit, "{} events", page.events.len());
        if page.events.is_empty() {
            return (events, page.next);
        }
        events.extend(page.events);
        cursor = page.next;
    }
}

/// The task ids of `events`, in order.
fn ids(events: &[Published]) -> Vec<Value> {
    events
        .iter()
        .map(|published| published.event.payload["task_id"].clone())
        .collect()
}

/// Acceptance 2 read literally ("the cursor of the first host reads every event of the second
/// host"), on a clock that moves forward between the runs, as the system clock does: the second
/// memory host makes more writes than the first, so each new store reaches the position the
/// cursor marks before the cursor is read. Both the cursor a page answered and the head cursor.
#[test]
fn a_new_memory_store_past_the_mark_is_read_from_its_start() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let memory = bound("memory");
    let mut first = open(state.path(), &memory, first_run());
    script(&mut first);
    let (before, end) = paged(&mut first, &Cursor::start(), 100);
    assert_eq!(before.len(), 4, "{before:?}");
    let head = first.published_head().expect("the head reads");
    let (end, head) = (end.to_string(), head.to_string());
    drop(first);

    let mut second = open(state.path(), &memory, later_run(1));
    longer_script(&mut second);
    let whole = second.published().expect("the second host's list reads");
    assert_eq!(whole.len(), 9, "{whole:?}");
    for (form, text) in [("end", &end), ("head", &head)] {
        let cursor = Cursor::parse(text).expect("the cursor reads in the second host");
        for limit in [1, 100] {
            let (events, _) = paged(&mut second, &cursor, limit);
            assert_eq!(
                ids(&events),
                ids(&whole),
                "the {form} cursor of the first host, limit {limit}"
            );
            let (beside, _) = paged_beside(&second, &cursor, limit);
            assert_eq!(
                ids(&beside),
                ids(&whole),
                "the {form} cursor of the first host, limit {limit}, beside the host's thread"
            );
        }
    }
}

/// [`a_new_memory_store_past_the_mark_is_read_from_its_start`] on the fixed clock acceptance 2
/// names (`run --clock <at>`): the second host makes more writes than the first.
#[test]
fn a_new_memory_store_past_the_mark_is_read_from_its_start_on_a_fixed_clock() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let memory = bound("memory");
    let mut first = open(state.path(), &memory, first_run());
    script(&mut first);
    let (before, end) = paged(&mut first, &Cursor::start(), 100);
    assert_eq!(before.len(), 4, "{before:?}");
    let text = end.to_string();
    drop(first);

    let mut second = open(state.path(), &memory, first_run());
    longer_script(&mut second);
    let whole = second.published().expect("the second host's list reads");
    assert_eq!(whole.len(), 9, "{whole:?}");
    let cursor = Cursor::parse(&text).expect("the cursor reads in the second host");
    for limit in [1, 100] {
        let (events, _) = paged(&mut second, &cursor, limit);
        assert_eq!(ids(&events), ids(&whole), "limit {limit}");
    }
}

/// `POST /api/reset` before a restart, on SQLite under `--state` with a clock that moves
/// forward: the reset opens an empty SQLite file, whose positions start over, and the restarted
/// host opens that file. The cursor taken before the reset reads every event committed after
/// it, also once the new file reached the positions the cursor marks.
#[test]
fn a_cursor_from_before_a_reset_and_a_restart_reads_every_event_after_the_reset() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let sqlite = laptop();
    let mut first = open(state.path(), &sqlite, first_run());
    script(&mut first);
    let (before, end) = paged(&mut first, &Cursor::start(), 100);
    assert_eq!(before.len(), 4, "{before:?}");
    let text = end.to_string();
    first.reset().expect("the reset empties every store");
    drop(first);

    let mut second = open(state.path(), &sqlite, later_run(1));
    longer_script(&mut second);
    let whole = second.published().expect("the second host's list reads");
    assert_eq!(whole.len(), 9, "only the events after the reset: {whole:?}");
    let cursor = Cursor::parse(&text).expect("the cursor reads after the restart");
    for limit in [1, 100] {
        let (events, _) = paged(&mut second, &cursor, limit);
        assert_eq!(ids(&events), ids(&whole), "limit {limit}");
    }
}

/// `POST /api/reset` after a restart, on SQLite with a clock that moves forward: a cursor of the
/// run before the restart reads every event committed after the reset, also once the new file
/// reached the positions the cursor marks.
#[test]
fn a_cursor_from_another_run_reads_every_event_after_a_reset_in_this_run() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let sqlite = laptop();
    let mut first = open(state.path(), &sqlite, first_run());
    script(&mut first);
    let (before, end) = paged(&mut first, &Cursor::start(), 100);
    assert_eq!(before.len(), 4, "{before:?}");
    let text = end.to_string();
    drop(first);

    let mut second = open(state.path(), &sqlite, later_run(1));
    second.reset().expect("the reset empties every store");
    longer_script(&mut second);
    let whole = second.published().expect("the second host's list reads");
    assert_eq!(whole.len(), 9, "only the events after the reset: {whole:?}");
    let cursor = Cursor::parse(&text).expect("the cursor reads after the restart");
    for limit in [1, 100] {
        let (events, _) = paged(&mut second, &cursor, limit);
        assert_eq!(ids(&events), ids(&whole), "limit {limit}");
    }
}

/// On SQLite with a fixed clock, a cursor at the end read after a restart with nothing new
/// answers no event, at limits 0, 1 and 100; and the cursor it answers, at limit 0 too, reads
/// the next command once, for the end and the head cursor. The mutant `head >= mark.position` → `>` in `range::resumed` fails here.
#[test]
fn an_idle_cursor_from_before_a_restart_answers_nothing_on_a_fixed_clock() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let sqlite = laptop();
    let mut first = open(state.path(), &sqlite, first_run());
    script(&mut first);
    let (_, end) = paged(&mut first, &Cursor::start(), 100);
    let head = first.published_head().expect("the head reads");
    let (end, head) = (end.to_string(), head.to_string());
    drop(first);

    let mut second = open(state.path(), &sqlite, first_run());
    for (form, text) in [("end", &end), ("head", &head)] {
        let cursor = Cursor::parse(text).expect("the cursor reads after the restart");
        for limit in [0, 1, 100] {
            let page = second
                .published_after(&cursor, limit)
                .expect("the idle page reads");
            assert_eq!(
                ids(&page.events),
                Vec::<Value>::new(),
                "the {form} cursor, limit {limit}: nothing is answered again"
            );
        }
    }
    let cursor = Cursor::parse(&end).expect("the cursor reads");
    let zero = second
        .published_after(&cursor, 0)
        .expect("a page of no event reads");
    submit(
        &mut second,
        "tasks.board.OpenTask",
        json!({"task_id": "t3", "title": "Third"}),
    );
    let (events, _) = paged(&mut second, &zero.next, 1);
    assert_eq!(ids(&events), [json!("t3")], "{events:?}");
}

/// On SQLite with a fixed clock, a cursor taken two restarts back and not read in between reads
/// the events of both later runs, once each, in order, at limits 1 and 100.
#[test]
fn a_cursor_from_two_runs_back_reads_both_runs_events_on_a_fixed_clock() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let sqlite = laptop();
    let mut first = open(state.path(), &sqlite, first_run());
    script(&mut first);
    let (_, end) = paged(&mut first, &Cursor::start(), 3);
    let text = end.to_string();
    drop(first);

    let mut second = open(state.path(), &sqlite, first_run());
    submit(
        &mut second,
        "tasks.board.OpenTask",
        json!({"task_id": "t3", "title": "Third"}),
    );
    drop(second);
    let mut third = open(state.path(), &sqlite, first_run());
    submit(
        &mut third,
        "tasks.board.CloseTask",
        json!({"task_id": "t3"}),
    );
    let cursor = Cursor::parse(&text).expect("the cursor reads two runs later");
    for limit in [1, 100] {
        let (events, _) = paged(&mut third, &cursor, limit);
        assert_eq!(
            ids(&events),
            [json!("t3"), json!("t3"), json!("t3")],
            "opened, closed and the ledger's record, limit {limit}: {events:?}"
        );
    }
}

/// `website/docs/reference/cli.md` (`GET /api/events`): "with it at the end the answer is no
/// event and the same cursor, or, for a cursor taken before a restart, a cursor of the new run
/// that reads on from the same place". A cursor at the end, read after a restart with nothing
/// new, answers no event and a cursor of the new run, which answers itself when read again and
/// reads the next command once. (Correction round 1, F3: this case asserted the same cursor; the
/// behaviour stays and the reference text changed.)
#[test]
fn a_cursor_at_the_end_answers_a_cursor_of_the_new_run_after_a_restart() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let sqlite = laptop();
    let mut first = open(state.path(), &sqlite, first_run());
    script(&mut first);
    let (_, end) = paged(&mut first, &Cursor::start(), 100);
    let text = end.to_string();
    drop(first);

    let mut second = open(state.path(), &sqlite, later_run(1));
    let cursor = Cursor::parse(&text).expect("the cursor reads after the restart");
    let page = second
        .published_after(&cursor, 100)
        .expect("the idle page reads");
    assert_eq!(ids(&page.events), Vec::<Value>::new());
    let again = second
        .published_after(&page.next, 100)
        .expect("the idle page reads again");
    assert_eq!(ids(&again.events), Vec::<Value>::new());
    assert_eq!(
        again.next, page.next,
        "the cursor of the new run at the end answers itself"
    );
    submit(
        &mut second,
        "tasks.board.OpenTask",
        json!({"task_id": "t3", "title": "Third"}),
    );
    let (events, _) = paged(&mut second, &page.next, 1);
    assert_eq!(ids(&events), [json!("t3")], "{events:?}");
}

/// [`paged`] through the host's reader, beside its thread.
fn paged_beside(host: &Host, after: &Cursor, limit: usize) -> (Vec<Published>, Cursor) {
    let reader = host.reader();
    let mut events = Vec::new();
    let mut cursor = after.clone();
    loop {
        let page = reader
            .published_after(&cursor, limit)
            .unwrap_or_else(|error| panic!("a page reads beside: {error}"));
        if page.events.is_empty() {
            return (events, page.next);
        }
        events.extend(page.events);
        cursor = page.next;
    }
}

/// One write on `store`: `events` events `TaskOpened`, task ids `<prefix>-0` on, attributed to
/// one `OpenTask` command at `sequence`.
fn one_write(store: &mut dyn Store, prefix: &str, events: usize, sequence: u64) {
    store.attribute(Some(Origin {
        command: "tasks.board.OpenTask".to_owned(),
        sequence,
        cause: None,
    }));
    let written = (0..events)
        .map(|index| Event {
            name: "tasks.board.TaskOpened".to_owned(),
            payload: object(json!({"task_id": format!("{prefix}-{index}"), "title": "Old"})),
        })
        .collect::<Vec<_>>();
    store
        .commit_events(&written, &[])
        .expect("the store records the write");
    store.attribute(None);
}

/// The civil time `at` in microseconds, the sequence a command run at it takes.
fn micros(at: SystemTime) -> u64 {
    u64::try_from(
        at.duration_since(SystemTime::UNIX_EPOCH)
            .expect("after the epoch")
            .as_micros(),
    )
    .expect("the time fits")
}

/// A host on `board` and a SQLite ledger under `state`, on a clock standing at `at`.
fn supplied(state: &Path, board: Box<dyn Store>, at: SystemTime) -> Host {
    let ledger = SqliteStore::open(&state.join("ledger.sqlite")).expect("SQLite opens");
    let given: Stores = [
        ("board".to_owned(), board),
        ("ledger".to_owned(), Box::new(ledger) as Box<dyn Store>),
    ]
    .into_iter()
    .collect();
    Host::supplied(
        tasks(),
        &bound("elsewhere"),
        Arc::new(ManualClock::at(at)),
        None,
        given,
    )
    .expect("the host on the given stores opens")
}

/// The id of a SQLite store's log: `0` while it holds no event, then not `0`, the same once the
/// file is opened again, through its reading handle and past a reset that empties it in place;
/// another file holding the same events has another id.
#[test]
fn a_sqlite_log_id_is_kept_by_its_file_and_differs_between_files() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let first = state.path().join("first.sqlite");
    let mut store = SqliteStore::open(&first).expect("SQLite opens");
    assert_eq!(store.events_id().expect("the id reads"), 0, "an empty log");
    one_write(&mut store, "w", 2, 7);
    let id = store.events_id().expect("the id reads");
    assert_ne!(id, 0, "a log with an event names itself");
    let reading = store
        .reading()
        .expect("a SQLite store has a reading handle");
    assert_eq!(reading.events_id().expect("the id reads beside"), id);
    one_write(&mut store, "x", 1, 8);
    assert_eq!(
        store.events_id().expect("the id reads"),
        id,
        "a later write"
    );
    store.forget_events().expect("the events are forgotten");
    assert_eq!(
        store.events_id().expect("the id reads"),
        id,
        "a reset in place"
    );
    drop(reading);
    drop(store);
    let mut again = SqliteStore::open(&first).expect("SQLite opens again");
    assert_eq!(again.events_id().expect("the id reads"), id, "opened again");

    let mut other = SqliteStore::open(&state.path().join("other.sqlite")).expect("SQLite opens");
    one_write(&mut other, "w", 2, 7);
    let other_id = other.events_id().expect("the id reads");
    assert_ne!(other_id, 0);
    assert_ne!(other_id, id, "another file holding the same events");
}

/// The id of a memory store's log: never `0`, the same through its reading handle and past a
/// reset in place, another for another store.
#[test]
fn a_memory_log_id_is_kept_by_its_store_and_differs_between_stores() {
    let mut store = MemoryStore::default();
    let id = store.events_id().expect("the id reads");
    assert_ne!(id, 0, "an empty memory store names its log");
    one_write(&mut store, "w", 1, 7);
    assert_eq!(store.events_id().expect("the id reads"), id);
    let reading = store
        .reading()
        .expect("a memory store has a reading handle");
    assert_eq!(reading.events_id().expect("the id reads beside"), id);
    store.forget_events().expect("the events are forgotten");
    assert_eq!(
        store.events_id().expect("the id reads"),
        id,
        "a reset in place"
    );
    let mut other = MemoryStore::default();
    assert_ne!(other.events_id().expect("the id reads"), id);
}

/// On SQLite with a fixed clock, a cursor taken while `ledger` held no event (its mark names no
/// log, at position 0), the end and the head cursor, reads after a restart the ledger's first
/// events and the board's new ones, once each.
#[test]
fn a_mark_taken_on_an_empty_store_reads_its_first_events_after_a_restart() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let sqlite = laptop();
    let mut first = open(state.path(), &sqlite, first_run());
    submit(
        &mut first,
        "tasks.board.OpenTask",
        json!({"task_id": "t1", "title": "First"}),
    );
    let (before, end) = paged(&mut first, &Cursor::start(), 100);
    assert_eq!(ids(&before), [json!("t1")], "the ledger is empty");
    let head = first.published_head().expect("the head reads");
    let (end, head) = (end.to_string(), head.to_string());
    drop(first);

    let mut second = open(state.path(), &sqlite, first_run());
    submit(
        &mut second,
        "tasks.board.CloseTask",
        json!({"task_id": "t1"}),
    );
    let whole = second.published().expect("the whole list reads");
    let since = whole[1..].to_vec();
    assert_eq!(
        since
            .iter()
            .map(|event| event.component.as_str())
            .collect::<Vec<_>>(),
        ["board", "ledger"],
        "{whole:?}"
    );
    for (form, text) in [("end", &end), ("head", &head)] {
        let cursor = Cursor::parse(text).expect("the cursor reads after the restart");
        for limit in [1, 100] {
            let (events, _) = paged(&mut second, &cursor, limit);
            assert_eq!(events, since, "the {form} cursor, limit {limit}");
            let (beside, _) = paged_beside(&second, &cursor, limit);
            assert_eq!(beside, since, "the {form} cursor, limit {limit}, beside");
        }
    }
}

/// `POST /api/reset` on SQLite with `--state`, then a read in the same run while the new file is
/// still empty (its mark names no log), then writes and a restart: the cursor of that read
/// reads every event committed after it, before and after the restart, once each.
#[test]
fn a_cursor_taken_on_a_file_a_reset_emptied_reads_on_after_a_restart() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let sqlite = laptop();
    let mut first = open(state.path(), &sqlite, first_run());
    script(&mut first);
    let (_, end) = paged(&mut first, &Cursor::start(), 100);
    first.reset().expect("the reset empties every store");
    let (none, emptied) = paged(&mut first, &end, 100);
    assert_eq!(
        ids(&none),
        Vec::<Value>::new(),
        "nothing after the reset yet"
    );
    let text = emptied.to_string();
    longer_script(&mut first);
    drop(first);

    let mut second = open(state.path(), &sqlite, later_run(1));
    submit(
        &mut second,
        "tasks.board.OpenTask",
        json!({"task_id": "t9", "title": "After"}),
    );
    let whole = second.published().expect("the whole list reads");
    assert_eq!(whole.len(), 10, "{whole:?}");
    let cursor = Cursor::parse(&text).expect("the cursor reads after the restart");
    for limit in [1, 100] {
        let (events, _) = paged(&mut second, &cursor, limit);
        assert_eq!(ids(&events), ids(&whole), "limit {limit}");
    }
}

/// `state move` copies a log into PostgreSQL with new event ids, the commands' origins kept
/// (`state_move.rs`: "actor, subject, request, trace, cause and `occurred_at` are kept"), so the
/// copy names another log. Stood in for here by a second SQLite file holding the same write at
/// the same command sequence: a cursor at the end of the first, read on the copy by a later run
/// on a clock that moved forward, reads every event the copy holds again, once each and in
/// order, and loses none: the next command's events are answered after them, and once to the
/// cursor that read the copy. (Correction round 2, F1 decided C: a cursor of another log reads
/// its events again; this case asserted none was answered again.)
#[test]
fn a_cursor_from_another_log_reads_every_event_again_and_loses_none() {
    const EVENTS: usize = 3;
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let (from, to) = (state.path().join("from"), state.path().join("to"));
    std::fs::create_dir_all(&from).expect("a directory");
    std::fs::create_dir_all(&to).expect("a directory");
    let sequence = micros(first_run());

    let mut board = SqliteStore::open(&from.join("board.sqlite")).expect("SQLite opens");
    one_write(&mut board, "w", EVENTS, sequence);
    let mut first = supplied(&from, Box::new(board), first_run());
    let (before, end) = paged(&mut first, &Cursor::start(), 100);
    assert_eq!(before.len(), EVENTS, "{before:?}");
    let text = end.to_string();
    drop(first);

    let mut moved = SqliteStore::open(&to.join("board.sqlite")).expect("SQLite opens");
    one_write(&mut moved, "w", EVENTS, sequence);
    let mut second = supplied(&to, Box::new(moved), later_run(1));
    assert_eq!(
        second.published().expect("the copy reads"),
        before,
        "the copy holds the same events"
    );
    let cursor = Cursor::parse(&text).expect("the cursor reads on the copy");
    let mut read_on = None;
    for limit in [1, 2, 100] {
        let (events, next) = paged(&mut second, &cursor, limit);
        assert_eq!(
            events, before,
            "every event the copy holds, once each and in order, limit {limit}"
        );
        let (beside, _) = paged_beside(&second, &cursor, limit);
        assert_eq!(beside, before, "limit {limit}, beside the host's thread");
        read_on = Some(next);
    }
    let read_on = read_on.expect("a cursor was answered");
    submit(
        &mut second,
        "tasks.board.OpenTask",
        json!({"task_id": "t9", "title": "After"}),
    );
    let whole = second.published().expect("the copy reads");
    for limit in [1, 100] {
        let (events, _) = paged(&mut second, &cursor, limit);
        assert_eq!(ids(&events), ids(&whole), "none is lost, limit {limit}");
        let (later, _) = paged(&mut second, &read_on, limit);
        assert_eq!(ids(&later), [json!("t9")], "limit {limit}: {later:?}");
    }
}

/// `cursor` as 0.23.0 wrote it: each mark without the id of its store's log.
fn as_released(cursor: &Cursor) -> String {
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.to_string())
        .expect("a cursor is base64");
    let mut value = serde_json::from_slice::<Value>(&bytes).expect("a cursor is JSON");
    for mark in value[3].as_array_mut().expect("the marks are a list") {
        mark.as_array_mut().expect("a mark is a list").truncate(5);
    }
    URL_SAFE_NO_PAD.encode(value.to_string())
}

/// A cursor 0.23.0 gave (marks without a log id), held across an upgrade and a restart onto new
/// memory stores, on a clock that moved forward: 0.23.0 read such a store from its start and
/// skipped by the sequences; read where the new store already reached the marked positions,
/// every event of the new run is answered.
#[test]
fn a_released_cursor_reads_every_event_of_a_new_memory_store_past_its_mark() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let memory = bound("memory");
    let mut first = open(state.path(), &memory, first_run());
    script(&mut first);
    let (_, end) = paged(&mut first, &Cursor::start(), 100);
    let text = as_released(&end);
    drop(first);

    let mut second = open(state.path(), &memory, later_run(1));
    longer_script(&mut second);
    let whole = second.published().expect("the second host's list reads");
    let cursor = Cursor::parse(&text).expect("a released cursor reads");
    for limit in [1, 100] {
        let (events, _) = paged(&mut second, &cursor, limit);
        assert_eq!(ids(&events), ids(&whole), "limit {limit}");
    }
}
