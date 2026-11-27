//! `run --log-format json`: every line a served run writes on stderr is one JSON object with
//! `time`, `level`, `message` and the fields it names (`story:structured-json-logs`).
//!
//! `tests/fixtures/retried` binds `NoteWritten` to `IndexNote` with `on_failure: retry`, and
//! `IndexNote` never runs, so each written note drops one bound command after 5 attempts.
//! `tests/fixtures/seed` refuses a `RetireRole` of a role nobody granted.
//! `tests/fixtures/stdin-lines` requires no `ingress.network`, so `run` reads stdin there.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const RETRIED: &str = "crates/spindle-cli/tests/fixtures/retried";
const SEEDED: &str = "crates/spindle-cli/tests/fixtures/seed";
const STDIN: &str = "crates/spindle-cli/tests/fixtures/stdin-lines";
const LAPTOP: &str = "examples/environments/laptop.toml";
const PAD: &str = "pad";
const WRITE_NOTE: &str = "notes.pad.WriteNote";
const INDEX_NOTE: &str = "notes.index.IndexNote";
const RETIRE: &str = "vault.access.RetireRole";
const OPERATOR: &str = "vault.access.Operator";

/// The drop line `run` prints without `--log-format`, as it printed it before the flag existed.
const DROP_LINE: &str = "b10x-spindle: index did not run notes.index.IndexNote (index-note), \
                         dropped after 5 attempts: unsupported: `input_absent:`, in \
                         `notes.index.IndexNote/body-missing`";

/// How long `run` may take to admit, open its state and print where it listens.
const STARTUP: Duration = Duration::from_secs(30);
/// How long one request, or one awaited stderr line, may take.
const ANSWER: Duration = Duration::from_secs(10);
/// How often a wait looks again.
const POLL: Duration = Duration::from_millis(20);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory")
}

/// Every line `run` wrote on stderr so far, read on a thread of its own.
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

    /// The first line holding `needle`, waiting at most [`ANSWER`].
    fn until(&self, needle: &str) -> String {
        let deadline = Instant::now() + ANSWER;
        loop {
            if let Some(line) = self.lines().into_iter().find(|line| line.contains(needle)) {
                return line;
            }
            assert!(
                Instant::now() < deadline,
                "no stderr line holds {needle:?} within {ANSWER:?}: {:#?}",
                self.lines()
            );
            std::thread::sleep(POLL);
        }
    }
}

