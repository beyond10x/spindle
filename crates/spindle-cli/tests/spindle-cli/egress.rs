//! `run` serving with an `[egress]` table: each published event the environment binds to an
//! `[http.<name>]` call is POSTed there at least once, in commit order per component store, and
//! delivery resumes after a restart (`story:event-egress-webhook`).
//!
//! The fixture `tests/fixtures/ingress` serves `board`, whose `OpenTask` publishes
//! `tasks.board.TaskOpened` and whose `CloseTask` publishes `tasks.board.TaskClosed`, through the
//! binding `ingress-http.yaml` (`PUT /tasks/{task_id}`, `POST /tasks/{task_id}/close`). The webhook
//! is a stub on 127.0.0.1 that this test runs: it records the body of every request it is sent
//! with the status it answered, and every assertion reads those bodies.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

pub(super) const SPEC: &str = "crates/spindle-cli/tests/fixtures/ingress";
pub(super) const BINDING: &str = "crates/spindle-cli/tests/fixtures/ingress-http.yaml";
const OPENED: &str = "tasks.board.TaskOpened";
const CLOSED: &str = "tasks.board.TaskClosed";

/// How long `run` may take to admit, open its state and print where it listens.
const STARTUP: Duration = Duration::from_secs(30);
/// How long one request may take to be answered.
const ANSWER: Duration = Duration::from_secs(10);
/// How long a posted event may take to reach the webhook.
const DELIVERY: Duration = Duration::from_secs(10);

pub(super) fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// One request the webhook was sent: its request line, its body, and the status it answered.
#[derive(Debug, Clone)]
pub(super) struct Received {
    pub(super) line: String,
    pub(super) body: Value,
    pub(super) status: u16,
}

/// A webhook on 127.0.0.1 answering every request with the status it holds at the time.
pub(super) struct Webhook {
    address: SocketAddr,
    status: Arc<AtomicU16>,
    received: Arc<Mutex<Vec<Received>>>,
}

