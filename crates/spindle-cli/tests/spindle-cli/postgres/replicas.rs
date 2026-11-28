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

/// The bound a leader exits within after SIGTERM (`run --help`): `--drain`, 20 s by default but
/// counted only while requests wait, so 0 s here; the egress call's `timeout_ms`, 2000 in the
/// egress tests' environment; and the teardown allowance of 5 s.
const EXIT_BOUND: Duration = Duration::from_secs(2 + 5);

/// The longest pause between two posts of one event by an egress lane (`BACKOFF_CAP`): the most
/// the signal may come after the older transaction opened, so that it is held at most 13 s.
const RETRY_CAP: Duration = Duration::from_secs(5);

/// `story:leader-exits-promptly-on-sigterm`: a leader whose egress lane waits for the feed, which
/// withholds the cursor the leader saved behind a transaction another session opened before it,
/// exits within [`EXIT_BOUND`] of SIGTERM while that transaction is still open, the cursor saved
/// and its leases released: the standby takes over at once and does not post t1 again.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_leader_exits_within_the_bound_under_an_open_transaction() {
    use super::super::egress::Webhook;

    const OPENED: &str = "tasks.board.TaskOpened";
    let webhook = Webhook::start(500);
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment =
        super::super::egress::environment(directory.path(), "postgres", &webhook.url(), &[OPENED]);
    let schema = schema();
    let (leader, standby) = leader_and_standby(&environment, &schema);
    let (status, answer) = leader.open("t1");
    assert_eq!(status, 201, "{answer}");
    let deadline = Instant::now() + Duration::from_secs(30);
    while webhook.received().is_empty() {
        assert!(Instant::now() < deadline, "the leader never posted t1");
        std::thread::sleep(Duration::from_millis(20));
    }

    // T0: another session opens a transaction with an id, so the feed withholds every write
    // committed after it until it ends.
    let holder = Holder::open();
    webhook.answer_with(200);
    accepted(&webhook, 1);
    // The leader saved its cursor after T0, and its lane reads the feed again, waiting for it.
    let saved = Instant::now() + Duration::from_secs(5);
    while super::sql(&[format!(
        "SELECT count(*) FROM {schema}.spindle_board_events WHERE event_name = 'spindle.egress'"
    )])[0]
        == "0"
    {
        assert!(Instant::now() < saved, "the leader never saved its cursor");
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(300));

    let signalled = Instant::now();
    let since = signalled.saturating_duration_since(holder.opened);
    assert!(
        since <= RETRY_CAP,
        "the signal came {since:?} after the older transaction opened, past {RETRY_CAP:?}"
    );
    holder.end_at(signalled + EXIT_BOUND + Duration::from_secs(1));
    let mut leader = leader;
    leader.signal(rustix::process::Signal::TERM);
    let (status, exited) = exit_status_within(&mut leader, signalled + EXIT_BOUND);
    eprintln!(
        "the leader exited {:?} after SIGTERM",
        exited.saturating_duration_since(signalled)
    );
    assert_eq!(status.code(), Some(143), "{}", leader.printed());

    let took = standby
        .line_by(TOOK, exited + Duration::from_millis(1_334))
        .unwrap_or_else(|| {
            panic!(
                "the standby did not take the released lease within 1.33 s of the exit: {}",
                standby.printed()
            )
        });
    eprintln!(
        "the standby took the lease {:?} after the exit",
        took.saturating_duration_since(exited)
    );
    standby.ready_by(exited + Duration::from_secs(10));
    std::thread::sleep(Duration::from_secs(3));
    holder.join();

    let received = webhook.received();
    let printed = format!("{received:?}");
    assert!(
        received.iter().all(
            |posted| posted.body["event"] == OPENED && posted.body["fields"]["task_id"] == "t1"
        ),
        "only t1 was posted: {printed}"
    );
    let statuses: Vec<u16> = received.iter().map(|posted| posted.status).collect();
    assert_eq!(statuses.first(), Some(&500), "{printed}");
    assert_eq!(
        statuses.iter().filter(|status| **status == 200).count(),
        1,
        "t1 was accepted once: {printed}"
    );
    assert_eq!(
        statuses.last(),
        Some(&200),
        "the old leader's accepted post was the last; the new leader posted nothing: {printed}"
    );
}

