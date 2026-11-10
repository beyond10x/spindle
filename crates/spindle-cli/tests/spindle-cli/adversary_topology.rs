//! Adversary cases for `GET /__spindle/topology` (`story:live-topology`, wave 18 U4): numbers that
//! must equal the ones `/__spindle/metrics` and `/__spindle/requests` report, node ids the
//! schema says are unique, the bytes of two reads with no traffic between them, and the metrics
//! route answering as it did before `Asked::Metrics` became `Asked::Hosted(Reserved)`.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

const INGRESS: &str = "crates/spindle-cli/tests/fixtures/ingress";
const INGRESS_HTTP: &str = "crates/spindle-cli/tests/fixtures/ingress-http.yaml";
const RALLY: &str = "examples/rally/ess";
const REVIEW: &str = "crates/spindle-cli/tests/fixtures/served-review";
const LAPTOP: &str = "examples/environments/laptop.toml";
const SCHEMA: &str = "ess/generated/jsonschema/schema/types/spindle.observe.Topology.schema.json";

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

fn serve(spec: &Path, arguments: &[&str]) -> Served {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .arg("run")
        .arg(spec)
        .args(["--environment", LAPTOP, "--state"])
        .arg(state.path().join("state"))
        .args(arguments)
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
    let line = match receiver.recv_timeout(STARTUP) {
        Ok(line) => line,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("run printed no line within {STARTUP:?}: {error}");
        }
    };
    let ready: Value = serde_json::from_str(&line).unwrap_or_else(|error| {
        let _ = child.kill();
        let _ = child.wait();
        panic!("run's first line {line:?} is not JSON: {error}")
    });
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

/// One HTTP/1.1 exchange: the status, the head and the body as text.
fn exchange(
    address: SocketAddr,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> (u16, String, String) {
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
    stream
        .read_to_end(&mut answer)
        .expect("the answer is read before the timeout");
    let answer = String::from_utf8(answer).expect("the answer is UTF-8");
    let (head, body) = answer
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("an answer with a head: {answer:?}"));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {head:?}"));
    (status, head.to_owned(), body.to_owned())
}

fn call(address: SocketAddr, method: &str, path: &str, body: Option<&Value>) -> (u16, Value) {
    let (status, _, text) = exchange(address, method, path, body);
    let body = if text.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&text).unwrap_or_else(|error| panic!("{error}: {text:?}"))
    };
    (status, body)
}

fn topology(served: &Served) -> Value {
    let (status, body) = call(served.address, "GET", "/__spindle/topology", None);
    assert_eq!(status, 200, "the topology is answered: {body}");
    body
}

type Sample = (String, BTreeMap<String, String>, f64);

fn counters(served: &Served) -> Vec<Sample> {
    use openmetrics_parser::PrometheusValue;
    let (status, _, exposition) = exchange(served.address, "GET", "/__spindle/metrics", None);
    assert_eq!(status, 200, "the metrics are answered: {exposition}");
    let parsed = openmetrics_parser::prometheus::parse_prometheus(&exposition)
        .unwrap_or_else(|error| panic!("{error:?}: {exposition}"));
    let mut samples = Vec::new();
    for (name, family) in &parsed.families {
        for sample in family.iter_samples() {
            let PrometheusValue::Counter(counter) = &sample.value else {
                continue;
            };
            let labels = sample
                .get_labelset()
                .expect("a sample of a family")
                .iter()
                .map(|(label, value)| (label.clone(), value.clone()))
                .collect();
            samples.push((name.clone(), labels, counter.value.as_f64()));
        }
    }
    samples
}

fn total(samples: &[Sample], family: &str, labels: &[(&str, &str)]) -> f64 {
    samples
        .iter()
        .filter(|(name, held, _)| {
            name == family
                && labels
                    .iter()
                    .all(|(label, value)| held.get(*label).map(String::as_str) == Some(*value))
        })
        .map(|(.., value)| value)
        .sum()
}

fn edges<'a>(topology: &'a Value, kind: &str) -> Vec<&'a Value> {
    topology["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .filter(|edge| edge["kind"] == kind)
        .collect()
}

