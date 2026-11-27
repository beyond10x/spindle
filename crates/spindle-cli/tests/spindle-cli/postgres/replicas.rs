//! Several `run` processes of one specification on one PostgreSQL store
//! (`story:several-writers-one-postgres`): one leads, holding the lease of each component's
//! tables, and writes; the others stand by, answering `GET /__spindle/live` `200`,
//! `GET /__spindle/ready` `503` with `standby: true` and every other request `503`, and write
//! nothing; a standby takes the leases once the leader is gone, and a leader paused past its lease
//! lands nothing once it resumes, and exits 1.
//!
//! The fixture `tests/fixtures/ingress` serves `board` (`PUT /tasks/{task_id}`,
//! `POST /tasks/{task_id}/close`, `GET /tasks`); each closed task is recorded by `ledger` through
//! a binding. Both components keep their state in PostgreSQL, each case in a schema of its own,
//! with a lease of 2 s. Every case reaches a database, so each is ignored without
//! `SPINDLE_TEST_DATABASE_URL`; `task check:postgres` runs them.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::{ANSWER, STARTUP, call, owner_url, schema, spindle};

const SPEC: &str = "crates/spindle-cli/tests/fixtures/ingress";
const BINDING: &str = "crates/spindle-cli/tests/fixtures/ingress-http.yaml";
/// The lease every case runs with, in seconds.
const LEASE: &str = "2";
/// The line a standby prints once it took the leases, for the component `board`.
const TOOK: &str = "took the lease of board";

/// A `run` process: its address, and every stderr line it printed with when it arrived.
struct Replica {
    child: Child,
    address: SocketAddr,
    lines: Arc<Mutex<Vec<(Instant, String)>>>,
}

