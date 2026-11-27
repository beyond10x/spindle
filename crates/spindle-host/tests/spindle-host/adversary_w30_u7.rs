//! Adversary, wave 30 unit U7 (`story:events-cursor-across-restart`): an events cursor of
//! another run against a new memory store that already reached the positions the cursor marks,
//! a reset on either side of a restart, a cursor from two runs back, an idle read after a
//! restart, and the "same cursor" an idle read answers.
//!
//! The console fixture declares two components: `board`, which a request reaches, and `ledger`,
//! which only a binding reaches.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Cursor, Host, ManualClock, Published};

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
/// event and the same cursor", next to "still resumes after a restart". A cursor at the end,
/// read after a restart with nothing new, answers no event and a cursor of the new run.
#[test]
fn a_cursor_at_the_end_answers_the_same_cursor_after_a_restart() {
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
    assert_eq!(
        page.next.to_string(),
        text,
        "at the end, the answer is the same cursor"
    );
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