fn count(edge: &Value) -> f64 {
    edge["traffic"]["count"]
        .as_f64()
        .unwrap_or_else(|| panic!("an edge with a count: {edge}"))
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

/// Calls `operation` at its route, each path variable filled from `body`, a string or a number.
fn submit(
    served: &Served,
    surface: &spindle_ingress::Surface,
    operation: &str,
    body: &Value,
) -> (u16, Value) {
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
    call(
        served.address,
        &route.method.as_str().to_uppercase(),
        &path,
        Some(body),
    )
}

/// The sum of the `emits` edges into each event of `ir`, beside its `spindle_events_appended_total`.
fn emitted_and_appended(
    ir: &ess_compiler::EssIr,
    answered: &Value,
    samples: &[Sample],
) -> Vec<(String, f64, f64)> {
    let emitted = edges(answered, "emits");
    ir.events()
        .keys()
        .map(|event| {
            let node = format!("event:{event}");
            let into = emitted
                .iter()
                .filter(|edge| edge["to"] == node.as_str())
                .map(|edge| count(edge))
                .sum::<f64>();
            let event = event.to_string();
            let appended = total(
                samples,
                "spindle_events_appended_total",
                &[("event", &event)],
            );
            (event, into, appended)
        })
        .collect()
}

/// The desk of `crates/spindle-kernel/tests/spindle-kernel/adversary_set_outcomes.rs`, reached over
/// the network: `StampTeam` publishes `Stamped` once per ticket it changed (ess/25 `each: changed`).
const DESK_SYSTEM: &str = "format: ess/34
system: probe
version: v1
summary: A desk of tickets filed by team and stamped together.
domains:
  - probe.desk
components:
  - component: desk
    summary: Files and stamps tickets.
    owns:
      domains:
        - probe.desk
    accepts:
      commands:
        - probe.desk.File
        - probe.desk.Close
        - probe.desk.StampTeam
    publishes:
      events:
        - probe.desk.Filed
        - probe.desk.Closed
        - probe.desk.Stamped
    reached_by: network
";

const DESK_DOMAIN: &str = "domain: probe.desk
summary: Tickets filed by team, stamped together.
entities:
  - name: probe.desk.Ticket
    identity:
      name: ticket_id
      type: String
    fields:
      - name: team
        type: String
      - name: token
        type: Uuid
    lifecycle:
      initial: Open
      states: [Open, Closed]
      terminal: [Closed]
      transitions:
        - {name: close, from: [Open], to: Closed}
commands:
  - name: probe.desk.File
    input:
      - {name: ticket_id, type: String}
      - {name: team, type: String}
    outcomes:
      - name: filed
        creates: probe.desk.Ticket
        instance: ticket_id
        sets:
          team: input.team
          token: {generated: true}
        emits: [probe.desk.Filed]
        payload:
          probe.desk.Filed:
            ticket_id: input.ticket_id
        summary: The ticket is filed for the team.
  - name: probe.desk.Close
    input:
      - {name: ticket_id, type: String}
    outcomes:
      - name: closed
        moves: probe.desk.Ticket.close
        instance: ticket_id
        emits: [probe.desk.Closed]
        payload:
          probe.desk.Closed:
            ticket_id: input.ticket_id
        summary: The ticket is closed.
  - name: probe.desk.StampTeam
    input:
      - {name: team, type: String}
    outcomes:
      - name: stamped
        updates: probe.desk.Ticket
        instances: {where: team == input.team}
        sets:
          token: {generated: true}
        emits:
          - {event: probe.desk.Stamped, each: changed}
        payload:
          probe.desk.Stamped:
            ticket_id: {changed: ticket_id}
            receipt: {generated: true}
        summary: Every ticket of the team takes a fresh token, each with a receipt of its own.
events:
  - name: probe.desk.Filed
    fields:
      - {name: ticket_id, type: String}
  - name: probe.desk.Closed
    fields:
      - {name: ticket_id, type: String}
  - name: probe.desk.Stamped
    fields:
      - {name: ticket_id, type: String}
      - {name: receipt, type: Uuid}
views:
  - name: probe.desk.TicketCard
    source: probe.desk.Ticket
    consistency: read_your_writes
    fields:
      - {name: ticket_id, type: String}
      - {name: team, type: String}
";

fn desk(directory: &Path) -> PathBuf {
    let target = directory.join("desk");
    std::fs::create_dir_all(target.join("domains")).expect("the desk's directories");
    std::fs::write(target.join("system.yaml"), DESK_SYSTEM).expect("the system is written");
    std::fs::write(target.join("domains/desk.yaml"), DESK_DOMAIN).expect("the domain is written");
    target
}

/// The commit message and the module documentation promise that "the edges into one event add up
/// to `spindle_events_appended_total`". A set outcome publishing an event once per changed row
/// appends one event per row, while the `emits` edge counts the outcome once.
#[test]
fn emits_edges_add_up_to_the_events_a_per_row_set_outcome_appended() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let spec = desk(directory.path());
    let (ir, surface) = surface(&spec);
    let served = serve(&spec, &["--observe"]);
    for ticket in ["t1", "t2", "t3"] {
        let (status, answer) = submit(
            &served,
            &surface,
            "probe.desk.File",
            &json!({"ticket_id": ticket, "team": "a"}),
        );
        assert!((200..300).contains(&status), "{status} {answer}");
    }
    let (status, answer) = submit(
        &served,
        &surface,
        "probe.desk.StampTeam",
        &json!({"team": "a"}),
    );
    assert!((200..300).contains(&status), "{status} {answer}");

    let answered = topology(&served);
    let samples = counters(&served);
    assert_eq!(
        total(
            &samples,
            "spindle_events_appended_total",
            &[("event", "probe.desk.Stamped")]
        ),
        3.0,
        "one Stamped per stamped ticket"
    );
    for (event, into, appended) in emitted_and_appended(&ir, &answered, &samples) {
        assert_eq!(
            into, appended,
            "{event}: the emits edges into it add up to spindle_events_appended_total: {answered}"
        );
    }
}

