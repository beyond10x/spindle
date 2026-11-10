//! Adversary pass 2 for `GET /__spindle/topology` (`story:live-topology`, wave 18 U4): the fix
//! round's `Host::emitted`, which counts the events of each successful `execute`, against
//! `spindle_events_appended_total`, which counts the events a store commit carries. The module
//! documentation of `observe/topology.rs` and `Host::emitted` both promise that the `emits` edges
//! into one event add up to the appended counter.
//!
//! The kernel answers events no store records in two shapes it documents itself: an outcome with
//! no subject, and a set outcome that selected no row (`execute/set.rs`, "With no row selected the
//! events are answered and no store records them, as for an outcome without a subject").

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

const LAPTOP: &str = "examples/environments/laptop.toml";
const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

struct Served {
    child: Child,
    address: SocketAddr,
    _state: tempfile::TempDir,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn serve(spec: &Path) -> Served {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .arg("run")
        .arg(spec)
        .args(["--environment", LAPTOP, "--state"])
        .arg(state.path().join("state"))
        .arg("--observe")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("b10x-spindle starts");
    let stdout = child.stdout.take().expect("stdout is piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = sender.send(line);
    });
    let line = receiver.recv_timeout(STARTUP).unwrap_or_else(|error| {
        let _ = child.kill();
        let _ = child.wait();
        panic!("run printed no line within {STARTUP:?}: {error}")
    });
    let ready: Value = serde_json::from_str(&line).expect("run's first line is JSON");
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("run's first line names no address: {ready}"));
    Served {
        child,
        address,
        _state: state,
    }
}

fn exchange(address: SocketAddr, method: &str, path: &str, body: Option<&Value>) -> (u16, String) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    let body = body.map(Value::to_string).unwrap_or_default();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .expect("the request is written");
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).expect("the answer is read");
    let answer = String::from_utf8(answer).expect("the answer is UTF-8");
    let (head, body) = answer
        .split_once("\r\n\r\n")
        .expect("an answer with a head");
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .expect("a status line");
    (status, body.to_owned())
}

fn submit(
    served: &Served,
    surface: &spindle_ingress::Surface,
    operation: &str,
    body: &Value,
) -> (u16, String) {
    let route = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == operation)
        .unwrap_or_else(|| panic!("a route for {operation}"));
    let path = route
        .variables()
        .iter()
        .fold(route.path.clone(), |path, variable| {
            let value = match &body[variable.field.as_str()] {
                Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            path.replace(&format!("{{{}}}", variable.field), &value)
        });
    exchange(
        served.address,
        &route.method.as_str().to_uppercase(),
        &path,
        Some(body),
    )
}

fn surface(spec: &Path) -> (ess_compiler::EssIr, spindle_ingress::Surface) {
    let ir = spindle_spec::load(spec).expect("the fixture compiles").ir;
    let components = ir
        .components()
        .values()
        .filter(|component| matches!(component.reached_by, ess_domain::component::Reach::Network))
        .map(|component| component.name.as_str().to_owned())
        .collect::<Vec<_>>();
    let surface = spindle_ingress::Surface::new(&ir, &components, None)
        .expect("the fixture's surface is routed");
    (ir, surface)
}

/// The `emits` edges into each event, summed, beside `spindle_events_appended_total{event}`.
fn emitted_and_appended(ir: &ess_compiler::EssIr, served: &Served) -> BTreeMap<String, (f64, f64)> {
    use openmetrics_parser::PrometheusValue;
    let (status, body) = exchange(served.address, "GET", "/__spindle/topology", None);
    assert_eq!(status, 200, "{body}");
    let topology: Value = serde_json::from_str(&body).expect("the topology is JSON");
    let (status, exposition) = exchange(served.address, "GET", "/__spindle/metrics", None);
    assert_eq!(status, 200, "{exposition}");
    let parsed = openmetrics_parser::prometheus::parse_prometheus(&exposition)
        .unwrap_or_else(|error| panic!("{error:?}: {exposition}"));
    let mut appended = BTreeMap::<String, f64>::new();
    for (name, family) in &parsed.families {
        if name != "spindle_events_appended_total" {
            continue;
        }
        for sample in family.iter_samples() {
            let PrometheusValue::Counter(counter) = &sample.value else {
                continue;
            };
            let labels = sample.get_labelset().expect("a sample of a family");
            let event = labels
                .iter()
                .find(|(label, _)| label.as_str() == "event")
                .map(|(_, value)| value.clone())
                .expect("an event label");
            *appended.entry(event).or_default() += counter.value.as_f64();
        }
    }
    ir.events()
        .keys()
        .map(|event| {
            let node = format!("event:{event}");
            let into = topology["edges"]
                .as_array()
                .expect("edges")
                .iter()
                .filter(|edge| edge["kind"] == "emits" && edge["to"] == node.as_str())
                .map(|edge| edge["traffic"]["count"].as_f64().expect("a count"))
                .sum::<f64>();
            let event = event.to_string();
            let appended = appended.get(&event).copied().unwrap_or_default();
            (event, (into, appended))
        })
        .collect()
}

