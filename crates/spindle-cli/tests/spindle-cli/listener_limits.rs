//! `run --max-connections`, `--max-head-bytes` and `--request-timeout` bound the served listener
//! (`story:listener-limits`). One run served with `--max-connections 1 --max-head-bytes 8192
//! --request-timeout 1` shows each flag taking effect, each check one the defaults (512
//! connections, a head of 16 KiB, 30 s per request) would not pass.
//!
//! The fixture `tests/fixtures/served-held-cause` holds its command on an external cause the
//! environment binds to an HTTP call: the desk this test runs answers a request whose id starts
//! with [`HELD`] only once the test released it, or after [`HOLD`].

use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/served-held-cause";
const ASK: &str = "probe.approval.Ask";
const CAPABILITY: &str = "external.probe.approval.Ask.approved";
const SECRET: &str = "SPINDLE_TEST_LIMITS_TOKEN";
/// The prefix of a request id the desk holds until the test releases it.
const HELD: &str = "held-";
/// How long the desk holds a [`HELD`] request the test does not release.
const HOLD: Duration = Duration::from_secs(5);
/// How long `run` may take to admit, open its state and print where it listens.
const STARTUP: Duration = Duration::from_secs(60);
/// How long a check waits for an answer that is due.
const DUE: Duration = Duration::from_secs(2);
/// How long a check waits to see that no answer comes.
const WITHHELD: Duration = Duration::from_millis(500);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A local approval desk on 127.0.0.1 that approves every request, a [`HELD`] one once the test
/// released the desk or after [`HOLD`].
struct Desk {
    address: SocketAddr,
    released: Arc<(Mutex<bool>, Condvar)>,
}

impl Desk {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("the desk binds");
        let address = listener.local_addr().expect("the desk has an address");
        let released = Arc::new((Mutex::new(false), Condvar::new()));
        let releasing = Arc::clone(&released);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                let releasing = Arc::clone(&releasing);
                std::thread::spawn(move || approve(stream, &releasing));
            }
        });
        Self { address, released }
    }

    fn release(&self) {
        let (released, signal) = &*self.released;
        *released.lock().expect("the desk's release") = true;
        signal.notify_all();
    }
}

/// Reads one request from `stream` and approves it, a [`HELD`] one once `released` or after
/// [`HOLD`].
fn approve(stream: TcpStream, released: &(Mutex<bool>, Condvar)) {
    let _ = stream.set_read_timeout(Some(HOLD));
    let mut reader = BufReader::new(stream);
    let mut length = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() || line.is_empty() {
            return;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0; length];
    if reader.read_exact(&mut body).is_err() {
        return;
    }
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    if body["input"]["request_id"]
        .as_str()
        .is_some_and(|id| id.starts_with(HELD))
    {
        let (flag, signal) = released;
        let flag = flag.lock().expect("the desk's release");
        let _ = signal
            .wait_timeout_while(flag, HOLD, |released| !*released)
            .expect("the desk's release");
    }
    let answer = json!({"holds": true}).to_string();
    let mut stream = reader.into_inner();
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}",
        answer.len()
    );
}

/// The environment document binding the cause to the desk, its state in memory.
fn environment(directory: &Path, desk: &Desk) -> PathBuf {
    let text = format!(
        "format = \"spindle-environment/1\"\nname = \"limits\"\nos = \"linux\"\narch = \"x86_64\"\n\n\
         [provides]\n\"store.durable\" = [\"memory\"]\n\"store.ephemeral\" = [\"memory\"]\n\
         \"ingress.network\" = [\"http-loopback\"]\n\"identity.mint\" = [\"uuid-v4\"]\n\
         clock = [\"system\"]\ntimer = [\"tokio\"]\nchannel = [\"in-process\"]\n\
         projection = [\"in-process\"]\n\"{CAPABILITY}\" = [\"approval-desk\"]\n\
         \"external.*\" = [\"operator-prompt\"]\n\n\
         [http.approval-desk]\nmethod = \"POST\"\nurl = \"http://{}/approvals\"\n\
         timeout_ms = 30000\nsecret_env = \"{SECRET}\"\n",
        desk.address
    );
    let path = directory.join("limits.toml");
    std::fs::write(&path, text).expect("the environment is written");
    path
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

fn serve(environment: &Path, flags: &[&str]) -> Served {
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--environment"])
        .arg(environment)
        .args(flags)
        .env(SECRET, "limits-token")
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
    let Ok(line) = receiver.recv_timeout(STARTUP) else {
        let _ = child.kill();
        let _ = child.wait();
        panic!("run printed no line within {STARTUP:?}");
    };
    let address = serde_json::from_str::<Value>(&line)
        .ok()
        .and_then(|ready| ready["listening"].as_str()?.parse().ok());
    let Some(address) = address else {
        let _ = child.kill();
        let _ = child.wait();
        panic!("run's first line {line:?} names no address");
    };
    Served { child, address }
}

/// The method and path ESS's `OpenAPI` projection publishes for `id` on the fixture.
fn route(id: &str) -> (String, String) {
    let loaded = spindle_spec::load(&root().join(SPEC)).expect("the fixture compiles");
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

fn connect(address: SocketAddr) -> TcpStream {
    TcpStream::connect_timeout(&address, DUE).expect("the connection is made")
}

/// The status of the one answer read off `stream` within `within`, by its `Content-Length`;
/// `None` where none arrived, the connection closing or the time passing first.
fn answer(stream: &mut TcpStream, within: Duration) -> Option<u16> {
    let until = Instant::now() + within;
    let mut read = Vec::new();
    let mut chunk = [0u8; 4096];
    let head = loop {
        if let Some(end) = read.windows(4).position(|window| window == b"\r\n\r\n") {
            break end;
        }
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return None;
        }
        stream.set_read_timeout(Some(left)).expect("a read timeout");
        match stream.read(&mut chunk) {
            Ok(0) => return None,
            Ok(count) => read.extend_from_slice(&chunk[..count]),
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                return None;
            }
            Err(_) => return None,
        }
    };
    let text = String::from_utf8_lossy(&read[..head]).into_owned();
    let length: usize = text
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse().ok())
        .unwrap_or(0);
    while read.len() < head + 4 + length {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(count) => read.extend_from_slice(&chunk[..count]),
        }
    }
    text.split(' ').nth(1)?.parse().ok()
}

