//! `story:entity-event-history`: the committed events of one entity instance, oldest first, each
//! with its store position, the time of its command, the command, its caller and its outcome,
//! paged after a cursor ([`Host::history`], [`Reader::history`]). The events of an instance are
//! every event of every write that wrote it, a group of writes (`affects:`) included.
//!
//! The fixture `history`: an editor (`notes.records.Editor`) writes, revises and pairs records on
//! `desk`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{History, HistoryCursor, Host, ManualClock, Revision};
use spindle_kernel::{Provenance, Sender};

const RECORD: &str = "notes.records.Record";
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
            "name = \"history-{store}\"\n",
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

fn open(store: &str, state: &Path) -> Host {
    Host::new(notes(), &bound(store), Arc::new(ManualClock::new()), state)
        .unwrap_or_else(|error| panic!("the {store} host opens: {error}"))
}

fn editor(request: &str) -> Sender {
    Sender {
        actor: Some(EDITOR.to_owned()),
        attributes: object(json!({"editor_id": "e-7"})),
        request: Some(request.to_owned()),
    }
}

/// Sends `command` as the editor `e-7`, as the request `request`.
fn send(host: &mut Host, command: &str, input: Value, request: &str) {
    let sender = editor(request);
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

/// Three commands change `r1`, one changes `r2`.
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

fn every(host: &mut Host, identity: &str) -> Vec<Revision> {
    read(host, identity, &HistoryCursor::start(), usize::MAX).events
}

/// What each revision says, but its position and time: the command, the outcome, the request
/// and the event.
fn said(revisions: &[Revision]) -> Vec<(String, String, String, String)> {
    revisions
        .iter()
        .map(|revision| {
            (
                revision.command.clone().unwrap_or_default(),
                revision.outcome.clone().unwrap_or_default(),
                revision
                    .cause
                    .as_ref()
                    .and_then(Provenance::sender)
                    .and_then(|sender| sender.request.clone())
                    .unwrap_or_default(),
                revision.event.name.clone(),
            )
        })
        .collect()
}

fn three() -> Vec<(String, String, String, String)> {
    [
        ("WriteRecord", "written", "q1", "RecordWritten"),
        ("ReviseRecord", "revised", "q2", "RecordRevised"),
        ("ReviseRecord", "revised", "q4", "RecordRevised"),
    ]
    .into_iter()
    .map(|(command, outcome, request, event)| {
        (
            format!("notes.records.{command}"),
            outcome.to_owned(),
            request.to_owned(),
            format!("notes.records.{event}"),
        )
    })
    .collect()
}

#[test]
fn an_instance_reads_its_events_oldest_first() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = open("memory", state.path());
    script(&mut host);
    let revisions = every(&mut host, "r1");
    assert_eq!(said(&revisions), three(), "{revisions:#?}");
    let positions: Vec<u64> = revisions.iter().map(|revision| revision.position).collect();
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "oldest first: {positions:?}"
    );
    for revision in &revisions {
        assert_eq!(
            revision.cause.as_ref().and_then(Provenance::sender),
            Some(&editor(
                revision
                    .cause
                    .as_ref()
                    .and_then(Provenance::sender)
                    .and_then(|sender| sender.request.as_deref())
                    .unwrap_or_default()
            )),
            "the caller: {revision:?}"
        );
        let at = revision.at.as_deref().expect("a time");
        assert!(
            at.len() >= 20 && at.ends_with('Z') && at.contains('T'),
            "an RFC 3339 time: {at}"
        );
        assert_eq!(revision.event.payload["record_id"], json!("r1"));
    }
    let reader = host.reader();
    assert_eq!(
        reader
            .history(RECORD, &json!("r1"), &HistoryCursor::start(), usize::MAX)
            .expect("the reader reads the history")
            .events,
        revisions,
        "the reader reads it alike"
    );
}

#[test]
fn an_instance_history_pages_with_a_cursor() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = open("memory", state.path());
    script(&mut host);
    let first = read(&mut host, "r1", &HistoryCursor::start(), 2);
    assert_eq!(said(&first.events), three()[..2], "{first:#?}");
    let text = first.next.to_string();
    let cursor = HistoryCursor::parse(&text).expect("the cursor reads back");
    assert!(
        text.bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'),
        "{text}"
    );
    let second = read(&mut host, "r1", &cursor, 2);
    assert_eq!(said(&second.events), three()[2..], "{second:#?}");
    let last = read(&mut host, "r1", &second.next, 2);
    assert!(last.events.is_empty(), "{last:#?}");
    assert_eq!(last.next, second.next, "at the end the cursor stays");
    send(
        &mut host,
        "ReviseRecord",
        json!({"record_id": "r1", "text": "four"}),
        "q5",
    );
    let later = read(&mut host, "r1", &last.next, 2);
    assert_eq!(
        said(&later.events)
            .into_iter()
            .map(|(_, _, request, _)| request)
            .collect::<Vec<_>>(),
        ["q5"],
        "a later revision reads after the cursor"
    );
}

#[test]
fn an_instance_no_command_changed_reads_empty() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = open("memory", state.path());
    script(&mut host);
    assert_eq!(every(&mut host, "r9"), Vec::new());
    assert_eq!(every(&mut host, "r2").len(), 1);
}

#[test]
fn an_affected_instance_reads_its_group_events() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut host = open("memory", state.path());
    script(&mut host);
    send(
        &mut host,
        "PairRecords",
        json!({"record_id": "r1", "other_id": "r2", "text": "pair"}),
        "q6",
    );
    for identity in ["r1", "r2"] {
        let revisions = every(&mut host, identity);
        let last = revisions.last().expect("a revision");
        assert_eq!(last.event.name, "notes.records.RecordsPaired", "{identity}");
        assert_eq!(last.outcome.as_deref(), Some("paired"), "{identity}");
    }
}

#[test]
fn an_instance_history_survives_a_restart_on_sqlite() {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let (before, page) = {
        let mut host = open("sqlite", state.path());
        script(&mut host);
        let page = read(&mut host, "r1", &HistoryCursor::start(), 2);
        (every(&mut host, "r1"), page)
    };
    assert_eq!(said(&before), three());
    let mut host = open("sqlite", state.path());
    assert_eq!(
        every(&mut host, "r1"),
        before,
        "the same events after a restart"
    );
    let rest = read(&mut host, "r1", &page.next, 2);
    assert_eq!(
        rest.events,
        before[2..],
        "a cursor taken before the restart reads on after its last event"
    );
}
