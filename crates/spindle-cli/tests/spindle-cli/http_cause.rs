//! `run` with an external cause the environment binds to an HTTP call: the `[http.<name>]` table
//! of a `spindle-environment/1` document names the call, and the implementation `provides` binds
//! the cause's capability to is that name. The command that asks the cause sends one request to
//! the call, and the reply's `holds` answers it (`story:external-cause-bound-to-http-call`).
//!
//! The fixtures `tests/fixtures/http-cause` (in process, read from stdin) and
//! `tests/fixtures/served-http-cause` (reached over the network) declare `probe.approval.Ask`,
//! whose external outcome `approved` creates a request and publishes `probe.approval.Approved`, and
//! whose default branch `not-approved` refuses. The call is answered by a desk on 127.0.0.1 that
//! this test runs and that records every request it is sent.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/http-cause";
const SERVED_SPEC: &str = "crates/spindle-cli/tests/fixtures/served-http-cause";
const ASK: &str = "probe.approval.Ask";
const CAPABILITY: &str = "external.probe.approval.Ask.approved";
/// The environment variable the environment document names as the call's secret.
const SECRET: &str = "SPINDLE_TEST_APPROVAL_TOKEN";
/// Its value, which no document in this test holds.
const TOKEN: &str = "approval-token-for-tests";

/// How long `run` may take to admit, open its state and print where it listens.
const STARTUP: Duration = Duration::from_secs(60);
/// How long one request may take to be answered.
const ANSWER: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// One request the desk was sent.
#[derive(Debug, Clone)]
struct Received {
    /// `POST /approvals`.
    line: String,
    /// Each header, its name lower-cased.
    headers: Vec<(String, String)>,
    body: Value,
}

impl Received {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header, _)| header == name)
            .map(|(_, value)| value.as_str())
    }
}

/// How the desk answers.
#[derive(Clone)]
enum Reply {
    /// This status and JSON body.
    Json(u16, Value),
    /// Nothing, until the caller gives up.
    Silent,
}

/// A local HTTP server on 127.0.0.1 standing in for the approval desk.
struct Desk {
    address: SocketAddr,
    received: Arc<Mutex<Vec<Received>>>,
}

impl Desk {
    fn start(reply: Reply) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("the desk binds");
        let address = listener.local_addr().expect("the desk has an address");
        let received = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&received);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                let recorded = Arc::clone(&recorded);
                let reply = reply.clone();
                std::thread::spawn(move || answer(stream, &recorded, &reply));
            }
        });
        Self { address, received }
    }

    fn url(&self) -> String {
        format!("http://{}/approvals", self.address)
    }

    fn received(&self) -> Vec<Received> {
        self.received.lock().expect("the desk's record").clone()
    }
}

/// Reads one request from `stream`, records it, and answers it as `reply` says.
fn answer(stream: TcpStream, recorded: &Mutex<Vec<Received>>, reply: &Reply) {
    let _ = stream.set_read_timeout(Some(ANSWER));
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || line.is_empty() {
        return;
    }
    let mut headers = Vec::new();
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).is_err() {
            return;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            headers.push((name.trim().to_lowercase(), value.trim().to_owned()));
        }
    }
    let length = headers
        .iter()
        .find(|(name, _)| name == "content-length")
        .and_then(|(_, value)| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0; length];
    if reader.read_exact(&mut body).is_err() {
        return;
    }
    let body = serde_json::from_slice(&body).unwrap_or(Value::Null);
    recorded.lock().expect("the desk's record").push(Received {
        line: line.trim_end().to_owned(),
        headers,
        body,
    });
    let mut stream = reader.into_inner();
    match reply {
        Reply::Json(status, body) => {
            let body = body.to_string();
            let _ = write!(
                stream,
                "HTTP/1.1 {status} Answer\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
        Reply::Silent => std::thread::sleep(ANSWER),
    }
}

/// Writes an environment document binding the cause to `url` in `directory`; `served` adds what
/// a run serving `ingress.network` needs.
fn environment(directory: &tempfile::TempDir, url: &str, timeout_ms: u64, served: bool) -> String {
    let ingress = if served {
        "\"ingress.network\" = [\"http-loopback\"]\n\"identity.mint\" = [\"uuid-v4\"]\nclock = [\"system\"]\ntimer = [\"tokio\"]\nchannel = [\"in-process\"]\nprojection = [\"in-process\"]\n"
    } else {
        ""
    };
    let text = format!(
        "format = \"spindle-environment/1\"\nname = \"approvals\"\nos = \"linux\"\narch = \"x86_64\"\n\n\
         [provides]\n\"store.durable\" = [\"memory\"]\n\"store.ephemeral\" = [\"memory\"]\n{ingress}\
         \"{CAPABILITY}\" = [\"approval-desk\"]\n\"external.*\" = [\"operator-prompt\"]\n\n\
         [http.approval-desk]\nmethod = \"POST\"\nurl = \"{url}\"\ntimeout_ms = {timeout_ms}\n\
         secret_env = \"{SECRET}\"\n"
    );
    let path = directory.path().join("approvals.toml");
    std::fs::write(&path, text).expect("the environment is written");
    path.to_str().expect("a UTF-8 path").to_owned()
}

/// `run` on the in-process fixture reading `lines` from stdin, the secret set to [`TOKEN`] unless
/// `secret` is `false`.
fn run(environment: &str, lines: &str, secret: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"));
    command
        .current_dir(root())
        .args(["run", SPEC, "--environment", environment])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if secret {
        command.env(SECRET, TOKEN);
    } else {
        command.env_remove(SECRET);
    }
    let mut child = command.spawn().expect("b10x-spindle starts");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(lines.as_bytes())
        .expect("the commands are written");
    child.wait_with_output().expect("b10x-spindle runs")
}

fn results(output: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("each result is one JSON line"))
        .collect()
}