/// A running `b10x-spindle run` serving HTTP; killed when dropped.
struct Served {
    child: Child,
    address: SocketAddr,
    stderr: Stderr,
    _scratch: tempfile::TempDir,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Served {
    /// Sends SIGTERM and waits for the exit: its status.
    fn terminate(&mut self) -> ExitStatus {
        rustix::process::kill_process(
            rustix::process::Pid::from_child(&self.child),
            rustix::process::Signal::TERM,
        )
        .expect("SIGTERM is sent");
        let deadline = Instant::now() + STARTUP;
        loop {
            if let Some(status) = self.child.try_wait().expect("waited on") {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "run did not exit within {STARTUP:?} of SIGTERM"
            );
            std::thread::sleep(POLL);
        }
    }

    /// Every stderr line once the stream has ended.
    fn all_lines(&self) -> Vec<String> {
        let deadline = Instant::now() + ANSWER;
        let mut seen = self.stderr.lines().len();
        loop {
            std::thread::sleep(Duration::from_millis(100));
            let now = self.stderr.lines().len();
            if now == seen || Instant::now() > deadline {
                return self.stderr.lines();
            }
            seen = now;
        }
    }
}

/// `b10x-spindle run <spec> <arguments>`, its `TMPDIR` in a scratch directory, once it listens.
fn serve(spec: &str, arguments: &[&str]) -> Served {
    let directory = scratch();
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .env("TMPDIR", directory.path())
        .arg("run")
        .arg(spec)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let stderr = Stderr::read(&mut child);
    let stdout = child.stdout.take().expect("stdout is piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = sender.send(line);
    });
    let Ok(line) = receiver.recv_timeout(STARTUP) else {
        let _ = child.kill();
        let _ = child.wait();
        panic!("run printed no line within {STARTUP:?}");
    };
    let ready: Value = serde_json::from_str(&line).unwrap_or_else(|error| {
        let _ = child.kill();
        let _ = child.wait();
        panic!(
            "run's first line {line:?} is not JSON ({error}); stderr: {:#?}",
            stderr.lines()
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
        _scratch: directory,
    }
}

/// The method and path ESS publishes for the operation `id` of the fixture `spec`'s component
/// [`PAD`].
fn route(spec: &str, id: &str) -> (String, String) {
    let loaded = spindle_spec::load(&root().join(spec)).expect("the fixture compiles");
    let component = loaded
        .ir
        .components()
        .values()
        .find(|component| component.name.as_str() == PAD)
        .expect("the fixture declares the component");
    let document: Value = serde_json::from_str(&ess_gen::openapi::json(&loaded.ir, component))
        .expect("the projection is JSON");
    let paths = document["paths"]
        .as_object()
        .expect("the document has paths");
    for (path, item) in paths {
        for (method, operation) in item.as_object().expect("a path item is an object") {
            if operation["operationId"] == id {
                return (method.to_uppercase(), path.clone());
            }
        }
    }
    panic!("the contract declares no operation {id}")
}

/// One HTTP/1.1 exchange on its own connection: the status.
fn exchange(address: SocketAddr, method: &str, path: &str, body: &Value) -> u16 {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    let body = body.to_string();
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
    let answer = String::from_utf8_lossy(&answer);
    answer
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("a status line: {answer:?}"))
}

/// Sends `WriteNote` with `input` on the retried fixture: its status.
fn send_note(served: &Served, input: &Value) -> u16 {
    let (method, path) = route(RETRIED, WRITE_NOTE);
    exchange(served.address, &method, &path, input)
}

/// Writes the note `id` on the retried fixture: its status.
fn write_note(served: &Served, id: &str) -> u16 {
    send_note(served, &json!({"note_id": id, "text": "kept"}))
}

/// `line` as one JSON object with `time` (RFC 3339 in UTC), `level` and `message`.
fn object(line: &str) -> serde_json::Map<String, Value> {
    let Ok(Value::Object(object)) = serde_json::from_str::<Value>(line) else {
        panic!("a stderr line that is not one JSON object: {line:?}");
    };
    let time = object["time"]
        .as_str()
        .unwrap_or_else(|| panic!("no time: {line}"));
    assert!(
        time.ends_with('Z') && spindle_cli::cli::instant(time).is_ok(),
        "time is RFC 3339 in UTC: {line}"
    );
    assert!(
        ["info", "warn", "error"].contains(&object["level"].as_str().unwrap_or_default()),
        "level is info, warn or error: {line}"
    );
    assert!(object["message"].is_string(), "message is a string: {line}");
    object
}

/// A run sent a command, a refused command and SIGTERM writes only JSON objects on stderr.
#[test]
fn every_stderr_line_is_one_json_object() {
    // SIGTERM drains the run (`--drain`, `story:health-and-graceful-drain`) before it exits 143.
    let mut served = serve(
        RETRIED,
        &[
            "--environment",
            LAPTOP,
            "--log-format",
            "json",
            "--drain",
            "1",
        ],
    );
    assert_eq!(write_note(&served, "n1"), 202);
    served.stderr.until(INDEX_NOTE);
    let refused = send_note(&served, &json!({"note_id": "n2"}));
    assert!(
        (400..500).contains(&refused),
        "a note without its required text is refused: {refused}"
    );
    let status = served.terminate();
    assert_eq!(status.code(), Some(143), "{status:?}");
    let lines = served.all_lines();
    assert!(
        lines.len() >= 2,
        "the start line and the drop line at least: {lines:#?}"
    );
    for line in &lines {
        object(line);
    }
}

/// The dropped bound command's line names it in fields of its own.
#[test]
fn a_dropped_binding_command_names_its_fields() {
    let served = serve(RETRIED, &["--environment", LAPTOP, "--log-format", "json"]);
    assert_eq!(write_note(&served, "n1"), 202);
    let line = object(&served.stderr.until(INDEX_NOTE));
    assert_eq!(line["component"], "index", "{line:?}");
    assert_eq!(line["command"], INDEX_NOTE, "{line:?}");
    assert_eq!(line["binding"], "index-note", "{line:?}");
    assert_eq!(line["attempts"], 5, "{line:?}");
    assert!(
        line["error"]
            .as_str()
            .is_some_and(|error| !error.is_empty()),
        "{line:?}"
    );
    assert_eq!(line["level"], "warn", "{line:?}");
}

/// The line of a refused seed entry carries its request id.
#[test]
fn a_seed_entry_line_names_its_request_id() {
    let directory = scratch();
    let seed = directory.path().join("seed.json");
    std::fs::write(
        &seed,
        json!([
            {"command": RETIRE, "input": {"role_id": "ghost"}, "actor": OPERATOR,
             "caller": {"tenant": "north"}, "request_id": "seed-retire-ghost"},
        ])
        .to_string(),
    )
    .expect("the seed is written");
    let environment = directory.path().join("environment.toml");
    std::fs::write(
        &environment,
        "format = \"spindle-environment/1\"\nname = \"seeded\"\nos = \"linux\"\narch = \"x86_64\"\n\
         \n[provides]\n\"store.durable\" = [\"memory\"]\n\"store.ephemeral\" = [\"memory\"]\n\
         \"identity.mint\" = [\"uuid-v4\"]\nclock = [\"system\"]\ntimer = [\"tokio\"]\n\
         channel = [\"in-process\"]\nprojection = [\"in-process\"]\n\
         \"ingress.network\" = [\"http-loopback\"]\n\"external.*\" = [\"operator-prompt\"]\n",
    )
    .expect("the environment is written");
    let served = serve(
        SEEDED,
        &[
            "--environment",
            environment.to_str().expect("a UTF-8 path"),
            "--seed",
            seed.to_str().expect("a UTF-8 path"),
            "--answer-default",
            "--log-format",
            "json",
        ],
    );
    let line = object(&served.stderr.until("seed-retire-ghost"));
    assert_eq!(line["request_id"], "seed-retire-ghost", "{line:?}");
    assert_eq!(line["command"], RETIRE, "{line:?}");
    assert_eq!(line["error"], "vault.access.RoleMissing", "{line:?}");
}

/// A run reading stdin has no served log to write as JSON: it exits 1 before reading a line.
#[test]
fn json_logs_on_a_stdin_run_exit_1() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args([
            "run",
            STDIN,
            "--environment",
            LAPTOP,
            "--log-format",
            "json",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let line = json!({"command": "shelf.books.AddBook", "input": {"title": "Middlemarch"}});
    // The run may have exited before the line is written.
    let _ = child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(format!("{line}\n").as_bytes());
    let output = child.wait_with_output().expect("b10x-spindle runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        output.stdout.is_empty(),
        "no stdin line is answered: {output:?}"
    );
    assert!(stderr.contains("--log-format"), "{stderr}");
}

/// Without the flag the drop line is the text it was before the flag.
#[test]
fn text_stays_the_default() {
    let served = serve(RETRIED, &["--environment", LAPTOP]);
    assert_eq!(write_note(&served, "n1"), 202);
    assert_eq!(served.stderr.until(INDEX_NOTE), DROP_LINE);
}
