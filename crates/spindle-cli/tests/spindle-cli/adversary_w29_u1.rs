//! Adversary pass on `story:binding-failure-escalates` (wave 29 U1): `run` and `conform` on the
//! escalation fixture `crates/spindle-host/tests/fixtures/escalation`.
//!
//! Attacked here: the acceptance `conform_runs_an_escalating_binding` read literally (no scenario
//! that selects an escalated refusal is unsupported); the line a served run logs for a bound
//! command that did not run when its binding escalated it; how a served run and a stdin run
//! report an escalation that is unmet.

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

const STARTUP: Duration = Duration::from_secs(30);
const ANSWER: Duration = Duration::from_secs(10);

/// The policy of `ship-on-order` as the fixture writes it.
const PER_REFUSAL: &str = "      drop: [cancelled]
      escalate:
        emits: depot.shipping.ShipmentStuck
        outcomes: [no-address, oversize]
      retry: {except: [cancelled, no-address, oversize]}
";
/// `ship-on-order` escalating every failure.
const UNIVERSAL: (&str, &str) = (
    PER_REFUSAL,
    "      escalate:\n        emits: depot.shipping.ShipmentStuck\n",
);
/// `orders` reached over the network, so `run` serves it.
const NETWORKED: (&str, &str) = (
    "        - depot.orders.OrderFlagged\n    reached_by: in_process",
    "        - depot.orders.OrderFlagged\n    reached_by: network",
);
/// `depot.shipping.ShipmentStuck` carrying `carrier`, a field `Ship`'s input does not hold.
const CARRIER: (&str, &str) = (
    "  - name: depot.shipping.ShipmentStuck\n    fields:\n",
    "  - name: depot.shipping.ShipmentStuck\n    fields:\n      - name: carrier\n        type: String\n",
);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory")
}

fn utf8(path: &Path) -> &str {
    path.to_str().expect("a UTF-8 path")
}

/// A copy of the fixture in `directory`, each `(from, to)` of `system` replaced in `system.yaml`
/// and each of `shipping` in `domains/shipping.yaml`.
fn depot(directory: &Path, system: &[(&str, &str)], shipping: &[(&str, &str)]) -> PathBuf {
    let fixture = root().join(DEPOT);
    let copy = directory.join("depot");
    std::fs::create_dir_all(copy.join("domains")).expect("the copy's directory");
    for (file, replaced) in [
        ("system.yaml", system),
        ("domains/orders.yaml", &[][..]),
        ("domains/shipping.yaml", shipping),
    ] {
        let mut text = std::fs::read_to_string(fixture.join(file)).expect("the fixture reads");
        for (from, to) in replaced {
            assert!(text.contains(from), "{file} holds {from:?}");
            text = text.replace(from, to);
        }
        std::fs::write(copy.join(file), text).expect("the copy is written");
    }
    copy
}

/// A decisions file in `directory` holding `text`.
fn decisions(directory: &Path, text: &str) -> PathBuf {
    let path = directory.join("decisions.toml");
    std::fs::write(&path, text).expect("the decisions file is written");
    path
}

/// `Ship` takes `outcome` for every order.
fn taking(outcome: &str) -> String {
    let mut text = String::from("[\"depot.shipping.Ship\"]\n");
    for declared in ["cancelled", "no-address", "oversize", "busy"] {
        text.push_str(&format!("{declared} = {}\n", declared == outcome));
    }
    text
}

/// A served run of `spec`; its stderr goes to a file read after the run is stopped.
struct Served {
    child: Child,
    address: SocketAddr,
    stderr: PathBuf,
}

impl Served {
    /// Stops the run and answers what it wrote to stderr.
    fn stop(mut self) -> String {
        let _ = self.child.kill();
        let _ = self.child.wait();
        std::fs::read_to_string(&self.stderr).unwrap_or_default()
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn serve(directory: &Path, spec: &Path, decisions: &Path) -> Served {
    let stderr = directory.join("stderr.log");
    let state = directory.join("state");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", utf8(spec), "--environment", LAPTOP])
        .args(["--decisions", utf8(decisions), "--state", utf8(&state)])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(std::fs::File::create(&stderr).expect("the stderr file"))
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
            panic!(
                "run printed no line within {STARTUP:?}: {error}; stderr: {}",
                std::fs::read_to_string(&stderr).unwrap_or_default()
            );
        }
    };
    let ready: Value = serde_json::from_str(&line).unwrap_or_else(|error| {
        panic!(
            "run's first line {line:?} is not JSON: {error}; stderr: {}",
            std::fs::read_to_string(&stderr).unwrap_or_default()
        )
    });
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("run's first line names no address: {ready}"));
    Served {
        child,
        address,
        stderr,
    }
}

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

/// The names of the events in the served run's history.
fn history(address: SocketAddr) -> Vec<String> {
    let (status, events) = exchange(address, "GET", "/api/events", None);
    assert_eq!(status, 200, "{events}");
    events["events"]
        .as_array()
        .expect("the events")
        .iter()
        .map(|event| event["event"].as_str().unwrap_or_default().to_owned())
        .collect()
}

