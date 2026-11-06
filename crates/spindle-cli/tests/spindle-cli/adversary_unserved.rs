//! Adversarial cases for `run` serving what runs and answering the rest `501`.
//!
//! `tests/fixtures/adversary_unserved` declares one command, `WriteNote`, which runs, and two
//! views: `AllNotes`, which the host reads, and `KeptNotes`, whose filter reads a field rather
//! than the state, which the host does not read. `tests/fixtures/adversary_unserved-http.yaml`
//! binds `AllNotes` to `GET /notes` with `body: array`, which this ingress does not serve.
//! `tests/fixtures/adversary_escalate` binds one event to a command twice: `audit-note` with
//! `on_failure: escalate` and `index-note` with `on_failure: retry`.

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const UNSERVED: &str = "crates/spindle-cli/tests/fixtures/adversary_unserved";
const UNSERVED_HTTP: &str = "crates/spindle-cli/tests/fixtures/adversary_unserved-http.yaml";
const ESCALATE: &str = "crates/spindle-cli/tests/fixtures/adversary_escalate";
const PARTIAL: &str = "crates/spindle-cli/tests/fixtures/partial";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "pad";

const WRITE_NOTE: &str = "notes.pad.WriteNote";
const ALL_NOTES: &str = "notes.pad.AllNotes";
const KEPT_NOTES: &str = "notes.pad.KeptNotes";
const COPY_NOTE: &str = "notes.pad.CopyNote";

const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(10);
const EXIT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(20);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory")
}

/// The `OpenAPI` document ESS projects for `pad`, with `binding` where one is given.
struct Contract(Value);

impl Contract {
    fn load(spec: &str, binding: Option<&str>) -> Self {
        let loaded = spindle_spec::load(&root().join(spec)).expect("the fixture compiles");
        let ir = &loaded.ir;
        let component = ir
            .components()
            .values()
            .find(|component| component.name.as_str() == COMPONENT)
            .expect("the fixture declares the component");
        let binding = binding.map(|path| {
            let text = std::fs::read_to_string(root().join(path)).expect("the binding is read");
            let spec = ess_http::HttpSpec::from_yaml(&text).expect("the binding is YAML");
            let compiled = ess_http::compile(&spec, ir, &ess_gen::http::derived(ir))
                .unwrap_or_else(|diagnostics| panic!("the binding compiles: {diagnostics}"));
            ess_gen::http::Binding::new(compiled)
                .unwrap_or_else(|open| panic!("the binding is closed: {open}"))
        });
        let document = ess_gen::openapi::bound_json(ir, component, binding.as_ref());
        Self(serde_json::from_str(&document).expect("the projection is JSON"))
    }

    /// Every operation the contract declares: its id, method and path.
    fn operations(&self) -> Vec<(String, String, String)> {
        let mut out = Vec::new();
        for (path, item) in self.0["paths"].as_object().expect("paths") {
            for (method, operation) in item.as_object().expect("a path item") {
                if let Some(id) = operation["operationId"].as_str() {
                    out.push((id.to_owned(), method.to_uppercase(), path.clone()));
                }
            }
        }
        out
    }

    fn route(&self, id: &str) -> (String, String) {
        self.operations()
            .into_iter()
            .find(|(declared, _, _)| declared == id)
            .map(|(_, method, path)| (method, path))
            .unwrap_or_else(|| panic!("the contract declares no operation {id}"))
    }

    /// The statuses the contract declares for `id`.
    fn statuses(&self, id: &str) -> Vec<String> {
        let (method, path) = self.route(id);
        self.0["paths"][&path][method.to_lowercase()]["responses"]
            .as_object()
            .map(|responses| responses.keys().cloned().collect())
            .unwrap_or_default()
    }
}

#[derive(Clone, Default)]
struct Stderr(Arc<Mutex<Vec<String>>>);

impl Stderr {
    fn read(child: &mut Child) -> Self {
        let lines = Self::default();
        let stderr = child.stderr.take().expect("stderr is piped");
        let filled = lines.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                filled.0.lock().expect("the stderr lines").push(line);
            }
        });
        lines
    }

    fn lines(&self) -> Vec<String> {
        self.0.lock().expect("the stderr lines").clone()
    }

    fn until(&self, needle: &str) -> Vec<String> {
        let deadline = Instant::now() + ANSWER;
        loop {
            let lines = self.lines();
            if lines.iter().any(|line| line.contains(needle)) {
                return lines;
            }
            assert!(
                Instant::now() < deadline,
                "no stderr line contains {needle:?} within {ANSWER:?}: {lines:#?}"
            );
            std::thread::sleep(POLL);
        }
    }
}

/// A child process killed and waited on when dropped, on every path out of a test.
struct Running {
    child: Child,
    stdin: Option<ChildStdin>,
    stderr: Stderr,
    _scratch: tempfile::TempDir,
}

