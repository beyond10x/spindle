//! Adversary pass 1 on `story:external-cause-bound-to-http-call` (wave 18 U3): an external cause
//! the environment binds to an `[http.<name>]` call, attacked where the unit's own cases do not
//! look — the observed request log, a URL carrying credentials, an unbounded reply, and the
//! served `501` the commit promises but no case of `http_cause` exercises.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/http-cause";
const SERVED_SPEC: &str = "crates/spindle-cli/tests/fixtures/served-http-cause";
const ASK: &str = "probe.approval.Ask";
const CAPABILITY: &str = "external.probe.approval.Ask.approved";
const SECRET: &str = "SPINDLE_ADVERSARY_APPROVAL_TOKEN";
const TOKEN: &str = "adversary-token-value";
const STARTUP: Duration = Duration::from_secs(60);
const ANSWER: Duration = Duration::from_secs(30);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// One request the desk was sent: its request line and its headers, names lower-cased.
#[derive(Debug, Clone)]
struct Received {
    line: String,
    headers: Vec<(String, String)>,
}

/// The raw reply the desk writes to its `n`th request (from 0).
type Replies = Arc<dyn Fn(usize, &mut TcpStream) + Send + Sync>;

struct Desk {
    address: SocketAddr,
    received: Arc<Mutex<Vec<Received>>>,
}

impl Desk {
    fn start(replies: Replies) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("the desk binds");
        let address = listener.local_addr().expect("an address");
        let received = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&received);
        let count = Arc::new(AtomicUsize::new(0));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                let recorded = Arc::clone(&recorded);
                let replies = Arc::clone(&replies);
                let count = Arc::clone(&count);
                std::thread::spawn(move || {
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
                    recorded.lock().expect("the record").push(Received {
                        line: line.trim_end().to_owned(),
                        headers,
                    });
                    let n = count.fetch_add(1, Ordering::SeqCst);
                    let mut stream = reader.into_inner();
                    replies(n, &mut stream);
                });
            }
        });
        Self { address, received }
    }

    fn received(&self) -> Vec<Received> {
        self.received.lock().expect("the record").clone()
    }
}

fn json_reply(stream: &mut TcpStream, status: u16, body: &Value) {
    let body = body.to_string();
    let _ = write!(
        stream,
        "HTTP/1.1 {status} Answer\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

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

fn ask(id: &str) -> String {
    format!(r#"{{"command": "{ASK}", "input": {{"request_id": "{id}", "amount": 7}}}}"#) + "\n"
}

fn tempdir() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a temporary directory")
}

// ---- 1. a URL carrying userinfo -------------------------------------------------------------

/// The story: "No secret is written in a specification or an environment document; the document
/// names a reference." A URL's userinfo is a secret written in the document. Either the document
/// is refused, or at the very least the credentials are not put on the wire as the `Host` header
/// (RFC 9110 § 7.2: `Host` is `uri-host [ ":" port ]`, never userinfo).
#[test]
fn a_url_carrying_userinfo_never_puts_the_password_on_the_wire_as_host() {
    let desk = Desk::start(Arc::new(|_, stream: &mut TcpStream| {
        json_reply(stream, 200, &json!({"holds": true}));
    }));
    let directory = tempdir();
    let url = format!("http://desk-user:desk-password@{}/approvals", desk.address);
    let environment = environment(&directory, &url, 5_000, false);
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--environment", &environment])
        .env(SECRET, TOKEN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(ask("r1").as_bytes())
        .expect("the line is written");
    let output = child.wait_with_output().expect("b10x-spindle runs");
    for request in desk.received() {
        for (name, value) in &request.headers {
            assert!(
                !value.contains("desk-password"),
                "the URL's password is sent as the `{name}` header: {name}: {value} \
                 (request {request:?}; run {output:?})"
            );
        }
    }
}

// ---- 2. an unbounded reply ------------------------------------------------------------------

/// The peak resident size of `pid`, in KiB (`VmHWM`).
fn peak_kib(pid: u32) -> u64 {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).expect("the status");
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|rest| rest.trim().trim_end_matches("kB").trim().parse().ok())
        .expect("a VmHWM line")
}

/// The reply a call reads is `{"holds": true | false}`: a few bytes. The served side bounds
/// every body it reads (`spindle-ingress` `BODY_LIMIT`); the call reads whatever the desk sends
/// into memory (`calls.rs` `collect()`), so a desk that sends 256 MiB is held whole, twice.
#[test]
fn a_huge_reply_is_not_held_in_memory_whole() {
    const PADDING: usize = 256 * 1024 * 1024;
    let desk = Desk::start(Arc::new(|_, stream: &mut TcpStream| {
        let tail = br#"{"holds": true}"#;
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            PADDING + tail.len()
        );
        let chunk = vec![b' '; 1024 * 1024];
        for _ in 0..PADDING / chunk.len() {
            if stream.write_all(&chunk).is_err() {
                return;
            }
        }
        let _ = stream.write_all(tail);
    }));
    let directory = tempdir();
    let environment = environment(
        &directory,
        &format!("http://{}/approvals", desk.address),
        30_000,
        false,
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--environment", &environment])
        .env(SECRET, TOKEN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("b10x-spindle starts");
    let mut stdin = child.stdin.take().expect("stdin");
    stdin.write_all(ask("r1").as_bytes()).expect("the line");
    stdin.flush().expect("flushed");
    let mut line = String::new();
    BufReader::new(child.stdout.take().expect("stdout"))
        .read_line(&mut line)
        .expect("a result line");
    let peak = peak_kib(child.id());
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        peak < 128 * 1024,
        "a {PADDING}-byte reply raised the run's peak resident size to {} MiB (result {line})",
        peak / 1024
    );
}

// ---- served ---------------------------------------------------------------------------------

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
    let stdout = child.stdout.take().expect("stdout");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = sender.send(line);
    });
    let line = receiver.recv_timeout(STARTUP).expect("a first line");
    let address = serde_json::from_str::<Value>(&line)
        .ok()
        .and_then(|ready| ready["listening"].as_str()?.parse().ok())
        .unwrap_or_else(|| panic!("no address in {line:?}"));
    Served { child, address }
}

