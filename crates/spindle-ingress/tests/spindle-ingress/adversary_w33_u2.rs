//! Adversary cases of wave 33, unit U2 (`story:new-leader-write-while-old-paused`).
//!
//! The unit answers a command whose store returned [`StoreError::Busy`] `503`
//! `{"refused", "committed": false}` and keeps every other store failure of a command at the
//! `501` of a command the runtime cannot complete. `busy_store.rs` pins the first half only: a
//! mapping that answered every store failure `503` (`ExecuteError::Store(_) => Answer::busy`)
//! would leave it green. These cases pin the second half, the `503` of a busy read inside a
//! command (`load`, before anything is written), and what the unit's documents promise for
//! reads and writes held so (`website/docs/concepts/capabilities.md`,
//! `website/docs/concepts/environment-and-admission.md` § Several processes on one store, the
//! `CHANGELOG.md` entry): a view answers `501` `{"refused"}`, `GET /api/events` and
//! `GET /api/history` answer `500` `{"refused"}`, and "a write to the new leader held behind
//! them answers `503` with `{"refused", "committed": false}` ... the same request is safe to send
//! again, and the new leader keeps leading".
//!
//! The fixture `tests/fixtures/desk` declares one component, `desk`, owning `Desk`, created by
//! `study.desk.OpenDesk`, loaded by `study.desk.TakeDesk` and listed by `study.desk.AllDesks`.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock, Stores};
use spindle_ingress::{Exchange, Listener, Surface, console};
use spindle_kernel::{
    Change, Committed, MemoryStore, Position, Reading, Removal, Scan, Store, StoreError, Stored,
    Write as Written,
};

const COMPONENT: &str = "desk";
const DESK: &str = "study.desk.Desk";
/// What eventlog's PostgreSQL store returns for a connection that went away mid-write: a store
/// failure that is not a deadline, which the kernel keeps as [`StoreError::Backend`].
const DOWN: &str = "PostgreSQL connection unavailable";
/// What eventlog's `Deadline` reads, as the PostgreSQL store's lock wait ends it.
const DEADLINE: &str = "event store deadline exceeded during database statement or lock";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn environment() -> Environment {
    Environment::parse(concat!(
        "format = \"spindle-environment/1\"\n",
        "name = \"adversary-w33-u2\"\n",
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

/// Which of the store's operations fail, and how.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Failing {
    /// Every write fails `Backend`, as a connection lost mid-write.
    WritesDown,
    /// Every load fails `Busy`, as a read waiting out its lock wait.
    LoadsBusy,
    /// Every read of the published events fails `Busy`, as eventlog's feed read waiting out its
    /// lock wait behind the publication lock another session holds.
    EventsBusy,
    /// Every read, on the host's thread and through the reading handle, fails `Busy`.
    ReadsBusy,
    /// Every write, a reset's mark among them, fails `Busy` while the switch the store is given
    /// is on, as an append waits out its lock wait behind the publication lock a paused former
    /// leader's session holds, until that session ends; once it is off the store answers. Reads
    /// answer, as eventlog's stream reads take no publication lock.
    WritesBusyWhileOn,
}

/// A memory store whose writes, loads or event reads fail as `failing` says, counting the
/// writes asked of it.
struct FailingStore {
    inner: MemoryStore,
    failing: Failing,
    writes: Arc<AtomicUsize>,
    /// The switch of [`Failing::WritesBusyWhileOn`].
    on: Arc<AtomicBool>,
    /// The reads asked of the reading handle.
    read: Arc<AtomicUsize>,
}

fn busy() -> StoreError {
    StoreError::Busy(DEADLINE.to_owned())
}

impl FailingStore {
    fn write(&self) -> Result<(), StoreError> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        if self.failing == Failing::WritesDown {
            return Err(StoreError::Backend(DOWN.to_owned()));
        }
        Err(busy())
    }

    /// Whether writes fail busy now.
    fn writes_busy(&self) -> bool {
        self.failing == Failing::WritesBusyWhileOn && self.on.load(Ordering::SeqCst)
    }

    /// Whether reads fail busy now.
    fn reads_busy(&self) -> bool {
        self.failing == Failing::ReadsBusy
    }
}

impl Store for FailingStore {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        if self.failing == Failing::LoadsBusy || self.reads_busy() {
            return Err(busy());
        }
        self.inner.load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        if self.failing == Failing::WritesBusyWhileOn && !self.writes_busy() {
            return self.inner.commit(change);
        }
        self.write()
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        if self.failing == Failing::WritesBusyWhileOn && !self.writes_busy() {
            return self.inner.remove(removal);
        }
        self.write()
    }

    fn commit_all(&mut self, writes: &[Written]) -> Result<(), StoreError> {
        if self.failing == Failing::WritesBusyWhileOn && !self.writes_busy() {
            return self.inner.commit_all(writes);
        }
        self.write()
    }

    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError> {
        if self.reads_busy() {
            return Err(busy());
        }
        self.inner.scan(entity)
    }

    fn reading(&mut self) -> Option<Arc<dyn Reading>> {
        if self.failing == Failing::ReadsBusy {
            return Some(Arc::new(BusyReading {
                read: Arc::clone(&self.read),
            }));
        }
        self.inner.reading()
    }

    fn forget_events(&mut self) -> Result<(), StoreError> {
        if self.writes_busy() {
            self.writes.fetch_add(1, Ordering::SeqCst);
            return Err(busy());
        }
        self.inner.forget_events()
    }

    fn events_after(&mut self, position: u64, limit: usize) -> Result<Vec<Committed>, StoreError> {
        if self.failing == Failing::EventsBusy || self.reads_busy() {
            return Err(busy());
        }
        self.inner.events_after(position, limit)
    }

    fn events_head(&mut self) -> Result<u64, StoreError> {
        if self.failing == Failing::EventsBusy || self.reads_busy() {
            return Err(busy());
        }
        self.inner.events_head()
    }
}