fn place(address: SocketAddr) {
    let (status, answer) = exchange(
        address,
        "POST",
        "/orders/commands/PlaceOrder",
        Some(&json!({"order_id": "o-1"})),
    );
    assert_eq!(status, 202, "{answer}");
    assert_eq!(answer["outcome"], "placed", "{answer}");
}

/// The acceptance, read literally: `conform` executes every scenario that selects an escalated
/// refusal, none skipped as `unsupported`, and each passes. The unit's own case accepts a
/// scenario whose invocation checks are all `unsupported`.
#[test]
fn conform_skips_no_escalating_scenario_as_unsupported() {
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
    let mut skipped = Vec::new();
    for refusal in ["no-address", "oversize"] {
        let id = format!("{SHIPPING}/binding/refusal/{refusal}");
        let scenario = scenarios
            .iter()
            .find(|scenario| scenario["scenario"] == id.as_str())
            .unwrap_or_else(|| panic!("ESS synthesises {id}"));
        let unsupported = scenario["checks"]
            .as_array()
            .expect("checks")
            .iter()
            .filter(|check| check["status"] == "unsupported")
            .map(|check| format!("{} ({})", check["about"], check["code"]))
            .collect::<Vec<_>>();
        if scenario["status"] != "passed" || !unsupported.is_empty() {
            skipped.push(format!(
                "{id}: status {}, unsupported {unsupported:?}",
                scenario["status"]
            ));
        }
    }
    assert!(
        skipped.is_empty(),
        "a scenario that selects an escalated refusal is skipped: {skipped:#?}"
    );
}

/// A served run logs a bound command that did not run as `dropped`; under a universal
/// escalation the attempt was escalated, not dropped, and its event reached `flag-stuck`.
#[test]
fn a_served_run_does_not_log_an_escalated_attempt_as_dropped() {
    let directory = scratch();
    let spec = depot(directory.path(), &[UNIVERSAL, NETWORKED], &[]);
    // No answer for `Ship`: each attempt fails with no declared outcome.
    let file = decisions(directory.path(), "");
    let served = serve(directory.path(), &spec, &file);
    place(served.address);
    let names = history(served.address);
    let stderr = served.stop();
    assert_eq!(
        names,
        ["depot.orders.OrderPlaced", "depot.orders.OrderFlagged"],
        "the failed attempt was escalated and flag-stuck ran: {stderr}"
    );
    let lines = stderr
        .lines()
        .filter(|line| line.contains(SHIPPING))
        .collect::<Vec<_>>();
    assert!(!lines.is_empty(), "the failed attempt is logged: {stderr}");
    assert!(
        lines.iter().all(|line| !line.contains("dropped")),
        "an escalated attempt is logged as dropped: {lines:#?}"
    );
}

/// A served run says nothing of an escalation that is unmet: no event is published, and no line
/// names the escalation.
#[test]
fn a_served_run_reports_an_unmet_escalation() {
    let directory = scratch();
    let spec = depot(directory.path(), &[NETWORKED], &[CARRIER]);
    let file = decisions(directory.path(), &taking("no-address"));
    let served = serve(directory.path(), &spec, &file);
    place(served.address);
    let names = history(served.address);
    let stderr = served.stop();
    assert_eq!(names, ["depot.orders.OrderPlaced"], "{stderr}");
    assert!(
        stderr
            .lines()
            .any(|line| line.contains(SHIPPING) && line.contains("unmet")),
        "the unmet escalation of {SHIPPING} is reported: {stderr:?}"
    );
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
    let _ = child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(lines.as_bytes());
    child.wait_with_output().expect("b10x-spindle runs")
}

/// On stdin an unmet escalation is on the refused command's line, and nothing fires.
#[test]
fn a_stdin_line_reports_an_unmet_escalation() {
    let directory = scratch();
    let spec = depot(directory.path(), &[], &[CARRIER]);
    let file = decisions(directory.path(), &taking("oversize"));
    let output = run(
        &[
            "run",
            utf8(&spec),
            "--environment",
            LAPTOP,
            "--decisions",
            utf8(&file),
        ],
        &format!(
            "{}\n",
            json!({"command": PLACE, "input": {"order_id": "o-1"}})
        ),
    );
    let results = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("each result is one JSON line"))
        .collect::<Vec<_>>();
    let shipped = results
        .iter()
        .find(|result| result["command"] == SHIP)
        .unwrap_or_else(|| panic!("Ship was invoked: {results:?}"));
    assert_eq!(shipped["outcome"], "oversize", "{shipped}");
    assert_eq!(shipped["escalated"]["event"], STUCK, "{shipped}");
    assert!(shipped["escalated"].get("payload").is_none(), "{shipped}");
    assert!(
        shipped["escalated"]["unmet"]
            .as_str()
            .is_some_and(|why| why.contains("carrier")),
        "{shipped}"
    );
    assert!(
        results
            .iter()
            .all(|result| result["command"] != FLAG && result["caused_by"]["binding"] != FLAGGING),
        "nothing was published: {results:?}"
    );
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr
            .lines()
            .any(|line| line.contains(SHIPPING) && line.contains("unmet")),
        "{stderr}"
    );
}
