//! `run` serving HTTP on a specification some of whose operations it does not run: it serves the
//! rest, answers each of those `501` with ESS's envelope, and lists them at startup.
//!
//! `tests/fixtures/partial` declares three commands on one component reached over the network:
//! `CopyNote`, whose outcome answers with the command's retained result (`replays:`), which this
//! ingress does not serve; `DiscardNote`, whose set outcome (`instances:`) the kernel does not
//! run; and `WriteNote`, which runs. `tests/fixtures/unservable` holds only the first two.
//! `tests/fixtures/retried` binds an event to a command with `on_failure: retry`, and the command
//! it invokes fails. `tests/fixtures/list-parameter` declares `WriteNote`, which runs, `AllNotes`,
//! which the host reads, and `NotesTagged`, whose `List<String>` parameter the query string
//! carries as its key repeated once per element (`story:view-list-query-parameters`).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const PARTIAL: &str = "crates/spindle-cli/tests/fixtures/partial";
const UNSERVABLE: &str = "crates/spindle-cli/tests/fixtures/unservable";
const RETRIED: &str = "crates/spindle-cli/tests/fixtures/retried";
const ESCALATE: &str = "crates/spindle-cli/tests/fixtures/adversary_escalate";
const INGRESS: &str = "crates/spindle-cli/tests/fixtures/ingress";
const LIST_PARAMETER: &str = "crates/spindle-cli/tests/fixtures/list-parameter";
const LAPTOP: &str = "examples/environments/laptop.toml";
const COMPONENT: &str = "pad";

const WRITE_NOTE: &str = "notes.pad.WriteNote";
const DISCARD_NOTE: &str = "notes.pad.DiscardNote";
const COPY_NOTE: &str = "notes.pad.CopyNote";
const ALL_NOTES: &str = "notes.pad.AllNotes";
const NOTES_TAGGED: &str = "notes.pad.NotesTagged";

/// How long `run` may take to admit, open its state and print where it listens.
const STARTUP: Duration = Duration::from_secs(60);
/// How long one request may take to be answered.
const ANSWER: Duration = Duration::from_secs(10);
/// How long a refused or signalled `run` may take to exit.
const EXIT: Duration = Duration::from_secs(60);
/// How often a wait looks again.
const POLL: Duration = Duration::from_millis(20);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The `OpenAPI` document ESS projects for the fixture's served component.
struct Contract(Value);

impl Contract {
    fn load(spec: &str) -> Self {
        let loaded = spindle_spec::load(&root().join(spec)).expect("the fixture compiles");
        let component = loaded
            .ir
            .components()
            .values()
            .find(|component| component.name.as_str() == COMPONENT)
            .expect("the fixture declares the component");
        let document = ess_gen::openapi::json(&loaded.ir, component);
        Self(serde_json::from_str(&document).expect("the projection is JSON"))
    }

    /// The method and path of the operation `id`.
    fn route(&self, id: &str) -> (String, String) {
        let paths = self.0["paths"].as_object().expect("the document has paths");
        for (path, item) in paths {
            for (method, operation) in item.as_object().expect("a path item is an object") {
                if operation["operationId"] == id {
                    return (method.to_uppercase(), path.clone());
                }
            }
        }
        panic!("the contract declares no operation {id}")
    }

    /// Fails unless `body` validates against the schema the contract declares for `id` under
    /// `status`.
    fn validate(&self, id: &str, status: u16, body: &Value) {
        let (method, path) = self.route(id);
        let schema = &self.0["paths"][&path][method.to_lowercase()]["responses"]
            [status.to_string()]["content"]["application/json"]["schema"];
        assert!(
            !schema.is_null(),
            "the contract declares no JSON answer of {id} under {status}"
        );
        let mut document = self.0.clone();
        document["allOf"] = json!([schema]);
        let validator =
            jsonschema::draft202012::new(&document).expect("the contract compiles as a schema");
        let errors = validator
            .iter_errors(body)
            .map(|error| format!("{} at {}", error, error.instance_path()))
            .collect::<Vec<_>>();
        assert!(
            errors.is_empty(),
            "{id} answered {status} with {body}, which its projected schema refuses: {errors:?}"
        );
    }
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

