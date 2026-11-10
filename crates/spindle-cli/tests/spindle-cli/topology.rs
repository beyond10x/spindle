//! `run --observe` answering `GET /__spindle/topology` (`story:live-topology`): the graph derived
//! from the loaded specification, from the ingress down to the bindings, each edge but `contains`
//! with live numbers read from the counters `GET /__spindle/metrics` exposes and the ring
//! `GET /__spindle/requests` holds.
//!
//! The ingress fixture `tests/fixtures/ingress` is the one `ess/fixtures/observe/topology/tasks.json`
//! draws; `tests/fixtures/partial` has routes answered `501`; `tests/fixtures/served-review` asks
//! external causes; the rally example carries events between its three components over event
//! bindings, served by `run` in one process.

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
const PARTIAL: &str = "crates/spindle-cli/tests/fixtures/partial";
const REVIEW: &str = "crates/spindle-cli/tests/fixtures/served-review";
const RALLY: &str = "examples/rally/ess";
const LAPTOP: &str = "examples/environments/laptop.toml";
const TASKS: &str = "ess/fixtures/observe/topology/tasks.json";
const SCHEMA: &str = "ess/generated/jsonschema/schema/types/spindle.observe.Topology.schema.json";

/// How long `run` may take to admit, open its state and print where it listens.
const STARTUP: Duration = Duration::from_secs(60);
/// How long one request may take to be answered.
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A running `b10x-spindle run` serving HTTP; stopped when dropped.
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

/// `run` on the specification at `spec` (relative to the repository, or absolute) and laptop,
/// with `arguments` added.
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

/// One HTTP/1.1 exchange on its own connection: the status and the body as text.
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
    (status, body.to_owned())
}

/// `exchange`, its body read as JSON.
fn call(address: SocketAddr, method: &str, path: &str, body: Option<&Value>) -> (u16, Value) {
    let (status, text) = exchange(address, method, path, body);
    let body = if text.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&text).unwrap_or_else(|error| panic!("{error}: {text:?}"))
    };
    (status, body)
}

/// `GET /__spindle/topology`, answered `200`.
fn topology(served: &Served) -> Value {
    let (status, body) = call(served.address, "GET", "/__spindle/topology", None);
    assert_eq!(status, 200, "the topology is answered: {body}");
    body
}

/// `GET /__spindle/requests`, answered `200`.
fn log(served: &Served) -> Value {
    let (status, body) = call(served.address, "GET", "/__spindle/requests", None);
    assert_eq!(status, 200, "the request log is answered: {body}");
    body
}

/// One counter sample: its family, its labels and its value.
type Sample = (String, BTreeMap<String, String>, f64);

/// Every counter sample of `GET /__spindle/metrics`, as a Prometheus text parser reads it.
fn counters(served: &Served) -> Vec<Sample> {
    use openmetrics_parser::PrometheusValue;
    let (status, exposition) = exchange(served.address, "GET", "/__spindle/metrics", None);
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

/// The sum of every sample of `family` whose labels include each of `labels`.
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

/// The edges of `topology` of `kind`.
fn edges<'a>(topology: &'a Value, kind: &str) -> Vec<&'a Value> {
    topology["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .filter(|edge| edge["kind"] == kind)
        .collect()
}

/// An edge's count, as a number.
fn count(edge: &Value) -> f64 {
    edge["traffic"]["count"]
        .as_f64()
        .unwrap_or_else(|| panic!("an edge with a count: {edge}"))
}