fn ask(id: &str) -> String {
    format!(r#"{{"command": "{ASK}", "input": {{"request_id": "{id}", "amount": 7}}}}"#) + "\n"
}

#[test]
fn a_cause_bound_to_an_http_call_sends_one_request_and_commits_what_the_reply_selects() {
    let desk = Desk::start(Reply::Json(200, json!({"holds": true})));
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let environment = environment(&directory, &desk.url(), 5_000, false);
    let document = std::fs::read_to_string(&environment).expect("the environment is read");
    assert!(
        !document.contains(TOKEN),
        "the document names the secret, never holds it"
    );

    let output = run(&environment, &ask("r1"), true);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(results[0]["outcome"], "approved", "{results:?}");
    assert_eq!(
        results[0]["events"][0]["event"], "probe.approval.Approved",
        "the outcome the reply selects is committed: {results:?}"
    );

    let received = desk.received();
    assert_eq!(received.len(), 1, "exactly one request: {received:?}");
    let request = &received[0];
    assert!(request.line.starts_with("POST /approvals "), "{request:?}");
    assert_eq!(
        request.header("host"),
        Some(desk.address.to_string().as_str()),
        "Host is the URL's host and port: {request:?}"
    );
    assert_eq!(
        request.header("authorization"),
        Some(format!("Bearer {TOKEN}").as_str()),
        "the secret is read from the variable the document names: {request:?}"
    );
    assert_eq!(
        request.body,
        json!({
            "format": "spindle-external/1",
            "capability": CAPABILITY,
            "command": ASK,
            "outcome": "approved",
            "cause": "the approval desk approves the request",
            "input": {"request_id": "r1", "amount": 7},
        }),
        "{request:?}"
    );
}

#[test]
fn a_reply_that_the_cause_does_not_hold_selects_the_next_outcome() {
    let desk = Desk::start(Reply::Json(200, json!({"holds": false})));
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let environment = environment(&directory, &desk.url(), 5_000, false);

    let output = run(&environment, &(ask("r1") + &ask("r2")), true);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 2, "{results:?}");
    for result in &results {
        assert_eq!(result["outcome"], "not-approved", "{results:?}");
        assert_eq!(
            result["error"]["error"], "probe.approval.NotApproved",
            "{results:?}"
        );
    }
    assert_eq!(desk.received().len(), 2, "one request per command run");
}

#[test]
fn a_call_that_times_out_fails_the_command_and_commits_nothing() {
    let desk = Desk::start(Reply::Silent);
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let environment = environment(&directory, &desk.url(), 200, false);

    let output = run(&environment, &ask("r1"), true);
    assert_eq!(output.status.code(), Some(0), "the run goes on: {output:?}");
    let results = results(&output);
    assert_eq!(results.len(), 1, "{results:?}");
    assert_eq!(results[0]["failure"]["kind"], "undecided", "{results:?}");
    let message = results[0]["failure"]["message"]
        .as_str()
        .expect("a failure message");
    assert!(
        message.contains("approval-desk") && message.contains("200 ms"),
        "names the call and its timeout: {message}"
    );
    assert!(
        !message.contains(TOKEN),
        "never prints the secret: {message}"
    );
    assert!(results[0].get("events").is_none(), "{results:?}");
}

#[test]
fn a_failed_reply_fails_the_command() {
    let desk = Desk::start(Reply::Json(503, json!({"holds": true})));
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let environment = environment(&directory, &desk.url(), 5_000, false);

    let output = run(&environment, &ask("r1"), true);
    assert_eq!(output.status.code(), Some(0), "the run goes on: {output:?}");
    let results = results(&output);
    assert_eq!(results[0]["failure"]["kind"], "undecided", "{results:?}");
    let message = results[0]["failure"]["message"]
        .as_str()
        .expect("a failure message");
    assert!(message.contains("503"), "names the status: {message}");
    assert_eq!(desk.received().len(), 1);
}