impl Drop for Running {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Running {
    fn start(arguments: &[&str], stdin: Stdio) -> Self {
        let scratch = scratch();
        let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
            .current_dir(root())
            .env("TMPDIR", scratch.path())
            .args(arguments)
            .stdin(stdin)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("b10x-spindle starts");
        let stderr = Stderr::read(&mut child);
        let stdin = child.stdin.take();
        Self {
            child,
            stdin,
            stderr,
            _scratch: scratch,
        }
    }

    fn signal(&self, signal: rustix::process::Signal) {
        rustix::process::kill_process(rustix::process::Pid::from_child(&self.child), signal)
            .expect("the signal is sent");
    }

    fn exited(&mut self) -> std::process::ExitStatus {
        let deadline = Instant::now() + EXIT;
        loop {
            if let Some(status) = self.child.try_wait().expect("waited on") {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "run did not exit within {EXIT:?}"
            );
            std::thread::sleep(POLL);
        }
    }
}

/// `run <spec>` serving HTTP; the running process and the address it listens on.
fn serve(spec: &str, arguments: &[&str]) -> (Running, SocketAddr) {
    let mut all = vec!["run", spec, "--environment", LAPTOP];
    all.extend_from_slice(arguments);
    let mut running = Running::start(&all, Stdio::null());
    let stdout = running.child.stdout.take().expect("stdout is piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = sender.send(line);
    });
    let line = receiver
        .recv_timeout(STARTUP)
        .unwrap_or_else(|error| panic!("run printed no line within {STARTUP:?}: {error}"));
    let ready: Value = serde_json::from_str(&line).unwrap_or_else(|error| {
        panic!(
            "run's first line {line:?} is not JSON ({error}); stderr: {:#?}",
            running.stderr.lines()
        )
    });
    let address = ready["listening"]
        .as_str()
        .and_then(|address| address.parse().ok())
        .unwrap_or_else(|| panic!("run's first line names no address: {ready}"));
    (running, address)
}

fn exchange(address: SocketAddr, method: &str, path: &str, body: Option<&str>) -> (u16, Value) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    let body = body.unwrap_or_default();
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
    let body = serde_json::from_str(body).unwrap_or(Value::String(body.to_owned()));
    (status, body)
}