fn exchange(address: SocketAddr, method: &str, path: &str, body: Option<&Value>) -> (u16, Value) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream.set_read_timeout(Some(ANSWER)).expect("a timeout");
    let body = body.map(Value::to_string).unwrap_or_default();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .expect("the request is written");
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).expect("the answer");
    let answer = String::from_utf8(answer).expect("UTF-8");
    let (head, body) = answer.split_once("\r\n\r\n").expect("a head");
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .expect("a status");
    let body = serde_json::from_str(body).unwrap_or(Value::Null);
    (status, body)
}

fn route(operation: &str) -> (String, String) {
    let ir = spindle_spec::load(&root().join(SERVED_SPEC))
        .expect("the fixture compiles")
        .ir;
    let surface = spindle_ingress::Surface::new(&ir, &["probe".to_owned()], None)
        .expect("the surface is routed");
    let route = surface
        .routes()
        .iter()
        .find(|route| route.operation.name() == operation)
        .unwrap_or_else(|| panic!("a route for {operation}"));
    (route.method.as_str().to_uppercase(), route.path.clone())
}

fn call_ask(served: &Served, id: &str) -> (u16, Value) {
    let (method, path) = route(ASK);
    exchange(
        served.address,
        &method,
        &path,
        Some(&json!({"request_id": id, "amount": 7})),
    )
}

// ---- 3. the observed request log ------------------------------------------------------------

/// `story:external-answers-observed`: every external cause a served command asks about is listed
/// in its request's `externals` with what answered it. A cause answered by its HTTP call is
/// answered outside the observed decider (`calls.over` wraps `decisions::deciding`), so the
/// request log of a served `--observe` run says no external cause was asked.
#[test]
fn a_cause_answered_by_its_http_call_is_listed_in_the_observed_request() {
    let desk = Desk::start(Arc::new(|_, stream: &mut TcpStream| {
        json_reply(stream, 200, &json!({"holds": true}));
    }));
    let directory = tempdir();
    let environment = environment(
        &directory,
        &format!("http://{}/approvals", desk.address),
        5_000,
        true,
    );
    let served = serve(&environment, &["--observe"]);
    let (status, answer) = call_ask(&served, "r1");
    assert!((200..300).contains(&status), "{status} {answer}");
    assert_eq!(answer["outcome"], "approved", "{answer}");
    assert_eq!(desk.received().len(), 1);
    let (status, log) = exchange(served.address, "GET", "/__spindle/requests", None);
    assert_eq!(status, 200, "{log}");
    let record = &log["requests"][0];
    assert_eq!(record["operation"], ASK, "{record}");
    let externals = record["externals"].as_array().expect("an externals list");
    assert_eq!(
        externals.len(),
        1,
        "the cause the call answered is listed: {record}"
    );
    assert_eq!(externals[0]["command"], ASK, "{record}");
    assert_eq!(externals[0]["outcome"], "approved", "{record}");
    assert_eq!(externals[0]["holds"], true, "{record}");
}

// ---- 4. served failure: 501 and nothing written ---------------------------------------------

/// The commit: "another status ... fails the command as `undecided` and writes nothing (`501`
/// when served)". `http_cause` serves only the answering case. The desk fails the first call and
/// approves the second; the same request id must then be created, which it cannot be if the first
/// attempt wrote it.
#[test]
fn a_served_call_that_fails_answers_501_and_writes_nothing() {
    let desk = Desk::start(Arc::new(|n, stream: &mut TcpStream| {
        if n == 0 {
            json_reply(stream, 503, &json!({"holds": true}));
        } else {
            json_reply(stream, 200, &json!({"holds": true}));
        }
    }));
    let directory = tempdir();
    let environment = environment(
        &directory,
        &format!("http://{}/approvals", desk.address),
        5_000,
        true,
    );
    let served = serve(&environment, &[]);
    let (status, answer) = call_ask(&served, "r1");
    assert_eq!(status, 501, "{answer}");
    assert!(
        !answer.to_string().contains(TOKEN),
        "the refusal never carries the secret: {answer}"
    );
    let (status, answer) = call_ask(&served, "r1");
    assert!((200..300).contains(&status), "{status} {answer}");
    assert_eq!(answer["outcome"], "approved", "{answer}");
    let received = desk.received();
    assert_eq!(received.len(), 2);
    assert!(
        received
            .iter()
            .all(|request| request.line.starts_with("POST /approvals ")),
        "{received:?}"
    );
}