/// `topology` without its live numbers: its format, specifications and nodes, and each edge's
/// ends and kind, `traffic` kept as whether it is there.
fn structure(topology: &Value) -> Value {
    let edges = topology["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .map(|edge| {
            json!({
                "from": edge["from"],
                "to": edge["to"],
                "kind": edge["kind"],
                "traffic": edge.get("traffic").is_some(),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "format": topology["format"],
        "specifications": topology["specifications"],
        "nodes": topology["nodes"],
        "edges": edges,
    })
}

/// The node ids and the `(from, to, kind)` of every edge of `topology`.
fn graph(topology: &Value) -> (BTreeSet<String>, BTreeSet<(String, String, String)>) {
    let text = |value: &Value| value.as_str().expect("a string").to_owned();
    let nodes = topology["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .map(|node| text(&node["id"]))
        .collect();
    let edges = topology["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .map(|edge| (text(&edge["from"]), text(&edge["to"]), text(&edge["kind"])))
        .collect();
    (nodes, edges)
}

/// The fixture the topology of the ingress fixture is held to.
fn tasks() -> Value {
    serde_json::from_str(&std::fs::read_to_string(root().join(TASKS)).expect("the fixture is read"))
        .expect("the fixture is JSON")
}

/// The node id of the operation a route serves: `command:<name>` or `view:<name>`.
fn operation_node(route: &spindle_ingress::Route) -> String {
    match &route.operation {
        ess_http::Operation::Command(_) => format!("command:{}", route.operation.name()),
        ess_http::Operation::View(_) => format!("view:{}", route.operation.name()),
    }
}

/// The surface `run` serves for `spec`: every component reached over the network.
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

/// Calls `operation` at the route `surface` puts it at, with `body`.
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
            let value = body[variable.field.as_str()].as_str().unwrap_or_default();
            path.replace(&format!("{{{}}}", variable.field), value)
        });
    call(
        served.address,
        &route.method.as_str().to_uppercase(),
        &path,
        Some(body),
    )
}

/// The scripted sequence of `the_request_log_lists_a_scripted_sequence_newest_first`
/// (`crates/spindle-cli/tests/spindle-cli/observe.rs`) on the ingress fixture and its binding.
fn scripted(served: &Served) {
    let steps: [(&str, &str, Option<Value>, u16); 7] = [
        (
            "PUT",
            "/tasks/t1",
            Some(json!({"title": "Water the plants"})),
            201,
        ),
        ("POST", "/tasks/t1/close", None, 200),
        ("POST", "/tasks/t1/close", None, 410),
        ("GET", "/tasks", None, 200),
        (
            "PUT",
            "/tasks/t2",
            Some(json!({"title": "Sweep", "priority": 1})),
            400,
        ),
        ("GET", "/nowhere", None, 404),
        ("DELETE", "/tasks", None, 405),
    ];
    for (method, path, body, expected) in steps {
        let (status, answer) = call(served.address, method, path, body.as_ref());
        assert_eq!(status, expected, "{method} {path}: {answer}");
    }
}

/// `topology` with its `at` checked to be a string and taken out.
fn timeless(mut topology: Value) -> Value {
    let at = topology
        .as_object_mut()
        .expect("a topology is an object")
        .remove("at");
    assert!(at.is_some_and(|at| at.is_string()), "{topology}");
    topology
}

#[test]
fn without_observe_the_topology_answers_404() {
    let served = serve(Path::new(INGRESS), &["--http", INGRESS_HTTP]);
    let (status, body) = call(served.address, "GET", "/__spindle/topology", None);
    assert_eq!(status, 404, "{body}");
    assert!(body["refused"].is_string(), "{body}");
}

/// A command `ADDED` adds to the ingress fixture's board: no event, no external cause.
const ADDED: &str = "
  - name: tasks.board.PingBoard
    input:
      - name: note
        type: String
    outcomes:
      - name: pinged
        accepts: nothing
        summary: Nothing changes.
";

/// Copies the ingress fixture into `directory`, `ADDED` declared in the board and accepted by its
/// component where `added`.
fn ingress_copy(directory: &Path, added: bool) -> PathBuf {
    let source = root().join(INGRESS);
    let target = directory.join(if added { "added" } else { "plain" });
    std::fs::create_dir_all(target.join("domains")).expect("the copy's directories");
    let mut system =
        std::fs::read_to_string(source.join("system.yaml")).expect("the system is read");
    let mut board =
        std::fs::read_to_string(source.join("domains/board.yaml")).expect("the board is read");
    if added {
        system = system.replacen(
            "        - tasks.board.CloseTask\n",
            "        - tasks.board.CloseTask\n        - tasks.board.PingBoard\n",
            1,
        );
        board = board.replacen("\nevents:\n", &format!("{ADDED}events:\n"), 1);
    }
    std::fs::write(target.join("system.yaml"), system).expect("the system is written");
    std::fs::write(target.join("domains/board.yaml"), board).expect("the board is written");
    std::fs::copy(
        source.join("domains/ledger.yaml"),
        target.join("domains/ledger.yaml"),
    )
    .expect("the ledger is copied");
    target
}