/// Waits until `replica` exits, at most until `deadline`: its status, and the instant the test
/// read it.
fn exit_status_within(replica: &mut Replica, deadline: Instant) -> (ExitStatus, Instant) {
    loop {
        if let Some(status) = replica.child.try_wait().expect("the run is waited on") {
            return (status, Instant::now());
        }
        assert!(
            Instant::now() < deadline,
            "the run did not exit within the bound: {}",
            replica.printed()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A transaction another session holds open with a transaction id, from [`Holder::opened`] until
/// the instant [`Holder::end_at`] names, or for 13 s.
struct Holder {
    opened: Instant,
    end: mpsc::Sender<Instant>,
    thread: std::thread::JoinHandle<()>,
}

impl Holder {
    /// The longest the transaction is held.
    const MOST: Duration = Duration::from_secs(13);

    fn open() -> Self {
        let url = owner_url();
        let (opened, open) = mpsc::channel();
        let (end, ending) = mpsc::channel::<Instant>();
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a runtime");
            let (mut client, connection) = runtime
                .block_on(tokio_postgres::connect(&url, tokio_postgres::NoTls))
                .expect("a connection of the test's own");
            let driver = runtime.spawn(connection);
            let transaction = runtime
                .block_on(client.transaction())
                .expect("a transaction");
            runtime
                .block_on(transaction.simple_query("SELECT pg_current_xact_id()"))
                .expect("the transaction takes an id");
            let began = Instant::now();
            opened.send(began).expect("the test waits");
            let most = began + Self::MOST;
            let until = ending
                .recv_timeout(Self::MOST)
                .map_or(most, |until| until.min(most));
            std::thread::sleep(until.saturating_duration_since(Instant::now()));
            runtime
                .block_on(transaction.commit())
                .expect("the transaction ends");
            drop(client);
            let _ = runtime.block_on(driver);
        });
        let opened = open.recv().expect("the transaction is open");
        Self {
            opened,
            end,
            thread,
        }
    }

    fn end_at(&self, until: Instant) {
        let _ = self.end.send(until);
    }

    fn join(self) {
        self.thread.join().expect("the transaction ends");
    }
}

/// Whether a database session writing to `schema`'s tables is inside a write transaction, between
/// two of its statements, holding what it wrote or locked (it has a transaction id), for at least
/// 150 ms. The session is found by the locks it holds on the tables of `schema`, which only the
/// leader writes in a case, so the lookup holds wherever the database runs, a cluster on this host
/// or a container behind a port mapping, which shows the server other client ports than this
/// host's sockets have.
fn inside_a_transaction(schema: &str) -> bool {
    super::sql(&[format!(
        "SELECT count(*) FROM pg_stat_activity activity WHERE activity.state = \
         'idle in transaction' AND activity.backend_xid IS NOT NULL \
         AND activity.xact_start < now() - interval '150 ms' \
         AND EXISTS (SELECT 1 FROM pg_locks held JOIN pg_class class ON class.oid = held.relation \
         JOIN pg_namespace space ON space.oid = class.relnamespace \
         WHERE held.pid = activity.pid AND space.nspname = '{schema}')"
    )])[0]
        != "0"
}

/// A leader paused inside a write keeps its lease row locked until the store's idle timeout of
/// 10 s ends its transaction, so the standby takes over by the lease's end, those 10 s and one
/// ask of the standby (an ask waits up to a third of the lease, and the next comes a third later).
const PAUSED_TAKEOVER: Duration = Duration::from_millis(2_000 + 10_000 + 1_334);

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_leader_paused_inside_a_write_is_taken_over_whole_within_the_lease_and_the_idle_timeout() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let schema = schema();
    let (mut leader, standby) = leader_and_standby(&environment, &schema);
    let (status, answer) = leader.open("t1");
    assert_eq!(status, 201, "{answer}");

    // Commands reach the leader one after another, so it is inside a write most of the time; it is
    // paused until a pause lands inside one, between two of its statements. Each command waits
    // for its answer, also across the pause, so every answer is the one the leader gave.
    let address = leader.address;
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stopped = Arc::clone(&stop);
    let firing = std::thread::spawn(move || {
        let mut answers = Vec::new();
        let mut index = 0;
        while !stopped.load(std::sync::atomic::Ordering::SeqCst) {
            index += 1;
            let Ok(mut stream) = TcpStream::connect_timeout(&address, ANSWER) else {
                break;
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
            let body = json!({"title": "fired"}).to_string();
            let _ = write!(
                stream,
                "PUT /tasks/f{index} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\
                 Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            let mut answer = String::new();
            let _ = stream.read_to_string(&mut answer);
            let status = answer
                .split_whitespace()
                .nth(1)
                .and_then(|status| status.parse::<u16>().ok())
                .unwrap_or(0);
            answers.push((format!("f{index}"), status));
        }
        answers
    });
    let mut inside = false;
    for _ in 0..60 {
        leader.signal(rustix::process::Signal::STOP);
        std::thread::sleep(Duration::from_millis(200));
        if inside_a_transaction(&schema) {
            inside = true;
            break;
        }
        leader.signal(rustix::process::Signal::CONT);
        std::thread::sleep(Duration::from_millis(20));
    }
    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    if !inside {
        leader.signal(rustix::process::Signal::CONT);
        let _ = firing.join();
        panic!("no pause landed inside a write of the leader");
    }
    let paused = Instant::now() - Duration::from_millis(200);

    let took = standby
        .line_by(TOOK, paused + PAUSED_TAKEOVER)
        .unwrap_or_else(|| {
            panic!(
                "the standby did not take over within {PAUSED_TAKEOVER:?} of a pause inside a \
                 write: {}",
                standby.printed()
            )
        });
    eprintln!("took over {:?} after the pause", took - paused);
    standby.ready_by(Instant::now() + ANSWER);
    let (status, answer) = standby.open("t2");
    assert_eq!(status, 201, "the new leader writes: {answer}");

    // Once resumed the old leader lands nothing more and exits 1; every command it answered `201`
    // is held, none it answered otherwise, and none is written twice.
    leader.signal(rustix::process::Signal::CONT);
    let code = leader.exit_within(ANSWER).code();
    let answers = firing.join().expect("the commands were sent");
    let tasks = standby.tasks();
    let held: Vec<String> = tasks["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .filter_map(|row| row["task_id"].as_str().map(str::to_owned))
        .collect();
    let printed = format!(
        "answers {answers:?}; held {held:?}; old leader exited {code:?}\nleader:\n{}",
        leader.printed()
    );
    assert_eq!(code, Some(1), "{printed}");
    for (id, status) in &answers {
        assert_eq!(
            *status == 201,
            held.contains(id),
            "{id} answered {status}: {printed}"
        );
    }
    assert!(
        held.contains(&"t1".to_owned()) && held.contains(&"t2".to_owned()),
        "{printed}"
    );
    let opened = super::sql(&[format!(
        "SELECT count(*) FROM {schema}.spindle_board_events WHERE event_name = \
         'tasks.board.TaskOpened'"
    )])[0]
        .clone();
    assert_eq!(
        opened,
        held.len().to_string(),
        "one TaskOpened for each task held, none written twice: {printed}"
    );
}

// `story:seed-under-a-lease`: a run given `database_lease` decides its seed from the row `seed` of
// the lease table of its first component in name order, whatever lease token it took, as
// `seed.json` decides it on a `--state` directory.

/// A seed file of `tasks.board.OpenTask` entries, one per task id of `tasks`, written into
/// `directory` as `name`.
fn seed_file(directory: &Path, name: &str, tasks: &[&str]) -> PathBuf {
    let entries: Vec<Value> = tasks
        .iter()
        .map(|task| {
            json!({
                "command": "tasks.board.OpenTask",
                "input": {"task_id": task, "title": format!("seeded {task}")},
                "request_id": format!("seed-{task}"),
            })
        })
        .collect();
    let path = directory.join(name);
    std::fs::write(&path, Value::Array(entries).to_string()).expect("the seed is written");
    path
}

/// [`run`] with `--seed seed`.
fn seeded(environment: &Path, schema: &str, seed: &Path) -> Command {
    let mut command = run(environment, schema);
    command.arg("--seed").arg(seed);
    command
}

/// The body of the row `seed` of `component`'s lease table in `schema`, read as the database's
/// owner; `None` where there is none.
fn recorded(schema: &str, component: &str) -> Option<Value> {
    super::sql(&[format!(
        "SELECT body::text FROM {schema}.{}_p_lease WHERE row_key = 'seed'",
        spindle_kernel::postgres_prefix(component)
    )])
    .first()
    .map(|text| serde_json::from_str(text).expect("the seed row holds JSON"))
}

/// The task ids `GET /tasks` answers at `replica`, in order.
fn task_ids(replica: &Replica) -> Vec<String> {
    replica.tasks()["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .filter_map(|row| row["task_id"].as_str().map(str::to_owned))
        .collect()
}

impl Replica {
    /// The first stderr line holding `text`, waiting until `deadline`.
    fn line(&self, text: &str, deadline: Instant) -> Option<String> {
        self.line_by(text, deadline)?;
        self.lines
            .lock()
            .expect("the lines")
            .iter()
            .find(|(_, line)| line.contains(text))
            .map(|(_, line)| line.clone())
    }

    /// Whether a stderr line holds `text`.
    fn has_line(&self, text: &str) -> bool {
        self.line_by(text, Instant::now()).is_some()
    }

    /// Sends SIGTERM and waits for the exit 143 within [`ANSWER`].
    fn stop(&mut self) {
        self.signal(rustix::process::Signal::TERM);
        assert_eq!(
            self.exit_within(ANSWER).code(),
            Some(143),
            "{}",
            self.printed()
        );
    }
}

/// Asserts the line `replica` printed for a seed it skips on the record names the store, by the
/// row `seed` of `schema`'s lease table, and no lease token, and that it sent no entry.
fn skipped_on_the_record(replica: &Replica, schema: &str) {
    let line = replica
        .line("already ran", Instant::now() + ANSWER)
        .unwrap_or_else(|| panic!("no line says the seed already ran: {}", replica.printed()));
    assert!(
        line.contains("the PostgreSQL store")
            && line.contains(&format!("{schema}.spindle_board_p_lease"))
            && line.contains("no entry is sent"),
        "the line names the store's record: {line}"
    );
    assert!(!line.contains("token"), "the line names no token: {line}");
    assert!(
        !replica.has_line("ended:"),
        "no entry is sent: {}",
        replica.printed()
    );
}

/// The leases of board and ledger were taken at token 1 and released, as a process that exited
/// 3 on a misfit leaves them: the next run leads at token 2 and sends the whole seed.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_seed_after_a_lease_taken_without_seeding_runs() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let seed = seed_file(directory.path(), "seed.json", &["s1", "s2"]);
    let schema = schema();
    let mut database = spindle_kernel::Database::new(owner_url());
    database.schema.clone_from(&schema);
    let mut lease = spindle_kernel::Lease::connect(&database, Duration::from_secs(2))
        .expect("the test connects for the leases");
    for component in ["board", "ledger"] {
        let token = lease
            .take(component)
            .expect("the lease is read")
            .expect("the lease is free");
        assert_eq!(token, 1);
        lease
            .release(component, token)
            .expect("the lease is released");
    }
    drop(lease);

    let leader = Replica::start(seeded(&environment, &schema, &seed));
    assert!(
        leader
            .line_by("ended: 2 sent, 2 applied", Instant::now() + ANSWER)
            .is_some(),
        "{}",
        leader.printed()
    );
    assert_eq!(
        super::sql(&[format!(
            "SELECT body->>'token' FROM {schema}.spindle_board_p_lease WHERE row_key = 'lease'"
        )]),
        ["2"],
        "the run leads at token 2"
    );
    leader.ready_by(Instant::now() + ANSWER);
    assert_eq!(task_ids(&leader), ["s1", "s2"], "each seeded row once");
    let record = recorded(&schema, "board").expect("the seed is recorded");
    assert_eq!(record["done"], 2, "{record}");
}

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_restart_after_a_completed_seed_skips_it() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let seed = seed_file(directory.path(), "seed.json", &["s1"]);
    let schema = schema();
    let mut first = Replica::start(seeded(&environment, &schema, &seed));
    assert!(
        first
            .line_by("ended: 1 sent, 1 applied", Instant::now() + ANSWER)
            .is_some(),
        "{}",
        first.printed()
    );
    first.stop();

    let second = Replica::start(seeded(&environment, &schema, &seed));
    let started = Instant::now();
    second.ready_by(started + Duration::from_secs(10));
    skipped_on_the_record(&second, &schema);
    assert_eq!(task_ids(&second), ["s1"]);
}

/// D7 A: a store with rows and a complete record of one file, and a restart with another.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn another_seed_file_is_refused_before_serving() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let first_file = seed_file(directory.path(), "first.json", &["s1"]);
    let other_file = seed_file(directory.path(), "other.json", &["s1", "s2"]);
    let schema = schema();
    let mut first = Replica::start(seeded(&environment, &schema, &first_file));
    assert!(
        first
            .line_by("ended: 1 sent, 1 applied", Instant::now() + ANSWER)
            .is_some(),
        "{}",
        first.printed()
    );
    first.stop();
    let record = recorded(&schema, "board").expect("the seed is recorded");
    let before = counts(&schema);

    let refused = super::refused(seeded(&environment, &schema, &other_file));
    assert_eq!(refused.status.code(), Some(1), "{}", refused.stderr);
    let digest = record["digest"].as_str().expect("a digest");
    assert!(
        refused.stderr.contains(digest) && refused.stderr.matches("sha256:").count() >= 2,
        "the refusal names both digests: {}",
        refused.stderr
    );
    assert!(!refused.stderr.contains("ended:"), "{}", refused.stderr);
    assert_eq!(counts(&schema), before, "the rows are as they were");
    assert_eq!(
        recorded(&schema, "board"),
        Some(record),
        "the record is as it was"
    );
}

/// D2 A: a store with no row and a record of one file, as a reset with an empty world leaves it,
/// and a restart with another file.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn another_seed_file_on_an_empty_store_seeds_from_the_first_entry() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let first_file = seed_file(directory.path(), "first.json", &["s1"]);
    let other_file = seed_file(directory.path(), "other.json", &["s1", "s2"]);
    let schema = schema();
    let mut first = Replica::start(seeded(&environment, &schema, &first_file));
    assert!(
        first
            .line_by("ended: 1 sent, 1 applied", Instant::now() + ANSWER)
            .is_some(),
        "{}",
        first.printed()
    );
    let (status, body) = call(
        first.address,
        "POST",
        "/api/reset",
        Some(&json!({"rows": []})),
    );
    assert_eq!(status, 200, "{body}");
    assert!(task_ids(&first).is_empty());
    first.stop();
    let record = recorded(&schema, "board").expect("the seed is recorded");
    assert_eq!(record["done"], 1, "{record}");

    let second = Replica::start(seeded(&environment, &schema, &other_file));
    assert!(
        second
            .line_by("ended: 2 sent, 2 applied", Instant::now() + ANSWER)
            .is_some(),
        "{}",
        second.printed()
    );
    let now = recorded(&schema, "board").expect("the seed is recorded");
    assert_ne!(now["digest"], record["digest"], "{now}");
    assert_eq!(now["done"], 2, "{now}");
    assert_eq!(task_ids(&second), ["s1", "s2"]);
}