const BELL_SYSTEM: &str = "format: ess/34
system: bell
version: v1
summary: Rings a bell and sweeps tickets by team.
domains:
  - bell.desk
components:
  - component: desk
    summary: Rings and sweeps.
    owns:
      domains:
        - bell.desk
    accepts:
      commands:
        - bell.desk.File
        - bell.desk.Ring
        - bell.desk.Sweep
    publishes:
      events:
        - bell.desk.Filed
        - bell.desk.Rang
        - bell.desk.Swept
    reached_by: network
";

const BELL_DOMAIN: &str = "domain: bell.desk
summary: Tickets filed by team, swept together; a bell rung by anyone.
entities:
  - name: bell.desk.Ticket
    identity:
      name: ticket_id
      type: String
    fields:
      - name: team
        type: String
    lifecycle:
      initial: Open
      states: [Open, Closed]
      terminal: [Closed]
      transitions:
        - {name: close, from: [Open], to: Closed}
commands:
  - name: bell.desk.File
    input:
      - {name: ticket_id, type: String}
      - {name: team, type: String}
    outcomes:
      - name: filed
        creates: bell.desk.Ticket
        instance: ticket_id
        sets:
          team: input.team
        emits: [bell.desk.Filed]
        payload:
          bell.desk.Filed:
            ticket_id: input.ticket_id
        summary: The ticket is filed for the team.
  - name: bell.desk.Ring
    input:
      - {name: note, type: String}
    outcomes:
      - name: rang
        emits: [bell.desk.Rang]
        payload:
          bell.desk.Rang:
            note: input.note
        summary: The bell rings; no row changes.
  - name: bell.desk.Sweep
    input:
      - {name: team, type: String}
    outcomes:
      - name: swept
        moves: bell.desk.Ticket.close
        instances: {where: team == input.team}
        emits: [bell.desk.Swept]
        payload:
          bell.desk.Swept:
            team: input.team
        summary: Every open ticket of the team is closed, and one Swept says so.
events:
  - name: bell.desk.Filed
    fields:
      - {name: ticket_id, type: String}
  - name: bell.desk.Rang
    fields:
      - {name: note, type: String}
  - name: bell.desk.Swept
    fields:
      - {name: team, type: String}
views:
  - name: bell.desk.TicketCard
    source: bell.desk.Ticket
    consistency: read_your_writes
    fields:
      - {name: ticket_id, type: String}
      - {name: team, type: String}
";

fn bell(directory: &Path) -> PathBuf {
    let target = directory.join("bell");
    std::fs::create_dir_all(target.join("domains")).expect("the bell's directories");
    std::fs::write(target.join("system.yaml"), BELL_SYSTEM).expect("the system is written");
    std::fs::write(target.join("domains/desk.yaml"), BELL_DOMAIN).expect("the domain is written");
    target
}

/// An outcome with no subject publishes its event and no store records it, so
/// `spindle_events_appended_total` stays 0; the `emits` edge reads `Host::emitted`, which counted it.
#[test]
fn an_outcome_without_a_subject_adds_to_the_emits_edge_what_the_appended_counter_adds() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let spec = bell(directory.path());
    let (ir, surface) = surface(&spec);
    let served = serve(&spec);
    let (status, answer) = submit(&served, &surface, "bell.desk.Ring", &json!({"note": "hi"}));
    assert!((200..300).contains(&status), "{status} {answer}");
    let counts = emitted_and_appended(&ir, &served);
    let (into, appended) = counts["bell.desk.Rang"];
    assert_eq!(
        into, appended,
        "bell.desk.Rang: the emits edges into it add up to spindle_events_appended_total: {counts:?}"
    );
}

/// A set outcome that selected no row answers its once-per-outcome event and commits nothing.
#[test]
fn a_set_outcome_over_no_row_adds_to_the_emits_edge_what_the_appended_counter_adds() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let spec = bell(directory.path());
    let (ir, surface) = surface(&spec);
    let served = serve(&spec);
    let (status, answer) = submit(
        &served,
        &surface,
        "bell.desk.File",
        &json!({"ticket_id": "t1", "team": "a"}),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    let (status, answer) = submit(&served, &surface, "bell.desk.Sweep", &json!({"team": "a"}));
    assert!((200..300).contains(&status), "{status} {answer}");
    let (status, answer) = submit(
        &served,
        &surface,
        "bell.desk.Sweep",
        &json!({"team": "nobody"}),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    let counts = emitted_and_appended(&ir, &served);
    let (into, appended) = counts["bell.desk.Swept"];
    assert_eq!(
        into, appended,
        "bell.desk.Swept: the emits edges into it add up to spindle_events_appended_total: {counts:?}"
    );
}
