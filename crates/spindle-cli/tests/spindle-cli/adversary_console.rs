//! Adversary cases for the console routes of a loopback `run`: the wave 4 transport rules on the
//! console paths, a refused reset that must not empty anything, paths that are not the console's,
//! a reset that survives a restart, and the model byte-equal on every specification in the
//! repository.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/ingress";
const LAPTOP: &str = "examples/environments/laptop.toml";

const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(30);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

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

fn serve(state: &Path) -> Served {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--environment", LAPTOP, "--state"])
        .arg(state)
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
    let ready: Value = match serde_json::from_str(&line) {
        Ok(ready) => ready,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("run's first line {line:?} is not JSON: {error}");
        }
    };
    let Some(address) = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
    else {
        let _ = child.kill();
        let _ = child.wait();
        panic!("run's first line names no address: {ready}");
    };
    Served { child, address }
}

/// Writes `request` as it is on its own connection, then `rest` after `pause`; the status, the
/// head and the body answered.
fn send(
    address: SocketAddr,
    request: &[u8],
    pause: Duration,
    rest: &[u8],
) -> (u16, String, String) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    stream.write_all(request).expect("the request is written");
    if !rest.is_empty() {
        std::thread::sleep(pause);
        let _ = stream.write_all(rest);
    }
    let mut answer = Vec::new();
    let _ = stream.read_to_end(&mut answer);
    let answer = String::from_utf8_lossy(&answer).into_owned();
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

fn exchange(
    address: SocketAddr,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> (u16, String, String) {
    let body = body.map(Value::to_string).unwrap_or_default();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    send(address, request.as_bytes(), Duration::ZERO, &[])
}

/// The method and path ESS publishes for the operation `id` of `board`.
fn route(id: &str) -> (String, String) {
    let loaded = spindle_spec::load(&root().join(SPEC)).expect("the fixture compiles");
    let component = loaded
        .ir
        .components()
        .values()
        .find(|component| component.name.as_str() == "board")
        .expect("the fixture declares board");
    let document: Value = serde_json::from_str(&ess_gen::openapi::json(&loaded.ir, component))
        .expect("the projection is JSON");
    for (path, item) in document["paths"].as_object().expect("paths") {
        for (method, operation) in item.as_object().expect("a path item") {
            if operation["operationId"] == id {
                return (method.to_uppercase(), path.clone());
            }
        }
    }
    panic!("the contract declares no operation {id}")
}

fn open(served: &Served, task: &str) {
    let (method, path) = route("tasks.board.OpenTask");
    let (status, _, body) = exchange(
        served.address,
        &method,
        &path,
        Some(&json!({"task_id": task, "title": task})),
    );
    assert_eq!(status, 202, "{task} opens: {body}");
}

fn all_tasks(served: &Served) -> usize {
    let (method, path) = route("tasks.board.AllTasks");
    let (status, _, body) = exchange(served.address, &method, &path, None);
    assert_eq!(status, 200, "{body}");
    let body: Value = serde_json::from_str(&body).expect("rows are JSON");
    body["rows"].as_array().expect("rows").len()
}

fn events(served: &Served) -> usize {
    let (status, _, body) = exchange(served.address, "GET", "/api/events", None);
    assert_eq!(status, 200, "{body}");
    let body: Value = serde_json::from_str(&body).expect("events are JSON");
    body["events"].as_array().expect("events").len()
}

fn state() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory")
}

#[test]
fn adversary_the_derived_routes_are_where_the_probe_reads_them() {
    let dir = state();
    let served = serve(&dir.path().join("state"));
    open(&served, "t1");
    assert_eq!(all_tasks(&served), 1);
    assert_eq!(events(&served), 1);
}

#[test]
fn adversary_a_reset_refused_at_the_transport_empties_nothing() {
    let dir = state();
    let served = serve(&dir.path().join("state"));
    open(&served, "t1");
    let address = served.address;

    let (status, head, body) = send(
        address,
        format!(
            "POST /api/reset HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
            (1u64 << 20) + 1
        )
        .as_bytes(),
        Duration::ZERO,
        &[],
    );
    assert_eq!(status, 413, "{head}\n{body}");

    let (status, head, body) = send(
        address,
        format!(
            "POST /api/reset HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n"
        )
        .as_bytes(),
        Duration::ZERO,
        &[],
    );
    assert_eq!(status, 411, "{head}\n{body}");

    let (status, head, body) = exchange(address, "DELETE", "/api/reset", None);
    assert_eq!(status, 405, "{head}\n{body}");
    assert!(head.to_ascii_lowercase().contains("allow: post"), "{head}");

    let (status, head, body) = exchange(address, "post", "/api/reset", None);
    assert_eq!(
        status, 405,
        "the method is compared exactly: {head}\n{body}"
    );

    for path in [
        "/api/reset/",
        "/api/Reset",
        "/API/reset",
        "/api/%72eset",
        "/api//reset",
        "//api/reset",
        "/api/./reset",
        "/api/x/../reset",
    ] {
        let (status, head, body) = exchange(address, "POST", path, None);
        assert_eq!(
            status, 404,
            "{path} is not the console's reset: {head}\n{body}"
        );
    }

    assert_eq!(all_tasks(&served), 1, "no refused reset emptied board");
    assert_eq!(events(&served), 1, "no refused reset cleared the event log");
}