#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_standby_that_takes_over_after_the_seed_skips_it() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let seed = seed_file(directory.path(), "seed.json", &["s1"]);
    let schema = schema();
    let mut leader = Replica::start(seeded(&environment, &schema, &seed));
    assert!(
        leader
            .line_by("ended: 1 sent, 1 applied", Instant::now() + ANSWER)
            .is_some(),
        "{}",
        leader.printed()
    );
    leader.ready_by(Instant::now() + ANSWER);
    let standby = Replica::start(seeded(&environment, &schema, &seed));
    let (status, body) = standby.ready();
    assert_eq!(status, 503, "{body}");

    let killed = leader.kill();
    standby
        .line_by(TOOK, killed + Duration::from_secs(5))
        .unwrap_or_else(|| panic!("the standby never took over: {}", standby.printed()));
    standby.ready_by(killed + Duration::from_secs(10));
    skipped_on_the_record(&standby, &schema);
    assert_eq!(task_ids(&standby), ["s1"]);
}

/// The fixture `tests/fixtures/seed`: `gate`, whose `VouchFor` asks an `external:` cause.
const VAULT: &str = "crates/spindle-cli/tests/fixtures/seed";

/// An environment document keeping `gate`'s state in PostgreSQL, `VouchFor`'s cause answered by
/// the desk at `desk`, written into `directory`.
fn vault_environment(directory: &Path, desk: SocketAddr) -> PathBuf {
    let text = format!(
        "format = \"spindle-environment/1\"\nname = \"vault\"\nos = \"linux\"\n\
         arch = \"x86_64\"\n\n[provides]\n\"store.durable\" = [\"postgres\"]\n\
         \"store.ephemeral\" = [\"memory\"]\n\"identity.mint\" = [\"uuid-v4\"]\n\
         clock = [\"system\"]\ntimer = [\"tokio\"]\nchannel = [\"in-process\"]\n\
         projection = [\"in-process\"]\n\"ingress.network\" = [\"http-loopback\"]\n\
         \"external.*\" = [\"operator-prompt\"]\n\
         \"external.vault.access.VouchFor.vouched\" = [\"desk\"]\n\n[http.desk]\n\
         method = \"POST\"\nurl = \"http://{desk}/vouch\"\ntimeout_ms = 30000\n"
    );
    let path = directory.join("vault.toml");
    std::fs::write(&path, text).expect("the environment is written");
    path
}

