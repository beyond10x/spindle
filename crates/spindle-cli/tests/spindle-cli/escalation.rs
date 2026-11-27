//! `run` and `conform` on a binding whose failure policy escalates
//! (story:binding-failure-escalates). The fixture `crates/spindle-host/tests/fixtures/escalation`
//! ships each placed order through `ship-on-order`, which drops `cancelled`, escalates
//! `no-address` and `oversize` into `depot.shipping.ShipmentStuck` and retries any other refusal;
//! `flag-stuck` flags the order of a stuck shipment. Its components are in process, so `run`
//! reads stdin; a copy whose `orders` is reached over the network is served.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

const DEPOT: &str = "crates/spindle-host/tests/fixtures/escalation";
const LAPTOP: &str = "examples/environments/laptop.toml";
const PLACE: &str = "depot.orders.PlaceOrder";
const SHIP: &str = "depot.shipping.Ship";
const FLAG: &str = "depot.orders.FlagOrder";
const SHIPPING: &str = "ship-on-order";
const FLAGGING: &str = "flag-stuck";
const STUCK: &str = "depot.shipping.ShipmentStuck";

/// How long a served `run` may take to admit, open its state and print where it listens.
const STARTUP: Duration = Duration::from_secs(30);
/// How long one request may take to be answered.
const ANSWER: Duration = Duration::from_secs(10);

/// The sentence both documents say of an escalation event, in parts.
const NOT_STORED: [&str; 3] = [
    "goes to the outbox",
    "is not in the event history",
    "is lost if the process stops before it is delivered",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory")
}

/// A copy of the fixture in `directory` with each `(from, to)` replaced in `system.yaml`.
fn depot(directory: &Path, replaced: &[(&str, &str)]) -> PathBuf {
    let fixture = root().join(DEPOT);
    let copy = directory.join("depot");
    std::fs::create_dir_all(copy.join("domains")).expect("the copy's directory");
    for file in [
        "system.yaml",
        "domains/orders.yaml",
        "domains/shipping.yaml",
    ] {
        let mut text = std::fs::read_to_string(fixture.join(file)).expect("the fixture reads");
        if file == "system.yaml" {
            for (from, to) in replaced {
                assert!(text.contains(from), "{file} holds {from:?}");
                text = text.replace(from, to);
            }
        }
        std::fs::write(copy.join(file), text).expect("the copy is written");
    }
    copy
}

/// `orders` reached over the network, so `run` serves it.
const NETWORKED: (&str, &str) = (
    "        - depot.orders.OrderFlagged\n    reached_by: in_process",
    "        - depot.orders.OrderFlagged\n    reached_by: network",
);

/// `ship-on-order` escalating into an event `shipping` does not publish.
const UNPUBLISHED: (&str, &str) = (
    "emits: depot.shipping.ShipmentStuck",
    "emits: depot.orders.OrderFlagged",
);

/// A decisions file in `directory`: `Ship` takes `outcome` for each order of `orders`, and ships
/// every other one.
fn decisions(directory: &Path, answers: &[(&str, &str)]) -> PathBuf {
    let mut text = String::from(
        "[\"depot.shipping.Ship\"]\ncancelled = false\nno-address = false\noversize = false\nbusy = false\n",
    );
    for (order, outcome) in answers {
        text.push_str(&format!(
            "\n[[rules]]\ncommand = \"{SHIP}\"\noutcome = \"{outcome}\"\nanswer = true\nwhen = {{ \"input.order_id\" = \"{order}\" }}\n"
        ));
    }
    let path = directory.join("decisions.toml");
    std::fs::write(&path, text).expect("the decisions file is written");
    path
}

fn utf8(path: &Path) -> &str {
    path.to_str().expect("a UTF-8 path")
}

fn run(arguments: &[&str], lines: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    // A run that stops before it reads stdin closes the pipe; the write may then fail.
    let _ = child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(lines.as_bytes());
    child.wait_with_output().expect("b10x-spindle runs")
}

fn results(output: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("each result is one JSON line"))
        .collect()
}

fn place(order: &str) -> String {
    format!(
        "{}\n",
        json!({"command": PLACE, "input": {"order_id": order}})
    )
}

