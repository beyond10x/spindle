//! `story:events-record-their-caller`: every event a store commits records what caused its
//! command ([`Origin::cause`]): the caller of the request that sent it, with the request's id, or
//! the binding that ran it with the caller of the event the binding carried, or, in a run split
//! over processes, the binding with no cause known.
//!
//! The fixture `history`: an editor (`notes.records.Editor`, attribute `editor_id`) writes and
//! publishes records on `desk`; `keep-copy` carries each `RecordPublished` to `archive`'s
//! `KeepCopy`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock, Stores};
use spindle_kernel::{
    Committed, MemoryStore, Origin, Provenance, Reading, Sender, SqliteStore, Store,
};

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

/// A reading handle on a store, by its component.
type Handles = Vec<(String, Arc<dyn Reading>)>;

/// An environment whose stores are all given to the host ([`Host::supplied`]).
fn elsewhere() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"caller-elsewhere\"\n",
        "os = \"linux\"\n",
        "arch = \"x86_64\"\n",
        "\n",
        "[provides]\n",
        "\"store.durable\" = [\"elsewhere\"]\n",
        "\"store.ephemeral\" = [\"elsewhere\"]\n",
        "clock = [\"system\"]\n",
        "timer = [\"tokio\"]\n",
        "channel = [\"in-process\"]\n",
        "projection = [\"in-process\"]\n",
        "\"ingress.network\" = [\"http-loopback\"]\n",
    ))
    .expect("the environment parses")
}

/// A host on memory stores it is given, and a reading handle on each, by component.
fn hosted() -> (Host, Handles) {
    let mut given = Stores::new();
    let mut handles = Vec::new();
    for component in ["archive", "desk"] {
        let mut store = MemoryStore::default();
        let reading = store
            .reading()
            .expect("a memory store gives a reading handle");
        handles.push((component.to_owned(), reading));
        given.insert(component.to_owned(), Box::new(store) as Box<dyn Store>);
    }
    let host = Host::supplied(
        notes(),
        &elsewhere(),
        Arc::new(ManualClock::new()),
        None,
        given,
    )
    .expect("the host on given stores opens");
    (host, handles)
}

fn events(handles: &Handles, component: &str) -> Vec<Committed> {
    handles
        .iter()
        .find(|(name, _)| name == component)
        .unwrap_or_else(|| panic!("{component} is held"))
        .1
        .events_after(0, usize::MAX)
        .unwrap_or_else(|error| panic!("the events of {component} read: {error}"))
}

fn cause(committed: &Committed) -> Option<Provenance> {
    committed
        .origin
        .as_ref()
        .and_then(|origin| origin.cause.clone())
}

fn editor(request: Option<&str>) -> Sender {
    Sender {
        actor: Some(EDITOR.to_owned()),
        attributes: object(json!({"editor_id": "e-7"})),
        request: request.map(str::to_owned),
    }
}

/// Sends `command` as the editor `e-7`, as the request `request`.
fn send(host: &mut Host, command: &str, input: Value, request: &str) {
    let sender = editor(Some(request));
    let report = host
        .submit_caused(
            command,
            object(input),
            sender.actor.as_deref(),
            Some(&sender.attributes),
            &std::collections::BTreeMap::new(),
            Provenance::Request(sender.clone()),
        )
        .unwrap_or_else(|error| panic!("{command}: {error}"));
    assert!(report.failed.is_empty(), "{command}: {:?}", report.failed);
}

/// A command sent by a caller commits events whose origin names the command and the caller: its
/// actor, its attributes and the request's id. One sent without a request id names the caller
/// with none.
#[test]
fn an_event_records_the_caller_of_its_request() {
    let (mut host, handles) = hosted();
    send(
        &mut host,
        "notes.records.WriteRecord",
        json!({"record_id": "r1", "text": "first"}),
        "req-42",
    );
    let attributes = object(json!({"editor_id": "e-7"}));
    host.submit_as(
        "notes.records.ReviseRecord",
        object(json!({"record_id": "r1", "text": "second"})),
        Some(EDITOR),
        Some(&attributes),
    )
    .expect("the revision runs");

    let desk = events(&handles, "desk");
    assert_eq!(desk.len(), 2, "{desk:?}");
    let first = desk[0].origin.as_ref().expect("an origin");
    assert_eq!(first.command, "notes.records.WriteRecord");
    assert_eq!(
        first.cause,
        Some(Provenance::Request(editor(Some("req-42")))),
        "the caller and the request id"
    );
    assert_eq!(
        cause(&desk[1]),
        Some(Provenance::Request(editor(None))),
        "a request sent with no id names its caller and no id"
    );
}