/// The operations named on a `not served:` line of `stderr`.
fn listed(stderr: &[String], operations: &[(String, String, String)]) -> BTreeSet<String> {
    stderr
        .iter()
        .filter(|line| line.contains("not served:"))
        .flat_map(|line| {
            operations
                .iter()
                .filter(|(id, _, _)| line.contains(&format!("`{id}`")))
                .map(|(id, _, _)| id.clone())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The startup list names each operation that answers `501` and none that serves: every
/// operation of the contract is asked once, and the set that answered `501` is the set listed.
#[test]
fn adversary_startup_lists_exactly_the_operations_that_answer_501() {
    let contract = Contract::load(UNSERVED, None);
    let state = scratch();
    let file = state.path().join("state");
    let (running, address) = serve(UNSERVED, &["--state", file.to_str().expect("a UTF-8 path")]);
    let operations = contract.operations();
    let mut answered = BTreeSet::new();
    for (id, method, path) in &operations {
        let body = (id == WRITE_NOTE).then_some(r#"{"note_id": "n1", "text": "kept"}"#);
        let (status, answer) = exchange(address, method, path, body);
        if status == 501 {
            answered.insert(id.clone());
        }
        if id == KEPT_NOTES {
            assert_eq!(status, 501, "the host does not read this filter: {answer}");
        }
    }
    let listed = listed(&running.stderr.lines(), &operations);
    assert_eq!(
        listed,
        answered,
        "the operations listed at startup are the operations that answer 501; stderr: {:#?}",
        running.stderr.lines()
    );
}

/// A view this ingress cannot route keeps its route and is marked unserved, as a command is: the
/// run starts, lists the view at startup naming what it uses, answers it `501` naming the
/// construct, and serves the rest. The OpenAPI ESS projects for the binding declares no `501` for
/// the view, as it declares none for a view the host does not read, which also answers `501`.
#[test]
fn adversary_an_unroutable_view_answers_501_by_name() {
    let contract = Contract::load(UNSERVED, Some(UNSERVED_HTTP));
    assert!(
        !contract.statuses(ALL_NOTES).contains(&"501".to_owned()),
        "the contract declares no 501 for {ALL_NOTES}"
    );
    let state = scratch();
    let file = state.path().join("state");
    let (running, address) = serve(
        UNSERVED,
        &[
            "--http",
            UNSERVED_HTTP,
            "--state",
            file.to_str().expect("a UTF-8 path"),
        ],
    );
    let stderr = running.stderr.until(ALL_NOTES);
    assert!(
        stderr.iter().any(|line| line.contains("not served:")
            && line.contains(&format!("`{ALL_NOTES}`"))
            && line.contains("a view answered other than as rows")),
        "startup names the view and what it uses: {stderr:#?}"
    );
    let (method, path) = contract.route(WRITE_NOTE);
    let (status, answer) = exchange(
        address,
        &method,
        &path,
        Some(r#"{"note_id": "n1", "text": "kept"}"#),
    );
    assert_eq!(status, 202, "{answer}");
    let (method, path) = contract.route(ALL_NOTES);
    let (status, answer) = exchange(address, &method, &path, None);
    assert_eq!(status, 501, "{answer}");
    assert_eq!(answer["committed"], false, "{answer}");
    assert!(
        answer["refused"].as_str().is_some_and(|refused| {
            refused.contains(ALL_NOTES) && refused.contains("a view answered other than as rows")
        }),
        "the 501 names the view and the construct: {answer}"
    );
}

/// Without `--state`, `run` reading stdin keeps sqlite state in a temporary directory "removed
/// when stdin ends"; a SIGTERM before stdin ends leaves it behind.
#[test]
fn adversary_stdin_run_removes_its_temporary_state_on_sigterm() {
    let mut running = Running::start(&["run", "ess", "--environment", LAPTOP], Stdio::piped());
    let lines = running.stderr.until("state is kept in");
    let line = lines
        .iter()
        .find(|line| line.contains("state is kept in"))
        .expect("the line naming the state");
    let directory = PathBuf::from(
        line.split("state is kept in ")
            .nth(1)
            .and_then(|rest| rest.split(", removed").next())
            .unwrap_or_else(|| panic!("the line names a path: {line}")),
    );
    assert!(directory.is_dir(), "{line}");
    running.signal(rustix::process::Signal::TERM);
    let status = running.exited();
    let left = directory.exists();
    if left {
        let _ = std::fs::remove_dir_all(&directory);
    }
    assert!(
        !left,
        "the temporary state {} is left behind after SIGTERM ({status:?})",
        directory.display()
    );
}

/// Each serving run without `--state` gets a fresh temporary directory, removed on SIGINT as on
/// SIGTERM.
#[test]
fn adversary_temporary_state_is_fresh_per_run_and_removed_on_sigint() {
    let mut seen = Vec::new();
    for _ in 0..2 {
        let (mut running, address) = serve(PARTIAL, &[]);
        let lines = running.stderr.until("state is kept in");
        let line = lines
            .iter()
            .find(|line| line.contains("state is kept in"))
            .expect("the line naming the state")
            .clone();
        let directory = PathBuf::from(
            line.split("state is kept in ")
                .nth(1)
                .and_then(|rest| rest.split(", removed").next())
                .unwrap_or_else(|| panic!("the line names a path: {line}")),
        );
        let (status, answer) = exchange(
            address,
            "POST",
            "/pad/commands/WriteNote",
            Some(r#"{"note_id": "n1", "text": "kept"}"#),
        );
        assert_eq!(status, 202, "a fresh store accepts n1 again: {answer}");
        running.signal(rustix::process::Signal::INT);
        assert_eq!(running.exited().code(), Some(130));
        assert!(!directory.exists(), "{} is removed", directory.display());
        assert!(!seen.contains(&directory), "{} reused", directory.display());
        seen.push(directory);
    }
}

/// `--drop-retries` lifts the refusal of `retry` only; `escalate` stays refused, naming its
/// binding, and nothing listens.
#[test]
fn adversary_escalate_is_refused_even_with_drop_retries() {
    let state = scratch();
    let file = state.path().join("state");
    let mut running = Running::start(
        &[
            "run",
            ESCALATE,
            "--environment",
            LAPTOP,
            "--drop-retries",
            "--state",
            file.to_str().expect("a UTF-8 path"),
        ],
        Stdio::null(),
    );
    let status = running.exited();
    let mut stdout = String::new();
    running
        .child
        .stdout
        .take()
        .expect("stdout")
        .read_to_string(&mut stdout)
        .expect("stdout is read");
    let stderr = running.stderr.until("b10x-spindle:");
    assert!(!status.success(), "{status:?} {stdout} {stderr:#?}");
    assert!(!stdout.contains("listening"), "{stdout}");
    assert!(
        stderr
            .iter()
            .any(|line| line.contains("audit-note") && line.contains("Escalate")),
        "{stderr:#?}"
    );
}

/// The console routes answer beside the routes that answer `501`, and a `501` route answers
/// `501` whatever body it is sent.
#[test]
fn adversary_console_routes_answer_beside_501_routes() {
    let state = scratch();
    let file = state.path().join("state");
    let (_running, address) = serve(PARTIAL, &["--state", file.to_str().expect("a UTF-8 path")]);
    let (status, answer) = exchange(address, "POST", "/pad/commands/CopyNote", Some("not json"));
    assert_eq!(status, 501, "{answer}");
    assert!(
        answer["refused"]
            .as_str()
            .is_some_and(|refused| refused.contains(COPY_NOTE)),
        "{answer}"
    );
    assert_eq!(exchange(address, "GET", "/api/model", None).0, 200);
    assert_eq!(
        exchange(address, "GET", "/api/events", None),
        (200, json!({"events": []}))
    );
    assert_eq!(exchange(address, "POST", "/api/reset", None).0, 200);
    assert_eq!(exchange(address, "GET", "/api/reset", None).0, 405);
}