    /// The lines once one contains `needle`, waiting at most [`ANSWER`].
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

/// A running `b10x-spindle run` serving HTTP; stopped when dropped.
struct Served {
    child: Child,
    address: SocketAddr,
    stderr: Stderr,
    _state: tempfile::TempDir,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A scratch directory under the test target, used as the run's `TMPDIR` and for its state.
fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory")
}

/// `b10x-spindle run <spec> --environment laptop <arguments>`, its `TMPDIR` under `scratch`.
fn spawn(spec: &str, arguments: &[&str], scratch: &Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .env("TMPDIR", scratch)
        .args(["run", spec, "--environment", LAPTOP])
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts")
}

/// `run` serving `spec`, its state in a scratch directory unless `arguments` say otherwise.
fn serve(spec: &str, arguments: &[&str]) -> Served {
    let state = scratch();
    let mut with_state = arguments.to_vec();
    let file = state.path().join("state");
    let file = file.to_str().expect("a UTF-8 path").to_owned();
    if !arguments.contains(&"--no-state") {
        with_state.extend(["--state", file.as_str()]);
    }
    with_state.retain(|argument| *argument != "--no-state");
    let mut child = spawn(spec, &with_state, state.path());
    let stderr = Stderr::read(&mut child);
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
        _state: state,
    }
}