/// A keep-alive `GET /api/model`, `extra` among its headers.
fn model(address: SocketAddr, extra: &str) -> String {
    format!("GET /api/model HTTP/1.1\r\nHost: {address}\r\n{extra}\r\n")
}

/// The first `[default: …]` `help` gives after `flag`.
fn default_after<'a>(help: &'a str, flag: &str) -> &'a str {
    let at = help
        .find(flag)
        .unwrap_or_else(|| panic!("run --help names {flag}: {help}"));
    let after = &help[at..];
    let start = after
        .find("[default: ")
        .unwrap_or_else(|| panic!("run --help gives {flag} a default: {help}"));
    let end = after[start..].find(']').expect("the default closes");
    &after[start..=start + end]
}

#[test]
fn run_flags_bound_the_listener() {
    let help = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .args(["run", "--help"])
        .output()
        .expect("run --help runs");
    let help = String::from_utf8_lossy(&help.stdout).into_owned();
    for (flag, default) in [
        ("--max-connections", "[default: 512]"),
        ("--max-head-bytes", "[default: 16384]"),
        ("--request-timeout", "[default: 30]"),
    ] {
        assert_eq!(default_after(&help, flag), default, "{flag}");
    }

    let desk = Desk::start();
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path(), &desk);
    let served = serve(
        &environment,
        &[
            "--max-connections",
            "1",
            "--max-head-bytes",
            "8192",
            "--request-timeout",
            "1",
        ],
    );
    let address = served.address;
    // Each check is made whatever an earlier one found, and every one that fails is named.
    let mut failed = Vec::new();

    // A head of 12 KiB: over the flag's 8192, under the default 16 KiB.
    let mut stream = connect(address);
    let pad = format!("X-Pad: {}\r\n", "a".repeat(12 * 1024));
    stream
        .write_all(model(address, &pad).as_bytes())
        .expect("the request is written");
    let head = answer(&mut stream, DUE);
    if head != Some(431) {
        failed.push(format!(
            "a head of 12 KiB answers 431 under --max-head-bytes 8192: {head:?}"
        ));
    }
    drop(stream);

    // One connection open holds the only place.
    let mut first = connect(address);
    first
        .write_all(model(address, "").as_bytes())
        .expect("the request is written");
    assert_eq!(answer(&mut first, DUE), Some(200), "the first connection");
    let mut second = connect(address);
    second
        .write_all(model(address, "").as_bytes())
        .expect("the request is written");
    let early = answer(&mut second, WITHHELD);
    if early.is_some() {
        failed.push(format!(
            "a second connection waits while the first stays open under --max-connections 1: \
             answered {early:?}"
        ));
    }
    drop(first);
    if early.is_none() {
        assert_eq!(
            answer(&mut second, DUE),
            Some(200),
            "the second connection is answered once the first closed"
        );
    }
    drop(second);

    // A command held on the desk answers 504 at its request time.
    let (method, path) = route(ASK);
    let body = json!({"request_id": "held-1", "amount": 7}).to_string();
    let mut held = connect(address);
    held.write_all(
        format!(
            "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\
             Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .as_bytes(),
    )
    .expect("the request is written");
    let answered = answer(&mut held, DUE);
    desk.release();
    if answered != Some(504) {
        failed.push(format!(
            "a command held past --request-timeout 1 answers 504 within 2 s: {answered:?}"
        ));
    }
    assert!(failed.is_empty(), "{failed:#?}");
}