/// A reading handle whose every read fails busy, counting the reads asked of it.
struct BusyReading {
    read: Arc<AtomicUsize>,
}

impl BusyReading {
    fn refuse(&self) -> StoreError {
        self.read.fetch_add(1, Ordering::SeqCst);
        busy()
    }
}

impl Reading for BusyReading {
    fn scan(&self, _entity: &str) -> Result<Scan, StoreError> {
        Err(self.refuse())
    }

    fn counts(&self, _entities: &[String]) -> Result<BTreeMap<String, usize>, StoreError> {
        Err(self.refuse())
    }

    fn events_after(&self, _position: u64, _limit: usize) -> Result<Vec<Committed>, StoreError> {
        Err(self.refuse())
    }

    fn events_head(&self) -> Result<u64, StoreError> {
        Err(self.refuse())
    }

    fn at(&self, _entities: &[String]) -> Result<Box<dyn Position + '_>, StoreError> {
        Err(self.refuse())
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

/// What a desk host over a failing store is built with, and what it counts.
struct Built {
    host: Host,
    surface: Surface,
    writes: Arc<AtomicUsize>,
    on: Arc<AtomicBool>,
    read: Arc<AtomicUsize>,
}

/// A desk host over a store failing as `failing`, and the count of writes asked of the store.
fn host(failing: Failing) -> (Host, Surface, Arc<AtomicUsize>) {
    let built = built(failing);
    (built.host, built.surface, built.writes)
}

fn built(failing: Failing) -> Built {
    let ir = spindle_spec::load(&fixtures().join("desk"))
        .expect("the desk fixture compiles")
        .ir;
    let surface = Surface::new(&ir, &[COMPONENT.to_owned()], None)
        .expect("the desk surface is served")
        .observing(16)
        .with_console(spindle_ingress::HTTP_LOOPBACK)
        .expect("the console is served");
    let writes = Arc::new(AtomicUsize::new(0));
    let on = Arc::new(AtomicBool::new(true));
    let read = Arc::new(AtomicUsize::new(0));
    let store = FailingStore {
        inner: MemoryStore::default(),
        failing,
        writes: Arc::clone(&writes),
        on: Arc::clone(&on),
        read: Arc::clone(&read),
    };
    let host = Host::supplied(
        ir,
        &environment(),
        Arc::new(ManualClock::new()),
        None,
        Stores::from([(COMPONENT.to_owned(), Box::new(store) as Box<dyn Store>)]),
    )
    .unwrap_or_else(|error| panic!("the host opens on the failing store: {error}"));
    Built {
        host,
        surface,
        writes,
        on,
        read,
    }
}

/// What one command answered, the view's rows after it, the request log's record of it, and the
/// writes the store was asked for.
struct Served {
    status: u16,
    body: Value,
    rows: Value,
    record: Value,
    writes: usize,
}

/// Sends one `POST` with `input` to `operation`'s route on a desk host over a store failing as
/// `failing`, served on the host's thread as a served run's is; then reads the view and the
/// request log.
fn serve_once(failing: Failing, operation: &str, input: &Value) -> Served {
    let (mut host, surface, writes) = host(failing);
    let target = path(&surface, operation);
    let view = path(&surface, "study.desk.AllDesks");
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let input = input.clone();
    let asked = thread::spawn(move || {
        let sent = send(address, "POST", &target, Some(&input));
        let rows = send(address, "GET", &view, None);
        let log = send(address, "GET", "/__spindle/requests", None);
        (sent, rows, log)
    });
    while !asked.is_finished() {
        if let Some(exchange) = listener.next(Duration::from_millis(5)) {
            let _ = Exchange::serve(exchange, &mut host);
        }
    }
    let ((status, body), (listed, rows), (logged, log)) = asked.join().expect("the requests end");
    assert_eq!(listed, 200, "{rows}");
    assert_eq!(logged, 200, "{log}");
    let record = log["requests"]
        .as_array()
        .expect("requests")
        .iter()
        .find(|record| record["method"] == "POST")
        .cloned()
        .unwrap_or_else(|| panic!("the command is recorded: {log}"));
    Served {
        status,
        body,
        rows,
        record,
        writes: writes.load(Ordering::SeqCst),
    }
}

/// A command whose write the store failed for a reason other than its deadline, as a connection
/// lost mid-write, still answers the `501` of a command the runtime cannot complete, recorded
/// `not-served`: only [`StoreError::Busy`] is the `503` (`capabilities.md`, the unit's
/// `answer.rs` doc). Kills the mutant that answers every `ExecuteError::Store` `503`.
#[test]
fn a_write_the_store_failed_otherwise_still_answers_501() {
    let served = serve_once(
        Failing::WritesDown,
        "study.desk.OpenDesk",
        &json!({"desk_id": "desk-1", "holder": "a reader"}),
    );
    assert_eq!(served.status, 501, "{}", served.body);
    assert_eq!(
        served.body,
        json!({"refused": format!("store: {DOWN}"), "committed": false}),
        "{}",
        served.body
    );
    assert_eq!(served.writes, 1, "the command reached the store's write");
    assert_eq!(served.rows, json!({"rows": []}), "nothing was written");
    assert_eq!(served.record["status"], 501, "{}", served.record);
    assert_eq!(served.record["class"], "not-served", "{}", served.record);
}

/// A command whose read of its subject waits out the store's deadline ([`StoreError::Busy`] from
/// `load`, before anything is written) answers `503` `{"refused", "committed": false}` as a busy
/// commit does, recorded `failed`, and asks the store for no write.
#[test]
fn a_command_whose_read_is_busy_answers_503_and_writes_nothing() {
    let served = serve_once(
        Failing::LoadsBusy,
        "study.desk.TakeDesk",
        &json!({"desk_id": "desk-1"}),
    );
    assert_eq!(served.status, 503, "{}", served.body);
    assert_eq!(
        served.body,
        json!({"refused": format!("store: {DEADLINE}"), "committed": false}),
        "{}",
        served.body
    );
    assert_eq!(served.writes, 0, "no write was asked for");
    assert_eq!(served.record["status"], 503, "{}", served.record);
    assert_eq!(served.record["class"], "failed", "{}", served.record);
}

/// The unit's documents, as revised after round 1: "`GET /api/events` and `GET /api/history`
/// answer `500` `{"refused"}`" for a read held behind a busy store
/// (`website/docs/concepts/capabilities.md`, the `CHANGELOG.md` entry). `GET /api/events` whose
/// store waited out its deadline, as eventlog's feed read held behind the publication lock does
/// (`publication_gate` takes it exclusive), is that read.
#[test]
fn an_events_read_held_by_a_busy_store_answers_the_documented_500() {
    let (mut host, _, writes) = host(Failing::EventsBusy);
    let answer = console::events(&mut host);
    assert_eq!(writes.load(Ordering::SeqCst), 0, "a read asks for no write");
    assert_eq!(
        (answer.status, answer.body),
        (500, Some(json!({"refused": format!("store: {DEADLINE}")}))),
        "capabilities.md says a held events read answers 500 {{\"refused\"}}"
    );
}

/// The reads the documents name, each sent to a desk run over a store whose every read fails
/// busy: the view, `GET /api/events` whole, ranged and at its head, and `GET /api/history`. What
/// each answered, by name.
fn reads_held(address: SocketAddr, view: &str) -> Vec<(&'static str, u16, Value)> {
    let history = format!("/api/history?entity={DESK}&identity=desk-1");
    [
        ("view", view.to_owned()),
        ("events", "/api/events".to_owned()),
        ("events ranged", "/api/events?limit=1".to_owned()),
        ("events head", "/api/events?after=head".to_owned()),
        ("history", history),
    ]
    .into_iter()
    .map(|(name, target)| {
        let (status, body) = send(address, "GET", &target, None);
        (name, status, body)
    })
    .collect()
}

/// What the documents name for each read held so: a view `501` `{"refused"}`, the events and
/// the history `500` `{"refused"}`; none `503`, and none carries `committed`.
fn documented(name: &str) -> (u16, Value) {
    let refused = json!({"refused": format!("store: {DEADLINE}")});
    if name == "view" {
        (501, refused)
    } else {
        (500, refused)
    }
}

/// `capabilities.md`: "A read held so is not answered `503`: a view answers `501` `{"refused"}`,
/// and `GET /api/events` and `GET /api/history` answer `500` `{"refused"}`". Each read sent to a
/// run that reads on its host's thread only, over a store whose every read waits out its
/// deadline, answers what the document names, with the refusal alone.
#[test]
fn reads_held_on_the_hosts_thread_answer_the_documented_statuses() {
    let (mut host, surface, writes) = host(Failing::ReadsBusy);
    let view = path(&surface, "study.desk.AllDesks");
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let asked = thread::spawn(move || reads_held(address, &view));
    while !asked.is_finished() {
        if let Some(exchange) = listener.next(Duration::from_millis(5)) {
            let _ = Exchange::serve(exchange, &mut host);
        }
    }
    let answered = asked.join().expect("the reads end");
    for (name, status, body) in answered {
        assert_eq!((status, body), documented(name), "{name}");
    }
    assert_eq!(writes.load(Ordering::SeqCst), 0, "a read asks for no write");
}

/// As [`reads_held_on_the_hosts_thread_answer_the_documented_statuses`], for a served run's
/// reads, which are answered beside the host's thread through its reader
/// (`website/docs/reference/cli.md`: "Reads are answered beside them, each on a thread of its
/// own"), the store's reading handle failing every read busy. The handle was asked, so the
/// reader, not the host's thread, answered.
#[test]
fn reads_held_through_the_reader_answer_the_documented_statuses() {
    let Built {
        mut host,
        surface,
        writes,
        read,
        ..
    } = built(Failing::ReadsBusy);
    let view = path(&surface, "study.desk.AllDesks");
    let listener =
        Listener::bind_reading(0, surface, host.reader()).expect("the reading listener binds");
    let address = listener.local_addr();
    let asked = thread::spawn(move || reads_held(address, &view));
    while !asked.is_finished() {
        if let Some(exchange) = listener.next(Duration::from_millis(5)) {
            let _ = Exchange::serve(exchange, &mut host);
        }
    }
    let answered = asked.join().expect("the reads end");
    for (name, status, body) in answered {
        assert_eq!((status, body), documented(name), "{name}");
    }
    assert!(
        read.load(Ordering::SeqCst) > 0,
        "the reader answered through the reading handle"
    );
    assert_eq!(writes.load(Ordering::SeqCst), 0, "a read asks for no write");
}

/// `environment-and-admission.md` § Several processes on one store, added by the unit: "A write
/// to the new leader held behind them answers `503` with `{"refused", "committed": false}` ...
/// the same request is safe to send again, and the new leader keeps leading." `POST /api/reset`
/// is a write the run takes on its host's thread, as a command is: a reset of a PostgreSQL store
/// removes its rows and appends its reset mark (`forget_events`) through eventlog's append, which
/// takes the publication lock shared, behind the same lock. Sent while every write of the store
/// waits out its deadline, it should answer that `503`; once the store answers again, a command
/// should be written (`202`, the route's status of `opened`), the run still leading on its store.
#[test]
fn a_reset_held_by_a_busy_store_answers_the_documented_503_and_the_run_keeps_its_store() {
    let Built {
        mut host,
        surface,
        on,
        ..
    } = built(Failing::WritesBusyWhileOn);
    let open = path(&surface, "study.desk.OpenDesk");
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let switch = Arc::clone(&on);
    let asked = thread::spawn(move || {
        let reset = send(address, "POST", "/api/reset", None);
        switch.store(false, Ordering::SeqCst);
        let opened = send(
            address,
            "POST",
            &open,
            Some(&json!({"desk_id": "desk-1", "holder": "a reader"})),
        );
        (reset, opened)
    });
    while !asked.is_finished() {
        if let Some(exchange) = listener.next(Duration::from_millis(5)) {
            let _ = Exchange::serve(exchange, &mut host);
        }
    }
    let ((reset, reset_body), (opened, opened_body)) = asked.join().expect("the requests end");
    let printed = format!(
        "the reset answered {reset} {reset_body}; the command after it, the store answering \
         again, answered {opened} {opened_body}"
    );
    assert_eq!(
        (reset, reset_body["committed"].clone()),
        (503, json!(false)),
        "{printed}"
    );
    assert_eq!(
        (opened, opened_body["outcome"].clone()),
        (202, json!("opened")),
        "{printed}"
    );
}
