//! The caller a command is sent as, supplied per invocation beside its input: `"caller": {...}`
//! on a `run` stdin line, and a scenario step's `caller` under `run --scenario`.
//!
//! The specification is the kernel's fixture `crates/spindle-kernel/tests/fixtures/caller`: the
//! actor `Writer` carries `owner_id`, `CreateNotebook` records it, and `EditNotebook` refuses with
//! `not-yours` a writer who is not the owner.
//!
//! A loopback run serving HTTP uses [`SERVED`], `crates/spindle-cli/tests/fixtures/trusted-caller`,
//! whose component `front` is reached over the network and whose `RecordVisit` reads the
//! caller's `person_id`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const SPEC: &str = "crates/spindle-kernel/tests/fixtures/caller";
const SCENARIOS: &str = "crates/spindle-cli/tests/fixtures/caller-scenarios";
const LAPTOP: &str = "examples/environments/laptop.toml";
const SERVED: &str = "crates/spindle-cli/tests/fixtures/trusted-caller";
const RECORD: &str = "porch.visit.RecordVisit";
const VISIT: &str = "5b2d8f14-3c6a-4e7b-9d1f-2a3b4c5d6e7f";
/// Why the ingress does not serve [`RECORD`] where no caller is fixed or proven.
const REASON: &str = "this ingress does not serve the caller attributes it reads (`person_id`), \
                      which no HTTP caller proves here";
/// How long `run` may take to print its first line.
const STARTUP: Duration = Duration::from_secs(50);
/// How long one exchange may take.
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn spindle(arguments: &[&str], lines: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let written = child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(lines.as_bytes());
    // `run` may refuse at start and exit before it reads a line, which closes the pipe.
    if let Err(error) = written {
        assert_eq!(
            error.kind(),
            std::io::ErrorKind::BrokenPipe,
            "the lines are written: {error}"
        );
    }
    child.wait_with_output().expect("b10x-spindle runs")
}

#[test]
fn replay_passes_a_step_s_caller() {
    let scenario = format!("{SCENARIOS}/owner-edits.yaml");
    let output = spindle(
        &[
            "run",
            SPEC,
            "--environment",
            LAPTOP,
            "--scenario",
            &scenario,
        ],
        "",
    );
    let printed = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    for step in [
        "timeline[0] quill.notebook.CreateNotebook: holds",
        "timeline[1] quill.notebook.EditNotebook: holds",
        "timeline[2] quill.notebook.EditNotebook: holds",
        "assert[0] quill.notebook.NotebookCard: holds",
    ] {
        assert!(printed.contains(step), "{step:?} in {printed}");
    }
    assert!(!printed.contains("unsupported"), "{printed}");
}

#[test]
fn run_reads_a_line_s_caller_beside_its_input() {
    let lines = concat!(
        r#"{"command": "quill.notebook.CreateNotebook", "input": {"notebook_id": "5b2d8f14-3c6a-4e7b-9d1f-2a3b4c5d6e7f", "title": "field notes"}, "actor": "quill.notebook.Writer", "caller": {"owner_id": "0f1e2d3c-4b5a-4968-8776-a5b4c3d2e1f0"}}"#,
        "\n",
        r#"{"command": "quill.notebook.EditNotebook", "input": {"notebook_id": "5b2d8f14-3c6a-4e7b-9d1f-2a3b4c5d6e7f", "title": "stolen"}, "actor": "quill.notebook.Writer", "caller": {"owner_id": "9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d"}}"#,
        "\n",
        r#"{"command": "quill.notebook.EditNotebook", "input": {"notebook_id": "5b2d8f14-3c6a-4e7b-9d1f-2a3b4c5d6e7f", "title": "fair copy"}}"#,
        "\n",
        r#"{"command": "quill.notebook.EditNotebook", "input": {"notebook_id": "5b2d8f14-3c6a-4e7b-9d1f-2a3b4c5d6e7f", "title": "fair copy"}, "actor": "quill.notebook.Writer", "caller": {"owner_id": "0f1e2d3c-4b5a-4968-8776-a5b4c3d2e1f0"}}"#,
        "\n",
    );
    let output = spindle(&["run", SPEC, "--environment", LAPTOP], lines);
    assert_eq!(
        output.status.code(),
        Some(1),
        "a rejected line exits 1 once every line is answered (story:run-fails-on-rejected-line): {output:?}"
    );
    let results: Vec<serde_json::Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("each result is one JSON line"))
        .collect();
    assert_eq!(results.len(), 4, "{results:?}");
    assert_eq!(results[0]["outcome"], "created", "{results:?}");
    assert_eq!(
        results[0]["events"][0]["payload"]["owner_id"], "0f1e2d3c-4b5a-4968-8776-a5b4c3d2e1f0",
        "{results:?}"
    );
    assert_eq!(results[1]["outcome"], "not-yours", "{results:?}");
    assert_eq!(results[1]["error"]["error"], "quill.notebook.NotYours");
    assert!(
        results[2]["failure"].is_object(),
        "a line with no caller selects no branch the caller decides: {results:?}"
    );
    assert_eq!(results[3]["outcome"], "edited", "{results:?}");
}

