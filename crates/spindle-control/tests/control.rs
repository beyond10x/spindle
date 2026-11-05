//! The control process: what it serves, to whom, and what it keeps.
//!
//! Each case deploys the fixture system `tests/fixtures/notes` (two components, `pad` reached over
//! the network and `shelf` reached only by a binding) on the process target, every instance a
//! `spindle-control-fixture` process that runs its component on `spindle-host`, sends its
//! telemetry and, for `pad`, serves its ingress. Answers are validated against the JSON Schema
//! ESS generates for `spindle.observe` (`ess/generated/jsonschema`), read by ESS's own
//! `schema-contract`.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use serde_json::Value;
use spindle_control::{Clock, Control, History, Options, PlacementDocument};
use spindle_target::Artifact;

/// How long the control process and its instances may take to come up.
const STARTUP: Duration = Duration::from_secs(60);
/// How long one request may take.
const ANSWER: Duration = Duration::from_secs(10);
/// The telemetry period the fixture instances run with.
const PERIOD_MS: u64 = 200;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn spec() -> PathBuf {
    root().join("tests/fixtures/notes")
}

fn environment() -> PathBuf {
    root().join("tests/fixtures/loopback.toml")
}

/// A placement of one `pad` and one `shelf` instance on one process target; `shelf` takes the
/// extra settings in `shelf_settings`.
fn placement(shelf_settings: &str) -> String {
    format!(
        r#"format = "spindle-placement/1"

[[targets]]
id = "host"
kind = "Process"
environment = "loopback"

[[instances]]
id = "pad-0"
component = "pad"
target = "host"
settings = {{ period_ms = "{PERIOD_MS}", seed_note = "n-1" }}

[[instances]]
id = "shelf-0"
component = "shelf"
target = "host"
settings = {{ period_ms = "{PERIOD_MS}"{shelf_settings} }}
"#
    )
}

struct Deployed {
    control: Control,
    _work: tempfile::TempDir,
}

fn deploy(placement: &str) -> Deployed {
    let work = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a work directory");
    let loaded = spindle_spec::load(&spec()).expect("the fixture compiles");
    let text = std::fs::read_to_string(environment()).expect("the environment is read");
    let environment_document =
        spindle_capability::Environment::parse(&text).expect("the environment parses");
    let placement = PlacementDocument::parse(placement).expect("the placement parses");
    let fixture = PathBuf::from(env!("CARGO_BIN_EXE_spindle-control-fixture"));
    let args = vec![
        spec().display().to_string(),
        environment().display().to_string(),
    ];
    let control = Control::start(
        loaded.ir,
        environment_document,
        placement,
        Box::new(move |_component: &str| Artifact::HostBinary {
            path: fixture.clone(),
            args: args.clone(),
        }),
        Options::new(work.path()),
    )
    .expect("the control process starts");
    Deployed {
        control,
        _work: work,
    }
}

/// One answered request: its status and body.
struct Answered {
    status: u16,
    body: String,
}

/// `method path` with `host` as the `Host` header and `headers` beside it.
fn request(
    address: SocketAddr,
    method: &str,
    path: &str,
    host: &str,
    headers: &[(&str, &str)],
) -> Answered {
    let mut stream = TcpStream::connect(address).expect("the control process listens");
    stream.set_read_timeout(Some(ANSWER)).expect("a timeout");
    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("Content-Length: 0\r\n\r\n");
    stream
        .write_all(head.as_bytes())
        .expect("the request is sent");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("the answer is read");
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("no status line in {text:?}"));
    let body = if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        unchunk(body)
    } else {
        body.to_owned()
    };
    Answered { status, body }
}

fn unchunk(mut body: &str) -> String {
    let mut out = String::new();
    while let Some((size, rest)) = body.split_once("\r\n") {
        let Ok(size) = usize::from_str_radix(size.trim(), 16) else {
            break;
        };
        if size == 0 || rest.len() < size {
            break;
        }
        out.push_str(&rest[..size]);
        body = rest[size..].trim_start_matches("\r\n");
    }
    out
}

