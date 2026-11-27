//! Adversary of wave 31 U6 (`story:several-writers-one-postgres`): several served `run`
//! processes of one specification on one PostgreSQL store, each given `database_lease`, read
//! against the story's own words and the documents the unit wrote. One leads and writes; the
//! others stand by; a leader paused past its lease lands nothing once it resumes, answers the
//! requests queued `503` and exits 1; a standby keeps standing by while the leader writes; and a
//! `database_lease` that is a whole number above 0 is admitted.
//!
//! The fixture `tests/fixtures/ingress` serves `board` and `ledger`. Every case reaches a
//! database, so each is ignored without `SPINDLE_TEST_DATABASE_URL`; `task check:postgres` runs
//! them. A webhook, where one is used, is a stub on 127.0.0.1 run by this test.

/// The PostgreSQL lane (`task check:postgres`).
mod postgres {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{SocketAddr, TcpStream};
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::{Duration, Instant};

    use serde_json::{Value, json};

    use crate::egress::{BINDING, SPEC, Webhook};

    /// The lease every case runs with, in seconds: a standby polls every 667 ms, and a statement
    /// on the lease waits at most 667 ms for a lock.
    const LEASE: &str = "2";
    const TOOK: &str = "took the lease of board";
    const STARTUP: Duration = Duration::from_secs(60);
    const ANSWER: Duration = Duration::from_secs(10);
    const OPENED: &str = "tasks.board.TaskOpened";
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
        let schema = format!("adv_w31_u6_{letters}");
        sql(&[format!("CREATE SCHEMA {schema}")]);
        schema
    }

    /// An environment document keeping both components' state in PostgreSQL.
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

    /// `run` on the fixture with `environment`, its state in `schema`, a lease of `lease`.
    fn run(environment: &Path, schema: &str, lease: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"));
        command
            .current_dir(root())
            .args(["run", SPEC, "--http", BINDING, "--environment"]);
        command.arg(environment);
        for variable in SETTINGS {
            command.env_remove(variable);
        }
        command
            .env("SPINDLE_SETTING_DATABASE_URL", owner_url())
            .env("SPINDLE_SETTING_DATABASE_SCHEMA", schema)
            .env("SPINDLE_SETTING_DATABASE_LEASE", lease);
        command
    }

    fn call(address: SocketAddr, method: &str, path: &str, body: Option<&Value>) -> (u16, Value) {
        let mut stream =
            TcpStream::connect_timeout(&address, ANSWER).expect("the listener accepts");
        stream
            .set_read_timeout(Some(ANSWER))
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
        status_and_body(&answer)
    }

    fn status_and_body(answer: &str) -> (u16, Value) {
        let (head, text) = answer.split_once("\r\n\r\n").unwrap_or((answer, ""));
        let status = head
            .split(' ')
            .nth(1)
            .and_then(|status| status.parse().ok())
            .unwrap_or(0);
        (status, serde_json::from_str(text).unwrap_or(Value::Null))
    }

    /// A `run` process: its address, and every stderr line it printed.
    struct Replica {
        child: Child,
        address: SocketAddr,
        lines: Arc<Mutex<Vec<String>>>,
    }

    impl Drop for Replica {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    impl Replica {
        /// Starts `command`: the replica once it listens, else every stderr line it printed and
        /// how it exited.
        fn try_start(mut command: Command) -> Result<Self, String> {
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
                    kept.lock().expect("the lines").push(line);
                }
            });
            let line = receiver.recv_timeout(STARTUP).unwrap_or_default();
            let address = serde_json::from_str::<Value>(&line)
                .ok()
                .and_then(|ready| ready["listening"].as_str().map(str::to_owned))
                .and_then(|address| address.parse().ok());
            match address {
                Some(address) => Ok(Self {
                    child,
                    address,
                    lines,
                }),
                None => {
                    let _ = child.kill();
                    let status = child.wait();
                    std::thread::sleep(Duration::from_millis(200));
                    let printed = lines.lock().expect("the lines").join("\n");
                    Err(format!("stdout {line:?}, {status:?}, stderr:\n{printed}"))
                }
            }
        }

        fn start(command: Command) -> Self {
            Self::try_start(command).unwrap_or_else(|why| panic!("run did not listen: {why}"))
        }

        fn ready(&self) -> (u16, Value) {
            call(self.address, "GET", "/__spindle/ready", None)
        }

        fn ready_by(&self, deadline: Instant) {
            loop {
                let (status, body) = self.ready();
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

        fn has_line(&self, text: &str) -> bool {
            self.lines
                .lock()
                .expect("the lines")
                .iter()
                .any(|line| line.contains(text))
        }

        fn line_by(&self, text: &str, deadline: Instant) -> bool {
            loop {
                if self.has_line(text) {
                    return true;
                }
                if Instant::now() >= deadline {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }

        fn printed(&self) -> String {
            self.lines.lock().expect("the lines").join("\n")
        }

        fn signal(&self, signal: rustix::process::Signal) {
            rustix::process::kill_process(rustix::process::Pid::from_child(&self.child), signal)
                .expect("the signal is sent");
        }

        /// Whether the process still runs.
        fn running(&mut self) -> bool {
            self.child
                .try_wait()
                .expect("the run is waited on")
                .is_none()
        }

        fn exit_within(&mut self, within: Duration) -> Option<i32> {
            let deadline = Instant::now() + within;
            loop {
                if let Some(status) = self.child.try_wait().expect("the run is waited on") {
                    return status.code();
                }
                assert!(
                    Instant::now() < deadline,
                    "the run did not exit within {within:?}: {}",
                    self.printed()
                );
                std::thread::sleep(Duration::from_millis(20));
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

    /// A leader, ready, and a second process on the same store.
    fn leader_and_standby(environment: &Path, schema: &str) -> (Replica, Replica) {
        let leader = Replica::start(run(environment, schema, LEASE));
        leader.ready_by(Instant::now() + ANSWER);
        let standby = Replica::start(run(environment, schema, LEASE));
        let (status, body) = standby.ready();
        assert_eq!(status, 503, "{body}");
        assert_eq!(body["standby"], true, "{body}");
        (leader, standby)
    }

    /// Sends `PUT /tasks/{id}` to `address` on a thread, and answers the raw HTTP answer.
    fn late_command(address: SocketAddr, id: &'static str) -> std::thread::JoinHandle<String> {
        std::thread::spawn(move || {
            let mut stream =
                TcpStream::connect_timeout(&address, ANSWER).expect("the socket accepts");
            stream
                .set_read_timeout(Some(ANSWER))
                .expect("a read timeout");
            let body = json!({"title": format!("task {id}")}).to_string();
            write!(
                stream,
                "PUT /tasks/{id} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\
                 Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .expect("the command is written");
            let mut answer = String::new();
            let _ = stream.read_to_string(&mut answer);
            answer
        })
    }

    /// D4 A, `website/docs/concepts/environment-and-admission.md` § Several processes on one
    /// store: "A leader that finds its lease taken answers the requests already queued `503`".
    /// A command sent to a leader while it is paused past its lease, read once it resumes, is
    /// answered `503`, not with the store's refusal of the write.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_resumed_leader_answers_its_queued_command_503() {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment = environment(directory.path());
        let schema = schema();
        let (mut leader, standby) = leader_and_standby(&environment, &schema);
        let (status, answer) = leader.open("t1");
        assert_eq!(status, 201, "{answer}");

        leader.signal(rustix::process::Signal::STOP);
        let paused = Instant::now();
        assert!(
            standby.line_by(TOOK, paused + Duration::from_secs(10)),
            "the standby never took over: {}",
            standby.printed()
        );
        standby.ready_by(paused + Duration::from_secs(15));
        let (status, answer) = standby.open("t2");
        assert_eq!(status, 201, "{answer}");

        let late = late_command(leader.address, "t9");
        std::thread::sleep(Duration::from_millis(300));
        leader.signal(rustix::process::Signal::CONT);
        let code = leader.exit_within(Duration::from_secs(10));
        assert_eq!(code, Some(1), "{}", leader.printed());
        let answer = late.join().expect("the late command ends");
        let (status, body) = status_and_body(&answer);
        assert_eq!(
            status,
            503,
            "the command queued at the paused leader is answered 503: {answer}\n{}",
            leader.printed()
        );
        assert_ne!(body, Value::Null, "{answer}");
    }

    /// The fence covers the old leader's posts as well as its writes: a leader paused past its
    /// lease, while the new leader commits and posts `t2`, posts nothing once it resumes. Each
    /// event the webhook accepted is posted once by a process holding its store's lease: `t2`
    /// arrives once, from the new leader.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_resumed_leader_posts_no_event_the_new_leader_committed() {
        let webhook = Webhook::start(200);
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment =
            crate::egress::environment(directory.path(), "postgres", &webhook.url(), &[OPENED]);
        let schema = schema();
        let (mut leader, standby) = leader_and_standby(&environment, &schema);
        let (status, answer) = leader.open("t1");
        assert_eq!(status, 201, "{answer}");
        accepted(&webhook, 1);
        // The leader saves its cursor between two exchanges, every 50 ms.
        std::thread::sleep(Duration::from_millis(500));

        leader.signal(rustix::process::Signal::STOP);
        let paused = Instant::now();
        assert!(
            standby.line_by(TOOK, paused + Duration::from_secs(10)),
            "the standby never took over: {}",
            standby.printed()
        );
        standby.ready_by(paused + Duration::from_secs(15));
        let (status, answer) = standby.open("t2");
        assert_eq!(status, 201, "{answer}");
        accepted(&webhook, 2);

        leader.signal(rustix::process::Signal::CONT);
        let _ = leader.exit_within(Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(500));
        let posted: Vec<String> = crate::egress::named(&webhook.accepted())
            .into_iter()
            .map(|(_, task)| task)
            .collect();
        assert_eq!(
            posted.iter().filter(|task| *task == "t2").count(),
            1,
            "t2, which the new leader committed, is posted once, by it: {posted:?}\n{}",
            leader.printed()
        );
    }

    /// A leader's write holds the lease row locked for its whole transaction (the fence reads it
    /// with `get_for_update`), so a commit longer than a third of the lease holds a standby's
    /// poll past its lock wait. The standby keeps standing by: it is still running, and reads
    /// ready `503` with `standby: true`. The lock is taken here as the fence takes it, for
    /// 1.5 s.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_standby_outlives_a_leader_commit_longer_than_a_third_of_the_lease() {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment = environment(directory.path());
        let schema = schema();
        let (leader, mut standby) = leader_and_standby(&environment, &schema);
        let (status, answer) = leader.open("t1");
        assert_eq!(status, 201, "{answer}");

        let table = format!("{schema}.spindle_board_p_lease");
        sql(&[
            "BEGIN".to_owned(),
            format!("SELECT body FROM {table} FOR UPDATE"),
            "SELECT pg_sleep(1.5)".to_owned(),
            "COMMIT".to_owned(),
        ]);
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            standby.running(),
            "the standby exited while the leader held the lease row for 1.5 s: {}",
            standby.printed()
        );
        let (status, body) = standby.ready();
        assert_eq!(status, 503, "{body}");
        assert_eq!(body["standby"], true, "{body}");
    }

    /// The documents admit every whole number of seconds above 0 for `database_lease` ("A
    /// `database_lease` that is not a whole number above 0 exits 1 before connecting"): a lease
    /// of 10,000,000 s is one, and the run leads with it, or is refused naming the setting.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_long_lease_is_admitted_or_refused_by_name() {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment = environment(directory.path());
        let schema = schema();
        match Replica::try_start(run(&environment, &schema, "10000000")) {
            Ok(leader) => leader.ready_by(Instant::now() + ANSWER),
            Err(why) => assert!(
                why.contains("database_lease"),
                "a lease of 10000000 s is refused without naming database_lease: {why}"
            ),
        }
    }

    /// Two standbys race for one released lease: one of them leads, the other keeps standing
    /// by, still running, and the store holds each write once.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn two_standbys_race_for_one_released_lease() {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment = environment(directory.path());
        let schema = schema();
        let (mut leader, first) = leader_and_standby(&environment, &schema);
        let second = Replica::start(run(&environment, &schema, LEASE));
        let (status, answer) = leader.open("t1");
        assert_eq!(status, 201, "{answer}");
        leader.signal(rustix::process::Signal::TERM);
        assert_eq!(
            leader.exit_within(ANSWER),
            Some(143),
            "{}",
            leader.printed()
        );

        let deadline = Instant::now() + Duration::from_secs(10);
        let (mut winner, mut loser) = loop {
            if first.ready().0 == 200 {
                break (first, second);
            }
            if second.ready().0 == 200 {
                break (second, first);
            }
            assert!(Instant::now() < deadline, "no standby took over");
            std::thread::sleep(Duration::from_millis(50));
        };
        // Three of the loser's polls.
        std::thread::sleep(Duration::from_secs(2));
        assert!(loser.running(), "{}", loser.printed());
        let (status, body) = loser.ready();
        assert_eq!(status, 503, "{body}");
        assert_eq!(body["standby"], true, "{body}");
        assert!(!loser.has_line(TOOK), "{}", loser.printed());
        assert!(winner.running(), "{}", winner.printed());
        let (status, answer) = winner.open("t2");
        assert_eq!(status, 201, "{answer}");
        let (status, tasks) = call(winner.address, "GET", "/tasks", None);
        assert_eq!(status, 200, "{tasks}");
        assert_eq!(
            tasks,
            json!({"rows": [
                {"task_id": "t1", "state": "Open", "title": "task t1"},
                {"task_id": "t2", "state": "Open", "title": "task t2"},
            ]})
        );
    }

    /// D9: a lease "is also free once the holder's session ends". With a lease of 9 s a standby
    /// polls every 3 s; a killed leader's lease, last renewed at most 3 s before the kill, runs out
    /// no sooner than 6 s after it. The standby takes it within 5.5 s, so it took it because the
    /// leader's session ended, not because its time ran out. (The unit's own case, with a lease of
    /// 2 s and a bound of 5 s, passes either way.)
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_killed_leaders_lease_is_free_before_it_runs_out() {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment = environment(directory.path());
        let schema = schema();
        let mut leader = Replica::start(run(&environment, &schema, "9"));
        leader.ready_by(Instant::now() + ANSWER);
        let standby = Replica::start(run(&environment, &schema, "9"));
        let (status, body) = standby.ready();
        assert_eq!(status, 503, "{body}");
        leader.child.kill().expect("SIGKILL is sent");
        let _ = leader.child.wait();
        let killed = Instant::now();
        assert!(
            standby.line_by(TOOK, killed + Duration::from_millis(5500)),
            "the standby did not take the killed leader's lease within 5.5 s: {}",
            standby.printed()
        );
        eprintln!("taken {:?} after the kill", killed.elapsed());
    }

    /// D5 A, as the story decided it: "the seed runs only under the first token of the lease; on a
    /// takeover it is skipped with a log line." A leader killed after its seed ended, and a
    /// process started once its session ended, takes the leases at their second token: it skips
    /// the seed and logs that it did.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn the_seed_runs_only_under_the_first_token() {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment = environment(directory.path());
        let seed = directory.path().join("seed.json");
        std::fs::write(
            &seed,
            json!([{
                "command": "tasks.board.OpenTask",
                "input": {"task_id": "s1", "title": "seeded"},
                "request_id": "seed-1",
            }])
            .to_string(),
        )
        .expect("the seed is written");
        let schema = schema();
        let seeded = |schema: &str| {
            let mut command = run(&environment, schema, LEASE);
            command.arg("--seed").arg(&seed);
            command
        };
        let mut first = Replica::start(seeded(&schema));
        assert!(
            first.line_by("ended:", Instant::now() + ANSWER),
            "{}",
            first.printed()
        );
        first.child.kill().expect("SIGKILL is sent");
        let _ = first.child.wait();
        // The killed leader's session ends at once on loopback.
        std::thread::sleep(Duration::from_secs(1));

        let mut second = Replica::start(seeded(&schema));
        std::thread::sleep(Duration::from_millis(1500));
        assert!(
            second.has_line("is not submitted") && !second.has_line("ended:"),
            "the process leading under the second token skips the seed: {}",
            second.printed()
        );
        assert!(second.running(), "{}", second.printed());
        second.ready_by(Instant::now() + ANSWER);
    }

    /// The first column of the first row `query` answers, run as the test database's owner.
    fn sql_value(query: &str) -> Option<String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        runtime.block_on(async {
            let (client, connection) = tokio_postgres::connect(&owner_url(), tokio_postgres::NoTls)
                .await
                .expect("the test database accepts its owner");
            let driver = tokio::spawn(connection);
            let answered = client
                .simple_query(query)
                .await
                .unwrap_or_else(|error| panic!("{query}: {error}"))
                .into_iter()
                .find_map(|message| match message {
                    tokio_postgres::SimpleQueryMessage::Row(row) => row.get(0).map(str::to_owned),
                    _ => None,
                });
            drop(client);
            let _ = driver.await;
            answered
        })
    }

    /// Round 2, D4 and D9 together: a lease is free once its holder's database session ends, so
    /// a leader whose lease connection is dropped (a network blip, an idle-session timeout, an
    /// operator's `pg_terminate_backend`) loses its lease to the standby while it still runs, its
    /// last renewal recent. A command it takes after the standby took over is answered `503`, as
    /// D4 says for a leader that lost its lease, not with the store's refusal of the write.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_leader_whose_lease_session_ended_answers_a_command_503() {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment = environment(directory.path());
        let schema = schema();
        let (leader, standby) = leader_and_standby(&environment, &schema);
        let (status, answer) = leader.open("t1");
        assert_eq!(status, 201, "{answer}");
        // Commands go to the old leader one after another, from before its session ends until it
        // no longer accepts a connection: each lands (201) or is answered 503.
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
                let _ = stream.set_read_timeout(Some(ANSWER));
                let body = json!({"title": "fired"}).to_string();
                let written = write!(
                    stream,
                    "PUT /tasks/f{index} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\
                     Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                );
                if written.is_err() {
                    break;
                }
                let mut answer = String::new();
                let _ = stream.read_to_string(&mut answer);
                if answer.is_empty() {
                    break;
                }
                answers.push(answer);
            }
            answers
        });
        let pid = format!("SELECT body->>'pid' FROM {schema}.spindle_board_p_lease");
        let mut took = false;
        for _ in 0..5 {
            let holder = sql_value(&pid).expect("the lease row names its holder");
            sql(&[format!("SELECT pg_terminate_backend({holder})")]);
            if standby.line_by(TOOK, Instant::now() + Duration::from_secs(3)) {
                took = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(500));
        stop.store(true, std::sync::atomic::Ordering::SeqCst);
        let answers = firing.join().expect("the commands end");
        assert!(
            took,
            "the standby never took a lease whose session ended: {}",
            standby.printed()
        );
        let statuses: Vec<u16> = answers
            .iter()
            .map(|answer| status_and_body(answer).0)
            .collect();
        eprintln!(
            "the old leader answered {} commands: {} x 201, {} x 503",
            statuses.len(),
            statuses.iter().filter(|status| **status == 201).count(),
            statuses.iter().filter(|status| **status == 503).count()
        );
        let other: Vec<&String> = answers
            .iter()
            .filter(|answer| !matches!(status_and_body(answer).0, 201 | 503))
            .collect();
        assert!(
            other.is_empty(),
            "the old leader answers a command after losing its lease neither 201 nor 503: \
             {other:?}\n{}",
            leader.printed()
        );
    }

    /// Round 2, F1: a leader whose renewal is held up (the lease row locked for 1 s, half the
    /// lease, as a slow database holds it) still answers a command `201`, within 3 s, and keeps
    /// leading; the standby keeps standing by.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_leader_whose_renewal_is_held_up_still_answers() {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment = environment(directory.path());
        let schema = schema();
        let (mut leader, standby) = leader_and_standby(&environment, &schema);
        let (status, answer) = leader.open("t1");
        assert_eq!(status, 201, "{answer}");
        let table = format!("{schema}.spindle_board_p_lease");
        let locker = std::thread::spawn(move || {
            sql(&[
                "BEGIN".to_owned(),
                format!("SELECT body FROM {table} FOR UPDATE"),
                "SELECT pg_sleep(1.0)".to_owned(),
                "COMMIT".to_owned(),
            ]);
        });
        std::thread::sleep(Duration::from_millis(300));
        let sent = Instant::now();
        let (status, answer) = leader.open("t2");
        let took = sent.elapsed();
        locker.join().expect("the lock ends");
        eprintln!("answered in {took:?}");
        assert_eq!(status, 201, "{answer}\n{}", leader.printed());
        assert!(took < Duration::from_secs(3), "answered in {took:?}");
        std::thread::sleep(Duration::from_secs(1));
        assert!(leader.running(), "{}", leader.printed());
        let (status, body) = standby.ready();
        assert_eq!(status, 503, "{body}");
        assert_eq!(body["standby"], true, "{body}");
    }

    /// Round 2, F1: a pause just under two thirds of the lease (1.3 s of 2 s). A command sent
    /// meanwhile is answered `201` by the leader, which still leads, or, where the lease ran out
    /// and the standby took it, `503`; never otherwise, and no false `503` while no other
    /// process took the lease.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_pause_under_two_thirds_of_the_lease_answers_consistently() {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment = environment(directory.path());
        let schema = schema();
        let (mut leader, standby) = leader_and_standby(&environment, &schema);
        let (status, answer) = leader.open("t1");
        assert_eq!(status, 201, "{answer}");
        leader.signal(rustix::process::Signal::STOP);
        let late = late_command(leader.address, "t9");
        std::thread::sleep(Duration::from_millis(1300));
        leader.signal(rustix::process::Signal::CONT);
        let answer = late.join().expect("the late command ends");
        let (status, _) = status_and_body(&answer);
        std::thread::sleep(Duration::from_millis(700));
        match status {
            201 => {
                assert!(!standby.has_line(TOOK), "{}", standby.printed());
                assert!(leader.running(), "{}", leader.printed());
                let (status, body) = standby.ready();
                assert_eq!(status, 503, "{body}");
            }
            503 => assert!(
                standby.has_line(TOOK),
                "a false 503: no other process took the lease: {answer}\n{}",
                leader.printed()
            ),
            _ => panic!("{answer}\n{}", leader.printed()),
        }
    }

    /// Round 2, F3: the shortest lease, 1 s (a standby polls every 333 ms, a statement waits at
    /// most 333 ms). The leader answers a command every 300 ms for 2 s, each `201`; once it is
    /// killed the standby takes over and answers.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn the_shortest_lease_leads_and_fails_over() {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment = environment(directory.path());
        let schema = schema();
        let mut leader = Replica::start(run(&environment, &schema, "1"));
        leader.ready_by(Instant::now() + ANSWER);
        let standby = Replica::start(run(&environment, &schema, "1"));
        for index in 0..7 {
            let (status, answer) = leader.open(&format!("t{index}"));
            assert_eq!(status, 201, "{answer}\n{}", leader.printed());
            std::thread::sleep(Duration::from_millis(300));
        }
        assert!(leader.running(), "{}", leader.printed());
        assert!(!standby.has_line(TOOK), "{}", standby.printed());
        leader.child.kill().expect("SIGKILL is sent");
        let _ = leader.child.wait();
        standby.ready_by(Instant::now() + Duration::from_secs(5));
        let (status, answer) = standby.open("t9");
        assert_eq!(status, 201, "{answer}");
    }

    /// Round 2, F3: the longest lease, 86400 s, leads and answers a command; a second process
    /// stands by.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn the_longest_lease_leads() {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment = environment(directory.path());
        let schema = schema();
        let leader = Replica::start(run(&environment, &schema, "86400"));
        leader.ready_by(Instant::now() + ANSWER);
        let (status, answer) = leader.open("t1");
        assert_eq!(status, 201, "{answer}");
        let standby = Replica::start(run(&environment, &schema, "86400"));
        let (status, body) = standby.ready();
        assert_eq!(status, 503, "{body}");
        assert_eq!(body["standby"], true, "{body}");
    }

    /// Writes a one-entry seed into `directory`.
    fn seed_file(directory: &Path) -> PathBuf {
        let seed = directory.join("seed.json");
        std::fs::write(
            &seed,
            json!([{
                "command": "tasks.board.OpenTask",
                "input": {"task_id": "s1", "title": "seeded"},
                "request_id": "seed-1",
            }])
            .to_string(),
        )
        .expect("the seed is written");
        seed
    }

    /// Round 2, F4: one lease at its first token and another at its second. `ledger`'s lease was
    /// taken and released before (as by a process that served only it); the run takes `board`'s
    /// at token 1 and `ledger`'s at token 2, skips the seed, and names `ledger` and token 2.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_seed_with_one_lease_taken_before_is_skipped_naming_it() {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment = environment(directory.path());
        let seed = seed_file(directory.path());
        let schema = schema();
        let mut database = spindle_kernel::Database::new(owner_url());
        database.schema.clone_from(&schema);
        let mut lease = spindle_kernel::Lease::connect(&database, Duration::from_secs(2))
            .expect("the test connects for the lease");
        let token = lease
            .take("ledger")
            .expect("the lease is read")
            .expect("the lease is free");
        assert_eq!(token, 1);
        lease
            .release("ledger", token)
            .expect("the lease is released");
        drop(lease);

        let mut command = run(&environment, &schema, LEASE);
        command.arg("--seed").arg(&seed);
        let leader = Replica::start(command);
        leader.ready_by(Instant::now() + ANSWER);
        assert!(
            leader.line_by("is not submitted", Instant::now() + ANSWER),
            "{}",
            leader.printed()
        );
        assert!(
            leader.has_line("lease of ledger at token 2") && !leader.has_line("ended:"),
            "{}",
            leader.printed()
        );
    }

    /// Round 2, F4 and D5 A read literally: a single process given a seed and stopped cleanly
    /// (SIGTERM, which releases its leases) takes token 2 on its restart, so the seed is not
    /// submitted again, and the run serves.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_restart_after_a_clean_stop_skips_the_seed() {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment = environment(directory.path());
        let seed = seed_file(directory.path());
        let schema = schema();
        let seeded = || {
            let mut command = run(&environment, &schema, LEASE);
            command.arg("--seed").arg(&seed);
            command
        };
        let mut first = Replica::start(seeded());
        assert!(
            first.line_by("ended: 1 sent, 1 applied", Instant::now() + ANSWER),
            "{}",
            first.printed()
        );
        first.signal(rustix::process::Signal::TERM);
        assert_eq!(first.exit_within(ANSWER), Some(143), "{}", first.printed());
        let mut second = Replica::start(seeded());
        second.ready_by(Instant::now() + ANSWER);
        assert!(
            second.has_line("is not submitted") && second.has_line("at token 2"),
            "{}",
            second.printed()
        );
        assert!(!second.has_line("ended:"), "{}", second.printed());
        assert!(second.running(), "{}", second.printed());
    }

    /// Sends `PUT /tasks/<prefix><n>` to `address` one after another until `stop` is set or the
    /// address accepts no more: each id with the status it was answered, 0 for none.
    fn fire(
        address: SocketAddr,
        prefix: &'static str,
        stop: Arc<std::sync::atomic::AtomicBool>,
    ) -> std::thread::JoinHandle<Vec<(String, u16)>> {
        std::thread::spawn(move || {
            let mut answers = Vec::new();
            let mut index = 0;
            while !stop.load(std::sync::atomic::Ordering::SeqCst) {
                index += 1;
                let id = format!("{prefix}{index}");
                let Ok(mut stream) = TcpStream::connect_timeout(&address, ANSWER) else {
                    break;
                };
                let _ = stream.set_read_timeout(Some(ANSWER));
                let body = json!({"title": format!("task {id}")}).to_string();
                let written = write!(
                    stream,
                    "PUT /tasks/{id} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\
                     Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                );
                let mut answer = String::new();
                if written.is_ok() {
                    let _ = stream.read_to_string(&mut answer);
                }
                let status = status_and_body(&answer).0;
                answers.push((id, status));
                if status == 0 {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            answers
        })
    }

    /// The task ids `GET /tasks` lists at `address`.
    fn task_ids(address: SocketAddr) -> Vec<String> {
        let (status, body) = call(address, "GET", "/tasks", None);
        assert_eq!(status, 200, "{body}");
        body["rows"]
            .as_array()
            .expect("rows")
            .iter()
            .map(|row| row["task_id"].as_str().expect("an id").to_owned())
            .collect()
    }

    /// Round 3, change 1: a renewal lands only on the session that took the lease, so a dropped
    /// lease connection of a leader fails over. While commands flow to the leader its lease
    /// connection is terminated: the leader exits 1 naming the lease, the standby takes over, every
    /// command the old leader answered `201` is held once and none it answered `503`, every task
    /// opened is posted at least once, and a repeat names the same component, log and position
    /// as its first post.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_dropped_leader_session_fails_over_with_each_write_once() {
        let webhook = Webhook::start(200);
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment =
            crate::egress::environment(directory.path(), "postgres", &webhook.url(), &[OPENED]);
        let schema = schema();
        let (mut leader, standby) = leader_and_standby(&environment, &schema);
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let firing = fire(leader.address, "f", Arc::clone(&stop));
        std::thread::sleep(Duration::from_millis(500));
        let holder = sql_value(&format!(
            "SELECT body->>'pid' FROM {schema}.spindle_board_p_lease"
        ))
        .expect("the lease row names its holder");
        sql(&[format!("SELECT pg_terminate_backend({holder})")]);
        let code = leader.exit_within(Duration::from_secs(10));
        stop.store(true, std::sync::atomic::Ordering::SeqCst);
        let answers = firing.join().expect("the commands end");
        assert_eq!(code, Some(1), "{}", leader.printed());
        assert!(
            leader.line_by(
                "writes nothing more and exits",
                Instant::now() + Duration::from_secs(2)
            ),
            "the old leader names the lease: {}",
            leader.printed()
        );
        standby.ready_by(Instant::now() + Duration::from_secs(10));
        let held = task_ids(standby.address);
        let landed: Vec<&String> = answers
            .iter()
            .filter(|(_, status)| *status == 201)
            .map(|(id, _)| id)
            .collect();
        eprintln!(
            "the old leader answered {} commands: {} x 201, statuses {:?}",
            answers.len(),
            landed.len(),
            answers
                .iter()
                .filter(|(_, status)| *status != 201)
                .collect::<Vec<_>>()
        );
        for id in &landed {
            assert!(held.contains(id), "{id} was answered 201 and is lost");
        }
        for (id, status) in &answers {
            assert!(
                matches!(status, 201 | 503 | 0),
                "{id} answered {status}: {}",
                leader.printed()
            );
            if *status == 503 {
                assert!(!held.contains(id), "{id} was answered 503 and landed");
            }
        }
        let (status, answer) = standby.open("n1");
        assert_eq!(status, 201, "{answer}");
        // Every task opened is posted at least once (at least once, the egress's contract).
        let mut committed = held.clone();
        committed.push("n1".to_owned());
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let posted: Vec<String> = crate::egress::named(&webhook.accepted())
                .into_iter()
                .map(|(_, task)| task)
                .collect();
            let missing: Vec<&String> = committed
                .iter()
                .filter(|task| !posted.contains(task))
                .collect();
            if missing.is_empty() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "never posted within 30 s: {missing:?} of {committed:?}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        std::thread::sleep(Duration::from_millis(500));
        // A repeat across the failover (decided: the old leader's last unsaved page may be posted
        // again) names the same event as its first post: the same component, log and position.
        let bodies = webhook.accepted();
        let mut first: std::collections::BTreeMap<String, (Value, Value, Value)> =
            std::collections::BTreeMap::new();
        let mut repeats = 0;
        for body in &bodies {
            let task = body["fields"]["task_id"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            let named = (
                body["component"].clone(),
                body["log"].clone(),
                body["position"].clone(),
            );
            match first.get(&task) {
                None => {
                    first.insert(task, named);
                }
                Some(earlier) => {
                    repeats += 1;
                    assert_eq!(
                        earlier, &named,
                        "{task} is posted again naming another (component, log, position)"
                    );
                }
            }
        }
        eprintln!(
            "{} posts of {} tasks, {repeats} repeats, each naming its first post's event",
            bodies.len(),
            first.len()
        );
    }

    /// Round 3, change 1 seen from the standby: its own lease connection is dropped (it asks for
    /// the leases on one connection of its own, and the last thing it ran there is a take of
    /// board's lease). It stands by on, still running, and takes over once the leader is killed.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_dropped_standby_session_keeps_it_standing_by() {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment = environment(directory.path());
        let schema = schema();
        let (mut leader, mut standby) = leader_and_standby(&environment, &schema);
        // The standby polls every 667 ms; wait for one poll at least.
        std::thread::sleep(Duration::from_millis(800));
        let holder = sql_value(&format!(
            "SELECT pid FROM pg_stat_activity WHERE pid <> pg_backend_pid() AND query LIKE \
             'INSERT INTO {schema}.spindle_board_p_lease%' LIMIT 1"
        ))
        .expect("the standby's lease connection last ran a take");
        sql(&[format!("SELECT pg_terminate_backend({holder})")]);
        std::thread::sleep(Duration::from_millis(1500));
        assert!(
            standby.running(),
            "the standby exited once its lease connection was dropped: {}",
            standby.printed()
        );
        let (status, body) = standby.ready();
        assert_eq!(status, 503, "{body}");
        assert_eq!(body["standby"], true, "{body}");
        leader.child.kill().expect("SIGKILL is sent");
        let _ = leader.child.wait();
        standby.ready_by(Instant::now() + Duration::from_secs(5));
    }

    /// The `until` of board's lease row in `schema`, as text.
    fn until(schema: &str) -> Option<String> {
        sql_value(&format!(
            "SELECT body->>'until' FROM {schema}.spindle_board_p_lease"
        ))
    }

    /// Round 3, change 2: on the shortest lease (1 s) a healthy leader renews every fifth of it,
    /// about 10 renewals of board's lease in 2 s, not a storm: the row's `until` moves between 5
    /// and 15 times.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn renewals_on_the_shortest_lease_come_every_fifth_of_it() {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment = environment(directory.path());
        let schema = schema();
        let leader = Replica::start(run(&environment, &schema, "1"));
        leader.ready_by(Instant::now() + ANSWER);
        let mut seen = Vec::new();
        let end = Instant::now() + Duration::from_secs(2);
        while Instant::now() < end {
            let value = until(&schema);
            if seen.last() != Some(&value) {
                seen.push(value);
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        let moves = seen.len().saturating_sub(1);
        eprintln!("board's lease moved {moves} times in 2 s");
        assert!((5..=15).contains(&moves), "{moves} renewals in 2 s");
    }

    /// The time from `answered` until `webhook` accepted more than `before` bodies, at most 30 s: a
    /// feed read sees a commit only once older transactions end, and the lane's long scans hold
    /// theirs open up to about 20 s.
    fn posted_after(webhook: &Webhook, before: usize, answered: Instant) -> Duration {
        let deadline = answered + Duration::from_secs(30);
        while webhook.accepted().len() <= before {
            assert!(Instant::now() < deadline, "not posted within 30 s");
            std::thread::sleep(Duration::from_millis(5));
        }
        answered.elapsed()
    }

    /// Round 3, changes 2 and 3: on the shortest lease the lease row is locked for 0.7 s, under
    /// the four fifths of the lease a renewal is queued for. The leader keeps leading, a command
    /// sent meanwhile is answered `201` once the lock ends, its event is posted (the egress
    /// resumes once the renewal lands; awaited 30 s, the time printed), and the standby stands by.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_shortest_lease_held_up_by_a_row_lock_keeps_leading_and_posting() {
        let webhook = Webhook::start(200);
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment =
            crate::egress::environment(directory.path(), "postgres", &webhook.url(), &[OPENED]);
        let schema = schema();
        let mut leader = Replica::start(run(&environment, &schema, "1"));
        leader.ready_by(Instant::now() + ANSWER);
        let standby = Replica::start(run(&environment, &schema, "1"));
        let (status, answer) = leader.open("t1");
        assert_eq!(status, 201, "{answer}");
        accepted(&webhook, 1);
        let table = format!("{schema}.spindle_board_p_lease");
        let locker = std::thread::spawn(move || {
            sql(&[
                "BEGIN".to_owned(),
                format!("SELECT body FROM {table} FOR UPDATE"),
                "SELECT pg_sleep(0.7)".to_owned(),
                "COMMIT".to_owned(),
            ]);
        });
        std::thread::sleep(Duration::from_millis(100));
        let (status, answer) = leader.open("t2");
        let answered = Instant::now();
        assert_eq!(status, 201, "{answer}\n{}", leader.printed());
        let latency = posted_after(&webhook, 1, answered);
        locker.join().expect("the lock ends");
        eprintln!("t2 posted {latency:?} after its answer");
        std::thread::sleep(Duration::from_millis(500));
        assert!(leader.running(), "{}", leader.printed());
        assert!(!standby.has_line(TOOK), "{}", standby.printed());
    }

    /// Round 3, change 3: the egress of a healthy leader is not held back by the check that its
    /// lease is known held. With a lease of 1 s and of 10 s, each of five tasks opened 300 ms
    /// apart is posted promptly: no read of the store is refused (`cannot read the store`) by the
    /// check of the lease. Each post is awaited 30 s, the lag a feed read may have behind older
    /// transactions of other tests of the lane; the time is printed.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_healthy_leader_posts_promptly_on_a_short_and_a_long_lease() {
        for lease in ["1", "10"] {
            let webhook = Webhook::start(200);
            let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
            let environment =
                crate::egress::environment(directory.path(), "postgres", &webhook.url(), &[OPENED]);
            let schema = schema();
            let leader = Replica::start(run(&environment, &schema, lease));
            leader.ready_by(Instant::now() + ANSWER);
            let mut slowest = Duration::ZERO;
            for index in 0..5 {
                let before = webhook.accepted().len();
                let (status, answer) = leader.open(&format!("t{index}"));
                let answered = Instant::now();
                assert_eq!(status, 201, "{answer}");
                slowest = slowest.max(posted_after(&webhook, before, answered));
                std::thread::sleep(Duration::from_millis(300));
            }
            eprintln!("lease {lease} s: the slowest post {slowest:?} after its answer");
            assert!(
                !leader.has_line("cannot read the store"),
                "lease {lease} s: the egress of a healthy leader was held back: {}",
                leader.printed()
            );
        }
    }

    /// The standby's lease connection: the backend whose last statement was a take of board's
    /// lease in `schema`, waiting up to 5 s for one (a standby connects again at the poll after the
    /// one that found its connection closed).
    fn standby_backend(schema: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(pid) = sql_value(&format!(
                "SELECT pid FROM pg_stat_activity WHERE pid <> pg_backend_pid() AND query LIKE \
                 'INSERT INTO {schema}.spindle_board_p_lease%' LIMIT 1"
            )) {
                return pid;
            }
            assert!(
                Instant::now() < deadline,
                "no lease connection of the standby within 5 s"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Round 4, F1: the standby's lease connection is dropped three times, each time on the
    /// connection it made again. It stands by on each time, and once the leader is killed it takes over and answers.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_standby_dropped_three_times_still_takes_over() {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment = environment(directory.path());
        let schema = schema();
        let (mut leader, mut standby) = leader_and_standby(&environment, &schema);
        let mut dropped: Vec<String> = Vec::new();
        for _ in 0..3 {
            let deadline = Instant::now() + Duration::from_secs(5);
            let holder = loop {
                let holder = standby_backend(&schema);
                if !dropped.contains(&holder) {
                    break holder;
                }
                assert!(
                    Instant::now() < deadline,
                    "the standby never connected again"
                );
                std::thread::sleep(Duration::from_millis(50));
            };
            sql(&[format!("SELECT pg_terminate_backend({holder})")]);
            dropped.push(holder);
        }
        std::thread::sleep(Duration::from_millis(800));
        assert!(standby.running(), "{}", standby.printed());
        let (status, body) = standby.ready();
        assert_eq!(status, 503, "{body}");
        assert_eq!(body["standby"], true, "{body}");
        leader.child.kill().expect("SIGKILL is sent");
        let _ = leader.child.wait();
        standby.ready_by(Instant::now() + Duration::from_secs(5));
        let (status, answer) = standby.open("t9");
        assert_eq!(status, 201, "{answer}");
    }

    /// Round 4, F1: a standby whose lease connection the database refuses from now on (its role
    /// may no longer log in, as after a rotated password) stands by on, as F1 says, but cannot
    /// take over while it stands by: it reads `standby: true` and live, as a healthy standby does.
    /// So it says so on stderr: within three polls a line names the lease connection it cannot
    /// make, beyond the line that it stands by.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_standby_the_database_refuses_says_so() {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment = environment(directory.path());
        let schema = schema();
        let leader = Replica::start(run(&environment, &schema, LEASE));
        leader.ready_by(Instant::now() + ANSWER);
        let role = format!("{schema}_r");
        sql(&[
            format!("CREATE ROLE {role} LOGIN PASSWORD 'standby-pw'"),
            format!("GRANT USAGE ON SCHEMA {schema} TO {role}"),
            format!(
                "GRANT SELECT, INSERT, UPDATE ON {schema}.spindle_board_p_lease, \
                 {schema}.spindle_ledger_p_lease TO {role}"
            ),
        ]);
        let owner = owner_url();
        let (scheme, rest) = owner.split_once("://").expect("a URL with a scheme");
        let (_, location) = rest.split_once('@').expect("a URL with a user");
        let url = format!("{scheme}://{role}:standby-pw@{location}");
        let ca = std::env::var("SPINDLE_TEST_DATABASE_CA")
            .expect("SPINDLE_TEST_DATABASE_CA names the test server's CA (task check:postgres)");
        let mut command = run(&environment, &schema, LEASE);
        command
            .env("SPINDLE_SETTING_DATABASE_URL", &url)
            .env("SPINDLE_SETTING_DATABASE_CA", &ca)
            .env("SPINDLE_SETTING_DATABASE_CONNECTIONS", "100");
        let mut standby = Replica::start(command);
        let (status, body) = standby.ready();
        assert_eq!(status, 503, "{body}");
        assert_eq!(body["standby"], true, "{body}");

        sql(&[
            format!("ALTER ROLE {role} NOLOGIN"),
            format!(
                "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE usename = '{role}'"
            ),
        ]);
        std::thread::sleep(Duration::from_millis(2500));
        assert!(standby.running(), "{}", standby.printed());
        let lines = standby.printed();
        let told = lines
            .lines()
            .filter(|line| line.contains("lease") && !line.contains("standing by"))
            .count();
        assert!(
            told > 0,
            "a standby the database refuses says nothing, reading as a healthy standby: {lines}"
        );
    }

    /// Holds `lock` on `table` for `seconds` on a thread of its own, as the test database's owner:
    /// answers once the lock is held, and the thread ends once it is released.
    fn locked(table: String, lock: &'static str, seconds: f64) -> std::thread::JoinHandle<()> {
        let (held, holding) = mpsc::channel();
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
                    .batch_execute(&format!("BEGIN; LOCK TABLE {table} IN {lock} MODE"))
                    .await
                    .expect("the lock is taken");
                let _ = held.send(());
                client
                    .batch_execute(&format!("SELECT pg_sleep({seconds}); COMMIT"))
                    .await
                    .expect("the lock is released");
                drop(client);
                let _ = driver.await;
            });
        });
        holding
            .recv_timeout(ANSWER)
            .expect("the lock is held within 10 s");
        thread
    }

    /// Who leads once things settle: the old leader where it still answers ready `200` (`true`),
    /// else the standby, once it does (`false`), with its address.
    fn settled_leader(leader: &mut Replica, standby: &Replica) -> (bool, SocketAddr) {
        std::thread::sleep(Duration::from_secs(2));
        if leader.running() && leader.ready().0 == 200 {
            return (true, leader.address);
        }
        standby.ready_by(Instant::now() + Duration::from_secs(10));
        (false, standby.address)
    }

    /// Round 5, the transaction cut: a write held up longer than a short lease. On a 1 s lease
    /// board's event table is locked against inserts for 1.5 s while a command is sent. Whatever
    /// the caller is answered, the write is whole: answered `201` it is held, answered otherwise
    /// it is not, at whichever process leads once things settle. The answer, its time and who
    /// leads are printed.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_write_longer_than_a_short_lease_lands_whole_or_not_at_all() {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment = environment(directory.path());
        let schema = schema();
        let mut leader = Replica::start(run(&environment, &schema, "1"));
        leader.ready_by(Instant::now() + ANSWER);
        let standby = Replica::start(run(&environment, &schema, "1"));
        let (status, answer) = leader.open("t1");
        assert_eq!(status, 201, "{answer}");
        let lock = locked(format!("{schema}.spindle_board_events"), "SHARE", 1.5);
        let sent = Instant::now();
        let (status, body) = leader.open("t2");
        let took = sent.elapsed();
        lock.join().expect("the lock ends");
        let (old_leads, leads) = settled_leader(&mut leader, &standby);
        let held = task_ids(leads);
        eprintln!(
            "the write held up 1.5 s on a 1 s lease was answered {status} after {took:?}: {body}; \
             the old leader {}",
            if old_leads {
                "leads on"
            } else {
                "lost the lease"
            }
        );
        assert_eq!(
            status == 201,
            held.contains(&"t2".to_owned()),
            "answered {status} and held {held:?}: {body}"
        );
        assert!(held.contains(&"t1".to_owned()), "{held:?}");
    }

    /// Round 5, the transaction cut, under the bound the docs give a renewal: on a 2 s lease the
    /// write is held up 1.2 s, below the lease and below four fifths of it. It lands (`201`), the
    /// leader keeps leading and the standby stands by.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_write_held_up_within_the_lease_lands_and_keeps_the_leader() {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment = environment(directory.path());
        let schema = schema();
        let (mut leader, standby) = leader_and_standby(&environment, &schema);
        let (status, answer) = leader.open("t1");
        assert_eq!(status, 201, "{answer}");
        let lock = locked(format!("{schema}.spindle_board_events"), "SHARE", 1.2);
        let (status, body) = leader.open("t2");
        lock.join().expect("the lock ends");
        assert_eq!(status, 201, "{body}\n{}", leader.printed());
        std::thread::sleep(Duration::from_secs(2));
        assert!(leader.running(), "{}", leader.printed());
        assert!(!standby.has_line(TOOK), "{}", standby.printed());
        assert_eq!(leader.ready().0, 200);
    }

    /// Round 5, the transaction cut reaches reads too: on a 1 s lease board's event table is
    /// locked against every access for 1.5 s while `GET /tasks` is read. The documents bound
    /// "a transaction" by the lease, so a read, which writes nothing and holds no lease row, waits
    /// for the lock and answers `200`, as it does without a lease.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_read_held_up_longer_than_a_short_lease_answers() {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment = environment(directory.path());
        let schema = schema();
        let leader = Replica::start(run(&environment, &schema, "1"));
        leader.ready_by(Instant::now() + ANSWER);
        let (status, answer) = leader.open("t1");
        assert_eq!(status, 201, "{answer}");
        let lock = locked(
            format!("{schema}.spindle_board_events"),
            "ACCESS EXCLUSIVE",
            1.5,
        );
        let sent = Instant::now();
        let (status, body) = call(leader.address, "GET", "/tasks", None);
        let took = sent.elapsed();
        lock.join().expect("the lock ends");
        eprintln!("the read held up 1.5 s on a 1 s lease answered {status} after {took:?}");
        assert_eq!(status, 200, "{body}");
    }

    /// Round 5, F1's warning once per change: a standby the database refuses warns once, not at
    /// each of its polls (about six in 4 s), and once the database admits it again it prints one
    /// line that the connection is made again, and no further warning.
    #[test]
    #[ignore = "needs SPINDLE_TEST_DATABASE_URL, a loopback PostgreSQL: task check:postgres"]
    fn a_refused_standby_warns_once_and_says_when_it_is_back() {
        let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
        let environment = environment(directory.path());
        let schema = schema();
        let leader = Replica::start(run(&environment, &schema, LEASE));
        leader.ready_by(Instant::now() + ANSWER);
        let role = format!("{schema}_w");
        sql(&[
            format!("CREATE ROLE {role} LOGIN PASSWORD 'standby-pw'"),
            format!("GRANT USAGE ON SCHEMA {schema} TO {role}"),
            format!(
                "GRANT SELECT, INSERT, UPDATE ON {schema}.spindle_board_p_lease, \
                 {schema}.spindle_ledger_p_lease TO {role}"
            ),
        ]);
        let owner = owner_url();
        let (scheme, rest) = owner.split_once("://").expect("a URL with a scheme");
        let (_, location) = rest.split_once('@').expect("a URL with a user");
        let url = format!("{scheme}://{role}:standby-pw@{location}");
        let ca = std::env::var("SPINDLE_TEST_DATABASE_CA")
            .expect("SPINDLE_TEST_DATABASE_CA names the test server's CA (task check:postgres)");
        let mut command = run(&environment, &schema, LEASE);
        command
            .env("SPINDLE_SETTING_DATABASE_URL", &url)
            .env("SPINDLE_SETTING_DATABASE_CA", &ca)
            .env("SPINDLE_SETTING_DATABASE_CONNECTIONS", "100");
        let mut standby = Replica::start(command);
        let (status, body) = standby.ready();
        assert_eq!(status, 503, "{body}");

        sql(&[
            format!("ALTER ROLE {role} NOLOGIN"),
            format!(
                "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE usename = '{role}'"
            ),
        ]);
        std::thread::sleep(Duration::from_secs(4));
        let count = |replica: &Replica, text: &str| {
            replica
                .printed()
                .lines()
                .filter(|line| line.contains(text))
                .count()
        };
        let warned = count(&standby, "cannot be made");
        assert!(standby.running(), "{}", standby.printed());
        assert!(
            (1..=2).contains(&warned),
            "{warned} warnings in about six polls (one, or two where the closed connection and \
             the refusal differ): {}",
            standby.printed()
        );
        sql(&[format!("ALTER ROLE {role} LOGIN")]);
        assert!(
            standby.line_by("is made again", Instant::now() + Duration::from_secs(3)),
            "{}",
            standby.printed()
        );
        std::thread::sleep(Duration::from_secs(2));
        assert_eq!(count(&standby, "is made again"), 1, "{}", standby.printed());
        assert_eq!(
            count(&standby, "cannot be made"),
            warned,
            "{}",
            standby.printed()
        );
    }

    /// Waits until `webhook` accepted `count` bodies, at most 30 s.
    fn accepted(webhook: &Webhook, count: usize) {
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
}