/// The result line of the command `command` that `binding` invoked.
fn invoked<'a>(results: &'a [Value], binding: &str, command: &str) -> Vec<&'a Value> {
    results
        .iter()
        .filter(|result| result["command"] == command && result["caused_by"]["binding"] == binding)
        .collect()
}

/// A running `b10x-spindle run` serving HTTP; stopped when dropped.
struct Served {
    child: Child,
    address: SocketAddr,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn serve(spec: &Path, arguments: &[&str]) -> Served {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", utf8(spec), "--environment", LAPTOP])
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
    let ready: Value = serde_json::from_str(&line)
        .unwrap_or_else(|error| panic!("run's first line {line:?} is not JSON: {error}"));
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("run's first line names no address: {ready}"));
    Served { child, address }
}

/// One HTTP/1.1 exchange on its own connection: the status and the JSON body.
fn exchange(address: SocketAddr, method: &str, path: &str, body: Option<&Value>) -> (u16, Value) {
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
    let mut answer = String::new();
    stream
        .read_to_string(&mut answer)
        .expect("the answer is read before the timeout");
    let status = answer
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {answer:?}"));
    let json = answer
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .and_then(|body| serde_json::from_str(body).ok())
        .unwrap_or(Value::Null);
    (status, json)
}

#[test]
fn a_served_run_with_an_escalating_binding_listens() {
    let directory = scratch();
    let spec = depot(directory.path(), &[NETWORKED]);
    let file = decisions(directory.path(), &[("o-1", "no-address")]);
    let state = directory.path().join("state");
    let served = serve(
        &spec,
        &["--decisions", utf8(&file), "--state", utf8(&state)],
    );
    let (status, answer) = exchange(
        served.address,
        "POST",
        "/orders/commands/PlaceOrder",
        Some(&json!({"order_id": "o-1"})),
    );
    assert_eq!(status, 202, "{answer}");
    assert_eq!(answer["outcome"], "placed", "{answer}");
    let (status, events) = exchange(served.address, "GET", "/api/events", None);
    assert_eq!(status, 200, "{events}");
    let names = events["events"]
        .as_array()
        .expect("the events")
        .iter()
        .map(|event| event["event"].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        ["depot.orders.OrderPlaced", "depot.orders.OrderFlagged"],
        "the escalation reached flag-stuck, and is not itself in the history: {events}"
    );
}

#[test]
fn a_stdin_line_escalates_a_refusal() {
    let directory = scratch();
    let file = decisions(directory.path(), &[("o-1", "no-address")]);
    let output = run(
        &[
            "run",
            DEPOT,
            "--environment",
            LAPTOP,
            "--decisions",
            utf8(&file),
        ],
        &(place("o-1") + &place("o-2")),
    );
    let results = results(&output);
    assert_eq!(results[0]["outcome"], "placed", "{results:?}");
    let shipped = invoked(&results, SHIPPING, SHIP);
    assert_eq!(shipped.len(), 2, "{results:?}");
    let refused = shipped[0];
    assert_eq!(refused["outcome"], "no-address", "{refused}");
    assert_eq!(refused["attempts"], 1, "{refused}");
    assert_eq!(
        refused["caused_by"],
        json!({"line": 1, "binding": SHIPPING})
    );
    assert_eq!(
        refused["escalated"],
        json!({"event": STUCK, "payload": {"order_id": "o-1"}}),
        "{refused}"
    );
    let flagged = invoked(&results, FLAGGING, FLAG);
    assert_eq!(
        flagged.len(),
        1,
        "the escalation fired flag-stuck once: {results:?}"
    );
    assert_eq!(flagged[0]["outcome"], "flagged", "{}", flagged[0]);
    assert_eq!(flagged[0]["caused_by"]["line"], 1, "{}", flagged[0]);
    assert_eq!(shipped[1]["outcome"], "shipped", "{}", shipped[1]);
    assert!(shipped[1].get("escalated").is_none(), "{}", shipped[1]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "a refused bound command fails the run, escalated or not: {output:?}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr
            .lines()
            .any(|line| line.contains("line 1") && line.contains(SHIPPING)),
        "{stderr}"
    );
}