fn get(control: &Control, path: &str) -> Answered {
    let address = control.local_addr();
    request(
        address,
        "GET",
        path,
        &format!("127.0.0.1:{}", address.port()),
        &[],
    )
}

fn with_token(control: &Control, path: &str) -> String {
    let separator = if path.contains('?') { '&' } else { '?' };
    format!("{path}{separator}t={}", control.token())
}

/// The validator of `spindle.observe.<name>`, from the schema ESS generates.
fn validator(name: &str) -> schema_contract::bundle::RootValidator {
    let path = root().join(format!(
        "../../ess/generated/jsonschema/schema/types/spindle.observe.{name}.schema.json"
    ));
    let source = std::fs::read_to_string(&path).expect("the schema is read");
    let root = format!("spindle.observe.{name}.document");
    let bundle = schema_contract::bundle::import_document(
        &source,
        &root,
        &BTreeSet::new(),
        schema_contract::bundle::Dialect::Draft202012,
    )
    .expect("the schema imports");
    bundle.validator(&root).expect("the root compiles")
}

/// Waits until every instance of the deployment answer is `state`, at most [`STARTUP`].
fn until_every_instance_is(control: &Control, state: &str) -> Value {
    let deadline = Instant::now() + STARTUP;
    loop {
        let answered = get(control, &with_token(control, "/api/deployment"));
        assert_eq!(answered.status, 200, "{}", answered.body);
        let deployment: Value = serde_json::from_str(&answered.body).expect("JSON");
        let states = deployment["instances"]
            .as_array()
            .expect("instances")
            .iter()
            .map(|instance| instance["state"].as_str().unwrap_or_default().to_owned())
            .collect::<Vec<_>>();
        if states.iter().all(|seen| seen == state) {
            return deployment;
        }
        assert!(
            Instant::now() < deadline,
            "the instances are {states:?}, not all {state}, after {STARTUP:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// One message of `GET /api/stream`.
#[derive(Debug)]
struct Message {
    event: String,
    data: Value,
}

/// An open `GET /api/stream`.
struct Stream {
    reader: BufReader<TcpStream>,
}

impl Stream {
    fn open(control: &Control) -> Self {
        let address = control.local_addr();
        let mut stream = TcpStream::connect(address).expect("the control process listens");
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .expect("a timeout");
        let path = with_token(control, "/api/stream");
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n",
            address.port()
        )
        .expect("the request is sent");
        let mut reader = BufReader::new(stream);
        let mut status = String::new();
        let deadline = Instant::now() + ANSWER;
        while status.is_empty() {
            assert!(Instant::now() < deadline, "the stream does not answer");
            let _ = reader.read_line(&mut status);
        }
        assert!(status.contains(" 200 "), "the stream answers {status}");
        Self { reader }
    }

    /// The next message, waiting until `deadline`; `None` when none arrived by then. Chunk-size
    /// lines and blank lines between messages are skipped.
    fn next(&mut self, deadline: Instant) -> Option<Message> {
        let mut event = None;
        let mut line = String::new();
        while Instant::now() < deadline {
            line.clear();
            match self.reader.read_line(&mut line) {
                Ok(0) => return None,
                Ok(_) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    continue;
                }
                Err(error) => panic!("reading the stream: {error}"),
            }
            let line = line.trim_end_matches(['\r', '\n']);
            if let Some(name) = line.strip_prefix("event: ") {
                event = Some(name.to_owned());
            } else if let Some(data) = line.strip_prefix("data: ") {
                let data = serde_json::from_str(data).expect("a message's data is JSON");
                return Some(Message {
                    event: event.take().unwrap_or_else(|| "message".to_owned()),
                    data,
                });
            }
        }
        None
    }
}

#[test]
fn a_request_without_the_token_is_refused() {
    let deployed = deploy(&placement(""));
    let control = &deployed.control;
    let address = control.local_addr();
    let host = format!("127.0.0.1:{}", address.port());
    let token = control.token().to_owned();
    let wrong = format!("{token}x");
    let bearer = format!("Bearer {wrong}");
    for (path, headers) in [
        ("/api/deployment".to_owned(), vec![]),
        (format!("/api/deployment?t={wrong}"), vec![]),
        ("/api/deployment?t=".to_owned(), vec![]),
        (
            "/api/deployment".to_owned(),
            vec![("Authorization", bearer.as_str())],
        ),
    ] {
        let answered = request(address, "GET", &path, &host, &headers);
        assert!(
            (400..500).contains(&answered.status),
            "{path} {headers:?} answered {}",
            answered.status
        );
        for carried in ["pad-0", "shelf-0", "instances", "notes"] {
            assert!(
                !answered.body.contains(carried),
                "{path} {headers:?} carries {carried}: {}",
                answered.body
            );
        }
    }
    let right = format!("Bearer {token}");
    assert_eq!(
        request(
            address,
            "GET",
            "/api/deployment",
            &host,
            &[("Authorization", &right)]
        )
        .status,
        200
    );
    assert_eq!(
        get(control, &with_token(control, "/api/deployment")).status,
        200
    );
}

#[test]
fn a_request_with_a_foreign_host_header_is_refused() {
    let deployed = deploy(&placement(""));
    let control = &deployed.control;
    let address = control.local_addr();
    let port = address.port();
    let path = with_token(control, "/api/deployment");
    for foreign in [
        format!("example.test:{port}"),
        "127.0.0.1".to_owned(),
        format!("127.0.0.1:{}", port.wrapping_add(1)),
        format!("localhost.example.test:{port}"),
    ] {
        let answered = request(address, "GET", &path, &foreign, &[]);
        assert!(
            (400..500).contains(&answered.status),
            "Host {foreign} answered {}",
            answered.status
        );
        assert!(!answered.body.contains("pad-0"), "{}", answered.body);
    }
    for own in [format!("127.0.0.1:{port}"), format!("localhost:{port}")] {
        let answered = request(address, "GET", &path, &own, &[]);
        assert_eq!(answered.status, 200, "Host {own}: {}", answered.body);
    }
}

#[test]
fn the_listener_binds_loopback_only() {
    let deployed = deploy(&placement(""));
    let address = deployed.control.local_addr();
    assert_eq!(address.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
    assert_ne!(address.port(), 0);
    assert!(
        deployed
            .control
            .url()
            .starts_with(&format!("http://127.0.0.1:{}/?t=", address.port())),
        "{}",
        deployed.control.url()
    );
}

#[test]
fn the_deployment_answer_validates_against_its_schema() {
    let deployed = deploy(&placement(""));
    let control = &deployed.control;
    let deployment = until_every_instance_is(control, "Running");
    let errors = validator("Deployment").validate(&deployment);
    assert!(errors.is_empty(), "{errors:?}\n{deployment:#}");

    let names = |key: &str, field: &str| -> Vec<String> {
        deployment[key]
            .as_array()
            .expect("a list")
            .iter()
            .map(|item| item[field].as_str().expect("a name").to_owned())
            .collect()
    };
    assert_eq!(deployment["system"], "notes");
    assert_eq!(names("components", "name"), ["pad", "shelf"]);
    assert_eq!(names("bindings", "name"), ["shelve-note"]);
    assert_eq!(names("instances", "instance_id"), ["pad-0", "shelf-0"]);
    let binding = &deployment["bindings"][0];
    assert_eq!(binding["kind"], "event");
    assert_eq!(binding["from"], "pad");
    assert_eq!(binding["to"], "shelf");
    assert_eq!(binding["event"], "notes.pad.NoteWritten");
    assert_eq!(binding["command"], "notes.shelf.ShelveNote");
    let pad = &deployment["components"][0];
    assert_eq!(pad["commands"], serde_json::json!(["notes.pad.WriteNote"]));
    assert_eq!(pad["views"], serde_json::json!(["notes.pad.AllNotes"]));
    for instance in deployment["instances"].as_array().expect("instances") {
        assert_eq!(instance["target"], "host");
        assert_eq!(instance["target_kind"], "Process");
        assert!(instance["started_at"].is_string(), "{instance}");
    }
}

#[test]
fn the_stream_carries_one_sample_per_instance_per_period() {
    let deployed = deploy(&placement(""));
    let control = &deployed.control;
    let mut stream = Stream::open(control);
    let sample = validator("Sample");
    let mut seen: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    let deadline = Instant::now() + STARTUP;
    let mut samples = 0;
    while samples < 6 {
        let message = stream
            .next(deadline)
            .unwrap_or_else(|| panic!("six samples did not arrive; seen {seen:?}"));
        if message.event != "sample" {
            continue;
        }
        let errors = sample.validate(&message.data);
        assert!(errors.is_empty(), "{errors:?}\n{:#}", message.data);
        assert_eq!(message.data["period_ms"], PERIOD_MS);
        let instance = message.data["instance_id"]
            .as_str()
            .expect("an id")
            .to_owned();
        let sequences = seen.entry(instance.clone()).or_default();
        if sequences.len() == 3 {
            continue;
        }
        sequences.push(message.data["sequence"].as_u64().expect("a sequence"));
        samples += 1;
    }
    assert_eq!(
        seen.keys().collect::<Vec<_>>(),
        ["pad-0", "shelf-0"],
        "{seen:?}"
    );
    for (instance, sequences) in &seen {
        assert_eq!(sequences.len(), 3, "{instance}: {sequences:?}");
        assert!(
            sequences.windows(2).all(|pair| pair[1] == pair[0] + 1),
            "{instance} is not sampled once per period: {sequences:?}"
        );
    }
}

#[test]
fn a_state_change_is_streamed() {
    let deployed = deploy(&placement(", exit_after_ms = \"3000\""));
    let control = &deployed.control;
    let mut stream = Stream::open(control);
    let changes = validator("StateChange");
    let deadline = Instant::now() + STARTUP;
    let mut exited = Vec::new();
    while exited.is_empty() {
        let message = stream
            .next(deadline)
            .expect("shelf-0 exits within the startup bound");
        if message.event == "state" && message.data["instance_id"] == "shelf-0" {
            exited.push(message.data);
        }
    }
    let settle = Instant::now() + Duration::from_millis(PERIOD_MS * 5);
    while let Some(message) = stream.next(settle) {
        if message.event == "state" && message.data["instance_id"] == "shelf-0" {
            exited.push(message.data);
        }
    }
    assert_eq!(exited.len(), 1, "{exited:#?}");
    let change = &exited[0];
    let errors = changes.validate(change);
    assert!(errors.is_empty(), "{errors:?}\n{change:#}");
    assert_eq!(change["from"], "Running");
    assert_eq!(change["to"], "Exited");
    let deployment: Value =
        serde_json::from_str(&get(control, &with_token(control, "/api/deployment")).body)
            .expect("JSON");
    let states = deployment["instances"]
        .as_array()
        .expect("instances")
        .iter()
        .map(|instance| {
            (
                instance["instance_id"].as_str().unwrap_or_default(),
                instance["state"].as_str().unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(states, [("pad-0", "Running"), ("shelf-0", "Exited")]);
}

#[test]
fn a_view_is_answered_through_the_instance_ingress() {
    let deployed = deploy(&placement(""));
    let control = &deployed.control;
    until_every_instance_is(control, "Running");
    let ingress = control
        .ingress("pad-0")
        .expect("pad is reached over the network, so its instance has an ingress");
    assert_eq!(ingress.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
    let own_path = "/pad/views/AllNotes";
    let deadline = Instant::now() + STARTUP;
    let own = loop {
        if let Ok(mut stream) = TcpStream::connect(ingress) {
            let _ = stream.set_read_timeout(Some(ANSWER));
            let _ = write!(
                stream,
                "GET {own_path} HTTP/1.1\r\nHost: {ingress}\r\nConnection: close\r\n\r\n"
            );
            let mut raw = String::new();
            let _ = stream.read_to_string(&mut raw);
            if let Some((_, body)) = raw.split_once("\r\n\r\n")
                && raw.starts_with("HTTP/1.1 200")
            {
                break body.to_owned();
            }
        }
        assert!(
            Instant::now() < deadline,
            "pad-0 serves no view at {ingress}"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    let own: Value = serde_json::from_str(&own).expect("the instance answers JSON");
    assert_eq!(own["rows"][0]["note_id"], "n-1", "{own:#}");

    let path = with_token(control, "/api/views/pad/notes.pad.AllNotes");
    let answered = get(control, &path);
    assert_eq!(answered.status, 200, "{}", answered.body);
    let through: Value = serde_json::from_str(&answered.body).expect("JSON");
    assert_eq!(through, own);

    let address = control.local_addr();
    let host = format!("127.0.0.1:{}", address.port());
    for method in ["POST", "PUT", "DELETE"] {
        let refused = request(address, method, &path, &host, &[]);
        assert_eq!(refused.status, 405, "{method} {path}: {}", refused.body);
    }
    for unknown in [
        "/api/views/pad/notes.pad.NoView",
        "/api/views/nobody/notes.pad.AllNotes",
    ] {
        assert_eq!(get(control, &with_token(control, unknown)).status, 404);
    }
    assert_eq!(get(control, &path).body, answered.body);
}

/// A clock the test moves by hand.
struct Manual(std::sync::Mutex<SystemTime>);

impl Manual {
    fn advance(&self, by: Duration) {
        let mut now = self.0.lock().expect("the clock");
        *now += by;
    }
}

impl Clock for Manual {
    fn now(&self) -> SystemTime {
        *self.0.lock().expect("the clock")
    }
}

fn sample(instance: &str, sequence: u64) -> Value {
    serde_json::json!({
        "instance_id": instance,
        "sequence": sequence,
        "at": "2026-11-04T10:00:00Z",
        "period_ms": 1000,
        "counters": [],
        "histograms": [],
        "recent": [],
        "resources": {"cpu_seconds": 0.0, "memory_bytes": 0},
        "dropped_frames": 0
    })
}

#[test]
fn samples_older_than_fifteen_minutes_are_dropped() {
    let period = Duration::from_secs(1);
    let clock = Arc::new(Manual(std::sync::Mutex::new(SystemTime::UNIX_EPOCH)));
    let mut history = History::new(spindle_control::WINDOW, clock.clone());
    assert_eq!(spindle_control::WINDOW, Duration::from_secs(15 * 60));

    history.push("a-0", sample("a-0", 0));
    clock.advance(period);
    history.push("a-0", sample("a-0", 1));
    history.push("b-0", sample("b-0", 0));
    clock.advance(Duration::from_secs(15 * 60));
    history.push("a-0", sample("a-0", 2));

    let held = |history: &History, instance: &str| -> Vec<u64> {
        history
            .samples(instance)
            .iter()
            .map(|sample| sample["sequence"].as_u64().expect("a sequence"))
            .collect()
    };
    assert_eq!(
        held(&history, "a-0"),
        [1, 2],
        "the sample taken 15 minutes and one period before the newest is gone, the one within \
         the window is held"
    );
    assert_eq!(held(&history, "b-0"), [0]);
    clock.advance(period);
    history.push("a-0", sample("a-0", 3));
    assert_eq!(held(&history, "a-0"), [2, 3]);
    assert_eq!(held(&history, "b-0"), Vec::<u64>::new());
}
