//! `run` serving HTTP with a caller fixed at start (`--fixed-actor`, `--fixed-caller`): a
//! test-backend switch, not authentication. Every command a request names is sent as that actor
//! and caller, so a command that reads its caller is served; without the flags nothing changes.
//!
//! The fixture `tests/fixtures/fixed-caller` declares one component, `desk`, reached over the
//! network. The actor `Writer` carries `owner_id` and may create and share a notebook;
//! `CreateNotebook` records the caller's `owner_id` as the owner, and `ShareNotebook` refuses with
//! `with-self` a writer who names their own `owner_id`. The actor `Reader` carries `owner_id` too
//! and may only share.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/fixed-caller";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "desk";
const WRITER: &str = "quire.notebook.Writer";
const READER: &str = "quire.notebook.Reader";
const CREATE: &str = "quire.notebook.CreateNotebook";
const SHARE: &str = "quire.notebook.ShareNotebook";
const CARD: &str = "quire.notebook.NotebookCard";
const OWNER: &str = "0f1e2d3c-4b5a-4968-8776-a5b4c3d2e1f0";
const OTHER: &str = "9a8b7c6d-5e4f-4a3b-8c2d-1e0f9a8b7c6d";
const NOTEBOOK: &str = "5b2d8f14-3c6a-4e7b-9d1f-2a3b4c5d6e7f";

/// How long `run` may take to print its first line.
const STARTUP: Duration = Duration::from_secs(60);
/// How long one exchange may take.
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The method and path the served surface puts `operation` at.
fn route(operation: &str) -> (String, String) {
    let ir = spindle_spec::load(&root().join(SPEC))
        .expect("the fixed-caller fixture compiles")
        .ir;
    let surface = spindle_ingress::Surface::new(&ir, &[COMPONENT.to_owned()], None)
        .expect("the fixture's surface is routed");
    let route = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == operation)
        .unwrap_or_else(|| panic!("a route for {operation}"));
    (route.method.as_str().to_uppercase(), route.path.clone())
}

/// A running `b10x-spindle run` serving HTTP; stopped when dropped.
struct Served {
    child: Child,
    address: SocketAddr,
    stderr: mpsc::Receiver<String>,
}

impl Served {
    /// Stops the run and answers everything it printed on stderr.
    fn stop(mut self) -> String {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.stderr
            .recv_timeout(ANSWER)
            .expect("stderr is read to its end once the run stops")
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `run` on the fixture and laptop with `arguments` added; `Err` holds the exit code, stdout and
/// stderr of a run that stopped without listening.
fn serve(arguments: &[&str]) -> Result<Served, (Option<i32>, String, String)> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--environment", LAPTOP])
        .args(arguments)
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
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let _ = reader.read_line(&mut line);
        let mut rest = String::new();
        if line.is_empty() {
            let _ = reader.read_to_string(&mut rest);
        }
        let _ = sender.send((line, rest));
    });
    let Ok((line, rest)) = receiver.recv_timeout(STARTUP) else {
        let _ = child.kill();
        let _ = child.wait();
        panic!("run printed no line within {STARTUP:?}");
    };
    if line.is_empty() {
        let status = child.wait().expect("the run is reaped");
        let stderr = stderr_read
            .recv_timeout(ANSWER)
            .expect("stderr is read to its end");
        return Err((status.code(), rest, stderr));
    }
    let ready: Value = serde_json::from_str(&line)
        .unwrap_or_else(|error| panic!("run's first line {line:?} is not JSON ({error})"));
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("run's first line names no address: {ready}"));
    Ok(Served {
        child,
        address,
        stderr: stderr_read,
    })
}

fn serving(arguments: &[&str]) -> Served {
    serve(arguments).unwrap_or_else(|stopped| panic!("run stopped before listening: {stopped:?}"))
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
    let body = serde_json::from_str(body).unwrap_or_else(|error| panic!("{error}: {body:?}"));
    (status, body)
}

fn call(served: &Served, operation: &str, body: Option<&Value>) -> (u16, Value) {
    let (method, path) = route(operation);
    exchange(served.address, &method, &path, body)
}

fn writer() -> String {
    json!({"owner_id": OWNER}).to_string()
}