#[test]
fn drop_retries_keeps_escalations() {
    let directory = scratch();
    let file = decisions(directory.path(), &[("o-1", "oversize"), ("o-2", "busy")]);
    let output = run(
        &[
            "run",
            DEPOT,
            "--environment",
            LAPTOP,
            "--decisions",
            utf8(&file),
            "--drop-retries",
        ],
        &(place("o-1") + &place("o-2")),
    );
    let results = results(&output);
    let shipped = invoked(&results, SHIPPING, SHIP);
    assert_eq!(shipped.len(), 2, "{results:?}");
    assert_eq!(shipped[0]["outcome"], "oversize", "{}", shipped[0]);
    assert_eq!(
        shipped[0]["escalated"],
        json!({"event": STUCK, "payload": {"order_id": "o-1"}}),
        "an escalated refusal still escalates: {}",
        shipped[0]
    );
    assert_eq!(shipped[1]["outcome"], "busy", "{}", shipped[1]);
    assert_eq!(
        shipped[1]["attempts"], 1,
        "a retried refusal is dropped: {}",
        shipped[1]
    );
    assert!(shipped[1].get("escalated").is_none(), "{}", shipped[1]);
    assert_eq!(invoked(&results, FLAGGING, FLAG).len(), 1, "{results:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!(
            "binding {SHIPPING} retries on failure; --drop-retries"
        )),
        "{stderr}"
    );
}

#[test]
fn a_binding_escalating_to_an_event_the_component_does_not_publish_is_refused_at_start() {
    let directory = scratch();
    let file = decisions(directory.path(), &[]);
    for (name, replaced) in [
        ("stdin", &[UNPUBLISHED][..]),
        ("served", &[UNPUBLISHED, NETWORKED]),
    ] {
        let copy = scratch();
        let spec = depot(copy.path(), replaced);
        let output = run(
            &[
                "run",
                utf8(&spec),
                "--environment",
                LAPTOP,
                "--decisions",
                utf8(&file),
            ],
            &place("o-1"),
        );
        assert_eq!(output.status.code(), Some(1), "{name}: {output:?}");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.is_empty(),
            "{name}: nothing listens or answers: {stdout}"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        for named in [SHIPPING, "depot.orders.OrderFlagged", "shipping"] {
            assert!(
                stderr.contains(named),
                "{name}: stderr names {named}: {stderr}"
            );
        }
    }
}

#[test]
fn conform_runs_an_escalating_binding() {
    let directory = scratch();
    let results = directory.path().join("results.json");
    let output = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["conform", DEPOT, "--environment", LAPTOP, "--results"])
        .arg(&results)
        .stdin(Stdio::null())
        .output()
        .expect("b10x-spindle runs");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results: Value =
        serde_json::from_str(&std::fs::read_to_string(&results).expect("the results were written"))
            .expect("the results are JSON");
    let scenarios = results["scenarios"].as_array().expect("scenarios");
    let escalating = ["no-address", "oversize"]
        .map(|refusal| format!("{SHIPPING}/binding/refusal/{refusal}"))
        .map(|id| {
            scenarios
                .iter()
                .find(|scenario| scenario["scenario"] == id.as_str())
                .unwrap_or_else(|| panic!("ESS synthesises {id}: {results}"))
        });
    for scenario in escalating {
        let checks = scenario["checks"].as_array().expect("checks");
        let stuck = checks
            .iter()
            .filter(|check| {
                check["about"]
                    .as_str()
                    .is_some_and(|about| about.contains(STUCK))
            })
            .collect::<Vec<_>>();
        assert!(
            stuck.len() >= 2 && stuck.iter().all(|check| check["status"] == "passed"),
            "the escalation is published, once, and checked: {scenario}"
        );
        assert!(
            checks.iter().all(|check| check["status"] == "passed"),
            "a scenario that selects an escalated refusal is executed, its invocation checks \
             included: {scenario}"
        );
    }
    for scenario in scenarios {
        assert_eq!(scenario["status"], "passed", "{scenario}");
    }
}

#[test]
fn the_docs_say_an_escalation_is_not_stored() {
    for document in ["docs/design/spindle.md", "website/docs/reference/cli.md"] {
        let text = std::fs::read_to_string(root().join(document)).expect("the document reads");
        let said = text
            .lines()
            .filter(|line| line.contains("escalat"))
            .any(|line| NOT_STORED.iter().all(|part| line.contains(part)));
        assert!(
            said,
            "{document} says, beside an escalation, that its event {}",
            NOT_STORED.join(", ")
        );
    }
}
