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
    /// Set once its stderr was read to the end.
    read: Arc<std::sync::atomic::AtomicBool>,
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
        let read = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reading = Arc::clone(&read);
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                eprintln!("{line}");
                kept.lock().expect("the lines").push((Instant::now(), line));
            }
            reading.store(true, std::sync::atomic::Ordering::SeqCst);
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
                read,
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
                // Every line it printed is read before its output is asked for.
                let read = Instant::now() + Duration::from_secs(5);
                while !self.read.load(std::sync::atomic::Ordering::SeqCst) && Instant::now() < read
                {
                    std::thread::sleep(Duration::from_millis(10));
                }
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

/// The events board's and ledger's tables in `schema` hold, counted as the database's owner. A
/// count reads every committed row at once, where a store's head reads a write only once every
/// older transaction on the server has ended, which another test of the lane may hold open.
fn counts(schema: &str) -> (u64, u64) {
    let count = |component: &str| -> u64 {
        super::sql(&[format!(
            "SELECT count(*) FROM {schema}.spindle_{component}_events"
        )])[0]
            .parse()
            .expect("a count")
    };
    (count("board"), count("ledger"))
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
    let before = counts(&schema);

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
    assert_eq!(counts(&schema), before, "the standby wrote nothing");
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
    let before = counts(&schema);

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
    // The standby took both; the old leader names the first it found taken.
    assert!(
        leader.printed().contains("lease of board") || leader.printed().contains("lease of ledger"),
        "the old leader names the lease it lost: {}",
        leader.printed()
    );
    let answer = late.join().expect("the late command ends");
    assert!(
        answer.starts_with("HTTP/1.1 503"),
        "the old leader answers the late command 503: {answer}"
    );
    assert_eq!(
        counts(&schema),
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

/// The local TCP ports of the sockets process `pid` holds open: its connections to the database
/// among them, which `pg_stat_activity` names by `client_port`.
fn ports_of(pid: u32) -> Vec<u16> {
    let mut inodes = Vec::new();
    if let Ok(entries) = std::fs::read_dir(format!("/proc/{pid}/fd")) {
        for entry in entries.flatten() {
            if let Ok(target) = std::fs::read_link(entry.path())
                && let Some(inode) = target
                    .to_str()
                    .and_then(|target| target.strip_prefix("socket:["))
                    .and_then(|rest| rest.strip_suffix(']'))
            {
                inodes.push(inode.to_owned());
            }
        }
    }
    let mut ports = Vec::new();
    for table in ["/proc/net/tcp", "/proc/net/tcp6"] {
        let Ok(text) = std::fs::read_to_string(table) else {
            continue;
        };
        for line in text.lines().skip(1) {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() > 9 && inodes.iter().any(|inode| inode == fields[9]) {
                let port = fields[1]
                    .rsplit(':')
                    .next()
                    .and_then(|port| u16::from_str_radix(port, 16).ok());
                ports.extend(port);
            }
        }
    }
    ports
}

/// Whether a database session of process `pid` is inside a write transaction, between two of its
/// statements, holding what it wrote or locked (it has a transaction id), for at least 150 ms.
fn inside_a_transaction(pid: u32) -> bool {
    let ports = ports_of(pid);
    if ports.is_empty() {
        return false;
    }
    let list = ports
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    super::sql(&[format!(
        "SELECT count(*) FROM pg_stat_activity WHERE client_port IN ({list}) \
         AND state = 'idle in transaction' AND backend_xid IS NOT NULL \
         AND xact_start < now() - interval '150 ms'"
    )])[0]
        != "0"
}

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_leader_paused_inside_a_write_holds_up_no_takeover() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let schema = schema();
    let (mut leader, standby) = leader_and_standby(&environment, &schema);
    let (status, answer) = leader.open("t1");
    assert_eq!(status, 201, "{answer}");

    // Commands reach the leader one after another, so it is inside a write most of the time; it is
    // paused until a pause lands inside one, between two of its statements.
    let address = leader.address;
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stopped = Arc::clone(&stop);
    let firing = std::thread::spawn(move || {
        let mut index = 0;
        while !stopped.load(std::sync::atomic::Ordering::SeqCst) {
            index += 1;
            let Ok(mut stream) = TcpStream::connect_timeout(&address, ANSWER) else {
                break;
            };
            let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
            let body = json!({"title": "fired"}).to_string();
            let _ = write!(
                stream,
                "PUT /tasks/f{index} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\
                 Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            let mut answer = String::new();
            let _ = stream.read_to_string(&mut answer);
        }
    });
    let pid = leader.child.id();
    let mut inside = false;
    for _ in 0..60 {
        leader.signal(rustix::process::Signal::STOP);
        std::thread::sleep(Duration::from_millis(200));
        if inside_a_transaction(pid) {
            inside = true;
            break;
        }
        leader.signal(rustix::process::Signal::CONT);
        std::thread::sleep(Duration::from_millis(20));
    }
    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(inside, "no pause landed inside a write of the leader");
    let paused = Instant::now() - Duration::from_millis(200);

    // The paused leader's transaction ends within the lease, so the standby takes over and
    // writes as it would after any pause: by the lease's end and three polls, 4 s.
    let took = standby
        .line_by(TOOK, paused + Duration::from_secs(4))
        .unwrap_or_else(|| {
            panic!(
                "the standby did not take over within 4 s of a pause inside a write: {}",
                standby.printed()
            )
        });
    eprintln!("took over {:?} after the pause", took - paused);
    standby.ready_by(paused + Duration::from_secs(6));
    let (status, answer) = standby.open("t2");
    assert_eq!(
        status, 201,
        "the new leader writes past the paused leader's transaction: {answer}"
    );
    assert!(
        paused.elapsed() < Duration::from_secs(8),
        "t2 landed {:?} after the pause",
        paused.elapsed()
    );
    leader.signal(rustix::process::Signal::CONT);
    let _ = leader.exit_within(Duration::from_secs(10));
    let _ = firing.join();
}

/// A database session of the test's own, as the owner, inside a transaction that ran a statement
/// and holds what that statement locked until [`Holding::end`]; the server never ends it for
/// idling.
struct Holding {
    pid: i32,
    end: mpsc::Sender<()>,
    thread: std::thread::JoinHandle<Result<(), String>>,
}

impl Holding {
    /// Opens the session, begins a transaction and runs `statement` in it.
    fn start(statement: String) -> Self {
        let (started, starting) = mpsc::channel();
        let (end, ending) = mpsc::channel::<()>();
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a runtime");
            runtime.block_on(async {
                let (client, connection) =
                    tokio_postgres::connect(&owner_url(), tokio_postgres::NoTls)
                        .await
                        .expect("the test database accepts its owner");
                let driver = tokio::spawn(connection);
                client
                    .batch_execute("SET idle_in_transaction_session_timeout = 0; BEGIN")
                    .await
                    .expect("a transaction begins");
                client
                    .batch_execute(&statement)
                    .await
                    .unwrap_or_else(|error| panic!("{statement}: {error}"));
                let pid: i32 = client
                    .query_one("SELECT pg_backend_pid()", &[])
                    .await
                    .expect("the session's pid")
                    .get(0);
                started.send(pid).expect("the test waits for the session");
                let _ = ending.recv();
                let committed = client
                    .batch_execute("COMMIT")
                    .await
                    .map_err(|error| error.to_string());
                drop(client);
                let _ = driver.await;
                committed
            })
        });
        let pid = starting.recv().expect("the session holds its lock");
        Self { pid, end, thread }
    }

    /// Whether the session is still open on the server.
    fn alive(&self) -> bool {
        super::sql(&[format!(
            "SELECT count(*) FROM pg_stat_activity WHERE pid = {}",
            self.pid
        )])[0]
            == "1"
    }

    /// Commits the transaction: an error where the server ended the session.
    fn end(self) -> Result<(), String> {
        let _ = self.end.send(());
        self.thread.join().expect("the session's thread ends")
    }
}