#[test]
fn the_topology_is_derived_from_the_specification() {
    let served = serve(Path::new(INGRESS), &["--http", INGRESS_HTTP, "--observe"]);
    let answered = topology(&served);
    assert_eq!(structure(&answered), structure(&tasks()), "{answered}");

    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let plain = graph(&topology(&serve(
        &ingress_copy(directory.path(), false),
        &["--observe"],
    )));
    let added = graph(&topology(&serve(
        &ingress_copy(directory.path(), true),
        &["--observe"],
    )));
    let command = "command:tasks.board.PingBoard".to_owned();
    let mut expected = plain.clone();
    expected.0.insert(command.clone());
    expected
        .1
        .insert(("ingress".to_owned(), command.clone(), "routes".to_owned()));
    expected.1.insert((
        "domain:tasks.board".to_owned(),
        command,
        "contains".to_owned(),
    ));
    assert_eq!(
        added, expected,
        "only the command, its route and its domain"
    );
}

#[test]
fn every_served_route_has_one_routes_edge() {
    for (spec, unserved) in [(INGRESS, 0), (PARTIAL, 2)] {
        let (_, surface) = surface(&root().join(spec));
        assert_eq!(
            surface.unserved().len(),
            unserved,
            "{spec}: the fixture's routes answered 501"
        );
        let served = serve(Path::new(spec), &["--observe"]);
        let answered = topology(&served);
        let mut routed = edges(&answered, "routes")
            .into_iter()
            .map(|edge| {
                assert_eq!(edge["from"], "ingress", "{edge}");
                edge["to"].as_str().expect("a node id").to_owned()
            })
            .collect::<Vec<_>>();
        routed.sort();
        let mut routes = surface
            .routes()
            .iter()
            .map(operation_node)
            .collect::<Vec<_>>();
        routes.sort();
        assert_eq!(routed, routes, "{spec}: one edge per route: {answered}");
    }
}

#[test]
fn routes_edges_count_a_scripted_sequence() {
    let served = serve(Path::new(INGRESS), &["--http", INGRESS_HTTP, "--observe"]);
    scripted(&served);
    let answered = topology(&served);
    let samples = counters(&served);
    let logged = log(&served);
    let records = logged["requests"].as_array().expect("requests");
    let routed = edges(&answered, "routes");
    assert_eq!(routed.len(), 4, "{answered}");
    for edge in routed {
        let to = edge["to"].as_str().expect("a node id");
        let (kind, operation) = to.split_once(':').expect("a kind and a name");
        let labels = [("operation", operation), ("kind", kind)];
        assert_eq!(
            count(edge),
            total(&samples, "spindle_http_requests_total", &labels),
            "{edge}"
        );
        let statuses = edge["traffic"]["statuses"].as_array().expect("statuses");
        for class in ["2xx", "3xx", "4xx", "5xx"] {
            let listed = statuses
                .iter()
                .find(|status| status["status_class"] == class)
                .map_or(0.0, |status| status["count"].as_f64().expect("a count"));
            let mut labels = labels.to_vec();
            labels.push(("status_class", class));
            assert_eq!(
                listed,
                total(&samples, "spindle_http_requests_total", &labels),
                "{to} {class}: {edge}"
            );
        }
        let latest = records
            .iter()
            .filter(|record| record["operation"] == operation)
            .take(5)
            .map(|record| record["sequence"].clone())
            .collect::<Vec<_>>();
        assert_eq!(edge["traffic"]["latest"], json!(latest), "{to}: {logged}");
    }
    assert_eq!(
        timeless(answered),
        timeless(tasks()),
        "the fixture is this sequence's topology"
    );
}

