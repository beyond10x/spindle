//! A metrics read answers in milliseconds and blocks nothing (`story:metrics-read-without-scan`).
//!
//! `GET /__spindle/metrics` reads the rows each entity holds from the counts the host keeps, so
//! it scans no store; and on a model the size of one a downstream consumer measured (500
//! commands, 100 entities) over a SQLite store of 5,000 events it answers within
//! [`BUDGET`] in a debug build, as does a command sent while it is read: the median of [`READS`]
//! reads, so that one read the scheduler of a loaded machine held back does not decide it, and
//! every read within [`CEILING`], far below what one scan of every entity's store costs (42 s
//! to 56 s per read before).
//!
//! The model is generated: 100 domains, each with one entity, `Desk`, five commands (`OpenDesk`
//! creates one, `TakeDesk` moves it, `BookDesk`, `CheckDesk` and `NoteDesk` write nothing) and
//! three events, all owned and accepted by one component, `hall`, reached over the network.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock, Stores};
use spindle_ingress::{Exchange, Listener, Surface};
use spindle_kernel::{Change, MemoryStore, Removal, Store, StoreError, Stored};

/// How long a metrics read, and a command sent while one is read, may take in a debug build: the
/// median of [`READS`].
const BUDGET: Duration = Duration::from_millis(200);
/// How long any one of them may take.
const CEILING: Duration = Duration::from_secs(1);
/// How many domains the generated model declares, each with one entity and [`COMMANDS`].
const DOMAINS: usize = 100;
/// The commands of each domain.
const COMMANDS: [&str; 5] = ["OpenDesk", "TakeDesk", "BookDesk", "CheckDesk", "NoteDesk"];
/// How many desks each domain opens and takes: two stored events each.
const DESKS: usize = 25;
/// How many metrics reads the latency case times.
const READS: usize = 5;
const COMPONENT: &str = "hall";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn environment(store: &str) -> Environment {
    Environment::parse(&format!(
        concat!(
            "format = \"spindle-environment/1\"\n",
            "name = \"metrics-read\"\n",
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
            "\"identity.mint\" = [\"uuid-v4\"]\n",
            "\"ingress.network\" = [\"http-loopback\"]\n",
        ),
        store = store
    ))
    .expect("the environment parses")
}

/// One domain of the generated model.
fn domain(index: usize) -> String {
    format!(
        r"domain: load.d{index}
summary: Desks of room {index}, opened, taken, booked, checked and noted.
types:
  - name: load.d{index}.DeskId
    kind: newtype
    of: String
entities:
  - name: load.d{index}.Desk
    identity:
      name: desk_id
      type: load.d{index}.DeskId
    fields:
      - name: holder
        type: String
    lifecycle:
      initial: Free
      states: [Free, Taken]
      terminal: [Taken]
      transitions:
        - name: take
          from: [Free]
          to: Taken
errors:
  - name: load.d{index}.NoSuchDesk
    summary: No desk carries the id.
    fields: []
  - name: load.d{index}.Closed
    summary: The room is closed on the day asked for.
    fields: []
commands:
  - name: load.d{index}.OpenDesk
    input:
      - name: desk_id
        type: load.d{index}.DeskId
      - name: holder
        type: String
    outcomes:
      - name: opened
        creates: load.d{index}.Desk
        instance: desk_id
        sets:
          holder: input.holder
        emits: [load.d{index}.DeskOpened]
        payload:
          load.d{index}.DeskOpened: {{desk_id: input.desk_id}}
        summary: The desk is open for its first holder.
  - name: load.d{index}.TakeDesk
    input:
      - name: desk_id
        type: load.d{index}.DeskId
    outcomes:
      - name: no-such-desk
        unknown_instance: true
        error: load.d{index}.NoSuchDesk
        summary: No desk carries the id.
      - name: taken
        moves: load.d{index}.Desk.take
        instance: desk_id
        emits: [load.d{index}.DeskTaken]
        payload:
          load.d{index}.DeskTaken: {{desk_id: input.desk_id}}
        summary: The desk is taken.
{stateless}events:
  - name: load.d{index}.DeskOpened
    fields:
      - name: desk_id
        type: load.d{index}.DeskId
  - name: load.d{index}.DeskTaken
    fields:
      - name: desk_id
        type: load.d{index}.DeskId
  - name: load.d{index}.DeskBooked
    fields:
      - name: desk_id
        type: load.d{index}.DeskId
",
        stateless = ["BookDesk", "CheckDesk", "NoteDesk"]
            .iter()
            .map(|command| format!(
                r"  - name: load.d{index}.{command}
    input:
      - name: desk_id
        type: load.d{index}.DeskId
      - name: weekend
        type: Boolean
    outcomes:
      - name: closed
        when: weekend
        error: load.d{index}.Closed
        summary: A weekend booking is refused.
      - name: booked
        emits: [load.d{index}.DeskBooked]
        payload:
          load.d{index}.DeskBooked: {{desk_id: input.desk_id}}
        summary: A weekday booking is taken.
"
            ))
            .collect::<String>()
    )
}

