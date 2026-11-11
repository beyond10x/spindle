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

// story:metrics-observed-series: a scrape writes a series only once something was counted in it.

/// The series a scrape of the generated model writes before anything is executed: only
/// `spindle_resets_total`, which every scrape writes. 0.13.0 wrote 15,565 series for a model of
/// 471 commands, about 33 per declared command.
const UNEXECUTED_SERIES: usize = 1;

/// The sample lines of `exposition`: one per series, each histogram bucket its own.
fn sample_lines(exposition: &str) -> Vec<&str> {
    exposition
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .collect()
}

/// No counter sample of `exposition` is 0, but `spindle_resets_total`: a counter is written
/// only once counted.
fn no_zero_counter(exposition: &str) {
    for line in sample_lines(exposition) {
        let (series, value) = line.rsplit_once(' ').expect("a sample has a value");
        if series.ends_with("_total") || series.contains("_total{") {
            assert!(
                series == "spindle_resets_total" || value != "0",
                "{line} is written before it was counted:\n{exposition}"
            );
        }
    }
}

/// `GET /__spindle/metrics` on `address`, every exchange served on `host` until it is answered.
fn scraped(listener: &Listener, host: &mut Host, address: SocketAddr) -> String {
    let scrape = thread::spawn(move || send(address, "GET", "/__spindle/metrics", ""));
    serve_until(listener, host, &scrape);
    let (status, body) = scrape.join().expect("the scrape thread ends");
    assert_eq!(status, 200, "{body}");
    body
}

/// `method target` with `body`, every exchange served on `host` until it is answered.
fn sent(
    listener: &Listener,
    host: &mut Host,
    address: SocketAddr,
    (method, target): (&str, &str),
    body: &Value,
) -> (u16, String) {
    let (method, target, body) = (method.to_owned(), target.to_owned(), body.to_string());
    let asked = thread::spawn(move || send(address, &method, &target, &body));
    serve_until(listener, host, &asked);
    asked.join().expect("the request thread ends")
}

/// The generated model of [`DOMAINS`] domains served on memory stores, observed.
fn served_hall(root: &Path) -> (Listener, Host, Surface) {
    let ir = spindle_spec::load(&generated(root))
        .unwrap_or_else(|error| panic!("the generated model compiles: {error}"))
        .ir;
    assert!(
        ir.commands().len() >= 400,
        "{} commands",
        ir.commands().len()
    );
    let surface = Surface::new(&ir, &[COMPONENT.to_owned()], None)
        .expect("the hall surface is served")
        .observing(64);
    let state = root.join("state");
    std::fs::create_dir_all(&state).expect("a state directory");
    let host = Host::new(
        ir,
        &environment("memory"),
        Arc::new(ManualClock::new()),
        &state,
    )
    .unwrap_or_else(|error| panic!("the host admits the hall: {error}"));
    let listener = Listener::bind(0, surface.clone()).expect("the listener binds");
    (listener, host, surface)
}

#[test]
fn an_unexecuted_model_of_500_commands_writes_fewer_than_1000_series() {
    let root = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let (listener, mut host, _) = served_hall(root.path());
    let address = listener.local_addr();
    let body = scraped(&listener, &mut host, address);
    let series = sample_lines(&body).len();
    eprintln!(
        "{series} series, {} bytes, for {} commands executed none",
        body.len(),
        DOMAINS * COMMANDS.len()
    );
    assert!(series < 1_000, "{series} series:\n{body}");
    assert_eq!(series, UNEXECUTED_SERIES, "{body}");
    assert_eq!(sample_lines(&body), ["spindle_resets_total 0"], "{body}");
}

#[test]
fn a_series_appears_once_counted_and_stays_after_a_reset() {
    let root = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let (listener, mut host, surface) = served_hall(root.path());
    let address = listener.local_addr();
    let opening = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == "load.d7.OpenDesk")
        .expect("the surface serves OpenDesk");
    let (method, path) = (opening.method.as_str().to_uppercase(), opening.path.clone());

    let before = scraped(&listener, &mut host, address);
    assert!(!before.contains("load.d7."), "{before}");

    let (status, answer) = sent(
        &listener,
        &mut host,
        address,
        (&method, &path),
        &json!({"desk_id": "desk-1", "holder": "a reader"}),
    );
    assert!(status < 300, "{status} {answer}");
    let (status, answer) = sent(
        &listener,
        &mut host,
        address,
        (&method, &path),
        &json!({"desk_id": "desk-2", "holder": 5}),
    );
    assert_eq!(status, 400, "{answer}");

    let counted = [
        r#"spindle_commands_total{command="load.d7.OpenDesk",outcome="opened"} 1"#,
        r#"spindle_command_failures_total{command="load.d7.OpenDesk",failure="invalid_input"} 1"#,
        r#"spindle_command_duration_seconds_bucket{command="load.d7.OpenDesk",le="+Inf"} 2"#,
        r#"spindle_events_appended_total{event="load.d7.DeskOpened"} 1"#,
        r#"spindle_store_rows_written_total{component="hall"} 1"#,
    ];
    let after = scraped(&listener, &mut host, address);
    for line in counted {
        assert!(after.lines().any(|held| held == line), "{line}:\n{after}");
    }
    let rows = r#"spindle_entity_rows{entity="load.d7.Desk"}"#;
    assert!(
        after.lines().any(|line| line == format!("{rows} 1")),
        "{after}"
    );
    assert!(!after.contains("load.d8."), "{after}");
    assert!(!after.contains("load.d7.TakeDesk"), "{after}");
    no_zero_counter(&after);

    host.reset().expect("the stores are reset");
    let reset = scraped(&listener, &mut host, address);
    for line in counted {
        assert!(reset.lines().any(|held| held == line), "{line}:\n{reset}");
    }
    assert!(
        reset.lines().any(|line| line == format!("{rows} 0")),
        "an entity's rows stay written once counted:\n{reset}"
    );
    no_zero_counter(&reset);
}

#[test]
fn a_binding_series_appears_once_an_envelope_was_counted() {
    let ir = spindle_spec::load(&fixtures().join("library"))
        .expect("the library fixture compiles")
        .ir;
    let surface = Surface::new(&ir, &["lending".to_owned()], None)
        .expect("the lending surface is served")
        .observing(16);
    let close = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == "library.lending.CloseLoan")
        .expect("the surface serves CloseLoan");
    let (method, path) = (close.method.as_str().to_uppercase(), close.path.clone());
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut host = Host::new(
        ir,
        &environment("memory"),
        Arc::new(ManualClock::new()),
        state.path(),
    )
    .unwrap_or_else(|error| panic!("the host admits the library: {error}"));
    let listener = Listener::bind(0, surface).expect("the listener binds");
    let address = listener.local_addr();

    let before = scraped(&listener, &mut host, address);
    assert!(!before.contains("spindle_bindings_total{"), "{before}");
    let (status, answer) = sent(
        &listener,
        &mut host,
        address,
        (&method, &path),
        &json!({"loan_id": "loan-1", "member_id": "m-7"}),
    );
    assert!(status < 300, "{status} {answer}");
    let after = scraped(&listener, &mut host, address);
    assert!(
        after.lines().any(|line| {
            line.starts_with("spindle_bindings_total{") && line.contains("thank-on-close")
        }),
        "{after}"
    );
    no_zero_counter(&after);
}