impl Drop for Replica {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Replica {
    /// Starts `command` and waits for its listening line.
    fn start(mut command: Command) -> Self {
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("b10x-spindle starts");
        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            let _ = reader.read_line(&mut line);
            let _ = sender.send(line);
            let _ = std::io::copy(&mut reader, &mut std::io::sink());
        });
        let lines = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&lines);
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                eprintln!("{line}");
                kept.lock().expect("the lines").push((Instant::now(), line));
            }
        });
        let line = receiver.recv_timeout(STARTUP).unwrap_or_default();
        let address = serde_json::from_str::<Value>(&line)
            .ok()
            .and_then(|ready| ready["listening"].as_str().map(str::to_owned))
            .and_then(|address| address.parse().ok());
        match address {
            Some(address) => Self {
                child,
                address,
                lines,
            },
            None => {
                let _ = child.kill();
                let status = child.wait();
                panic!("run printed {line:?} and did not listen: {status:?}");
            }
        }
    }

    /// `GET /__spindle/ready`: the status and the body.
    fn ready(&self) -> (u16, Value) {
        call(self.address, "GET", "/__spindle/ready", None)
    }

    /// Waits until `GET /__spindle/ready` answers `200`, at most until `deadline`.
    fn ready_by(&self, deadline: Instant) {
        loop {
            let (status, body) = self.ready();
            if status == 200 {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "the run never read ready: {status} {body}"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// When the first stderr line holding `text` arrived, waiting until `deadline`.
    fn line_by(&self, text: &str, deadline: Instant) -> Option<Instant> {
        loop {
            let found = self
                .lines
                .lock()
                .expect("the lines")
                .iter()
                .find(|(_, line)| line.contains(text))
                .map(|(at, _)| *at);
            if found.is_some() || Instant::now() >= deadline {
                return found;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Every stderr line so far.
    fn printed(&self) -> String {
        self.lines
            .lock()
            .expect("the lines")
            .iter()
            .map(|(_, line)| line.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn signal(&self, signal: rustix::process::Signal) {
        rustix::process::kill_process(rustix::process::Pid::from_child(&self.child), signal)
            .expect("the signal is sent");
    }

    /// Sends SIGKILL and waits for the process to end: when it ended.
    fn kill(&mut self) -> Instant {
        self.child.kill().expect("SIGKILL is sent");
        let _ = self.child.wait();
        Instant::now()
    }

    /// Waits for the process to exit, at most `within`.
    fn exit_within(&mut self, within: Duration) -> ExitStatus {
        let deadline = Instant::now() + within;
        loop {
            if let Some(status) = self.child.try_wait().expect("the run is waited on") {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "the run did not exit within {within:?}: {}",
                self.printed()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// `PUT /tasks/{id}`.
    fn open(&self, id: &str) -> (u16, Value) {
        call(
            self.address,
            "PUT",
            &format!("/tasks/{id}"),
            Some(&json!({"title": format!("task {id}")})),
        )
    }

    /// `POST /tasks/{id}/close`.
    fn close(&self, id: &str) -> (u16, Value) {
        call(self.address, "POST", &format!("/tasks/{id}/close"), None)
    }

    /// `GET /tasks`, answered `200`.
    fn tasks(&self) -> Value {
        let (status, body) = call(self.address, "GET", "/tasks", None);
        assert_eq!(status, 200, "{body}");
        body
    }
}

/// An environment document keeping both components' state in PostgreSQL, written into
/// `directory`.
fn environment(directory: &Path) -> PathBuf {
    let text = "format = \"spindle-environment/1\"\nname = \"replicas\"\nos = \"linux\"\n\
         arch = \"x86_64\"\n\n[provides]\n\"store.durable\" = [\"postgres\"]\n\
         \"store.ephemeral\" = [\"memory\"]\n\"identity.mint\" = [\"uuid-v4\"]\n\
         clock = [\"system\"]\ntimer = [\"tokio\"]\nchannel = [\"in-process\"]\n\
         projection = [\"in-process\"]\n\"ingress.network\" = [\"http-loopback\"]\n\
         \"external.*\" = [\"operator-prompt\"]\n";
    let path = directory.join("replicas.toml");
    std::fs::write(&path, text).expect("the environment is written");
    path
}

/// `run` on the fixture with `environment`, its state in `schema`, a lease of [`LEASE`].
fn run(environment: &Path, schema: &str) -> Command {
    let url = owner_url();
    let environment = environment.to_str().expect("a UTF-8 path");
    spindle(
        &["run", SPEC, "--http", BINDING, "--environment", environment],
        &[
            ("database_url", url.as_str()),
            ("database_schema", schema),
            ("database_lease", LEASE),
        ],
    )
}

/// The store of `component` in `schema`, opened by the test: it takes no lease and writes nothing.
fn store(schema: &str, component: &str) -> spindle_kernel::PostgresStore {
    let mut database = spindle_kernel::Database::new(owner_url());
    database.schema = schema.to_owned();
    spindle_kernel::PostgresStore::open(&database, component, 1).expect("the store opens")
}

/// Every row of `entity` in `component`'s store in `schema`: identity, state, fields, version.
fn rows(schema: &str, component: &str, entity: &str) -> Vec<Value> {
    spindle_kernel::Store::scan(&mut store(schema, component), entity)
        .expect("the rows are read")
        .into_iter()
        .map(|(identity, stored)| {
            json!([
                identity,
                stored.row.state,
                stored.row.fields,
                stored.version
            ])
        })
        .collect()
}

/// The position of the last write in `component`'s store in `schema`.
fn head(schema: &str, component: &str) -> u64 {
    spindle_kernel::Store::events_head(&mut store(schema, component)).expect("the head is read")
}

/// The heads of board and ledger in `schema` once board's holds at least `events` events. A
/// head reads a write once every older transaction on the server has ended, so while another
/// test of the lane holds one open it reads behind for a while.
fn settled(schema: &str, events: u64) -> (u64, u64) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let heads = (head(schema, "board"), head(schema, "ledger"));
        if heads.0 >= events {
            return heads;
        }
        assert!(
            Instant::now() < deadline,
            "board's head read {} of {events} events for 30 s",
            heads.0
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A leader, ready, and a second process on the same store.
fn leader_and_standby(environment: &Path, schema: &str) -> (Replica, Replica) {
    let leader = Replica::start(run(environment, schema));
    leader.ready_by(Instant::now() + ANSWER);
    let standby = Replica::start(run(environment, schema));
    (leader, standby)
}

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_second_process_stands_by() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let schema = schema();
    let (leader, standby) = leader_and_standby(&environment, &schema);
    let (status, answer) = leader.open("t1");
    assert_eq!(status, 201, "{answer}");
    // The specification recorded and t1.
    let before = settled(&schema, 2);

    let (status, body) = standby.ready();
    assert_eq!(status, 503, "a standby is not ready: {body}");
    assert_eq!(body["standby"], true, "{body}");
    assert_eq!(body["ready"], false, "{body}");
    let (status, body) = call(standby.address, "GET", "/__spindle/live", None);
    assert_eq!(status, 200, "a standby is live: {body}");
    let (status, body) = standby.open("t2");
    assert_eq!(status, 503, "a standby answers a command 503: {body}");
    let (status, body) = standby.close("t1");
    assert_eq!(status, 503, "{body}");
    let (status, body) = call(standby.address, "GET", "/tasks", None);
    assert_eq!(status, 503, "a standby answers a view 503: {body}");

    // Three of the standby's polls of the leases, a third of the lease each.
    std::thread::sleep(Duration::from_secs(2));
    let (status, body) = standby.ready();
    assert_eq!(status, 503, "the standby still stands by: {body}");
    assert_eq!(
        (head(&schema, "board"), head(&schema, "ledger")),
        before,
        "the standby wrote nothing"
    );
    assert!(
        standby.line_by(TOOK, Instant::now()).is_none(),
        "{}",
        standby.printed()
    );
    let tasks = leader.tasks();
    assert_eq!(
        tasks,
        json!({"rows": [{"task_id": "t1", "state": "Open", "title": "task t1"}]}),
        "{tasks}"
    );
}

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_killed_leader_leaves_the_standby_serving() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let schema = schema();
    let (mut leader, standby) = leader_and_standby(&environment, &schema);
    let (status, answer) = leader.open("t1");
    assert_eq!(status, 201, "{answer}");
    let (status, body) = standby.ready();
    assert_eq!(status, 503, "{body}");

    let killed = leader.kill();
    let took = standby
        .line_by(TOOK, killed + Duration::from_secs(5))
        .unwrap_or_else(|| {
            panic!(
                "the standby did not take the lease within 5 s of the kill: {}",
                standby.printed()
            )
        });
    eprintln!(
        "the standby took the lease {:?} after the kill",
        took.saturating_duration_since(killed)
    );
    standby.ready_by(killed + Duration::from_secs(10));
    let (status, answer) = standby.open("t2");
    assert_eq!(status, 201, "{answer}");
    let (status, answer) = standby.close("t1");
    assert_eq!(status, 200, "{answer}");
    let tasks = standby.tasks();
    assert_eq!(
        tasks,
        json!({"rows": [
            {"task_id": "t1", "state": "Done", "title": "task t1"},
            {"task_id": "t2", "state": "Open", "title": "task t2"},
        ]}),
        "{tasks}"
    );
}

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn no_write_is_lost_across_a_failover() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let first_half = |replica: &Replica| {
        for id in ["t1", "t2", "t3"] {
            let (status, answer) = replica.open(id);
            assert_eq!(status, 201, "{answer}");
        }
        let (status, answer) = replica.close("t1");
        assert_eq!(status, 200, "{answer}");
    };
    let second_half = |replica: &Replica| {
        for id in ["t4", "t5"] {
            let (status, answer) = replica.open(id);
            assert_eq!(status, 201, "{answer}");
        }
        for id in ["t2", "t4"] {
            let (status, answer) = replica.close(id);
            assert_eq!(status, 200, "{answer}");
        }
        let (status, answer) = replica.close("t1");
        assert_eq!(status, 410, "t1 was closed before the failover: {answer}");
    };

    let failed_over = schema();
    let (mut leader, standby) = leader_and_standby(&environment, &failed_over);
    first_half(&leader);
    let killed = leader.kill();
    standby.ready_by(killed + Duration::from_secs(10));
    second_half(&standby);
    let after_failover = standby.tasks();
    drop(standby);

    let alone = schema();
    let single = Replica::start(run(&environment, &alone));
    single.ready_by(Instant::now() + ANSWER);
    first_half(&single);
    second_half(&single);
    assert_eq!(after_failover, single.tasks());
    drop(single);
    for (component, entity) in [
        ("board", "tasks.board.Task"),
        ("ledger", "tasks.ledger.Closure"),
    ] {
        assert_eq!(
            rows(&failed_over, component, entity),
            rows(&alone, component, entity),
            "{entity}: no write is lost or applied twice"
        );
    }
}

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_paused_leader_is_fenced() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let schema = schema();
    let (mut leader, standby) = leader_and_standby(&environment, &schema);
    let (status, answer) = leader.open("t1");
    assert_eq!(status, 201, "{answer}");

    leader.signal(rustix::process::Signal::STOP);
    let paused = Instant::now();
    standby
        .line_by(TOOK, paused + Duration::from_secs(10))
        .unwrap_or_else(|| panic!("the standby never took over: {}", standby.printed()));
    standby.ready_by(paused + Duration::from_secs(15));
    let (status, answer) = standby.open("t2");
    assert_eq!(status, 201, "{answer}");
    // The specification recorded, t1 and t2.
    let before = settled(&schema, 3);

    // A command reaches the paused leader's socket; it is read once the leader resumes.
    let address = leader.address;
    let late = std::thread::spawn(move || {
        let mut stream = TcpStream::connect_timeout(&address, ANSWER).expect("the socket accepts");
        stream
            .set_read_timeout(Some(ANSWER))
            .expect("a read timeout");
        let body = json!({"title": "task t9"}).to_string();
        write!(
            stream,
            "PUT /tasks/t9 HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\
             Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .expect("the command is written");
        let mut answer = String::new();
        let _ = stream.read_to_string(&mut answer);
        answer
    });
    std::thread::sleep(Duration::from_millis(300));
    leader.signal(rustix::process::Signal::CONT);
    let status = leader.exit_within(Duration::from_secs(10));
    assert_eq!(status.code(), Some(1), "{}", leader.printed());
    assert!(
        leader.printed().contains("lease of board"),
        "the old leader names the lease it lost: {}",
        leader.printed()
    );
    let answer = late.join().expect("the late command ends");
    assert!(
        !answer.starts_with("HTTP/1.1 2"),
        "the old leader landed the late command: {answer}"
    );
    assert_eq!(
        (head(&schema, "board"), head(&schema, "ledger")),
        before,
        "the old leader wrote nothing once it resumed"
    );
    let tasks = standby.tasks();
    assert_eq!(
        tasks,
        json!({"rows": [
            {"task_id": "t1", "state": "Open", "title": "task t1"},
            {"task_id": "t2", "state": "Open", "title": "task t2"},
        ]}),
        "{tasks}"
    );
}

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_standby_posts_no_webhook_and_keeps_no_dead_letter() {
    use super::super::egress::{Webhook, named};

    const OPENED: &str = "tasks.board.TaskOpened";
    let webhook = Webhook::start(200);
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment =
        super::super::egress::environment(directory.path(), "postgres", &webhook.url(), &[OPENED]);
    let schema = schema();
    let (leader, standby) = leader_and_standby(&environment, &schema);
    let (status, answer) = leader.open("t1");
    assert_eq!(status, 201, "{answer}");
    accepted(&webhook, 1);
    // Three of the standby's polls of the leases.
    std::thread::sleep(Duration::from_secs(2));
    assert_eq!(
        webhook.received().len(),
        1,
        "only the leader posts: {:?}",
        webhook.received()
    );
    let (status, body) = call(standby.address, "GET", "/api/dead-letters", None);
    assert_eq!(status, 503, "a standby lists no dead letter: {body}");
    for component in ["board", "ledger"] {
        assert_eq!(
            spindle_kernel::Store::dead_letters(&mut store(&schema, component))
                .expect("the dead letters are read"),
            Vec::new(),
            "{component} keeps no dead letter"
        );
    }

    let mut leader = leader;
    leader.signal(rustix::process::Signal::TERM);
    assert_eq!(leader.exit_within(ANSWER).code(), Some(143));
    standby.ready_by(Instant::now() + Duration::from_secs(10));
    let (status, answer) = standby.open("t2");
    assert_eq!(status, 201, "{answer}");
    accepted(&webhook, 2);
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        named(&webhook.accepted()),
        [
            (OPENED.to_owned(), "t1".to_owned()),
            (OPENED.to_owned(), "t2".to_owned()),
        ],
        "the new leader posts what it commits, and t1 not again"
    );
}

/// Waits until `webhook` accepted `count` bodies. An egress reads a write once every older
/// transaction on the server has ended, so while another test of the lane holds one open it posts
/// late: the wait is 30 s, where the egress tests' own is 10 s.
fn accepted(webhook: &super::super::egress::Webhook, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while webhook.accepted().len() < count {
        assert!(
            Instant::now() < deadline,
            "the webhook accepted {} of {count} bodies within 30 s: {:?}",
            webhook.accepted().len(),
            webhook.received()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