/// The generated model, written under `root`: [`DOMAINS`] domains and one component.
fn generated(root: &Path) -> PathBuf {
    let directory = root.join("hall");
    std::fs::create_dir_all(directory.join("domains")).expect("the model directory");
    let indices = 0..DOMAINS;
    let listed = |each: &dyn Fn(usize) -> Vec<String>| {
        indices
            .clone()
            .flat_map(each)
            .map(|line| format!("        - {line}\n"))
            .collect::<String>()
    };
    let system = format!(
        "format: ess/34\nsystem: load\nversion: v1\n\
         summary: A hall of {DOMAINS} reading rooms, each with its own desks.\n\
         domains:\n{domains}\
         components:\n\
         \x20 - component: {COMPONENT}\n\
         \x20   summary: Runs every reading room.\n\
         \x20   owns:\n\
         \x20     domains:\n{owned}\
         \x20   accepts:\n\
         \x20     commands:\n{commands}\
         \x20   publishes:\n\
         \x20     events:\n{events}\
         \x20   reached_by: network\n",
        domains = indices
            .clone()
            .map(|index| format!("  - load.d{index}\n"))
            .collect::<String>(),
        owned = listed(&|index| vec![format!("load.d{index}")]),
        commands = listed(&|index| COMMANDS
            .iter()
            .map(|command| format!("load.d{index}.{command}"))
            .collect()),
        events = listed(&|index| ["DeskOpened", "DeskTaken", "DeskBooked"]
            .iter()
            .map(|event| format!("load.d{index}.{event}"))
            .collect()),
    );
    std::fs::write(directory.join("system.yaml"), system).expect("the system is written");
    for index in indices {
        std::fs::write(
            directory.join(format!("domains/d{index}.yaml")),
            domain(index),
        )
        .expect("a domain is written");
    }
    directory
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

/// One `Connection: close` request: the status and the body as text.
fn send(address: SocketAddr, method: &str, target: &str, body: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(address).expect("the listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(600)))
        .expect("a read timeout");
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
    let (head, answered) = response
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("a response without a body: {response}"));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a response without a status: {head}"));
    (status, answered.to_owned())
}

/// Serves every exchange `listener` hands over on this thread, as a served run's host thread
/// does, until `done` ends.
fn serve_until<T>(listener: &Listener, host: &mut Host, done: &thread::JoinHandle<T>) {
    while !done.is_finished() {
        if let Some(exchange) = listener.next(Duration::from_millis(5)) {
            let _ = Exchange::serve(exchange, host);
        }
    }
}

#[test]
fn a_metrics_read_and_a_command_sent_during_it_answer_within_the_budget() {
    let root = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let ir = spindle_spec::load(&generated(root.path()))
        .unwrap_or_else(|error| panic!("the generated model compiles: {error}"))
        .ir;
    assert!(
        ir.commands().len() >= 500,
        "{} commands",
        ir.commands().len()
    );
    assert!(
        ir.entities().len() >= 100,
        "{} entities",
        ir.entities().len()
    );
    let surface = Surface::new(&ir, &[COMPONENT.to_owned()], None)
        .expect("the hall surface is served")
        .observing(64);
    let opening = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == "load.d7.OpenDesk")
        .expect("the surface serves OpenDesk");
    let (method, path) = (opening.method.as_str().to_uppercase(), opening.path.clone());
    let state = root.path().join("state");
    std::fs::create_dir_all(&state).expect("a state directory");
    let mut host = Host::new(
        ir,
        &environment("sqlite"),
        Arc::new(ManualClock::new()),
        &state,
    )
    .unwrap_or_else(|error| panic!("the host admits the hall on SQLite: {error}"));
    let mut stored = 0;
    for index in 0..DOMAINS {
        for desk in 0..DESKS {
            let desk_id = json!(format!("desk-{desk}"));
            host.submit(
                &format!("load.d{index}.OpenDesk"),
                object(json!({"desk_id": desk_id, "holder": "a reader"})),
            )
            .expect("OpenDesk runs");
            host.submit(
                &format!("load.d{index}.TakeDesk"),
                object(json!({"desk_id": desk_id})),
            )
            .expect("TakeDesk runs");
            stored += 2;
        }
    }
    assert!(stored >= 5_000, "{stored} events stored");

    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();
    let mut reads = Vec::new();
    let mut commands = Vec::new();
    for read in 0..READS {
        let (sent, arrived) = mpsc::channel();
        let scrape = thread::spawn(move || {
            let started = Instant::now();
            sent.send(()).expect("the command thread waits");
            let (status, body) = send(address, "GET", "/__spindle/metrics", "");
            (started.elapsed(), status, body)
        });
        let (method, path) = (method.clone(), path.clone());
        let command = thread::spawn(move || {
            arrived.recv().expect("the scrape starts");
            thread::sleep(Duration::from_millis(2));
            let body = json!({"desk_id": format!("late-{read}"), "holder": "late"}).to_string();
            let started = Instant::now();
            let (status, answer) = send(address, &method, &path, &body);
            (started.elapsed(), status, answer)
        });
        serve_until(&listener, &mut host, &scrape);
        serve_until(&listener, &mut host, &command);
        let (took, status, body) = scrape.join().expect("the scrape thread ends");
        assert_eq!(status, 200, "{body}");
        assert!(
            body.lines()
                .any(|line| line == r#"spindle_entity_rows{entity="load.d0.Desk"} 25"#),
            "the rows are counted: {body}"
        );
        reads.push(took);
        let (took, status, answer) = command.join().expect("the command thread ends");
        assert!(status < 300, "{status} {answer}");
        commands.push(took);
    }
    eprintln!("metrics reads: {reads:?}; commands sent during a read: {commands:?}");
    for (what, timed) in [
        ("a metrics read", reads),
        ("a command sent during a read", commands),
    ] {
        let mut sorted = timed.clone();
        sorted.sort();
        let median = sorted[sorted.len() / 2];
        assert!(
            median < BUDGET,
            "{what} took {median:?} at the median, over {BUDGET:?}: {timed:?}"
        );
        assert!(
            sorted.iter().all(|took| *took < CEILING),
            "{what} took over {CEILING:?}: {timed:?}"
        );
    }
}

/// A memory store whose every scan is counted.
#[derive(Clone, Default)]
struct Counted {
    store: Arc<Mutex<MemoryStore>>,
    scans: Arc<AtomicUsize>,
}

impl Store for Counted {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.store.lock().expect("the store").load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.store.lock().expect("the store").commit(change)
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.store.lock().expect("the store").remove(removal)
    }

    fn scan(&mut self, entity: &str) -> Result<spindle_kernel::Scan, StoreError> {
        self.scans.fetch_add(1, Ordering::SeqCst);
        self.store.lock().expect("the store").scan(entity)
    }
}