/// `run` on [`VAULT`] with `environment` and `--seed seed`, its state in `schema`, a lease of
/// [`LEASE`].
fn vault_seeded(environment: &Path, schema: &str, seed: &Path) -> Command {
    let url = owner_url();
    let environment = environment.to_str().expect("a UTF-8 path");
    let seed = seed.to_str().expect("a UTF-8 path");
    spindle(
        &["run", VAULT, "--environment", environment, "--seed", seed],
        &[
            ("database_url", url.as_str()),
            ("database_schema", schema),
            ("database_lease", LEASE),
        ],
    )
}

/// A seed of `vault.access.GrantRole`s, `(role, request id)`, and `vault.access.VouchFor`s,
/// `(voucher, request id)` with `vouch` set, in that order of `entries`, written into `directory`.
fn vault_seed(directory: &Path, entries: &[(&str, &str, bool)]) -> PathBuf {
    let entries: Vec<Value> = entries
        .iter()
        .map(|&(identity, request, vouch)| {
            if vouch {
                json!({
                    "command": "vault.access.VouchFor",
                    "input": {"voucher_id": identity, "role_id": "admin"},
                    "actor": "vault.access.Visitor",
                    "request_id": request,
                })
            } else {
                json!({
                    "command": "vault.access.GrantRole",
                    "input": {"role_id": identity, "title": format!("{identity} title")},
                    "actor": "vault.access.Operator",
                    "caller": {"tenant": "north"},
                    "request_id": request,
                })
            }
        })
        .collect();
    let path = directory.join("vault-seed.json");
    std::fs::write(&path, Value::Array(entries).to_string()).expect("the seed is written");
    path
}

