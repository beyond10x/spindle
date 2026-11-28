//! A command whose store did not answer within its deadline
//! (`story:new-leader-write-while-old-paused`, D1 A): the commit fails with
//! [`StoreError::Busy`], as eventlog's `Deadline` reaches the ingress, before anything was
//! written, so the command answers `503` `{"refused", "committed": false}`, safe to send again,
//! not the `501` of a command the runtime cannot complete; the request log records it `failed`.
//!
//! The fixture `tests/fixtures/desk` declares one component, `desk`, owning `Desk`, created by
//! `study.desk.OpenDesk` and listed by the view `study.desk.AllDesks`. Its store is a memory store
//! whose every commit fails busy.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock, Stores};
use spindle_ingress::{Exchange, Listener, Surface};
use spindle_kernel::{
    Change, MemoryStore, Reading, Removal, Scan, Store, StoreError, Stored, Write as Written,
};

const COMPONENT: &str = "desk";
/// What eventlog's `Deadline` reads, as the PostgreSQL store's lock wait ends it.
const DEADLINE: &str = "event store deadline exceeded during database statement or lock";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn environment() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"busy-store\"\n",
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

/// A memory store whose every write fails busy, counting the commits asked of it.
struct Busy {
    inner: MemoryStore,
    commits: Arc<AtomicUsize>,
}

impl Busy {
    fn refuse(&self) -> StoreError {
        self.commits.fetch_add(1, Ordering::SeqCst);
        StoreError::Busy(DEADLINE.to_owned())
    }
}

impl Store for Busy {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.inner.load(entity, identity)
    }

    fn commit(&mut self, _change: &Change) -> Result<(), StoreError> {
        Err(self.refuse())
    }

    fn remove(&mut self, _removal: &Removal) -> Result<(), StoreError> {
        Err(self.refuse())
    }

    fn commit_all(&mut self, _writes: &[Written]) -> Result<(), StoreError> {
        Err(self.refuse())
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

fn path(surface: &Surface, operation: &str) -> String {
    surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == operation)
        .unwrap_or_else(|| panic!("the surface serves {operation}"))
        .path
        .clone()
}

#[test]
fn a_store_deadline_answers_503_and_commits_nothing() {
    let ir = spindle_spec::load(&fixtures().join("desk"))
        .expect("the desk fixture compiles")
        .ir;
    let surface = Surface::new(&ir, &[COMPONENT.to_owned()], None)
        .expect("the desk surface is served")
        .observing(16);
    let open = path(&surface, "study.desk.OpenDesk");
    let view = path(&surface, "study.desk.AllDesks");
    let commits = Arc::new(AtomicUsize::new(0));
    let store = Busy {
        inner: MemoryStore::default(),
        commits: Arc::clone(&commits),
    };
    let mut host = Host::supplied(
        ir,
        &environment(),
        Arc::new(ManualClock::new()),
        None,
        Stores::from([(COMPONENT.to_owned(), Box::new(store) as Box<dyn Store>)]),
    )
    .unwrap_or_else(|error| panic!("the host opens on the busy store: {error}"));
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();

    let asked = thread::spawn(move || {
        let opened = send(
            address,
            "POST",
            &open,
            Some(&json!({"desk_id": "desk-1", "holder": "a reader"})),
        );
        let rows = send(address, "GET", &view, None);
        let log = send(address, "GET", "/__spindle/requests", None);
        (opened, rows, log)
    });
    // The host's thread, as a served run's: every exchange the listener hands it is answered here.
    while !asked.is_finished() {
        if let Some(exchange) = listener.next(Duration::from_millis(5)) {
            let _ = Exchange::serve(exchange, &mut host);
        }
    }
    let ((status, body), (listed, rows), (logged, log)) = asked.join().expect("the requests end");

    assert_eq!(status, 503, "{body}");
    assert_eq!(
        body,
        json!({"refused": format!("store: {DEADLINE}"), "committed": false}),
        "{body}"
    );
    assert_eq!(
        commits.load(Ordering::SeqCst),
        1,
        "the command reached the store"
    );
    assert_eq!(listed, 200, "{rows}");
    assert_eq!(rows, json!({"rows": []}), "nothing was written");
    assert_eq!(logged, 200, "{log}");
    let record = log["requests"]
        .as_array()
        .expect("requests")
        .iter()
        .find(|record| record["method"] == "POST")
        .unwrap_or_else(|| panic!("the command is recorded: {log}"));
    assert_eq!(record["status"], 503, "{log}");
    assert_eq!(record["class"], "failed", "{log}");
}
