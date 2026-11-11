//! A specification in any supported ESS format runs unchanged (`story:ess-format-range`).
//!
//! One fixture per supported format: the served example `tests/fixtures/ingress` (`tasks`), copied
//! with only its `format:` line set to the format. Each must be admitted on the laptop, serve every
//! command of its network component over HTTP, and pass the same conformance counts as the others.
//! Dropping a format from what Spindle reads fails here.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

/// Every format Spindle reads, oldest first.
const FORMATS: [&str; 3] = ["ess/34", "ess/35", "ess/36"];

/// The example every fixture is a copy of.
const BASE: &str = "crates/spindle-cli/tests/fixtures/ingress";
/// The format line the example declares, which each fixture replaces.
const BASE_FORMAT: &str = "format: ess/34\n";
const LAPTOP: &str = "examples/environments/laptop.toml";
/// The component `reached_by: network`, which `run` serves over HTTP.
const SERVED: &str = "board";

/// Each command the served component accepts, in the order sent, with an input its HTTP route
/// takes and the outcome it answers.
fn commands() -> [(&'static str, Value, &'static str); 2] {
    [
        (
            "tasks.board.OpenTask",
            json!({"task_id": "t1", "title": "Water the plants"}),
            "opened",
        ),
        ("tasks.board.CloseTask", json!({"task_id": "t1"}), "closed"),
    ]
}

const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn spindle(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(arguments)
        .stdin(Stdio::null())
        .output()
        .expect("b10x-spindle runs")
}

fn utf8(path: &Path) -> &str {
    path.to_str().expect("utf-8")
}

/// Copies the directory `from` to `to`.
fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("creating a directory");
    for entry in std::fs::read_dir(from).expect("reading a directory") {
        let entry = entry.expect("a directory entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a file type").is_dir() {
            copy(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copying a file");
        }
    }
}

/// The example copied into `dir`, its format line set to `format`.
fn fixture(dir: &Path, format: &str) -> PathBuf {
    let spec = dir.join(format.replace('/', "-"));
    copy(&root().join(BASE), &spec);
    let system = spec.join("system.yaml");
    let text = std::fs::read_to_string(&system).expect("the system file");
    let body = text
        .strip_prefix(BASE_FORMAT)
        .unwrap_or_else(|| panic!("{BASE}/system.yaml opens with {BASE_FORMAT:?}: {text}"));
    std::fs::write(&system, format!("format: {format}\n{body}")).expect("writing the format");
    spec
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

fn serve(spec: &Path) -> Served {
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", utf8(spec), "--environment", LAPTOP, "--state"])
        .arg(state.path().join("state"))
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
        panic!(
            "run on {} printed no line within {STARTUP:?}: {error}",
            spec.display()
        )
    });
    let ready: Value = match serde_json::from_str(&line) {
        Ok(ready) => ready,
        Err(error) => {
            let _ = child.kill();
            let output = child.wait_with_output();
            panic!("run's first line {line:?} is not JSON ({error}): {output:?}")
        }
    };
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

/// One HTTP/1.1 exchange on its own connection: the status and the JSON body.
fn exchange(address: SocketAddr, method: &str, path: &str, body: &Value) -> (u16, Value) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream.set_read_timeout(Some(ANSWER)).expect("a timeout");
    stream.set_write_timeout(Some(ANSWER)).expect("a timeout");
    let body = body.to_string();
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
        .unwrap_or_else(|| panic!("an answer with a head: {answer:?}"));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {head:?}"));
    let body = serde_json::from_str(body).unwrap_or_else(|error| panic!("{error}: {body:?}"));
    (status, body)
}

/// The method and path of each command operation ESS projects for the served component of
/// `spec`, by operation id.
fn command_routes(spec: &Path) -> Vec<(String, String, String)> {
    let loaded = spindle_spec::load(spec)
        .unwrap_or_else(|error| panic!("{} compiles: {error:#}", spec.display()));
    let component = loaded
        .ir
        .components()
        .values()
        .find(|component| component.name.as_str() == SERVED)
        .expect("the example declares the served component");
    let document: Value = serde_json::from_str(&ess_gen::openapi::json(&loaded.ir, component))
        .expect("the projection is JSON");
    let mut routes = Vec::new();
    for (path, item) in document["paths"].as_object().expect("paths") {
        for (method, operation) in item.as_object().expect("a path item") {
            if method == "post" {
                let id = operation["operationId"].as_str().expect("an operation id");
                routes.push((id.to_owned(), method.to_uppercase(), path.clone()));
            }
        }
    }
    routes.sort();
    routes
}

/// What one format's fixture showed: its admission, the commands it served and its conformance
/// counts.
#[derive(Debug, PartialEq)]
struct Seen {
    bindings: Value,
    served: Vec<(String, u16, Value)>,
    counts: Value,
}

fn observe(dir: &Path, format: &str) -> Seen {
    let spec = fixture(dir, format);

    let admitted = spindle(&["admit", utf8(&spec), "--environment", LAPTOP, "--json"]);
    assert_eq!(
        admitted.status.code(),
        Some(0),
        "{format}: admit refused the example: {admitted:?}"
    );
    let admission: Value = serde_json::from_slice(&admitted.stdout).expect("admit prints JSON");
    assert_eq!(admission["outcome"], "admitted", "{format}: {admission}");

    let routes = command_routes(&spec);
    let names = routes
        .iter()
        .map(|(id, _, _)| id.as_str())
        .collect::<Vec<_>>();
    let mut expected = commands().map(|(id, _, _)| id).to_vec();
    expected.sort_unstable();
    assert_eq!(names, expected, "{format}: the served commands");
    let served = serve(&spec);
    let mut answered = Vec::new();
    for (id, input, outcome) in commands() {
        let (_, method, path) = routes
            .iter()
            .find(|(route, _, _)| route == id)
            .expect("a route per command");
        let (status, body) = exchange(served.address, method, path, &input);
        assert_eq!(status, 202, "{format}: {id} over HTTP answered {body}");
        assert_eq!(body["outcome"], outcome, "{format}: {id} answered {body}");
        answered.push((id.to_owned(), status, body));
    }
    drop(served);

    let report = dir.join(format!("{}-report.json", format.replace('/', "-")));
    let conformed = spindle(&[
        "conform",
        utf8(&spec),
        "--environment",
        LAPTOP,
        "--report",
        utf8(&report),
    ]);
    assert_eq!(
        conformed.status.code(),
        Some(0),
        "{format}: conform failed: {conformed:?}"
    );
    let report: Value =
        serde_json::from_str(&std::fs::read_to_string(&report).expect("conform wrote its report"))
            .expect("the report is JSON");
    let counts = report["counts"].clone();
    assert!(
        counts["passed"].as_u64().is_some_and(|passed| passed > 0)
            && counts["failed"] == 0
            && counts["error"] == 0,
        "{format}: the suite does not pass: {report}"
    );

    Seen {
        bindings: admission["bindings"].clone(),
        served: answered,
        counts,
    }
}

#[test]
fn each_supported_format_admits_serves_and_conforms_alike() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    let seen = FORMATS.map(|format| (format, observe(dir.path(), format)));
    let (first, oldest) = &seen[0];
    for (format, each) in &seen[1..] {
        assert_eq!(each, oldest, "{format} runs otherwise than {first}");
    }
}