#[test]
fn a_route_whose_command_reads_the_caller_stays_unserved() {
    let ir = spindle_spec::load(&root().join(SPEC))
        .expect("the caller fixture compiles")
        .ir;
    let surface = spindle_ingress::Surface::new(&ir, &["shelf".to_owned()], None)
        .expect("the fixture's surface is routed");
    let unserved: Vec<String> = surface
        .unserved()
        .iter()
        .map(|route| {
            format!(
                "{}: {}",
                route.operation,
                route
                    .unserved()
                    .map_or_else(String::new, ToString::to_string)
            )
        })
        .collect();
    assert_eq!(
        unserved.len(),
        3,
        "every command of the fixture reads its caller: {unserved:#?}"
    );
    for route in &unserved {
        assert!(
            route.contains(
                "this ingress does not serve the caller attributes it reads (`owner_id`)"
            ),
            "how an HTTP caller proves its attributes is not decided, so the route answers 501: \
             {route}"
        );
    }
}

/// One HTTP/1.1 exchange with `address`: the status and the body as JSON.
fn exchange(address: SocketAddr, method: &str, path: &str, body: &str) -> (u16, serde_json::Value) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .expect("the request is written");
    let mut answer = String::new();
    stream
        .read_to_string(&mut answer)
        .expect("the answer is read before the timeout");
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

/// A loopback `run` of [`SERVED`], whose `RecordVisit` reads the caller's `person_id`, with
/// neither `--fixed-actor` with `--fixed-caller` nor `--trusted-caller-header` with
/// `--trusted-caller-secret`: the route stays not served for the reason it had, a request for it
/// is refused as before, and `run` prints one more stderr line naming both ways to serve it
/// (`story:caller-attributes-on-loopback`, D1 A).
#[test]
fn the_refusal_names_how_to_serve_a_caller_reading_command() {
    let ir = spindle_spec::load(&root().join(SERVED))
        .expect("the trusted-caller fixture compiles")
        .ir;
    let surface = spindle_ingress::Surface::new(&ir, &["front".to_owned()], None)
        .expect("the fixture's surface is routed");
    let route = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == RECORD)
        .expect("a route for RecordVisit");
    let (method, path) = (route.method.as_str().to_uppercase(), route.path.clone());

    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SERVED, "--environment", LAPTOP])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let (printed, stderr_read) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        let _ = printed.send(text);
    });
    let stdout = child.stdout.take().expect("stdout is piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = sender.send(line);
    });
    let line = receiver.recv_timeout(STARTUP).unwrap_or_default();
    let address: Option<SocketAddr> = serde_json::from_str::<serde_json::Value>(&line)
        .ok()
        .and_then(|ready| ready["listening"].as_str().and_then(|at| at.parse().ok()));
    let answered = address.map(|address| {
        (
            exchange(
                address,
                &method,
                &path,
                &format!(r#"{{"visit_id": "{VISIT}"}}"#),
            ),
            exchange(address, "GET", "/api/served", ""),
        )
    });
    let _ = child.kill();
    let _ = child.wait();
    let stderr = stderr_read
        .recv_timeout(ANSWER)
        .expect("stderr is read to its end once the run stops");
    let Some(((status, recorded), (_, served))) = answered else {
        panic!("run listens on loopback without either setting: {line:?}\n{stderr}");
    };

    assert!(
        stderr
            .lines()
            .any(|line| line.starts_with("b10x-spindle: not served: ")
                && line.contains(RECORD)
                && line.ends_with(&format!(": {REASON}"))),
        "the route's reason is unchanged: {stderr}"
    );
    let naming: Vec<&str> = stderr
        .lines()
        .filter(|line| {
            [
                "--fixed-actor",
                "--fixed-caller",
                "--trusted-caller-header",
                "--trusted-caller-secret",
            ]
            .iter()
            .all(|flag| line.contains(flag))
        })
        .collect();
    assert_eq!(
        naming.len(),
        1,
        "one stderr line names both settings: {stderr}"
    );
    assert_eq!(
        (status, recorded),
        (
            403,
            serde_json::json!({"refused": "not granted", "actor": null})
        ),
        "a request proving no caller is refused as before: the grant check answers before the \
         ingress's 501"
    );
    let row = served["operations"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["name"] == RECORD))
        .unwrap_or_else(|| panic!("GET /api/served lists {RECORD}: {served}"));
    assert_eq!(
        (&row["served"], &row["reason"]),
        (&serde_json::json!(false), &serde_json::json!(REASON)),
        "GET /api/served names the route not served for the same reason: {served}"
    );
}