impl Webhook {
    pub(super) fn start(status: u16) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("the webhook binds");
        let address = listener.local_addr().expect("the webhook has an address");
        let answering = Arc::new(AtomicU16::new(status));
        let received = Arc::new(Mutex::new(Vec::new()));
        let (recorded, status) = (Arc::clone(&received), Arc::clone(&answering));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                let (recorded, status) = (Arc::clone(&recorded), Arc::clone(&status));
                std::thread::spawn(move || answer(stream, &recorded, &status));
            }
        });
        Self {
            address,
            status: answering,
            received,
        }
    }

    pub(super) fn url(&self) -> String {
        format!("http://{}/events", self.address)
    }

    /// Answers every request from now on with `status`.
    pub(super) fn answer_with(&self, status: u16) {
        self.status.store(status, Ordering::SeqCst);
    }

    pub(super) fn received(&self) -> Vec<Received> {
        self.received.lock().expect("the webhook's record").clone()
    }

    /// The bodies the webhook answered `2xx`, in arrival order.
    pub(super) fn accepted(&self) -> Vec<Value> {
        self.received()
            .into_iter()
            .filter(|received| (200..300).contains(&received.status))
            .map(|received| received.body)
            .collect()
    }

    /// Waits until the webhook accepted `count` bodies, and answers them.
    pub(super) fn wait_for(&self, count: usize) -> Vec<Value> {
        let deadline = Instant::now() + DELIVERY;
        loop {
            let accepted = self.accepted();
            if accepted.len() >= count {
                return accepted;
            }
            assert!(
                Instant::now() < deadline,
                "the webhook accepted {} of {count} bodies within {DELIVERY:?}: {:?}",
                accepted.len(),
                self.received()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Waits until the webhook was sent at least one request, whatever it answered.
    pub(super) fn wait_for_any(&self) {
        let deadline = Instant::now() + DELIVERY;
        while self.received().is_empty() {
            assert!(
                Instant::now() < deadline,
                "the webhook was sent nothing within {DELIVERY:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// Reads one request from `stream`, records it, and answers the status `status` holds.
fn answer(stream: TcpStream, recorded: &Mutex<Vec<Received>>, status: &AtomicU16) {
    let _ = stream.set_read_timeout(Some(ANSWER));
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || line.is_empty() {
        return;
    }
    let mut length = 0;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).is_err() {
            return;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0; length];
    if reader.read_exact(&mut body).is_err() {
        return;
    }
    let status = status.load(Ordering::SeqCst);
    recorded
        .lock()
        .expect("the webhook's record")
        .push(Received {
            line: line.trim_end().to_owned(),
            body: serde_json::from_slice(&body).unwrap_or(Value::Null),
            status,
        });
    let mut stream = reader.into_inner();
    let _ = write!(
        stream,
        "HTTP/1.1 {status} Answer\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
}

/// An environment document keeping state in `store`, binding each of `events` to the webhook
/// at `url`, written into `directory`.
pub(super) fn environment(directory: &Path, store: &str, url: &str, events: &[&str]) -> PathBuf {
    let egress: String = events
        .iter()
        .map(|event| format!("\"{event}\" = \"hook\"\n"))
        .collect();
    let text = format!(
        "format = \"spindle-environment/1\"\nname = \"egress\"\nos = \"linux\"\narch = \"x86_64\"\n\n\
         [provides]\n\"store.durable\" = [\"{store}\"]\n\"store.ephemeral\" = [\"memory\"]\n\
         \"identity.mint\" = [\"uuid-v4\"]\nclock = [\"system\"]\ntimer = [\"tokio\"]\n\
         channel = [\"in-process\"]\nprojection = [\"in-process\"]\n\
         \"ingress.network\" = [\"http-loopback\"]\n\"external.*\" = [\"operator-prompt\"]\n\n\
         [egress]\n{egress}\n\
         [http.hook]\nmethod = \"POST\"\nurl = \"{url}\"\ntimeout_ms = 2000\n"
    );
    let path = directory.join("egress.toml");
    std::fs::write(&path, text).expect("the environment is written");
    path
}

/// A running `b10x-spindle run`; killed when dropped.
pub(super) struct Served {
    pub(super) child: Child,
    pub(super) address: SocketAddr,
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Served {
    /// Sends SIGTERM and waits for the run to exit.
    pub(super) fn terminate(mut self) -> ExitStatus {
        rustix::process::kill_process(
            rustix::process::Pid::from_child(&self.child),
            rustix::process::Signal::TERM,
        )
        .expect("SIGTERM is sent");
        self.wait()
    }

    /// Sends SIGKILL and waits for the run to end.
    pub(super) fn kill(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    fn wait(&mut self) -> ExitStatus {
        let deadline = Instant::now() + ANSWER;
        loop {
            if let Some(status) = self.child.try_wait().expect("the run is waited on") {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "run did not exit within {ANSWER:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Opens the task `id`.
    pub(super) fn open(&self, id: &str) {
        let (status, answer) = exchange(
            self.address,
            "PUT",
            &format!("/tasks/{id}"),
            Some(&json!({"title": format!("task {id}")})),
        );
        assert_eq!(status, 201, "{answer}");
    }

    /// Closes the task `id`.
    fn close(&self, id: &str) {
        let (status, answer) = exchange(self.address, "POST", &format!("/tasks/{id}/close"), None);
        assert_eq!(status, 200, "{answer}");
    }

    /// The events `GET /api/events` lists.
    pub(super) fn events(&self) -> Vec<Value> {
        let (status, answer) = exchange(self.address, "GET", "/api/events", None);
        assert_eq!(status, 200, "{answer}");
        answer["events"]
            .as_array()
            .expect("a list of events")
            .clone()
    }
}

/// `command`, started: its listening line read, or a panic naming how it ended.
pub(super) fn listening(mut command: Command) -> Served {
    let mut child = command
        .current_dir(root())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("b10x-spindle starts");
    let stdout = child.stdout.take().expect("stdout is piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let _ = reader.read_line(&mut line);
        let _ = sender.send(line);
        let _ = std::io::copy(&mut reader, &mut std::io::sink());
    });
    let line = receiver.recv_timeout(STARTUP).unwrap_or_default();
    let address = serde_json::from_str::<Value>(&line)
        .ok()
        .and_then(|ready| ready["listening"].as_str().map(str::to_owned))
        .and_then(|address| address.parse().ok());
    match address {
        Some(address) => Served { child, address },
        None => {
            let _ = child.kill();
            let status = child.wait();
            panic!("run printed {line:?} and did not listen: {status:?}");
        }
    }
}

/// `run` on the fixture with `environment`, keeping its state in `state`.
fn run(environment: &Path, state: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"));
    command
        .args(["run", SPEC, "--http", BINDING, "--environment"])
        .arg(environment)
        .arg("--state")
        .arg(state);
    command
}

/// One HTTP/1.1 exchange on its own connection: the status and the JSON body, `null` for none.
pub(super) fn exchange(
    address: SocketAddr,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> (u16, Value) {
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
    (status, serde_json::from_str(body).unwrap_or(Value::Null))
}

/// The event name and the task id of each body.
pub(super) fn named(bodies: &[Value]) -> Vec<(String, String)> {
    bodies
        .iter()
        .map(|body| {
            (
                body["event"].as_str().unwrap_or_default().to_owned(),
                body["fields"]["task_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
            )
        })
        .collect()
}

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a temporary directory")
}

#[test]
fn a_committed_event_reaches_the_webhook() {
    let webhook = Webhook::start(200);
    let directory = scratch();
    let environment = environment(directory.path(), "sqlite", &webhook.url(), &[OPENED]);
    let served = listening(run(&environment, &directory.path().join("state")));
    served.open("t1");

    let accepted = webhook.wait_for(1);
    let body = &accepted[0];
    let position = body["position"]
        .as_u64()
        .unwrap_or_else(|| panic!("the body names the event's store position as a number: {body}"));
    assert!(position > 0, "{body}");
    assert_eq!(
        body,
        &json!({
            "format": "spindle-egress/1",
            "event": OPENED,
            "component": "board",
            "position": position,
            "fields": {"task_id": "t1", "title": "task t1"},
        })
    );
    let request = &webhook.received()[0];
    assert!(request.line.starts_with("POST /events "), "{request:?}");
    // Posted once: nothing more arrives once it was accepted.
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(webhook.received().len(), 1, "{:?}", webhook.received());
}

#[test]
fn an_event_committed_while_the_webhook_is_down_is_posted_when_it_returns() {
    let started = Instant::now();
    let webhook = Webhook::start(503);
    let directory = scratch();
    let environment = environment(directory.path(), "sqlite", &webhook.url(), &[OPENED]);
    let served = listening(run(&environment, &directory.path().join("state")));
    served.open("t1");
    webhook.wait_for_any();
    let answering = Instant::now() + Duration::from_secs(5);
    std::thread::sleep(answering.saturating_duration_since(Instant::now()));
    webhook.answer_with(200);

    let accepted = webhook.wait_for(1);
    assert_eq!(named(&accepted), [(OPENED.to_owned(), "t1".to_owned())]);
    let refused: Vec<Received> = webhook
        .received()
        .into_iter()
        .filter(|received| received.status == 503)
        .collect();
    assert!(!refused.is_empty(), "the webhook was down at first");
    assert!(
        refused.iter().all(|received| received.body == accepted[0]),
        "each attempt carries the same body: {refused:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "the backoff is capped at 5 s: {:?}",
        started.elapsed()
    );
}

#[test]
fn an_unposted_event_is_posted_after_a_restart() {
    let webhook = Webhook::start(503);
    let directory = scratch();
    let state = directory.path().join("state");
    let environment = environment(directory.path(), "sqlite", &webhook.url(), &[OPENED]);
    let served = listening(run(&environment, &state));
    served.open("t1");
    webhook.wait_for_any();
    served.kill();

    webhook.answer_with(200);
    let _again = listening(run(&environment, &state));
    let accepted = webhook.wait_for(1);
    assert_eq!(named(&accepted), [(OPENED.to_owned(), "t1".to_owned())]);
}

#[test]
fn a_posted_event_is_not_posted_again_after_a_restart() {
    let webhook = Webhook::start(200);
    let directory = scratch();
    let state = directory.path().join("state");
    let environment = environment(directory.path(), "sqlite", &webhook.url(), &[OPENED]);
    let served = listening(run(&environment, &state));
    served.open("t1");
    webhook.wait_for(1);
    assert_eq!(served.terminate().code(), Some(143));

    let again = listening(run(&environment, &state));
    again.open("t2");
    webhook.wait_for(2);
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        named(&webhook.accepted()),
        [
            (OPENED.to_owned(), "t1".to_owned()),
            (OPENED.to_owned(), "t2".to_owned()),
        ],
        "t1 is not posted again"
    );
    // The cursor the first run saved is no published event.
    let events = again.events();
    assert_eq!(
        events
            .iter()
            .map(|event| event["event"].as_str().unwrap_or_default())
            .collect::<Vec<_>>(),
        [OPENED, OPENED],
        "{events:?}"
    );
}

#[test]
fn events_of_one_component_are_posted_in_commit_order() {
    let webhook = Webhook::start(200);
    let directory = scratch();
    let environment = environment(
        directory.path(),
        "sqlite",
        &webhook.url(),
        &[OPENED, CLOSED],
    );
    let served = listening(run(&environment, &directory.path().join("state")));
    served.open("t1");
    served.open("t2");
    served.close("t1");
    served.open("t3");
    served.close("t2");

    let accepted = webhook.wait_for(5);
    assert_eq!(
        named(&accepted),
        [
            (OPENED.to_owned(), "t1".to_owned()),
            (OPENED.to_owned(), "t2".to_owned()),
            (CLOSED.to_owned(), "t1".to_owned()),
            (OPENED.to_owned(), "t3".to_owned()),
            (CLOSED.to_owned(), "t2".to_owned()),
        ]
    );
    let positions: Vec<u64> = accepted
        .iter()
        .map(|body| body["position"].as_u64().expect("a position"))
        .collect();
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "positions rise strictly: {positions:?}"
    );
}

/// A reset replaces each SQLite file: the egress stops before it and goes on on the new files.
#[test]
fn an_event_committed_after_a_reset_is_posted() {
    let webhook = Webhook::start(200);
    let directory = scratch();
    let environment = environment(directory.path(), "sqlite", &webhook.url(), &[OPENED]);
    let served = listening(run(&environment, &directory.path().join("state")));
    served.open("t1");
    webhook.wait_for(1);
    let (status, answer) = exchange(served.address, "POST", "/api/reset", None);
    assert_eq!(status, 200, "{answer}");
    served.open("t2");
    webhook.wait_for(2);
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        named(&webhook.accepted()),
        [
            (OPENED.to_owned(), "t1".to_owned()),
            (OPENED.to_owned(), "t2".to_owned()),
        ]
    );
}

#[test]
fn an_egress_on_a_memory_store_is_refused() {
    let webhook = Webhook::start(200);
    let directory = scratch();
    let environment = environment(directory.path(), "memory", &webhook.url(), &[OPENED]);
    let output = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", SPEC, "--http", BINDING, "--environment"])
        .arg(&environment)
        .stdin(Stdio::null())
        .output()
        .expect("b10x-spindle runs");
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(OPENED) && stderr.contains("memory"),
        "the refusal names the event and the store: {stderr}"
    );
    assert!(webhook.received().is_empty());
}
