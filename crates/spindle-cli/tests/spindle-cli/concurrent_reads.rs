//! Reads are answered while a command runs (`story:concurrent-reads`).
//!
//! A served `run` answers a view read, `GET /api/model` and `GET /__spindle/requests` while a
//! command holds the thread that runs commands, from the state as of the last committed write, on
//! the memory and the SQLite store. The command is held by its external cause: the environment
//! binds it to an HTTP call, and the desk this test runs answers a request whose id starts with
//! [`HELD`] only after [`HOLD`]. Each read is timed [`ROUNDS`] times while the command is held:
//! the median within [`BUDGET`], so that one read the scheduler of a loaded machine held back
//! does not decide it, and every one within [`CEILING`], well inside the hold, before the held
//! command is answered.
//!
//! The fixture `tests/fixtures/served-held-cause` is `served-http-cause` with a view,
//! `probe.approval.Requests`, listing every approved request.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/served-held-cause";
const ASK: &str = "probe.approval.Ask";
const VIEW: &str = "probe.approval.Requests";
const CAPABILITY: &str = "external.probe.approval.Ask.approved";
const SECRET: &str = "SPINDLE_TEST_HELD_TOKEN";
/// The prefix of a request id the desk holds for [`HOLD`] before it answers.
const HELD: &str = "held-";
/// How long the desk holds a [`HELD`] request, and so the command that asked it.
const HOLD: Duration = Duration::from_secs(2);
/// How many times each read is timed while the command is held.
const ROUNDS: usize = 5;
/// The median a read may take while the command is held.
const BUDGET: Duration = Duration::from_millis(100);
/// How long any one read may take while the command is held: far below [`HOLD`].
const CEILING: Duration = Duration::from_millis(750);
/// How long `run` may take to admit, open its state and print where it listens.
const STARTUP: Duration = Duration::from_secs(60);
/// How long one request may take to be answered.
const ANSWER: Duration = Duration::from_secs(20);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A local approval desk on 127.0.0.1 that approves every request, a [`HELD`] one after [`HOLD`],
/// and records the id of each request as it arrives.
struct Desk {
    address: SocketAddr,
    arrived: Arc<Mutex<Vec<String>>>,
}

impl Desk {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("the desk binds");
        let address = listener.local_addr().expect("the desk has an address");
        let arrived = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&arrived);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                let recorded = Arc::clone(&recorded);
                std::thread::spawn(move || approve(stream, &recorded));
            }
        });
        Self { address, arrived }
    }

    fn url(&self) -> String {
        format!("http://{}/approvals", self.address)
    }

    /// Waits until a request for `id` arrived: the command that asks it is then held.
    fn wait_for(&self, id: &str) {
        let started = Instant::now();
        while started.elapsed() < ANSWER {
            if self
                .arrived
                .lock()
                .expect("the desk's record")
                .iter()
                .any(|arrived| arrived == id)
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("no request for {id} reached the desk within {ANSWER:?}");
    }
}

/// Reads one request from `stream`, records its id, and approves it, a [`HELD`] one after
/// [`HOLD`].
fn approve(stream: TcpStream, recorded: &Mutex<Vec<String>>) {
    let _ = stream.set_read_timeout(Some(ANSWER));
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
    let id = body["input"]["request_id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    recorded.lock().expect("the desk's record").push(id.clone());
    if id.starts_with(HELD) {
        std::thread::sleep(HOLD);
    }
    let answer = json!({"holds": true}).to_string();
    let mut stream = reader.into_inner();
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}",
        answer.len()
    );
}

/// The environment document binding the cause to the desk, its state kept by `store`.
fn environment(directory: &Path, desk: &Desk, store: &str) -> PathBuf {
    let text = format!(
        "format = \"spindle-environment/1\"\nname = \"held\"\nos = \"linux\"\narch = \"x86_64\"\n\n\
         [provides]\n\"store.durable\" = [\"{store}\"]\n\"store.ephemeral\" = [\"memory\"]\n\
         \"ingress.network\" = [\"http-loopback\"]\n\"identity.mint\" = [\"uuid-v4\"]\n\
         clock = [\"system\"]\ntimer = [\"tokio\"]\nchannel = [\"in-process\"]\n\
         projection = [\"in-process\"]\n\"{CAPABILITY}\" = [\"approval-desk\"]\n\
         \"external.*\" = [\"operator-prompt\"]\n\n\
         [http.approval-desk]\nmethod = \"POST\"\nurl = \"{}\"\ntimeout_ms = 30000\n\
         secret_env = \"{SECRET}\"\n",
        desk.url()
    );
    let path = directory.join(format!("{store}.toml"));
    std::fs::write(&path, text).expect("the environment is written");
    path
}

/// A running `b10x-spindle run --observe` serving HTTP; stopped when dropped.
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