#[test]
fn a_metrics_read_scans_no_store() {
    let ir = spindle_spec::load(&fixtures().join("desk"))
        .expect("the desk fixture compiles")
        .ir;
    let surface = Surface::new(&ir, &["desk".to_owned()], None)
        .expect("the desk surface is served")
        .observing(16);
    let store = Counted::default();
    let mut host = Host::supplied(
        ir,
        &environment("elsewhere"),
        Arc::new(ManualClock::new()),
        None,
        Stores::from([("desk".to_owned(), Box::new(store.clone()) as Box<dyn Store>)]),
    )
    .unwrap_or_else(|error| panic!("the host opens on the supplied store: {error}"));
    for desk in ["desk-1", "desk-2"] {
        host.submit(
            "study.desk.OpenDesk",
            object(json!({"desk_id": desk, "holder": "a reader"})),
        )
        .expect("OpenDesk runs");
    }
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();

    let before = store.scans.load(Ordering::SeqCst);
    let scrape = thread::spawn(move || send(address, "GET", "/__spindle/metrics", ""));
    serve_until(&listener, &mut host, &scrape);
    let (status, body) = scrape.join().expect("the scrape thread ends");
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        store.scans.load(Ordering::SeqCst) - before,
        0,
        "the metrics read scanned the store"
    );
    assert!(
        body.lines()
            .any(|line| line == r#"spindle_entity_rows{entity="study.desk.Desk"} 2"#),
        "{body}"
    );
}
