//! Adversary cases for the control process: what a deployment exposes beside the observe API, what
//! the telemetry socket believes, and what a stream opened late is told.
//!
//! Each case deploys the fixture system `tests/fixtures/notes` on the process target with
//! `spindle-control-fixture` instances, as `tests/control.rs` does.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use std::time::{Duration, Instant};

use serde_json::Value;
use spindle_control::{Control, Options, PlacementDocument};
use spindle_target::Artifact;

const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn spec() -> PathBuf {
    root().join("tests/fixtures/notes")
}

fn environment() -> PathBuf {
    root().join("tests/fixtures/loopback.toml")
}

fn placement(pad: &str, shelf: &str) -> String {
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
settings = {{ {pad} }}

[[instances]]
id = "shelf-0"
component = "shelf"
target = "host"
settings = {{ {shelf} }}
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

struct Answered {
    status: u16,
    body: String,
}

fn request(
    address: SocketAddr,
    method: &str,
    path: &str,
    host: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Answered {
    let mut stream = TcpStream::connect(address).expect("listening");
    stream.set_read_timeout(Some(ANSWER)).expect("a timeout");
    let mut head = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
    stream.write_all(head.as_bytes()).expect("sent");
    let mut raw = Vec::new();
    let _ = stream.read_to_end(&mut raw);
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("no status line in {text:?}"));
    Answered {
        status,
        body: body.to_owned(),
    }
}

fn get(control: &Control, path: &str) -> Answered {
    let address = control.local_addr();
    let separator = if path.contains('?') { '&' } else { '?' };
    let path = format!("{path}{separator}t={}", control.token());
    request(
        address,
        "GET",
        &path,
        &format!("127.0.0.1:{}", address.port()),
        &[],
        "",
    )
}

fn until_every_instance_is(control: &Control, state: &str) {
    let deadline = Instant::now() + STARTUP;
    loop {
        let answered = get(control, "/api/deployment");
        let deployment: Value = serde_json::from_str(&answered.body).expect("JSON");
        let all = deployment["instances"]
            .as_array()
            .expect("instances")
            .iter()
            .all(|instance| instance["state"] == state);
        if all {
            return;
        }
        assert!(Instant::now() < deadline, "not all {state}: {deployment}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Waits until pad-0's ingress answers its own view.
fn until_ingress_answers(ingress: SocketAddr, token: &str) {
    let bearer = format!("Bearer {token}");
    let deadline = Instant::now() + STARTUP;
    loop {
        if TcpStream::connect(ingress).is_ok() {
            let answered = request(
                ingress,
                "GET",
                "/pad/views/AllNotes",
                &ingress.to_string(),
                &[("Authorization", &bearer)],
                "",
            );
            if answered.status == 200 {
                return;
            }
        }
        assert!(Instant::now() < deadline, "no ingress at {ingress}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn write_note_route() -> (String, String) {
    let loaded = spindle_spec::load(&spec()).expect("the fixture compiles");
    let surface = spindle_ingress::Surface::new(&loaded.ir, &["pad".to_owned()], None)
        .expect("pad's surface");
    let route = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == "notes.pad.WriteNote")
        .expect("WriteNote is routed");
    (route.method.as_str().to_uppercase(), route.path.clone())
}

/// The control process says it stops DNS rebinding and other local users; a deployment's own
/// instance ingress, started by the same `deploy`, takes a write from a page whose name was
/// rebound to 127.0.0.1, with no token and a foreign `Host`.
#[test]
fn a_deployed_ingress_refuses_a_command_with_a_foreign_host() {
    let deployed = deploy(&placement(
        "period_ms = \"200\", seed_note = \"n-1\"",
        "period_ms = \"200\"",
    ));
    let control = &deployed.control;
    until_every_instance_is(control, "Running");
    let ingress = control.ingress("pad-0").expect("pad has an ingress");
    until_ingress_answers(ingress, control.ingress_token());

    let (method, path) = write_note_route();
    let foreign = format!("rebound.example.test:{}", ingress.port());
    let answered = request(
        ingress,
        &method,
        &path,
        &foreign,
        &[("Content-Type", "application/json")],
        r#"{"note_id":"n-planted","text":"from another site"}"#,
    );
    let view = get(control, "/api/views/pad/notes.pad.AllNotes");
    assert!(
        (400..500).contains(&answered.status) && !view.body.contains("n-planted"),
        "{method} {path} with Host {foreign} and no token answered {} {}; the view through \
         control now reads {}",
        answered.status,
        answered.body,
        view.body
    );
}

/// The same ingress empties every store of the instance on `POST /api/reset`, from any Host.
#[test]
fn a_deployed_ingress_refuses_a_reset_with_a_foreign_host() {
    let deployed = deploy(&placement(
        "period_ms = \"200\", seed_note = \"n-1\"",
        "period_ms = \"200\"",
    ));
    let control = &deployed.control;
    until_every_instance_is(control, "Running");
    let ingress = control.ingress("pad-0").expect("pad has an ingress");
    until_ingress_answers(ingress, control.ingress_token());

    let foreign = format!("rebound.example.test:{}", ingress.port());
    let answered = request(ingress, "POST", "/api/reset", &foreign, &[], "");
    let view = get(control, "/api/views/pad/notes.pad.AllNotes");
    assert!(
        (400..500).contains(&answered.status) && view.body.contains("n-1"),
        "POST /api/reset with Host {foreign} and no token answered {} {}; the view through \
         control now reads {}",
        answered.status,
        answered.body,
        view.body
    );
}

/// The socket path, as `Control`'s `Debug` names it.
fn socket_of(control: &Control) -> PathBuf {
    let debug = format!("{control:?}");
    let after = debug
        .split_once("socket: \"")
        .unwrap_or_else(|| panic!("no socket in {debug}"))
        .1;
    PathBuf::from(after.split_once('"').expect("a closing quote").0)
}

/// A connection to the telemetry socket that is not pad-0's sends a frame naming pad-0; the
/// stream publishes it as pad-0's sample.
#[test]
fn a_frame_naming_another_instance_is_not_published_as_its_sample() {
    let deployed = deploy(&placement("period_ms = \"200\"", "period_ms = \"200\""));
    let control = &deployed.control;
    until_every_instance_is(control, "Running");
    let socket = socket_of(control);

    let address = control.local_addr();
    let mut stream = TcpStream::connect(address).expect("listening");
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .expect("a timeout");
    write!(
        stream,
        "GET /api/stream?t={} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n",
        control.token(),
        address.port()
    )
    .expect("sent");

    let forged = serde_json::json!({
        "instance_id": "pad-0",
        "sequence": 987_654_321u64,
        "at": "2026-11-05T10:00:00Z",
        "period_ms": 200,
        "counters": [],
        "histograms": [],
        "recent": [],
        "resources": {"cpu_seconds": 0.0, "memory_bytes": 0},
        "dropped_frames": 0
    });
    let mut forger = UnixStream::connect(&socket).expect("the telemetry socket accepts");
    writeln!(forger, "{forged}").expect("the frame is sent");

    let mut reader = BufReader::new(stream);
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut line = String::new();
    while Instant::now() < deadline {
        line.clear();
        if reader.read_line(&mut line).is_err() {
            continue;
        }
        if line.starts_with("data: ") && line.contains("987654321") {
            panic!("a frame from a connection that is not pad-0's was published: {line}");
        }
    }
}