fn serve(environment: &Path, state: &Path, store: &str) -> Served {
    let mut command = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"));
    command
        .current_dir(root())
        .args(["run", SPEC, "--observe", "--environment"])
        .arg(environment)
        .env(SECRET, "held-token")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    if store == "sqlite" {
        command.arg("--state").arg(state);
    }
    let mut child = command.spawn().expect("b10x-spindle starts");
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

/// Sends `method path` with `body` on its own connection: the status and the body as text.
fn send(address: SocketAddr, method: &str, path: &str, body: &str) -> (u16, String) {
    let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
    stream
        .set_read_timeout(Some(ANSWER))
        .expect("a read timeout");
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
    (status, body.to_owned())
}

/// Asks the desk to approve `id` through the served command: the status and the envelope.
fn ask(address: SocketAddr, id: &str) -> (u16, Value) {
    let (method, path) = route(ASK);
    let (status, body) = send(
        address,
        &method,
        &path,
        &json!({"request_id": id, "amount": 7}).to_string(),
    );
    let body = serde_json::from_str(&body).unwrap_or_else(|error| panic!("{error}: {body:?}"));
    (status, body)
}

/// The ids the view lists, as `run` answers it.
fn listed(body: &str) -> Vec<String> {
    let body: Value = serde_json::from_str(body).unwrap_or_else(|error| panic!("{error}: {body}"));
    body["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("rows: {body}"))
        .iter()
        .map(|row| row["request_id"].as_str().unwrap_or_default().to_owned())
        .collect()
}

/// One read sent while a command was held: what it was, how long it took, its status and body.
struct Timed {
    what: &'static str,
    took: Duration,
    status: u16,
    body: String,
}

#[test]
fn reads_are_answered_from_the_last_committed_write_while_a_command_is_held() {
    let (_, view) = route(VIEW);
    let reads: [(&'static str, String); 3] = [
        ("a view read", view.clone()),
        ("GET /api/model", "/api/model".to_owned()),
        ("GET /__spindle/requests", "/__spindle/requests".to_owned()),
    ];
    for store in ["memory", "sqlite"] {
        let desk = Desk::start();
        let directory =
            tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a temporary directory");
        let environment = environment(directory.path(), &desk, store);
        let served = serve(&environment, directory.path(), store);
        let address = served.address;

        let (status, answer) = ask(address, "first");
        assert!((200..300).contains(&status), "{store}: {status} {answer}");
        let mut committed = vec!["first".to_owned()];

        let mut timed: Vec<Timed> = Vec::new();
        let mut early = Vec::new();
        let mut seen: Vec<Vec<String>> = Vec::new();
        for round in 0..ROUNDS {
            let id = format!("{HELD}{round}");
            let answered = Arc::new(AtomicBool::new(false));
            let held = {
                let answered = Arc::clone(&answered);
                let id = id.clone();
                std::thread::spawn(move || {
                    let asked = ask(address, &id);
                    answered.store(true, Ordering::SeqCst);
                    asked
                })
            };
            desk.wait_for(&id);
            let sent = reads
                .iter()
                .map(|(what, path)| {
                    let (what, path) = (*what, path.clone());
                    std::thread::spawn(move || {
                        let started = Instant::now();
                        let (status, body) = send(address, "GET", &path, "");
                        Timed {
                            what,
                            took: started.elapsed(),
                            status,
                            body,
                        }
                    })
                })
                .collect::<Vec<_>>();
            let round_reads = sent
                .into_iter()
                .map(|read| read.join().expect("the read's thread ends"))
                .collect::<Vec<_>>();
            if answered.load(Ordering::SeqCst) {
                early.push(round);
            }
            let (status, answer) = held.join().expect("the held command's thread ends");
            assert!((200..300).contains(&status), "{store}: {status} {answer}");
            // What the reads must have seen: every command answered before this round's.
            seen.extend(round_reads.iter().map(|_| committed.clone()));
            committed.push(id);
            timed.extend(round_reads);
        }

        let medians = reads
            .iter()
            .map(|(what, _)| {
                let mut took = timed
                    .iter()
                    .filter(|read| read.what == *what)
                    .map(|read| read.took)
                    .collect::<Vec<_>>();
                took.sort();
                (*what, took[took.len() / 2], took)
            })
            .collect::<Vec<_>>();
        for (what, median, took) in &medians {
            eprintln!("{store}: {what} while a command was held: median {median:?} of {took:?}");
        }
        for (what, median, took) in medians {
            assert!(
                median < BUDGET,
                "{store}: {what} took {median:?} at the median while a command was held, over \
                 {BUDGET:?}: {took:?}"
            );
            assert!(
                took.iter().all(|took| *took < CEILING),
                "{store}: {what} took over {CEILING:?} while a command was held: {took:?}"
            );
        }
        assert!(
            early.is_empty(),
            "{store}: the held command was answered before its round's reads were, in rounds \
             {early:?}"
        );
        for (read, seen) in timed.iter().zip(&seen) {
            assert_eq!(read.status, 200, "{store}: {}: {}", read.what, read.body);
            match read.what {
                "a view read" => assert_eq!(
                    &listed(&read.body),
                    seen,
                    "{store}: the view lists the last committed write, not the held one"
                ),
                "GET /__spindle/requests" => {
                    let log: Value = serde_json::from_str(&read.body).expect("the log is JSON");
                    let commands = log["requests"]
                        .as_array()
                        .expect("requests")
                        .iter()
                        .filter(|record| record["kind"] == "command")
                        .count();
                    assert_eq!(
                        commands,
                        seen.len(),
                        "{store}: only the commands answered: {log}"
                    );
                }
                _ => assert!(read.body.starts_with("{\"ir\":"), "{store}: {}", read.body),
            }
        }
        let (_, body) = send(address, "GET", &view, "");
        assert_eq!(listed(&body), committed, "{store}");
    }
}
