//! Adversary of wave 32 U3 (`story:leader-exits-promptly-on-sigterm`): a leading `run` on
//! PostgreSQL, given SIGTERM, exits within the bound `run --help` names: `--drain` (counted only
//! while requests wait), plus the longest `timeout_ms` of an `[egress]` call, plus 5 s; a post
//! under way is finished and its cursor saved before the leases are released, so a standby
//! taking over posts nothing the old leader posted.
//!
//! The fixture `tests/fixtures/ingress` serves `board` and `ledger`. Every case reaches a
//! database, so each is ignored without `SPINDLE_TEST_DATABASE_URL`; `task check:postgres` runs
//! them. A webhook, where one is used, is a stub on 127.0.0.1 run by this test.

/// The PostgreSQL lane (`task check:postgres`).
mod postgres {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{SocketAddr, TcpListener, TcpStream};
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, ExitStatus, Stdio};
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::{Duration, Instant};

    use serde_json::{Value, json};

    use crate::egress::{BINDING, SPEC};

    /// The lease every case runs with, in seconds, as the story's case.
    const LEASE: &str = "2";
    const TOOK: &str = "took the lease of board";
    const STARTUP: Duration = Duration::from_secs(60);
    const ANSWER: Duration = Duration::from_secs(10);
    const OPENED: &str = "tasks.board.TaskOpened";
    /// The teardown allowance `run --help` names.
    const ALLOWANCE: Duration = Duration::from_secs(5);
    const SETTINGS: [&str; 6] = [
        "SPINDLE_SETTING_DATABASE_URL",
        "SPINDLE_SETTING_DATABASE_CA",
        "SPINDLE_SETTING_DATABASE_SCHEMA",
        "SPINDLE_SETTING_DATABASE_POOL",
        "SPINDLE_SETTING_DATABASE_CONNECTIONS",
        "SPINDLE_SETTING_DATABASE_LEASE",
    ];

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn owner_url() -> String {
        std::env::var("SPINDLE_TEST_DATABASE_URL")
            .expect("SPINDLE_TEST_DATABASE_URL names a loopback PostgreSQL (task check:postgres)")
    }

    /// Runs `statements` as the test database's owner, one after another on one connection.
    fn sql(statements: &[String]) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        runtime.block_on(async {
            let (client, connection) = tokio_postgres::connect(&owner_url(), tokio_postgres::NoTls)
                .await
                .expect("the test database accepts its owner");
            let driver = tokio::spawn(connection);
            for statement in statements {
                client
                    .batch_execute(statement)
                    .await
                    .unwrap_or_else(|error| panic!("{statement}: {error}"));
            }
            drop(client);
            let _ = driver.await;
        });
    }

    /// A schema of its own, created as the test database's owner.
    fn schema() -> String {
        let mut value = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("a clock after 1970")
            .as_nanos()
            ^ u128::from(std::process::id()) << 64;
        let mut letters = String::new();
        for _ in 0..12 {
            letters.push(char::from(
                b'a' + u8::try_from(value % 26).expect("below 26"),
            ));
            value /= 26;
        }
        let schema = format!("adv_w32_u3_{letters}");
        sql(&[format!("CREATE SCHEMA {schema}")]);
        schema
    }

    /// An environment document keeping both components' state in PostgreSQL, with no egress.
    fn plain_environment(directory: &Path) -> PathBuf {
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
        let mut command = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"));
        command
            .current_dir(root())
            .args(["run", SPEC, "--http", BINDING, "--environment"]);
        command.arg(environment);
        for variable in SETTINGS {
            command.env_remove(variable);
        }
        command
            .env_remove("SPINDLE_SETTING_CONSOLE")
            .env("SPINDLE_SETTING_DATABASE_URL", owner_url())
            .env("SPINDLE_SETTING_DATABASE_SCHEMA", schema)
            .env("SPINDLE_SETTING_DATABASE_LEASE", LEASE);
        command
    }

    fn call(address: SocketAddr, method: &str, path: &str, body: Option<&Value>) -> (u16, Value) {
        let mut stream =
            TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
        stream
            .set_read_timeout(Some(Duration::from_secs(40)))
            .expect("a read timeout");
        let body = body.map(Value::to_string).unwrap_or_default();
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\
             Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .expect("the request is written");
        let mut answer = String::new();
        let _ = stream.read_to_string(&mut answer);
        let (head, text) = answer.split_once("\r\n\r\n").unwrap_or((&answer, ""));
        let status = head
            .split(' ')
            .nth(1)
            .and_then(|status| status.parse().ok())
            .unwrap_or(0);
        (status, serde_json::from_str(text).unwrap_or(Value::Null))
    }

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

        fn ready_by(&self, deadline: Instant) {
            loop {
                let (status, body) = call(self.address, "GET", "/__spindle/ready", None);
                if status == 200 {
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "the run never read ready: {status} {body}: {}",
                    self.printed()
                );
                std::thread::sleep(Duration::from_millis(50));
            }
        }

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

        fn printed(&self) -> String {
            self.lines
                .lock()
                .expect("the lines")
                .iter()
                .map(|(_, line)| line.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        }

        fn terminate(&self) -> Instant {
            rustix::process::kill_process(
                rustix::process::Pid::from_child(&self.child),
                rustix::process::Signal::TERM,
            )
            .expect("the signal is sent");
            Instant::now()
        }

        /// Waits until the process exits, at most until `deadline`: its status and when the
        /// test read it, or `None` with the process still running.
        fn exited_by(&mut self, deadline: Instant) -> Option<(ExitStatus, Instant)> {
            loop {
                if let Some(status) = self.child.try_wait().expect("the run is waited on") {
                    return Some((status, Instant::now()));
                }
                if Instant::now() >= deadline {
                    return None;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        fn open(&self, id: &str) -> (u16, Value) {
            call(
                self.address,
                "PUT",
                &format!("/tasks/{id}"),
                Some(&json!({"title": format!("task {id}")})),
            )
        }
    }

    /// A transaction another session holds open with a transaction id, from [`Holder::opened`]
    /// until the instant [`Holder::end_at`] names, or for 13 s.
    struct Holder {
        opened: Instant,
        end: mpsc::Sender<Instant>,
        thread: std::thread::JoinHandle<()>,
    }

    impl Holder {
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

    /// `run --help`: a leading run exits within `--drain` (counted only while requests wait), the
    /// longest `timeout_ms` of an `[egress]` call and 5 s of the signal, "also while another
    /// session on the server holds a transaction open" (`CHANGELOG.md`). Here no request waits
    /// for the run's thread and there is no egress, so the bound is 5 s. A client reads
    /// `GET /api/events` (on by default on loopback) after the run committed t2 behind another
    /// session's open transaction: the read is answered beside the run's thread, waits in the
    /// feed for t2 (`LogStore::published`, up to 30 s), and the listener's close at exit waits for
    /// that read to end. The transaction is held until 1 s past the bound; the run exits within
    /// the bound, before it ends.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_leader_with_an_events_read_under_way_exits_within_the_bound_under_an_open_transaction() {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment = plain_environment(directory.path());
        let schema = schema();
        let mut leader = Replica::start(run(&environment, &schema));
        leader.ready_by(Instant::now() + ANSWER);
        let (status, answer) = leader.open("t1");
        assert_eq!(status, 201, "{answer}");

        let holder = Holder::open();
        let (status, answer) = leader.open("t2");
        assert_eq!(
            status, 201,
            "t2 commits behind the open transaction: {answer}"
        );
        let address = leader.address;
        let (answered, reading) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            let answer = call(address, "GET", "/api/events", None);
            let _ = answered.send((Instant::now(), answer.0));
        });
        let waiting = reading.recv_timeout(Duration::from_millis(500));
        assert!(
            waiting.is_err(),
            "the events read waits for t2, which the open transaction withholds: {waiting:?}"
        );

        let signalled = leader.terminate();
        let bound = ALLOWANCE;
        holder.end_at(signalled + bound + Duration::from_secs(1));
        let exit = leader.exited_by(signalled + Duration::from_secs(12));
        let ended = Instant::now();
        let read = reading.recv_timeout(Duration::from_secs(5)).ok();
        let _ = reader.join();
        let since_open = ended.saturating_duration_since(holder.opened);
        holder.join();
        let (status, exited) = exit.unwrap_or_else(|| {
            panic!(
                "the run had not exited 12 s after SIGTERM: {}",
                leader.printed()
            )
        });
        let took = exited.saturating_duration_since(signalled);
        eprintln!(
            "the leader exited {took:?} after SIGTERM; the events read answered {read:?}; the \
             transaction was open {since_open:?} at the check"
        );
        assert!(
            took <= bound,
            "the leader exited {took:?} after SIGTERM, past the {bound:?} `run --help` names, \
             held by an events read waiting for the feed (status {status:?}): {}",
            leader.printed()
        );
        assert_eq!(status.code(), Some(143), "{}", leader.printed());
    }

    /// One request a webhook was sent: its body, and when it arrived.
    struct Arrived {
        body: Value,
    }

    /// A webhook on 127.0.0.1 that answers every request `200` once `delay` has passed since it
    /// read it: each request is recorded as it arrives.
    struct SlowWebhook {
        address: SocketAddr,
        arrived: Arc<Mutex<Vec<Arrived>>>,
    }

    impl SlowWebhook {
        fn start(delay: Duration) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("the webhook binds");
            let address = listener.local_addr().expect("the webhook has an address");
            let arrived = Arc::new(Mutex::new(Vec::new()));
            let recorded = Arc::clone(&arrived);
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { return };
                    let recorded = Arc::clone(&recorded);
                    std::thread::spawn(move || slow_answer(stream, &recorded, delay));
                }
            });
            Self { address, arrived }
        }

        fn url(&self) -> String {
            format!("http://{}/events", self.address)
        }

        fn bodies(&self) -> Vec<Value> {
            self.arrived
                .lock()
                .expect("the record")
                .iter()
                .map(|arrived| arrived.body.clone())
                .collect()
        }
    }

    fn slow_answer(stream: TcpStream, recorded: &Mutex<Vec<Arrived>>, delay: Duration) {
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
        recorded.lock().expect("the record").push(Arrived {
            body: serde_json::from_slice(&body).unwrap_or(Value::Null),
        });
        std::thread::sleep(delay);
        let mut stream = reader.into_inner();
        let _ = write!(
            stream,
            "HTTP/1.1 200 Answer\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
    }

    /// D2 A and the story's safety fact: a post under way when SIGTERM arrives is finished, its
    /// cursor saved, and only then are the leases released, so the standby that takes over posts
    /// nothing the old leader posted. The webhook answers each post 1.5 s after it read it, within
    /// the call's `timeout_ms` of 2000; SIGTERM comes while t1's post waits for its answer. The
    /// run exits within 2 s + 5 s; t1 reaches the webhook once, from the old leader.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_post_under_way_at_sigterm_is_saved_before_the_standby_takes_over() {
        let webhook = SlowWebhook::start(Duration::from_millis(1500));
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment =
            crate::egress::environment(directory.path(), "postgres", &webhook.url(), &[OPENED]);
        let schema = schema();
        let mut leader = Replica::start(run(&environment, &schema));
        leader.ready_by(Instant::now() + ANSWER);
        let standby = Replica::start(run(&environment, &schema));
        let (status, answer) = leader.open("t1");
        assert_eq!(status, 201, "{answer}");
        // Another test of the lane may hold a transaction open, which delays the post.
        let deadline = Instant::now() + Duration::from_secs(20);
        while webhook.bodies().is_empty() {
            assert!(
                Instant::now() < deadline,
                "the leader never posted t1: {}",
                leader.printed()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let signalled = leader.terminate();
        let bound = Duration::from_secs(2) + ALLOWANCE;
        let (status, exited) = leader.exited_by(signalled + bound).unwrap_or_else(|| {
            panic!(
                "the leader did not exit within {bound:?} of SIGTERM: {}",
                leader.printed()
            )
        });
        assert_eq!(status.code(), Some(143), "{}", leader.printed());
        assert!(
            standby
                .line_by(TOOK, exited + Duration::from_secs(5))
                .is_some(),
            "the standby took no lease: {}",
            standby.printed()
        );
        standby.ready_by(exited + ANSWER);
        std::thread::sleep(Duration::from_secs(3));
        let bodies = webhook.bodies();
        let t1: Vec<&Value> = bodies
            .iter()
            .filter(|body| body["event"] == OPENED && body["fields"]["task_id"] == "t1")
            .collect();
        assert_eq!(
            t1.len(),
            1,
            "t1 was posted once, by the old leader, and not again by the new one: {bodies:?}\n\
             leader:\n{}\nstandby:\n{}",
            leader.printed(),
            standby.printed()
        );
    }

    /// Starts a read of `target` on `address` on a thread: its status and when it answered.
    fn read_beside(
        address: SocketAddr,
        target: &'static str,
    ) -> (
        mpsc::Receiver<(Instant, u16, Value)>,
        std::thread::JoinHandle<()>,
    ) {
        let (answered, answer) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let (status, body) = call(address, "GET", target, None);
            let _ = answered.send((Instant::now(), status, body));
        });
        (answer, thread)
    }

    /// Round 2: a run standing by that takes the leases leads through the listener it stood by
    /// on ([`spindle_ingress::Listener::lead`]), whose reader the fix also stops by the signal's
    /// flag. Once it leads and writes t2 behind another session's open transaction, a
    /// `GET /api/events` under way when SIGTERM comes does not hold its exit past the 5 s bound
    /// (no egress, no request waiting), and is answered `503`.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_standby_that_leads_exits_within_the_bound_with_an_events_read_under_way() {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment = plain_environment(directory.path());
        let schema = schema();
        let mut leader = Replica::start(run(&environment, &schema));
        leader.ready_by(Instant::now() + ANSWER);
        let mut standby = Replica::start(run(&environment, &schema));
        let (status, answer) = leader.open("t1");
        assert_eq!(status, 201, "{answer}");
        let _ = leader.child.kill();
        let _ = leader.child.wait();
        assert!(
            standby
                .line_by(TOOK, Instant::now() + Duration::from_secs(10))
                .is_some(),
            "the standby took no lease: {}",
            standby.printed()
        );
        standby.ready_by(Instant::now() + ANSWER);

        let holder = Holder::open();
        let (status, answer) = standby.open("t2");
        assert_eq!(status, 201, "the new leader writes t2: {answer}");
        let (answer, reader) = read_beside(standby.address, "/api/events");
        let early = answer.recv_timeout(Duration::from_millis(500));
        assert!(
            early.is_err(),
            "the events read waits for t2, which the open transaction withholds: {early:?}"
        );
        let signalled = standby.terminate();
        holder.end_at(signalled + ALLOWANCE + Duration::from_secs(1));
        let exit = standby.exited_by(signalled + Duration::from_secs(12));
        let read = answer.recv_timeout(Duration::from_secs(5)).ok();
        let _ = reader.join();
        holder.join();
        let (status, exited) = exit.unwrap_or_else(|| {
            panic!(
                "the new leader had not exited 12 s after SIGTERM: {}",
                standby.printed()
            )
        });
        let took = exited.saturating_duration_since(signalled);
        eprintln!("the new leader exited {took:?} after SIGTERM; the read answered {read:?}");
        assert!(
            took <= ALLOWANCE,
            "the new leader exited {took:?} after SIGTERM, past {ALLOWANCE:?}: {}",
            standby.printed()
        );
        assert_eq!(status.code(), Some(143), "{}", standby.printed());
        assert_eq!(
            read.map(|(_, status, _)| status),
            Some(503),
            "the read under way is given up and answered 503"
        );
    }

    /// Round 2, the remaining waits for a read slot: with `database_pool` 2 the board's reading
    /// handle runs one read at a time. An events read waiting for the feed behind another
    /// session's open transaction holds that slot, and a view read (`GET /tasks`) waits behind it
    /// for its turn. Once SIGTERM comes the events read gives its slot up, the view read scans,
    /// and the run exits within the 5 s bound.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_view_read_waiting_for_a_slot_behind_an_events_read_does_not_hold_the_exit() {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment = plain_environment(directory.path());
        let schema = schema();
        let mut command = run(&environment, &schema);
        command.env("SPINDLE_SETTING_DATABASE_POOL", "2");
        let mut leader = Replica::start(command);
        leader.ready_by(Instant::now() + ANSWER);
        let (status, answer) = leader.open("t1");
        assert_eq!(status, 201, "{answer}");

        let holder = Holder::open();
        let (status, answer) = leader.open("t2");
        assert_eq!(
            status, 201,
            "t2 commits behind the open transaction: {answer}"
        );
        let (events, events_thread) = read_beside(leader.address, "/api/events");
        std::thread::sleep(Duration::from_millis(300));
        let (view, view_thread) = read_beside(leader.address, "/tasks");
        let early = view.recv_timeout(Duration::from_millis(500));
        assert!(
            early.is_err(),
            "the view read waits for the slot the events read holds: {early:?}"
        );
        let signalled = leader.terminate();
        holder.end_at(signalled + ALLOWANCE + Duration::from_secs(1));
        let exit = leader.exited_by(signalled + Duration::from_secs(12));
        let viewed = view.recv_timeout(Duration::from_secs(5)).ok();
        let read = events.recv_timeout(Duration::from_secs(5)).ok();
        let _ = view_thread.join();
        let _ = events_thread.join();
        holder.join();
        let (status, exited) = exit.unwrap_or_else(|| {
            panic!(
                "the run had not exited 12 s after SIGTERM: {}",
                leader.printed()
            )
        });
        let took = exited.saturating_duration_since(signalled);
        eprintln!(
            "the leader exited {took:?} after SIGTERM; the view answered {:?} {:?}; the events \
             read {:?}",
            viewed
                .as_ref()
                .map(|(at, _, _)| at.saturating_duration_since(signalled)),
            viewed.as_ref().map(|(_, status, body)| (status, body)),
            read.as_ref().map(|(_, status, _)| status)
        );
        assert!(
            took <= ALLOWANCE,
            "the leader exited {took:?} after SIGTERM, past {ALLOWANCE:?}: {}",
            leader.printed()
        );
        assert_eq!(status.code(), Some(143), "{}", leader.printed());
    }
}
