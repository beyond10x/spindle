//! Adversary, wave 28 unit U5 (`story:entity-event-history`,
//! `story:events-record-their-caller`): the history cursor at its boundaries, across a restart
//! on a fixed civil clock, and the history read on SQLite against memory.
//!
//! The fixture `history`: an editor (`notes.records.Editor`) writes, revises and pairs records on
//! `desk`; `keep-copy` carries each `RecordPublished` to `archive`'s `KeepCopy`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{History, HistoryCursor, Host, HostError, ManualClock, Revision};
use spindle_kernel::{Provenance, Sender};

const RECORD: &str = "notes.records.Record";
const COPY: &str = "notes.archive.Copy";
const EDITOR: &str = "notes.records.Editor";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn notes() -> ess_compiler::EssIr {
    spindle_spec::load(&root().join("crates/spindle-host/tests/fixtures/history"))
        .unwrap_or_else(|error| panic!("the history fixture compiles: {error:?}"))
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
            "name = \"adversary-history-{store}\"\n",
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

fn open_with(store: &str, state: &Path, clock: ManualClock) -> Host {
    Host::new(notes(), &bound(store), Arc::new(clock), state)
        .unwrap_or_else(|error| panic!("the {store} host opens: {error}"))
}

fn open(store: &str, state: &Path) -> Host {
    open_with(store, state, ManualClock::new())
}

/// The civil time `spindle serve --clock` would fix: 2027-01-15T08:00:00Z.
fn fixed() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000)
}

fn send(host: &mut Host, command: &str, input: Value, request: &str) {
    let sender = Sender {
        actor: Some(EDITOR.to_owned()),
        attributes: object(json!({"editor_id": "e-7"})),
        request: Some(request.to_owned()),
    };
    let report = host
        .submit_caused(
            &format!("notes.records.{command}"),
            object(input),
            sender.actor.as_deref(),
            Some(&sender.attributes),
            &std::collections::BTreeMap::new(),
            Provenance::Request(sender.clone()),
        )
        .unwrap_or_else(|error| panic!("{command}: {error}"));
    assert!(report.failed.is_empty(), "{command}: {:?}", report.failed);
}

fn script(host: &mut Host) {
    send(
        host,
        "WriteRecord",
        json!({"record_id": "r1", "text": "one"}),
        "q1",
    );
    send(
        host,
        "ReviseRecord",
        json!({"record_id": "r1", "text": "two"}),
        "q2",
    );
    send(
        host,
        "WriteRecord",
        json!({"record_id": "r2", "text": "other"}),
        "q3",
    );
    send(
        host,
        "ReviseRecord",
        json!({"record_id": "r1", "text": "three"}),
        "q4",
    );
}

fn read(host: &mut Host, identity: &str, after: &HistoryCursor, limit: usize) -> History {
    host.history(RECORD, &json!(identity), after, limit)
        .unwrap_or_else(|error| panic!("the history of {identity} reads: {error}"))
}

/// The request id each revision's caller names.
fn requests(revisions: &[Revision]) -> Vec<String> {
    revisions
        .iter()
        .map(|revision| {
            revision
                .cause
                .as_ref()
                .and_then(Provenance::sender)
                .and_then(|sender| sender.request.clone())
                .unwrap_or_default()
        })
        .collect()
}

/// What a revision says but its position and time.
type Said = (Option<String>, Option<String>, Option<Provenance>, String);

fn said(revisions: &[Revision]) -> Vec<Said> {
    revisions
        .iter()
        .map(|revision| {
            (
                revision.command.clone(),
                revision.outcome.clone(),
                revision.cause.clone(),
                revision.event.name.clone(),
            )
        })
        .collect()
}

/// A page of one walks the history one write at a time, and the cursor at the end stays where
/// it is; a page exactly as long as the history answers it whole and then an empty page; a cursor
/// read twice answers the same page.
#[test]
fn the_history_cursor_walks_one_write_at_a_time() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = open("memory", state.path());
    script(&mut host);
    let mut cursor = HistoryCursor::start();
    let mut walked = Vec::new();
    for _ in 0..3 {
        let page = read(&mut host, "r1", &cursor, 1);
        assert_eq!(page.events.len(), 1, "{page:#?}");
        walked.extend(requests(&page.events));
        let again = read(&mut host, "r1", &cursor, 1);
        assert_eq!(again, page, "a cursor read twice answers the same page");
        cursor = page.next;
    }
    assert_eq!(walked, ["q1", "q2", "q4"]);
    let end = read(&mut host, "r1", &cursor, 1);
    assert!(end.events.is_empty(), "{end:#?}");
    assert_eq!(end.next, cursor, "at the end the cursor stays");

    let whole = read(&mut host, "r1", &HistoryCursor::start(), 3);
    assert_eq!(requests(&whole.events), ["q1", "q2", "q4"]);
    let after = read(&mut host, "r1", &whole.next, 3);
    assert!(after.events.is_empty(), "{after:#?}");
}