/// The schema: a node id is "unique within one specification's topology". The specification's
/// node takes its bare system identifier as its id, the ingress takes `ingress`.
#[test]
fn node_ids_are_unique_when_the_system_is_called_ingress() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let source = root().join(INGRESS);
    let target = directory.path().join("ingress-system");
    std::fs::create_dir_all(target.join("domains")).expect("the copy's directories");
    let renamed = |file: &str| {
        std::fs::read_to_string(source.join(file))
            .expect("the file is read")
            .replace("system: tasks\n", "system: ingress\n")
            .replace("tasks.", "ingress.")
    };
    for file in ["system.yaml", "domains/board.yaml", "domains/ledger.yaml"] {
        std::fs::write(target.join(file), renamed(file)).expect("the file is written");
    }
    spindle_spec::load(&target).expect("a system called `ingress` compiles");
    let served = serve(&target, &["--observe"]);
    let answered = topology(&served);
    let ids = answered["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .map(|node| node["id"].as_str().expect("an id").to_owned())
        .collect::<Vec<_>>();
    let unique = ids.iter().cloned().collect::<BTreeSet<_>>();
    assert_eq!(ids.len(), unique.len(), "every node id is unique: {ids:?}");
}

/// Two reads with no traffic between them: the same bytes but for `at`.
#[test]
fn two_reads_without_traffic_answer_the_same_bytes_but_their_time() {
    let served = serve(Path::new(RALLY), &["--observe"]);
    let (_, surface) = surface(&root().join(RALLY));
    let (status, answer) = submit(
        &served,
        &surface,
        "rally.table.OpenMatch",
        &json!({"match_id": "m1", "target_score": 3}),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    let timeless = |text: String| {
        let start = text.find("\"at\":\"").expect("an `at`") + 6;
        let end = start + text[start..].find('"').expect("its end");
        format!("{}{}", &text[..start], &text[end..])
    };
    let read = || {
        let (status, _, body) = exchange(served.address, "GET", "/__spindle/topology", None);
        assert_eq!(status, 200, "{body}");
        timeless(body)
    };
    let first = read();
    let second = read();
    assert_eq!(first, second);
}

/// The metrics route, its neighbours and the topology's, as the base answered them: `HEAD` and
/// `POST` `405` naming `GET`, a query string ignored, a trailing slash `404`.
#[test]
fn the_reserved_hosted_routes_answer_other_methods_and_paths_as_before() {
    let served = serve(Path::new(INGRESS), &["--http", INGRESS_HTTP, "--observe"]);
    for route in ["/__spindle/metrics", "/__spindle/topology"] {
        for method in ["HEAD", "POST", "PUT", "DELETE"] {
            let (status, head, _) = exchange(served.address, method, route, None);
            assert_eq!(status, 405, "{method} {route}: {head}");
            assert!(
                head.to_ascii_lowercase().contains("\r\nallow: get"),
                "{method} {route}: {head}"
            );
        }
        let (status, _, body) = exchange(served.address, "GET", &format!("{route}?x=1"), None);
        assert_eq!(status, 200, "GET {route}?x=1: {body}");
        let (status, _, body) = exchange(served.address, "GET", &format!("{route}/"), None);
        assert_eq!(status, 404, "GET {route}/: {body}");
        let (status, _, body) = exchange(served.address, "GET", &format!("{route}/x"), None);
        assert_eq!(status, 404, "GET {route}/x: {body}");
    }
    let (_, head, body) = exchange(served.address, "GET", "/__spindle/metrics", None);
    assert!(
        head.to_ascii_lowercase()
            .contains("content-type: text/plain"),
        "{head}"
    );
    assert!(body.contains("spindle_http_requests_total"), "{body}");
    let (_, head, _) = exchange(served.address, "GET", "/__spindle/topology", None);
    assert!(
        head.to_ascii_lowercase()
            .contains("content-type: application/json"),
        "{head}"
    );
}

/// A reset empties the stores, and neither the metrics nor the topology forget a count.
#[test]
fn after_a_reset_the_routes_and_emits_edges_still_equal_the_metrics() {
    let (ir, _) = surface(&root().join(INGRESS));
    let served = serve(Path::new(INGRESS), &["--http", INGRESS_HTTP, "--observe"]);
    let (status, answer) = call(
        served.address,
        "PUT",
        "/tasks/t1",
        Some(&json!({"title": "One"})),
    );
    assert_eq!(status, 201, "{answer}");
    let (status, answer) = call(served.address, "POST", "/api/reset", None);
    assert!((200..300).contains(&status), "{status} {answer}");
    let (status, answer) = call(
        served.address,
        "PUT",
        "/tasks/t1",
        Some(&json!({"title": "One again"})),
    );
    assert_eq!(status, 201, "the reset emptied the store: {answer}");
    let answered = topology(&served);
    let samples = counters(&served);
    for edge in edges(&answered, "routes") {
        let to = edge["to"].as_str().expect("a node id");
        let (kind, operation) = to.split_once(':').expect("a kind and a name");
        assert_eq!(
            count(edge),
            total(
                &samples,
                "spindle_http_requests_total",
                &[("operation", operation), ("kind", kind)]
            ),
            "{edge}"
        );
    }
    let open = edges(&answered, "routes")
        .into_iter()
        .find(|edge| edge["to"] == "command:tasks.board.OpenTask")
        .expect("the OpenTask route");
    assert_eq!(count(open), 2.0, "{open}");
    for (event, into, appended) in emitted_and_appended(&ir, &answered, &samples) {
        assert_eq!(into, appended, "{event}: {answered}");
    }
}

/// Requests answered while the topology is read: each read is whole, and the last equals the
/// metrics once the requests are done.
#[test]
fn the_topology_read_beside_concurrent_requests_ends_equal_to_the_metrics() {
    let served = serve(Path::new(INGRESS), &["--http", INGRESS_HTTP, "--observe"]);
    let address = served.address;
    let writers = (0..4)
        .map(|writer| {
            std::thread::spawn(move || {
                for task in 0..10 {
                    let (status, answer) = call(
                        address,
                        "PUT",
                        &format!("/tasks/w{writer}-{task}"),
                        Some(&json!({"title": "Busy"})),
                    );
                    assert_eq!(status, 201, "{answer}");
                }
            })
        })
        .collect::<Vec<_>>();
    let mut last = 0.0;
    for _ in 0..10 {
        let answered = topology(&served);
        let open = edges(&answered, "routes")
            .into_iter()
            .find(|edge| edge["to"] == "command:tasks.board.OpenTask")
            .map(count)
            .expect("the OpenTask route");
        assert!(open >= last, "the count only grows: {open} after {last}");
        last = open;
    }
    for writer in writers {
        writer.join().expect("the writer finishes");
    }
    let answered = topology(&served);
    let samples = counters(&served);
    let open = edges(&answered, "routes")
        .into_iter()
        .find(|edge| edge["to"] == "command:tasks.board.OpenTask")
        .expect("the OpenTask route");
    assert_eq!(count(open), 40.0, "{open}");
    assert_eq!(
        count(open),
        total(
            &samples,
            "spindle_http_requests_total",
            &[("operation", "tasks.board.OpenTask")]
        )
    );
}

/// The schema holds for the topologies the unit's schema case did not read: one with bindings and
/// a periodic one, and one with answer sources.
#[test]
fn the_rally_and_review_topologies_validate_against_the_schema() {
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join(SCHEMA)).expect("the schema is read"),
    )
    .expect("the schema is JSON");
    let validator = jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(&schema)
        .expect("the schema compiles");

    let (_, rally) = surface(&root().join(RALLY));
    let served = serve(Path::new(RALLY), &["--observe"]);
    let (status, answer) = submit(
        &served,
        &rally,
        "rally.table.OpenMatch",
        &json!({"match_id": "m1", "target_score": 3}),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    let answered = topology(&served);
    let errors = validator
        .iter_errors(&answered)
        .map(|error| format!("{error} at {}", error.instance_path()))
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "rally: {errors:?}");
    let typed: Result<spindle_observe_api::Topology, _> = serde_json::from_value(answered.clone());
    assert!(typed.is_ok(), "rally: {typed:?}");
    drop(served);

    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let file = directory.path().join("decisions.toml");
    std::fs::write(&file, "[\"probe.review.Submit\"]\nflagged = false\n")
        .expect("the decisions file is written");
    let file = file.to_str().expect("a UTF-8 path");
    let (_, review) = surface(&root().join(REVIEW));
    let served = serve(
        Path::new(REVIEW),
        &["--decisions", file, "--answer-default", "--observe"],
    );
    let (_, answer) = submit(
        &served,
        &review,
        "probe.review.Submit",
        &json!({"submission_id": "s1", "amount": 5}),
    );
    assert_eq!(answer["outcome"], "submitted", "{answer}");
    let answered = topology(&served);
    let errors = validator
        .iter_errors(&answered)
        .map(|error| format!("{error} at {}", error.instance_path()))
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "review: {errors:?}");
}