#[test]
fn adversary_a_reset_whose_body_never_arrives_is_408_and_empties_nothing() {
    let dir = state();
    let served = serve(&dir.path().join("state"));
    open(&served, "t1");
    let address = served.address;
    let (status, head, body) = send(
        address,
        format!(
            "POST /api/reset HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Length: 10\r\n\r\n{{}}"
        )
        .as_bytes(),
        Duration::ZERO,
        &[],
    );
    assert_eq!(status, 408, "{head}\n{body}");
    assert_eq!(all_tasks(&served), 1);
    assert_eq!(events(&served), 1);
}

#[test]
fn adversary_the_console_model_and_events_obey_the_transport_rules() {
    let dir = state();
    let served = serve(&dir.path().join("state"));
    let address = served.address;
    for path in ["/api/model", "/api/events"] {
        let (status, head, body) = exchange(address, "HEAD", path, None);
        assert_eq!(status, 405, "{path}: {head}\n{body}");
        assert!(head.to_ascii_lowercase().contains("allow: get"), "{head}");
        let (status, head, body) = send(
            address,
            format!(
                "GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
                (1u64 << 20) + 1
            )
            .as_bytes(),
            Duration::ZERO,
            &[],
        );
        assert_eq!(status, 413, "{path}: {head}\n{body}");
        let (status, head, body) = exchange(address, "GET", &format!("{path}/"), None);
        assert_eq!(status, 404, "{path}/: {head}\n{body}");
        let (status, _, body) = exchange(address, "GET", &format!("{path}?since=0"), None);
        assert_eq!(status, 200, "{path} with a query: {body}");
    }
}

#[test]
fn adversary_a_reset_holds_across_a_restart_and_the_run_keeps_serving() {
    let dir = state();
    let directory = dir.path().join("state");
    {
        let served = serve(&directory);
        open(&served, "t1");
        open(&served, "t2");
        let (status, _, body) = exchange(served.address, "POST", "/api/reset", None);
        assert_eq!(status, 200, "{body}");
        open(&served, "t3");
        assert_eq!(all_tasks(&served), 1);
    }
    let served = serve(&directory);
    assert_eq!(
        all_tasks(&served),
        1,
        "after a restart only what was created after the reset is held"
    );
    open(&served, "t1");
    assert_eq!(all_tasks(&served), 2);
    let leftover: Vec<String> = std::fs::read_dir(&directory)
        .expect("the state directory reads")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| !name.starts_with("board.sqlite") && !name.starts_with("ledger.sqlite"))
        .collect();
    assert_eq!(leftover, Vec::<String>::new());
}

#[test]
fn adversary_the_model_is_byte_equal_to_ess_on_every_specification() {
    for spec in [
        "crates/spindle-cli/tests/fixtures/ingress",
        "crates/spindle-cli/tests/fixtures/replay",
        "crates/spindle-cli/tests/fixtures/review",
        "examples/rally/ess",
        "ess",
    ] {
        let path = root().join(spec);
        if !path.join("system.yaml").exists() && !path.is_dir() {
            continue;
        }
        let output = Command::new("ess")
            .current_dir(root())
            .args(["specify", "compile", "--path", spec, "--format", "json"])
            .output()
            .expect("the ess CLI runs");
        if !output.status.success() {
            assert!(
                spindle_spec::load(&path).is_err(),
                "{spec}: ess refuses it, so spindle must too: {output:?}"
            );
            continue;
        }
        let expected = String::from_utf8(output.stdout).expect("the IR is UTF-8");
        let loaded = spindle_spec::load(&path).unwrap_or_else(|error| panic!("{spec}: {error}"));
        let model = spindle_ingress::console::model(&loaded.ir);
        let parsed: Value = serde_json::from_str(&model).expect("the model is JSON");
        let ir = serde_json::to_string(&parsed["ir"]).expect("serializes");
        let expected_value: Value = serde_json::from_str(&expected).expect("ess prints JSON");
        assert_eq!(parsed["ir"], expected_value, "{spec}: the IR value");
        let prefix = "{\"ir\":";
        assert!(model.starts_with(prefix), "{spec}");
        assert!(
            model[prefix.len()..].starts_with(expected.trim_end_matches('\n')),
            "{spec}: the IR is answered byte for byte ({} vs {} bytes)",
            ir.len(),
            expected.len()
        );
    }
}