/// An entity the model does not declare is refused; an identity of another JSON type than the
/// one written reads empty; an entity of the archive reads from the archive's store.
#[test]
fn an_unknown_entity_is_refused_and_another_component_reads_its_own() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = open("memory", state.path());
    script(&mut host);
    match host.history(
        "notes.records.Nothing",
        &json!("r1"),
        &HistoryCursor::start(),
        5,
    ) {
        Err(HostError::NoEntity(entity)) => assert_eq!(entity, "notes.records.Nothing"),
        other => panic!("an unknown entity is refused: {other:?}"),
    }
    let none = host
        .history(RECORD, &json!(1), &HistoryCursor::start(), 5)
        .expect("a read");
    assert!(none.events.is_empty(), "{none:#?}");
    send(&mut host, "PublishRecord", json!({"record_id": "r1"}), "q9");
    let copy = host
        .history(COPY, &json!("r1"), &HistoryCursor::start(), 5)
        .expect("the archive's history reads");
    assert_eq!(
        copy.events
            .iter()
            .map(|revision| revision.event.name.as_str())
            .collect::<Vec<_>>(),
        ["notes.archive.CopyKept"]
    );
    let cause = copy.events[0].cause.clone().expect("a cause");
    match &cause {
        Provenance::Binding { binding, trigger } => {
            assert_eq!(binding, "keep-copy");
            let sender = trigger
                .as_deref()
                .and_then(Provenance::sender)
                .expect("the trigger names the editor's request");
            assert_eq!(sender.request.as_deref(), Some("q9"), "{cause:?}");
        }
        other => panic!("a binding's command: {other:?}"),
    }
}

/// The same commands read the same history on SQLite as on memory: command, outcome, cause and
/// event of every revision, an `affects:` group included.
#[test]
fn the_history_reads_alike_on_sqlite_and_memory() {
    let read_on = |store: &str| {
        let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
        let mut host = open(store, state.path());
        script(&mut host);
        send(
            &mut host,
            "PairRecords",
            json!({"record_id": "r1", "other_id": "r2", "text": "pair"}),
            "q5",
        );
        ["r1", "r2"].map(|identity| {
            said(&read(&mut host, identity, &HistoryCursor::start(), usize::MAX).events)
        })
    };
    let memory = read_on("memory");
    let sqlite = read_on("sqlite");
    assert_eq!(memory[0].len(), 4, "{:#?}", memory[0]);
    assert_eq!(memory[1].len(), 2, "{:#?}", memory[1]);
    assert_eq!(sqlite, memory);
}

/// A cursor taken before a restart reads on after its last event, and a revision made after the
/// restart is in the page it reads, when the host runs on a fixed civil clock, as
/// `spindle serve --clock` runs it. The cursor skips by the command sequence, and the host's
/// sequence restarts at the clock's time, so the commands after the restart take the sequences
/// the commands before it took.
#[test]
fn a_revision_after_a_restart_on_a_fixed_clock_is_read_after_an_earlier_cursor() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let cursor = {
        let mut host = open_with("sqlite", state.path(), ManualClock::at(fixed()));
        script(&mut host);
        let page = read(&mut host, "r1", &HistoryCursor::start(), usize::MAX);
        assert_eq!(requests(&page.events), ["q1", "q2", "q4"]);
        page.next
    };
    let mut host = open_with("sqlite", state.path(), ManualClock::at(fixed()));
    send(
        &mut host,
        "ReviseRecord",
        json!({"record_id": "r1", "text": "four"}),
        "q5",
    );
    let later = read(&mut host, "r1", &cursor, usize::MAX);
    assert_eq!(
        requests(&later.events),
        ["q5"],
        "the revision after the restart is read after the earlier cursor: {later:#?}"
    );
}
