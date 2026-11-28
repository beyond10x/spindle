//! A command whose store's commit outcome is unknown
//! (`story:unknown-commit-answered-as-unknown`, D1 A, D2 A): the commit fails with
//! [`StoreError::Unknown`], as eventlog's `UnknownCommit` reaches the ingress once the store's
//! deadline fired after COMMIT was sent, so the write may have landed. The command answers `500`
//! `{"refused", "committed": "unknown"}`, not the `501` with `committed: false` of a command that
//! wrote nothing; the request log records it `failed`, and a run that finds its lease lost does
//! not read it as uncommitted.
//!
//! The fixture `tests/fixtures/desk` declares one component, `desk`, owning `Desk`, created by
//! `study.desk.OpenDesk`. Its store is a memory store whose every write fails with an unknown
//! outcome.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock, Seed, Stores, World};
use spindle_ingress::console::Worlds;
use spindle_ingress::{Answer, Exchange, Listener, Surface};
use spindle_kernel::{
    Change, MemoryStore, Reading, Removal, Row, Scan, Store, StoreError, Stored, Write as Written,
};

const COMPONENT: &str = "desk";
const OPEN: &str = "study.desk.OpenDesk";
/// What eventlog's `UnknownCommit` reads, as the PostgreSQL store's deadline fires after COMMIT.
const UNKNOWN: &str = "commit outcome unknown; reconnect and resolve the same command identity";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn environment() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"unknown-commit\"\n",
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
        "\"identity.mint\" = [\"uuid-v4\"]\n",
        "\"ingress.network\" = [\"http-loopback\"]\n",
    ))
    .expect("the environment parses")
}

/// A memory store whose every write ends with its outcome unknown, counting the writes asked of it.
struct UnknownOutcome {
    inner: MemoryStore,
    commits: Arc<AtomicUsize>,
}

impl UnknownOutcome {
    fn lose(&self) -> StoreError {
        self.commits.fetch_add(1, Ordering::SeqCst);
        StoreError::Unknown(UNKNOWN.to_owned())
    }
}

impl Store for UnknownOutcome {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.inner.load(entity, identity)
    }

    fn commit(&mut self, _change: &Change) -> Result<(), StoreError> {
        Err(self.lose())
    }

    fn remove(&mut self, _removal: &Removal) -> Result<(), StoreError> {
        Err(self.lose())
    }

    fn commit_all(&mut self, _writes: &[Written]) -> Result<(), StoreError> {
        Err(self.lose())
    }

    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError> {
        self.inner.scan(entity)
    }

    fn reading(&mut self) -> Option<Arc<dyn Reading>> {
        self.inner.reading()
    }
}

/// One `Connection: close` request with `body`, as JSON where it has one: the status and the
/// body as JSON.
fn send(address: SocketAddr, method: &str, target: &str, body: Option<&Value>) -> (u16, Value) {
    let mut stream = TcpStream::connect(address).expect("the listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a read timeout");
    let body = body.map(Value::to_string).unwrap_or_default();
    write!(
        stream,
        "{method} {target} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .expect("the request is written");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("the response is read");
    let (head, body) = response
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("a response without a body: {response}"));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a response without a status: {head}"));
    let body = serde_json::from_str(body).unwrap_or_else(|error| panic!("{error}: {body}"));
    (status, body)
}

/// The desk host on a store whose every write ends unknown, the listener serving its surface, the
/// path of `study.desk.OpenDesk`, and the count of writes the store was asked for.
fn served() -> (Host, Listener, String, Arc<AtomicUsize>) {
    let ir = spindle_spec::load(&fixtures().join("desk"))
        .expect("the desk fixture compiles")
        .ir;
    let surface = Surface::new(&ir, &[COMPONENT.to_owned()], None)
        .expect("the desk surface is served")
        .observing(16);
    let open = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == OPEN)
        .unwrap_or_else(|| panic!("the surface serves {OPEN}"))
        .path
        .clone();
    let commits = Arc::new(AtomicUsize::new(0));
    let store = UnknownOutcome {
        inner: MemoryStore::default(),
        commits: Arc::clone(&commits),
    };
    let host = Host::supplied(
        ir,
        &environment(),
        Arc::new(ManualClock::new()),
        None,
        Stores::from([(COMPONENT.to_owned(), Box::new(store) as Box<dyn Store>)]),
    )
    .unwrap_or_else(|error| panic!("the host opens on the store: {error}"));
    let listener = Listener::bind(0, surface).expect("the listener binds");
    (host, listener, open, commits)
}

fn desk() -> Value {
    json!({"desk_id": "desk-1", "holder": "a reader"})
}