/// A desk on 127.0.0.1 answering every vouch request `{"holds": true}`, one at a time, each only
/// once `release` is sent; `asked` is sent as each request arrives.
fn holding_desk() -> (SocketAddr, mpsc::Receiver<()>, mpsc::Sender<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("the desk binds");
    let address = listener.local_addr().expect("the desk has an address");
    let (asked, arrived) = mpsc::channel();
    let (release, released) = mpsc::channel::<()>();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { return };
            let _ = stream.set_read_timeout(Some(ANSWER));
            let mut reader = BufReader::new(stream);
            let mut length = 0;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).is_err() {
                    return;
                }
                let header = header.trim_end().to_ascii_lowercase();
                if header.is_empty() {
                    break;
                }
                if let Some(value) = header.strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0; length];
            if reader.read_exact(&mut body).is_err() {
                return;
            }
            if asked.send(()).is_err() || released.recv_timeout(STARTUP).is_err() {
                return;
            }
            let body = json!({"holds": true}).to_string();
            let _ = write!(
                reader.into_inner(),
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (address, arrived, release)
}

/// A leader killed while the desk holds entry 2 of 3 has recorded entry 1 done; the next process
/// sends entries 2 and 3 alone.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_killed_leader_resumes_the_seed_after_its_count() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let (desk, asked, release) = holding_desk();
    let environment = vault_environment(directory.path(), desk);
    let seed = vault_seed(
        directory.path(),
        &[
            ("admin", "seed-1", false),
            ("v-1", "seed-2", true),
            ("auditor", "seed-3", false),
        ],
    );
    let schema = schema();
    let mut leader = Replica::start(vault_seeded(&environment, &schema, &seed));
    asked
        .recv_timeout(ANSWER)
        .unwrap_or_else(|_| panic!("entry 2 never asked the desk: {}", leader.printed()));
    let record = recorded(&schema, "gate").expect("the seed is recorded");
    assert_eq!(record["done"], 1, "entry 1 is done: {record}");
    leader.kill();
    // The desk answers the killed leader's ask into a closed connection, then takes the next.
    release.send(()).expect("the desk is released");

    let next = Replica::start(vault_seeded(&environment, &schema, &seed));
    asked
        .recv_timeout(ANSWER)
        .unwrap_or_else(|_| panic!("entry 2 never asked the desk again: {}", next.printed()));
    release.send(()).expect("the desk is released");
    let ended = next
        .line("ended:", Instant::now() + ANSWER)
        .unwrap_or_else(|| panic!("the seed never ended: {}", next.printed()));
    assert!(
        ended.contains("ended: 2 sent, 2 applied, 0 already there, 0 refused, 0 rejected"),
        "entries 2 and 3 alone: {ended}"
    );
    let record = recorded(&schema, "gate").expect("the seed is recorded");
    assert_eq!(record["done"], 3, "{record}");
}