/// The lease of board in `schema`, taken by the test for `term` through a connection of its own:
/// the lease and its token.
fn taken_by_the_test(schema: &str, term: Duration) -> (spindle_kernel::Lease, u64) {
    let mut database = spindle_kernel::Database::new(owner_url());
    database.schema = schema.to_owned();
    let mut lease = spindle_kernel::Lease::connect(&database, term).expect("the lease connects");
    let token = lease
        .take("board")
        .expect("the lease is read")
        .expect("the lease is free");
    lease.take("ledger").expect("the ledger's lease is read");
    (lease, token)
}

/// What a standby prints once it ended the sessions holding the lease row of board.
const ENDED: &str = "the lease of board ran out while a process still held its row: ended 1 of";

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_standby_ends_only_the_sessions_holding_a_lease_row_past_its_end() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let schema = schema();
    let term = Duration::from_secs(6);
    let taken = Instant::now();
    let (mut lease, token) = taken_by_the_test(&schema, term);
    let holding = Holding::start(format!(
        "SELECT body FROM {schema}.spindle_board_p_lease FOR UPDATE"
    ));
    let unrelated = Holding::start(format!(
        "CREATE TABLE {schema}.unrelated (x int); INSERT INTO {schema}.unrelated VALUES (1); \
         SELECT x FROM {schema}.unrelated FOR UPDATE"
    ));
    let standby = Replica::start(run(&environment, &schema));
    let (status, body) = standby.ready();
    assert_eq!(status, 503, "{body}");
    assert!(
        taken.elapsed() + Duration::from_millis(2700) < term,
        "the standby stood by {:?} after the lease was taken, too late to ask twice within it",
        taken.elapsed()
    );

    // While the lease runs, the session holding its row is a leader's write: it is not ended.
    std::thread::sleep(
        (taken + term - Duration::from_millis(600)).saturating_duration_since(Instant::now()),
    );
    assert!(
        holding.alive(),
        "a session holding a running lease's row was ended: {}",
        standby.printed()
    );
    assert!(
        !standby.printed().contains("ran out"),
        "{}",
        standby.printed()
    );

    // Past its end, the standby ends exactly the session holding the row and takes the lease.
    standby
        .line_by(TOOK, taken + term + Duration::from_secs(4))
        .unwrap_or_else(|| {
            panic!(
                "the standby did not take the lease within 4 s of its end: {}",
                standby.printed()
            )
        });
    assert!(
        standby.printed().contains(ENDED),
        "no line naming the lease and the sessions ended: {}",
        standby.printed()
    );
    assert!(
        holding.end().is_err(),
        "the session holding the row past the lease's end was not ended"
    );
    unrelated
        .end()
        .expect("a session holding an unrelated table's row is not ended");
    // The session the lease row names, which holds no lock on it, is not ended either.
    assert!(
        !lease
            .renew("board", token)
            .expect("the test's lease connection was not ended"),
        "the standby took the lease"
    );
}

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_standby_refused_ending_a_holder_warns_once_and_waits() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let schema = schema();
    let term = Duration::from_secs(2);
    let taken = Instant::now();
    let (_lease, _) = taken_by_the_test(&schema, term);
    let holding = Holding::start(format!(
        "SELECT body FROM {schema}.spindle_board_p_lease FOR UPDATE"
    ));
    // A standby whose role may not end the owner's sessions.
    let role = super::serving_role(&schema, 100);
    let mut command = run(&environment, &schema);
    command
        .env(
            "SPINDLE_SETTING_DATABASE_URL",
            super::as_role(&owner_url(), &role, super::PASSWORD),
        )
        .env("SPINDLE_SETTING_DATABASE_CA", super::ca())
        .env("SPINDLE_SETTING_DATABASE_CONNECTIONS", "100");
    let mut standby = Replica::start(command);
    let (status, body) = standby.ready();
    assert_eq!(status, 503, "{body}");

    std::thread::sleep(
        (taken + term + Duration::from_secs(3)).saturating_duration_since(Instant::now()),
    );
    let warned = standby
        .printed()
        .lines()
        .filter(|line| line.contains("PostgreSQL refused to end") && line.contains("board"))
        .count();
    assert_eq!(
        warned,
        1,
        "one warning naming the lease where the server refuses to end its holder: {}",
        standby.printed()
    );
    assert!(
        standby
            .child
            .try_wait()
            .expect("the standby's status")
            .is_none(),
        "the standby exited: {}",
        standby.printed()
    );
    assert!(holding.alive(), "{}", standby.printed());
    let (status, body) = standby.ready();
    assert_eq!(status, 503, "{body}");

    // Once the holder's transaction ends, the standby takes the lease at its next ask.
    holding.end().expect("the holder commits");
    let ended = Instant::now();
    assert!(
        standby
            .line_by(TOOK, ended + Duration::from_secs(3))
            .is_some(),
        "the standby did not take the lease within 3 s of the holder's end: {}",
        standby.printed()
    );
}