#[test]
fn a_command_reading_the_caller_runs_with_the_fixed_caller() {
    let caller = writer();
    let served = serving(&["--fixed-actor", WRITER, "--fixed-caller", &caller]);
    let (status, created) = call(
        &served,
        CREATE,
        Some(&json!({"notebook_id": NOTEBOOK, "title": "field notes"})),
    );
    assert_eq!(status, 202, "{created}");
    assert_eq!(created["outcome"], "created", "{created}");
    assert_eq!(
        created["published"][0]["payload"]["owner_id"], OWNER,
        "{{caller: owner_id}} reads the fixed caller: {created}"
    );
    let (status, refused) = call(
        &served,
        SHARE,
        Some(&json!({"notebook_id": NOTEBOOK, "with": OWNER})),
    );
    assert_eq!(
        refused["outcome"], "with-self",
        "the guard `with == caller.owner_id` reads the fixed owner_id: {status} {refused}"
    );
    assert_eq!(refused["error"], "quire.notebook.SharedWithSelf");
    let (status, shared) = call(
        &served,
        SHARE,
        Some(&json!({"notebook_id": NOTEBOOK, "with": OTHER})),
    );
    assert_eq!(status, 202, "{shared}");
    assert_eq!(
        shared["outcome"], "shared",
        "`owner_id != caller.owner_id` does not hold for the fixed owner: {shared}"
    );
}

#[test]
fn without_the_flag_nothing_changes() {
    let served = serving(&[]);
    let (status, body) = call(
        &served,
        SHARE,
        Some(&json!({"notebook_id": NOTEBOOK, "with": OTHER})),
    );
    assert_eq!(status, 501, "{body}");
    assert_eq!(body["committed"], false, "{body}");
    let refused = body["refused"].as_str().unwrap_or_default();
    assert!(
        refused.contains("the caller attributes it reads (`owner_id`)"),
        "{body}"
    );
    let stderr = served.stop();
    assert!(
        !stderr.contains("--fixed-actor"),
        "no caller is fixed by default: {stderr}"
    );
}

#[test]
fn the_fixed_caller_is_printed_on_stderr_at_startup() {
    let caller = writer();
    let served = serving(&["--fixed-actor", WRITER, "--fixed-caller", &caller]);
    let stderr = served.stop();
    let line = stderr
        .lines()
        .find(|line| line.contains("--fixed-actor"))
        .unwrap_or_else(|| {
            panic!("a line naming the fixed caller, with no request sent: {stderr}")
        });
    assert!(line.contains(WRITER), "{line}");
    assert!(line.contains("owner_id"), "{line}");
    assert!(line.contains(OWNER), "{line}");
    assert!(line.contains("not authentication"), "{line}");
}

#[test]
fn the_fixed_caller_does_not_bypass_the_grant_check() {
    let caller = writer();
    let served = serving(&["--fixed-actor", READER, "--fixed-caller", &caller]);
    let (status, body) = call(
        &served,
        CREATE,
        Some(&json!({"notebook_id": NOTEBOOK, "title": "field notes"})),
    );
    assert_eq!(status, 403, "{body}");
    assert_eq!(
        body,
        json!({"refused": "not granted", "actor": READER}),
        "ESS's standard refusal for an actor no grant admits"
    );
    let (status, rows) = call(&served, CARD, None);
    assert_eq!(status, 200, "{rows}");
    assert_eq!(rows["rows"], json!([]), "nothing is written: {rows}");
    let (status, events) = exchange(served.address, "GET", "/api/events", None);
    assert_eq!(status, 200, "{events}");
    assert_eq!(
        events["events"],
        json!([]),
        "nothing is published: {events}"
    );
}

#[test]
fn a_fixed_caller_missing_a_required_attribute_is_refused_at_start() {
    for arguments in [
        vec!["--fixed-actor", WRITER],
        vec!["--fixed-actor", WRITER, "--fixed-caller", "{}"],
    ] {
        let Err((code, stdout, stderr)) = serve(&arguments) else {
            panic!("{arguments:?} listened without the caller's owner_id");
        };
        assert_ne!(code, Some(0), "{arguments:?}: {stderr}");
        assert!(
            !stdout.contains("listening"),
            "{arguments:?} listened: {stdout}"
        );
        assert!(stderr.contains("owner_id"), "{arguments:?}: {stderr}");
    }
}