/// D5 B: a store with rows and no record, as 0.26.0 or `state move` leaves it.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_store_seeded_without_a_record_skips_and_records() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let seed = seed_file(directory.path(), "seed.json", &["s1"]);
    let schema = schema();
    let mut unseeded = Replica::start(run(&environment, &schema));
    unseeded.ready_by(Instant::now() + ANSWER);
    let (status, answer) = unseeded.open("t1");
    assert_eq!(status, 201, "{answer}");
    unseeded.stop();
    assert_eq!(recorded(&schema, "board"), None);

    let mut first = Replica::start(seeded(&environment, &schema, &seed));
    first.ready_by(Instant::now() + ANSWER);
    let line = first
        .line("records no seed", Instant::now() + ANSWER)
        .unwrap_or_else(|| {
            panic!(
                "no line says the store records no seed: {}",
                first.printed()
            )
        });
    assert!(line.contains("no entry is sent"), "{line}");
    assert_eq!(
        first
            .printed()
            .lines()
            .filter(|line| line.contains("no entry is sent"))
            .count(),
        1,
        "one line: {}",
        first.printed()
    );
    assert!(!first.has_line("ended:"), "{}", first.printed());
    assert_eq!(task_ids(&first), ["t1"], "no entry is sent");
    let record = recorded(&schema, "board").expect("the seed is recorded");
    assert_eq!(record["done"], 1, "{record}");
    assert_eq!(record["rejected"], json!([]), "{record}");
    assert!(
        record["digest"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:")),
        "{record}"
    );
    first.stop();

    let second = Replica::start(seeded(&environment, &schema, &seed));
    second.ready_by(Instant::now() + ANSWER);
    skipped_on_the_record(&second, &schema);
}