#[test]
fn an_unset_secret_variable_stops_the_run_before_any_request() {
    let desk = Desk::start(Reply::Json(200, json!({"holds": true})));
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let environment = environment(&directory, &desk.url(), 5_000, false);

    let output = run(&environment, &ask("r1"), false);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(SECRET) && stderr.contains("approval-desk"),
        "names the variable and the call: {stderr}"
    );
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(desk.received().is_empty());
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

fn serve(environment: &str, extra: &[&str]) -> Served {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SERVED_SPEC, "--environment", environment])
        .args(extra)
        .env(SECRET, TOKEN)
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
    let address = serde_json::from_str::<Value>(&line)
        .ok()
        .and_then(|ready| ready["listening"].as_str()?.parse().ok());
    let Some(address) = address else {
        let _ = child.kill();
        let output = child.wait_with_output();
        panic!("run's first line {line:?} names no address: {output:?}");
    };
    Served { child, address }
}

/// The method and path ESS's `OpenAPI` projection publishes for `id` on the served fixture.
fn route(id: &str) -> (String, String) {
    let loaded = spindle_spec::load(&root().join(SERVED_SPEC)).expect("the fixture compiles");
    let component = loaded
        .ir
        .components()
        .values()
        .next()
        .expect("the fixture declares a component");
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

/// Calls the operation `id` with `body`, on its own connection: the status and the JSON body.
fn call(served: &Served, id: &str, body: &Value) -> (u16, Value) {
    let (method, path) = route(id);
    let mut stream =
        TcpStream::connect_timeout(&served.address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    let body = body.to_string();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        served.address,
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

#[test]
fn a_served_command_asks_the_bound_http_call_once_without_decisions() {
    let desk = Desk::start(Reply::Json(200, json!({"holds": true})));
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let environment = environment(&directory, &desk.url(), 5_000, true);
    let served = serve(&environment, &[]);

    let (status, answer) = call(&served, ASK, &json!({"request_id": "r1", "amount": 7}));
    assert!((200..300).contains(&status), "{status} {answer}");
    assert_eq!(answer["outcome"], "approved", "{answer}");
    assert_eq!(
        answer["published"],
        json!([{"event": "probe.approval.Approved", "payload": {"request_id": "r1"}}]),
        "{answer}"
    );
    let received = desk.received();
    assert_eq!(received.len(), 1, "exactly one request: {received:?}");
    assert_eq!(
        received[0].body["input"],
        json!({"request_id": "r1", "amount": 7})
    );
}

/// `GET <path>` on the served run: the status and the body as text.
fn get(served: &Served, path: &str) -> (u16, String) {
    let mut stream =
        TcpStream::connect_timeout(&served.address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        served.address
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

/// Under `--observe`, a cause its call answered is listed with the source `call` and counted under
/// it; a call that fails is listed `unanswered`, as any cause a `501` leaves unanswered is.
#[test]
fn an_observed_cause_is_listed_as_answered_by_its_call_or_unanswered_when_the_call_fails() {
    for (reply, source) in [
        (Reply::Json(200, json!({"holds": true})), "call"),
        (Reply::Json(503, json!({"holds": true})), "unanswered"),
    ] {
        let desk = Desk::start(reply);
        let directory =
            tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
        let environment = environment(&directory, &desk.url(), 5_000, true);
        let served = serve(&environment, &["--observe"]);

        let (answered, answer) = call(&served, ASK, &json!({"request_id": "r1", "amount": 7}));
        if source == "call" {
            assert!((200..300).contains(&answered), "{answered} {answer}");
        } else {
            assert_eq!(answered, 501, "{answer}");
        }
        let (_, log) = get(&served, "/__spindle/requests");
        let log: Value = serde_json::from_str(&log).expect("the log is JSON");
        let mut external = json!({"command": ASK, "outcome": "approved", "source": source});
        if source == "call" {
            external["holds"] = json!(true);
        }
        assert_eq!(log["requests"][0]["externals"], json!([external]), "{log}");
        let (_, metrics) = get(&served, "/__spindle/metrics");
        let counted = metrics
            .lines()
            .filter(|line| line.starts_with("spindle_external_causes_total{"))
            .collect::<Vec<_>>();
        assert_eq!(counted.len(), 1, "{metrics}");
        assert!(
            counted[0].contains(&format!("source=\"{source}\"")) && counted[0].ends_with(" 1"),
            "{metrics}"
        );
    }
}