/// The body every command whose commit outcome is unknown answers: the store's text under
/// `refused`, and `committed` the string `"unknown"`, never `false`.
fn unknown_body() -> Value {
    json!({"refused": format!("store: {UNKNOWN}"), "committed": "unknown"})
}

#[test]
fn an_unknown_commit_answers_500_with_committed_unknown() {
    let (mut host, listener, open, commits) = served();
    let address = listener.local_addr();

    let asked = thread::spawn(move || {
        let opened = send(address, "POST", &open, Some(&desk()));
        let log = send(address, "GET", "/__spindle/requests", None);
        (opened, log)
    });
    // The host's thread, as a served run's: every exchange the listener hands it is answered here.
    while !asked.is_finished() {
        if let Some(exchange) = listener.next(Duration::from_millis(5)) {
            let _ = Exchange::serve(exchange, &mut host);
        }
    }
    let ((status, body), (logged, log)) = asked.join().expect("the requests end");

    assert_eq!(status, 500, "{body}");
    assert_eq!(body, unknown_body(), "{body}");
    let mut keys: Vec<&String> = body.as_object().expect("an object").keys().collect();
    keys.sort();
    assert_eq!(keys, ["committed", "refused"], "{body}");
    assert!(
        body["refused"]
            .as_str()
            .is_some_and(|refused| refused.contains("outcome unknown")),
        "the refusal names the outcome as unknown: {body}"
    );
    assert_eq!(
        commits.load(Ordering::SeqCst),
        1,
        "the command reached the store"
    );
    assert_eq!(logged, 200, "{log}");
    let record = log["requests"]
        .as_array()
        .expect("requests")
        .iter()
        .find(|record| record["method"] == "POST")
        .unwrap_or_else(|| panic!("the command is recorded: {log}"));
    assert_eq!(record["status"], 500, "{log}");
    assert_eq!(record["class"], "failed", "{log}");
}

/// A reset whose world row ends with its outcome unknown keeps the store and answers `500`
/// naming the unknown outcome, as before (the coordinator's decision D-U2-r1): the next command
/// reaches the same store rather than one lost in the reset.
#[test]
fn a_reset_whose_world_row_is_unknown_answers_500_and_keeps_the_store() {
    let (mut host, _listener, _open, commits) = served();
    let world: Worlds<'_> = &|_, _| {
        Ok(World {
            rows: vec![Seed {
                entity: "study.desk.Desk".to_owned(),
                identity: json!("desk-1"),
                row: Row {
                    state: "Free".to_owned(),
                    fields: BTreeMap::from([("holder".to_owned(), json!("a reader"))]),
                },
            }],
            answers: None,
            mint: None,
            clock: None,
        })
    };

    let answer = spindle_ingress::console::reset(&mut host, b"{}", Some(world));
    let body = answer.body.unwrap_or_default();
    assert_eq!(answer.status, 500, "{body}");
    assert!(
        body["refused"]
            .as_str()
            .is_some_and(|refused| refused.contains("outcome unknown")),
        "the refusal names the outcome as unknown: {body}"
    );
    assert_eq!(
        commits.load(Ordering::SeqCst),
        1,
        "the world row reached the store"
    );

    let Value::Object(input) = desk() else {
        unreachable!("the desk input is an object")
    };
    let next = host
        .submit(OPEN, input)
        .expect_err("the store's every write ends unknown");
    assert_eq!(
        commits.load(Ordering::SeqCst),
        2,
        "the store was kept: {next}"
    );
    assert!(
        !next.to_string().contains("lost in a reset"),
        "the store was kept: {next}"
    );
}

/// A run whose lease is found lost replaces an answer that committed nothing with its `503`
/// (`spindle-cli`'s `serve`); an unknown outcome is not such an answer, so the caller is told the
/// write may have landed.
#[test]
fn an_unknown_commit_is_not_read_as_uncommitted() {
    let (mut host, listener, open, _commits) = served();
    let address = listener.local_addr();

    let asked = thread::spawn(move || send(address, "POST", &open, Some(&desk())));
    let mut told = Vec::new();
    while !asked.is_finished() {
        if let Some(exchange) = listener.next(Duration::from_millis(5)) {
            let _ = exchange.serve_with_unless(
                &mut host,
                &|_, _| Err("this run loads no world".to_owned()),
                |uncommitted| {
                    told.push(uncommitted);
                    uncommitted.then(|| Answer::refusal(503, "the lease was lost"))
                },
            );
        }
    }
    let (status, body) = asked.join().expect("the request ends");

    assert_eq!(told, [false], "the answer is not read as uncommitted");
    assert_eq!(status, 500, "{body}");
    assert_eq!(body, unknown_body(), "{body}");
}