/// D6 A: a reset with no world records 0 entries done before it sends the first, and every one
/// once the seed ended; a reset with a world records every entry done and sends none.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_reset_of_a_leased_run_rewrites_the_record() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let (desk, asked, release) = holding_desk();
    let environment = vault_environment(directory.path(), desk);
    let seed = vault_seed(
        directory.path(),
        &[("v-1", "seed-1", true), ("admin", "seed-2", false)],
    );
    let schema = schema();
    let leader = Replica::start(vault_seeded(&environment, &schema, &seed));
    asked
        .recv_timeout(ANSWER)
        .unwrap_or_else(|_| panic!("the start's seed never asked: {}", leader.printed()));
    release.send(()).expect("the desk is released");
    assert!(
        leader
            .line_by("ended: 2 sent, 2 applied", Instant::now() + ANSWER)
            .is_some(),
        "{}",
        leader.printed()
    );
    assert_eq!(recorded(&schema, "gate").expect("a record")["done"], 2);

    let address = leader.address;
    let reset = std::thread::spawn(move || call(address, "POST", "/api/reset", None));
    asked
        .recv_timeout(ANSWER)
        .unwrap_or_else(|_| panic!("the reset's seed never asked: {}", leader.printed()));
    let before = recorded(&schema, "gate").expect("a record");
    assert_eq!(before["done"], 0, "0 done before the first entry: {before}");
    release.send(()).expect("the desk is released");
    let (status, body) = reset.join().expect("the reset is answered");
    assert_eq!(status, 200, "{body}");
    let after = recorded(&schema, "gate").expect("a record");
    assert_eq!(after["done"], 2, "every entry done once it ended: {after}");

    let (status, body) = call(address, "POST", "/api/reset", Some(&json!({"rows": []})));
    assert_eq!(status, 200, "{body}");
    let loaded = recorded(&schema, "gate").expect("a record");
    assert_eq!(
        loaded["done"], 2,
        "a world records every entry done: {loaded}"
    );
    assert_eq!(loaded["rejected"], json!([]), "{loaded}");
    assert!(
        asked.recv_timeout(Duration::from_millis(300)).is_err(),
        "a reset with a world sends no entry"
    );
    let ends = leader
        .printed()
        .lines()
        .filter(|line| line.contains("ended:"))
        .count();
    assert_eq!(ends, 2, "{}", leader.printed());
}

/// D6 A: a leased run without `--seed` on a store with a record.
#[test]
#[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
fn a_reset_by_a_run_without_a_seed_deletes_the_record() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let environment = environment(directory.path());
    let seed = seed_file(directory.path(), "seed.json", &["s1"]);
    let schema = schema();
    let mut first = Replica::start(seeded(&environment, &schema, &seed));
    assert!(
        first
            .line_by("ended: 1 sent, 1 applied", Instant::now() + ANSWER)
            .is_some(),
        "{}",
        first.printed()
    );
    first.stop();
    assert!(recorded(&schema, "board").is_some(), "the seed is recorded");

    let unseeded = Replica::start(run(&environment, &schema));
    unseeded.ready_by(Instant::now() + ANSWER);
    let (status, body) = call(unseeded.address, "POST", "/api/reset", None);
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        recorded(&schema, "board"),
        None,
        "the reset removed the record"
    );
}