/// `run` on `spec` that is expected to exit on its own: its status, stdout and stderr.
fn refused(spec: &str, arguments: &[&str]) -> (std::process::ExitStatus, String, String) {
    let state = scratch();
    let mut child = spawn(spec, arguments, state.path());
    let started = Instant::now();
    loop {
        if child.try_wait().expect("the child is waited on").is_some() {
            break;
        }
        if started.elapsed() > EXIT {
            let _ = child.kill();
            let _ = child.wait();
            panic!("run did not exit within {EXIT:?}");
        }
        std::thread::sleep(POLL);
    }
    let output = child.wait_with_output().expect("the output is read");
    (
        output.status,
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
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

fn call(served: &Served, contract: &Contract, id: &str, body: Option<&Value>) -> (u16, Value) {
    let (method, path) = contract.route(id);
    exchange(served.address, &method, &path, body)
}

fn events(served: &Served) -> Vec<Value> {
    let (status, body) = exchange(served.address, "GET", "/api/events", None);
    assert_eq!(status, 200, "{body}");
    body["events"]
        .as_array()
        .unwrap_or_else(|| panic!("the console answers events: {body}"))
        .clone()
}

/// The lines of `stderr` that name an operation as not served, in order.
fn unserved(stderr: &[String]) -> Vec<String> {
    stderr
        .iter()
        .filter(|line| line.contains("not served:"))
        .cloned()
        .collect()
}

#[test]
fn an_unrunnable_command_answers_501_and_the_rest_serve() {
    let contract = Contract::load(PARTIAL);
    let served = serve(PARTIAL, &[]);

    let (status, answer) = call(
        &served,
        &contract,
        WRITE_NOTE,
        Some(&json!({"note_id": "n1", "text": "kept"})),
    );
    assert_eq!(status, 202, "{answer}");
    assert_eq!(answer["outcome"], "written");
    contract.validate(WRITE_NOTE, status, &answer);
    let written = events(&served);
    assert_eq!(written.len(), 1, "{written:?}");
    let (status, rows) = call(&served, &contract, ALL_NOTES, None);
    assert_eq!(status, 200, "{rows}");
    assert_eq!(rows, json!({"rows": [{"note_id": "n1", "text": "kept"}]}));

    let (status, answer) = call(
        &served,
        &contract,
        DISCARD_NOTE,
        Some(&json!({"note_id": "n1"})),
    );
    assert_eq!(status, 501, "{answer}");
    contract.validate(DISCARD_NOTE, status, &answer);
    assert_eq!(answer["committed"], false, "{answer}");
    let refused = answer["refused"].as_str().expect("a refusal in words");
    assert!(
        refused.contains(DISCARD_NOTE) && refused.contains("`instances:`"),
        "the 501 names the command and the construct: {refused}"
    );

    let (status, answer) = call(
        &served,
        &contract,
        COPY_NOTE,
        Some(&json!({"text": "again"})),
    );
    assert_eq!(status, 501, "{answer}");
    assert_eq!(
        answer.as_object().map(|members| members.len()),
        Some(2),
        "ESS's 501 envelope is closed over refused and committed: {answer}"
    );
    assert_eq!(answer["committed"], false, "{answer}");
    let refused = answer["refused"].as_str().expect("a refusal in words");
    assert!(
        refused.contains(COPY_NOTE) && refused.contains("the command's response"),
        "the 501 names the command and the construct: {refused}"
    );

    assert_eq!(events(&served), written, "no event was published");
    let (status, rows) = call(&served, &contract, ALL_NOTES, None);
    assert_eq!(status, 200, "{rows}");
    assert_eq!(
        rows,
        json!({"rows": [{"note_id": "n1", "text": "kept"}]}),
        "no row was written or removed"
    );
}

#[test]
fn startup_lists_every_unserved_operation() {
    let served = serve(PARTIAL, &[]);
    let lines = unserved(&served.stderr.until(DISCARD_NOTE));
    assert_eq!(
        lines.len(),
        2,
        "one line per unserved operation: {lines:#?}"
    );
    assert!(
        lines[0].contains(COPY_NOTE) && lines[0].contains("the command's response"),
        "{lines:#?}"
    );
    assert!(
        lines[1].contains(DISCARD_NOTE) && lines[1].contains("`instances:`"),
        "{lines:#?}"
    );
    assert!(
        !lines.iter().any(|line| line.contains(WRITE_NOTE)),
        "a served command is not listed: {lines:#?}"
    );
}

#[test]
fn a_run_where_every_operation_is_served_lists_nothing() {
    let served = serve(INGRESS, &[]);
    let (status, _) = exchange(served.address, "GET", "/api/events", None);
    assert_eq!(status, 200);
    assert_eq!(unserved(&served.stderr.lines()), Vec::<String>::new());
}

#[test]
fn a_run_with_nothing_to_serve_is_refused() {
    let (status, stdout, stderr) = refused(UNSERVABLE, &[]);
    assert!(!status.success(), "{status:?} {stdout} {stderr}");
    assert!(!stdout.contains("listening"), "nothing listened: {stdout}");
    assert!(
        stderr.contains(COPY_NOTE) && stderr.contains(DISCARD_NOTE),
        "the refusal lists what is not served: {stderr}"
    );
    assert!(stderr.contains("serves no operation"), "{stderr}");
}

/// Was `a_view_with_a_list_parameter_answers_501_and_the_rest_serve`: since
/// `story:view-list-query-parameters` the list parameter is read from its repeated query key, so
/// the view is served with the rest and startup names no operation as not served.
#[test]
fn a_view_with_a_list_parameter_is_served_with_the_rest() {
    let contract = Contract::load(LIST_PARAMETER);
    let served = serve(LIST_PARAMETER, &[]);

    let (status, answer) = call(
        &served,
        &contract,
        WRITE_NOTE,
        Some(&json!({"note_id": "n1", "text": "kept"})),
    );
    assert_eq!(status, 202, "{answer}");
    assert_eq!(answer["outcome"], "written");
    contract.validate(WRITE_NOTE, status, &answer);
    let (status, rows) = call(&served, &contract, ALL_NOTES, None);
    assert_eq!(status, 200, "{rows}");
    contract.validate(ALL_NOTES, status, &rows);
    assert_eq!(rows, json!({"rows": [{"note_id": "n1", "text": "kept"}]}));

    let (method, path) = contract.route(NOTES_TAGGED);
    for target in [
        format!("{path}?tags=kept"),
        format!("{path}?tags=kept&tags=other"),
    ] {
        let (status, rows) = exchange(served.address, &method, &target, None);
        assert_eq!(status, 200, "{target}: {rows}");
        contract.validate(NOTES_TAGGED, status, &rows);
        assert_eq!(
            rows,
            json!({"rows": [{"note_id": "n1", "text": "kept"}]}),
            "{target}"
        );
    }
    let (status, rows) = exchange(served.address, &method, &format!("{path}?tags=other"), None);
    assert_eq!(status, 200, "{rows}");
    assert_eq!(rows, json!({"rows": []}));
    for target in [format!("{path}?tags=%ZZ"), path.clone()] {
        let (status, answer) = exchange(served.address, &method, &target, None);
        assert_eq!(status, 400, "{target}: {answer}");
        let refused = answer["refused"].as_str().expect("a refusal in words");
        assert!(
            refused.contains("tags"),
            "the 400 names the parameter: {refused}"
        );
    }
    assert!(
        unserved(&served.stderr.lines()).is_empty(),
        "startup names no operation as not served: {:#?}",
        served.stderr.lines()
    );
}

#[test]
fn retry_bindings_start_and_retry_without_drop_retries() {
    let contract = Contract::load(RETRIED);
    let served = serve(RETRIED, &[]);
    let (status, answer) = call(
        &served,
        &contract,
        WRITE_NOTE,
        Some(&json!({"note_id": "n1", "text": "kept"})),
    );
    assert_eq!(status, 202, "{answer}");
    let given_up = served.stderr.until("did not run notes.index.IndexNote");
    let line = given_up
        .iter()
        .find(|line| line.contains("did not run notes.index.IndexNote"))
        .expect("the given-up invocation is reported");
    assert!(
        line.contains("(index-note)") && line.contains("after 5 attempts"),
        "reported once, after its last attempt: {line}"
    );
    assert!(
        !served
            .stderr
            .lines()
            .iter()
            .any(|line| line.contains("--drop-retries")),
        "no retry is dropped without the flag: {:#?}",
        served.stderr.lines()
    );
}

#[test]
fn the_drop_retries_hint_is_given_only_for_a_retry_refusal() {
    let (status, stdout, stderr) = refused(ESCALATE, &[]);
    assert!(!status.success(), "{status:?} {stdout} {stderr}");
    assert!(
        stderr.contains("audit-note") && stderr.contains("Escalate"),
        "the escalate binding is refused first, by name: {stderr}"
    );
    assert!(
        !stderr.contains("--drop-retries"),
        "the flag does not lift an escalate refusal, so it is not offered: {stderr}"
    );
    let (status, stdout, stderr) = refused(ESCALATE, &["--drop-retries"]);
    assert!(!status.success(), "{status:?} {stdout} {stderr}");
    assert!(
        stderr.contains("audit-note") && !stderr.contains("--drop-retries runs"),
        "{stderr}"
    );
}

#[test]
fn drop_retries_starts_and_drops_a_failed_invocation() {
    let contract = Contract::load(RETRIED);
    let served = serve(RETRIED, &["--drop-retries"]);
    let startup = served.stderr.until("index-note");
    let named = startup
        .iter()
        .filter(|line| line.contains("index-note"))
        .collect::<Vec<_>>();
    assert_eq!(named.len(), 1, "one line per retry binding: {startup:#?}");
    assert!(
        named[0].contains("not retried"),
        "the line says what the flag does: {named:?}"
    );

    let (status, answer) = call(
        &served,
        &contract,
        WRITE_NOTE,
        Some(&json!({"note_id": "n1", "text": "kept"})),
    );
    assert_eq!(status, 202, "{answer}");
    let failed = served.stderr.until("did not run notes.index.IndexNote");
    let (status, answer) = call(
        &served,
        &contract,
        WRITE_NOTE,
        Some(&json!({"note_id": "n2", "text": "also kept"})),
    );
    assert_eq!(status, 202, "the run goes on: {answer}");
    let attempts = |lines: &[String]| {
        lines
            .iter()
            .filter(|line| line.contains("did not run notes.index.IndexNote"))
            .count()
    };
    assert_eq!(attempts(&failed), 1, "{failed:#?}");
    let deadline = Instant::now() + ANSWER;
    while attempts(&served.stderr.lines()) < 2 {
        assert!(
            Instant::now() < deadline,
            "the second invocation was not reported within {ANSWER:?}: {:#?}",
            served.stderr.lines()
        );
        std::thread::sleep(POLL);
    }
    let published = events(&served)
        .iter()
        .map(|event| event["event"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        published,
        [
            json!("notes.pad.NoteWritten"),
            json!("notes.pad.NoteWritten")
        ],
        "the failed invocations published nothing"
    );
    assert_eq!(
        attempts(&served.stderr.lines()),
        2,
        "each failed invocation ran once and was not retried: {:#?}",
        served.stderr.lines()
    );
}

#[test]
fn sqlite_state_defaults_to_a_removed_temporary_directory() {
    let mut served = serve(PARTIAL, &["--no-state"]);
    let lines = served.stderr.until("state is kept in");
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
    assert!(
        directory.starts_with(served._state.path()),
        "the state is kept in the run's temporary directory: {line}"
    );
    let contract = Contract::load(PARTIAL);
    let (status, answer) = call(
        &served,
        &contract,
        WRITE_NOTE,
        Some(&json!({"note_id": "n1", "text": "kept"})),
    );
    assert_eq!(status, 202, "{answer}");
    assert!(
        directory.join(format!("{COMPONENT}.sqlite")).is_file(),
        "the component's state is in {}",
        directory.display()
    );

    rustix::process::kill_process(
        rustix::process::Pid::from_child(&served.child),
        rustix::process::Signal::TERM,
    )
    .expect("SIGTERM is sent");
    let deadline = Instant::now() + EXIT;
    let status = loop {
        if let Some(status) = served.child.try_wait().expect("waited on") {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "run did not exit within {EXIT:?}"
        );
        std::thread::sleep(POLL);
    };
    assert_eq!(status.code(), Some(143), "{status:?}");
    assert!(
        !directory.exists(),
        "the state directory is removed when the run ends: {}",
        directory.display()
    );
}