#[test]
fn asks_edges_count_answers_by_source() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let file = directory.path().join("decisions.toml");
    std::fs::write(&file, "[\"probe.review.Submit\"]\nflagged = false\n")
        .expect("the decisions file is written");
    let file = file.to_str().expect("a UTF-8 path");
    let spec = root().join(REVIEW);
    let (_, surface) = surface(&spec);
    let served = serve(
        Path::new(REVIEW),
        &["--decisions", file, "--answer-default", "--observe"],
    );
    for id in ["s1", "s2"] {
        let body = json!({"submission_id": id, "amount": 5});
        let (_, answer) = submit(&served, &surface, "probe.review.Submit", &body);
        assert_eq!(answer["outcome"], "submitted", "{answer}");
    }
    let (status, answer) = submit(
        &served,
        &surface,
        "probe.review.Decide",
        &json!({"decision_id": "d1", "approve": true}),
    );
    assert_eq!(status, 501, "nothing answers `vetoed`: {answer}");

    let answered = topology(&served);
    let samples = counters(&served);
    let asked = edges(&answered, "asks");
    let mut ends = asked
        .iter()
        .map(|edge| format!("{} -> {}", edge["from"], edge["to"]))
        .collect::<Vec<_>>();
    ends.sort();
    assert_eq!(
        ends,
        [
            "\"command:probe.review.Decide\" -> \"external:probe.review.Decide/vetoed\"",
            "\"command:probe.review.Submit\" -> \"external:probe.review.Submit/escalated\"",
            "\"command:probe.review.Submit\" -> \"external:probe.review.Submit/flagged\"",
        ],
        "{answered}"
    );
    for edge in asked {
        let (command, outcome) = edge["to"]
            .as_str()
            .and_then(|to| to.strip_prefix("external:"))
            .and_then(|to| to.split_once('/'))
            .expect("an external cause");
        let labels = [("command", command), ("outcome", outcome)];
        let family = "spindle_external_causes_total";
        assert_eq!(count(edge), total(&samples, family, &labels), "{edge}");
        assert!(count(edge) > 0.0, "{edge}");
        for source in ["world", "file", "default", "unanswered"] {
            let listed = edge["traffic"]["sources"]
                .as_array()
                .expect("sources")
                .iter()
                .find(|listed| listed["source"] == source)
                .map_or(0.0, |listed| listed["count"].as_f64().expect("a count"));
            let mut labels = labels.to_vec();
            labels.push(("source", source));
            assert_eq!(listed, total(&samples, family, &labels), "{source}: {edge}");
        }
    }
}