/// An event a binding's command committed names the binding, and what caused the command whose
/// event the binding carried: the editor's request.
#[test]
fn an_event_a_binding_caused_records_its_cause() {
    let (mut host, handles) = hosted();
    send(
        &mut host,
        "notes.records.WriteRecord",
        json!({"record_id": "r1", "text": "first"}),
        "req-1",
    );
    send(
        &mut host,
        "notes.records.PublishRecord",
        json!({"record_id": "r1"}),
        "req-2",
    );
    let archive = events(&handles, "archive");
    assert_eq!(archive.len(), 1, "{archive:?}");
    assert_eq!(
        archive[0]
            .origin
            .as_ref()
            .map(|origin| origin.command.as_str()),
        Some("notes.archive.KeepCopy")
    );
    assert_eq!(
        cause(&archive[0]),
        Some(Provenance::Binding {
            binding: "keep-copy".to_owned(),
            trigger: Some(Box::new(Provenance::Request(editor(Some("req-2"))))),
        })
    );
    assert_eq!(
        host.staged_envelopes(),
        0,
        "no trigger is kept once invoked"
    );
}

/// One length-prefixed link message, as a peer writes it.
fn send_linked(stream: &mut std::net::TcpStream, message: &Value) {
    use std::io::Write as _;
    let body = serde_json::to_vec(message).expect("encodes");
    let mut bytes = u32::try_from(body.len())
        .expect("small")
        .to_be_bytes()
        .to_vec();
    bytes.extend_from_slice(&body);
    stream.write_all(&bytes).expect("the write");
}

/// In a run split over processes the archive runs on its own (`Host::component`): an envelope a
/// stand-in desk sends over the link runs `KeepCopy`, whose event names the binding and no cause,
/// since the command that published the event ran in another process.
#[test]
fn a_split_run_binding_event_records_an_unknown_cause() {
    use std::net::{TcpListener, TcpStream};

    let text = std::fs::read_to_string(root().join("examples/environments/laptop-processes.toml"))
        .expect("laptop-processes.toml reads");
    let environment = Environment::parse(&text).expect("an environment");
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let mut archive = Host::component(
        notes(),
        &environment,
        Arc::new(ManualClock::new()),
        state.path(),
        "archive",
    )
    .expect("the archive is admitted");
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let mut stream =
        TcpStream::connect(listener.local_addr().expect("bound")).expect("the desk connects");
    send_linked(
        &mut stream,
        &json!({"hello": {"from": "desk", "to": "archive"}}),
    );
    for _ in 0..400 {
        archive.accept(&listener).expect("the listener works");
        if archive.unlinked().is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(archive.unlinked(), Vec::<String>::new(), "the link opens");
    let frame = json!({"deliver": {
        "id": 1,
        "binding": "keep-copy",
        "from": "desk",
        "to": "archive",
        "delivery": "at_most_once",
        "on_failure": "drop",
        "attempt": 1,
        "command": {"name": "notes.archive.KeepCopy", "fields": {"record_id": "r1"}},
    }});
    send_linked(&mut stream, &json!({ "frame": frame }));
    let mut ran = Vec::new();
    for _ in 0..200 {
        let report = archive.deliver().expect("the archive delivers");
        ran.extend(report.ran);
        if !ran.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(ran.len(), 1, "KeepCopy ran: {ran:?}");
    drop(archive);

    let mut store =
        SqliteStore::open(&state.path().join("archive.sqlite")).expect("the archive's state opens");
    let committed = store.events_after(0, usize::MAX).expect("the events read");
    assert_eq!(committed.len(), 1, "{committed:?}");
    assert_eq!(
        committed[0].origin,
        Some(Origin {
            command: "notes.archive.KeepCopy".to_owned(),
            sequence: committed[0]
                .origin
                .as_ref()
                .map_or(0, |origin| origin.sequence),
            cause: Some(Provenance::Binding {
                binding: "keep-copy".to_owned(),
                trigger: None,
            }),
        })
    );
}