/// Opens the match `m1` on the rally example's table and ticks it once.
fn rally_opened(served: &Served, surface: &spindle_ingress::Surface) {
    let (status, answer) = submit(
        served,
        surface,
        "rally.table.OpenMatch",
        &json!({"match_id": "m1", "target_score": 3}),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
    rally_ticked(served, surface);
}

/// Ticks the match `m1` once.
fn rally_ticked(served: &Served, surface: &spindle_ingress::Surface) {
    let (status, answer) = submit(
        served,
        surface,
        "rally.table.Tick",
        &json!({"match_id": "m1"}),
    );
    assert!((200..300).contains(&status), "{status} {answer}");
}

#[test]
fn emits_and_carries_edges_follow_the_counters() {
    let (ir, surface) = surface(&root().join(RALLY));
    let served = serve(Path::new(RALLY), &["--observe"]);
    rally_opened(&served, &surface);
    let answered = topology(&served);
    let samples = counters(&served);
    let emitted = edges(&answered, "emits");
    for event in ir.events().keys() {
        let node = format!("event:{event}");
        let into = emitted
            .iter()
            .filter(|edge| edge["to"] == node.as_str())
            .map(|edge| count(edge))
            .sum::<f64>();
        let event = event.to_string();
        let appended = total(
            &samples,
            "spindle_events_appended_total",
            &[("event", &event)],
        );
        assert_eq!(into, appended, "{event}: {answered}");
    }
    assert!(
        emitted.iter().any(|edge| count(edge) > 0.0),
        "the match emitted: {answered}"
    );
    let carried = edges(&answered, "carries");
    assert_eq!(carried.len(), 4, "one per event binding: {answered}");
    for edge in carried {
        let binding = edge["to"]
            .as_str()
            .and_then(|to| to.strip_prefix("binding:"))
            .expect("a binding");
        let sent = total(
            &samples,
            "spindle_bindings_total",
            &[("binding", binding), ("carried", "sent")],
        );
        assert_eq!(count(edge), sent, "{edge}");
    }
    assert!(
        edges(&answered, "carries")
            .iter()
            .any(|edge| count(edge) > 0.0),
        "the match sent envelopes: {answered}"
    );
}

#[test]
fn invokes_edges_follow_the_counters() {
    let (ir, surface) = surface(&root().join(RALLY));
    let served = serve(Path::new(RALLY), &["--observe"]);
    rally_opened(&served, &surface);
    let delivered = |samples: &[Sample], binding: &str| {
        total(
            samples,
            "spindle_bindings_total",
            &[("binding", binding), ("carried", "delivered")],
        )
    };
    let invoked = |answered: &Value, binding: &str, command: &str| {
        let from = format!("binding:{binding}");
        let to = format!("command:{command}");
        let found = edges(answered, "invokes")
            .into_iter()
            .filter(|edge| edge["from"] == from.as_str())
            .collect::<Vec<_>>();
        assert_eq!(found.len(), 1, "{binding}: one invokes edge: {answered}");
        assert_eq!(found[0]["to"], to.as_str(), "{binding}: {answered}");
        count(found[0])
    };
    let bindings = ir
        .bindings()
        .values()
        .filter(|binding| binding.cause.event().is_some())
        .map(|binding| (binding.name.to_string(), binding.command.to_string()))
        .collect::<Vec<_>>();
    assert_eq!(bindings.len(), 4, "the rally example's event bindings");

    let answered = topology(&served);
    let samples = counters(&served);
    let mut before = BTreeMap::new();
    for (binding, command) in &bindings {
        let counted = invoked(&answered, binding, command);
        assert_eq!(
            counted,
            delivered(&samples, binding),
            "{binding}: {answered}"
        );
        before.insert(binding.clone(), counted);
    }
    rally_ticked(&served, &surface);
    let answered = topology(&served);
    let samples = counters(&served);
    for (binding, command) in &bindings {
        let counted = invoked(&answered, binding, command);
        assert_eq!(
            counted,
            delivered(&samples, binding),
            "{binding}: {answered}"
        );
        if binding.ends_with("-watches") {
            assert!(counted > before[binding], "{binding} grows: {answered}");
        }
    }
}

#[test]
fn the_topology_reads_as_its_specified_type() {
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string(root().join(SCHEMA)).expect("the schema is read"),
    )
    .expect("the schema is JSON");
    let validator = jsonschema::draft202012::new(&schema).expect("the schema compiles");
    assert!(
        !validator.is_valid(&json!({"format": "spindle-topology/1"})),
        "the schema refuses a topology without its fields, so it checks something"
    );
    let served = serve(Path::new(INGRESS), &["--http", INGRESS_HTTP, "--observe"]);
    scripted(&served);
    let answered = topology(&served);
    let typed: spindle_observe_api::Topology = serde_json::from_value(answered.clone())
        .unwrap_or_else(|error| panic!("{error}: {answered}"));
    assert_eq!(typed.specifications, ["tasks"], "{answered}");
    let errors = validator
        .iter_errors(&answered)
        .map(|error| format!("{error} at {}", error.instance_path()))
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "{answered}: {errors:?}");
}

/// Two specifications behind one listener, the first's event binding invoking the second's
/// command: one graph holds both, and the `invokes` edge crosses them. How `run` is given two
/// specifications, and the fixtures, are `story:one-ingress-several-specifications`'s to settle;
/// the arguments below stand in for them.
#[test]
#[ignore = "story:one-ingress-several-specifications"]
fn a_topology_spans_every_specification_behind_one_ingress() {
    let served = serve(
        Path::new("crates/spindle-cli/tests/fixtures/several/publisher"),
        &[
            "--also",
            "crates/spindle-cli/tests/fixtures/several/consumer",
            "--observe",
        ],
    );
    let answered = topology(&served);
    let specifications = answered["specifications"]
        .as_array()
        .expect("specifications");
    assert_eq!(specifications.len(), 2, "{answered}");
    let specification = |id: &Value| {
        answered["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .find(|node| node["id"] == *id)
            .map(|node| node["specification"].clone())
            .unwrap_or_else(|| panic!("a node {id}: {answered}"))
    };
    assert!(
        edges(&answered, "invokes")
            .iter()
            .any(|edge| specification(&edge["from"]) != specification(&edge["to"])),
        "an invokes edge from one specification's binding to the other's command: {answered}"
    );
}
