//! Second adversary pass on `play --component`: the rally example as three processes joined by
//! TCP, against a stand-in peer that speaks the link protocol by hand, and at further speeds.
//!
//! Every process a case starts is held by a [`Proc`] guard that sends `SIGKILL` to its process
//! group and reaps it when dropped, so a failing assertion never leaves a child running; every
//! wait has a hard bound.

use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rustix::process::{Pid, Signal, kill_process_group};
use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_host::{Host, ManualClock};
use spindle_target::{ENV_COMPONENT, ENV_LISTEN, ENV_PEER_PREFIX, env_name};

const TABLE: &str = "rally-table";
const WEST: &str = "rally-player-west";
const EAST: &str = "rally-player-east";
const COMPONENTS: [&str; 3] = [TABLE, WEST, EAST];
const MATCH_WITHIN: Duration = Duration::from_secs(180);
const POLL: Duration = Duration::from_millis(20);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace")
}

fn spec() -> PathBuf {
    root().join("examples/rally/ess")
}

fn environment(name: &str) -> PathBuf {
    root().join("examples/environments").join(name)
}

fn free_addresses(count: usize) -> Vec<SocketAddr> {
    let listeners = (0..count)
        .map(|_| TcpListener::bind("127.0.0.1:0").expect("a loopback port"))
        .collect::<Vec<_>>();
    listeners
        .iter()
        .map(|listener| listener.local_addr().expect("bound"))
        .collect()
}

/// One `b10x-spindle play --component` process, killed with its group and reaped on drop.
struct Proc {
    child: Child,
    console: PathBuf,
    status: Option<ExitStatus>,
}

impl Proc {
    fn spawn(
        dir: &Path,
        component: &str,
        listen: SocketAddr,
        peers: &[(&str, SocketAddr)],
        target_score: u32,
        speed: u32,
    ) -> Self {
        let tmp = dir.join(format!("{component}.tmp"));
        std::fs::create_dir_all(&tmp).expect("the process's TMPDIR");
        let console = dir.join(format!("{component}.log"));
        let out = std::fs::File::create(&console).expect("the console");
        let err = out.try_clone().expect("the console, twice");
        let mut command = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"));
        command
            .args([
                "play".to_owned(),
                spec().display().to_string(),
                "--environment".to_owned(),
                environment("laptop-processes.toml").display().to_string(),
                "--target-score".to_owned(),
                target_score.to_string(),
                "--speed".to_owned(),
                speed.to_string(),
            ])
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err)
            .process_group(0);
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("SPINDLE_") {
                command.env_remove(name);
            }
        }
        command
            .env(ENV_COMPONENT, component)
            .env(ENV_LISTEN, listen.to_string())
            .env("TMPDIR", &tmp);
        for (peer, address) in peers {
            command.env(
                format!("{ENV_PEER_PREFIX}{}", env_name(peer)),
                address.to_string(),
            );
        }
        let child = command.spawn().expect("b10x-spindle starts");
        Self {
            child,
            console,
            status: None,
        }
    }

    fn wait(&mut self, within: Duration) -> Option<ExitStatus> {
        let deadline = Instant::now() + within;
        loop {
            if let Some(status) = self.status {
                return Some(status);
            }
            if let Some(status) = self.child.try_wait().expect("observable") {
                self.status = Some(status);
                return Some(status);
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(POLL);
        }
    }

    fn console(&self) -> String {
        std::fs::read_to_string(&self.console).unwrap_or_default()
    }

    fn views(&self) -> Vec<Map<String, Value>> {
        self.console()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter_map(|line| line.get("view").and_then(Value::as_object).cloned())
            .collect()
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        if self.status.is_none() {
            let _ = kill_process_group(Pid::from_child(&self.child), Signal::KILL);
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn describe(status: Option<ExitStatus>) -> String {
    match status {
        None => "still running".to_owned(),
        Some(status) => match (status.code(), status.signal()) {
            (Some(code), _) => format!("exit {code}"),
            (None, Some(signal)) => format!("killed by signal {signal}"),
            _ => format!("{status:?}"),
        },
    }
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

fn step(view: &Map<String, Value>) -> Value {
    json!([
        view["ball_x"],
        view["ball_y"],
        view["west_score"],
        view["east_score"],
        view.get("winner")
    ])
}

/// The match one process plays, one step per tick until a winner, on a manual clock.
fn one_process_match(target_score: u32) -> Vec<Value> {
    let loaded = spindle_spec::load(&spec()).expect("rally compiles");
    let text = std::fs::read_to_string(environment("laptop.toml")).expect("laptop.toml");
    let laptop = Environment::parse(&text).expect("an environment");
    let state = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a state directory");
    let clock = Arc::new(ManualClock::new());
    let mut host = Host::new(loaded.ir, &laptop, clock.clone(), state.path()).expect("admitted");
    host.submit(
        "rally.table.OpenMatch",
        object(json!({"match_id": "rally", "target_score": target_score})),
    )
    .expect("the match opens");
    host.start("tick", object(json!({"match_id": "rally"})))
        .expect("the tick starts");
    let mut steps = Vec::new();
    for _ in 0..20_000 {
        clock.advance(Duration::from_secs(1));
        host.advance().expect("the host advances");
        let view = host
            .view("rally.table.MatchState", &json!("rally"))
            .expect("the view answers")
            .expect("the match exists");
        steps.push(step(&view));
        if view.get("winner").is_some() {
            return steps;
        }
    }
    panic!("one process found no winner in 20000 ticks");
}

fn three(dir: &Path, target_score: u32, speed: u32) -> (Proc, Proc, Proc) {
    let addresses = free_addresses(3);
    let peers_of = |me: &str| {
        COMPONENTS
            .iter()
            .zip(&addresses)
            .filter(|(other, _)| **other != me)
            .map(|(other, address)| (*other, *address))
            .collect::<Vec<_>>()
    };
    let west = Proc::spawn(
        dir,
        WEST,
        addresses[1],
        &peers_of(WEST),
        target_score,
        speed,
    );
    let east = Proc::spawn(
        dir,
        EAST,
        addresses[2],
        &peers_of(EAST),
        target_score,
        speed,
    );
    let table = Proc::spawn(
        dir,
        TABLE,
        addresses[0],
        &peers_of(TABLE),
        target_score,
        speed,
    );
    (table, west, east)
}

/// Three processes at `speed` print, tick for tick, the match one process plays: every view is
/// the next step, none skipped and none repeated.
fn every_tick_as_one_process(speed: u32) {
    let target_score = 1;
    let reference = one_process_match(target_score);
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let (mut table, _west, _east) = three(dir.path(), target_score, speed);
    let status = table.wait(MATCH_WITHIN);
    assert_eq!(
        status.and_then(|status| status.code()),
        Some(0),
        "the table: {}\n{}",
        describe(status),
        table.console()
    );
    let steps = table.views().iter().map(step).collect::<Vec<_>>();
    let first = steps
        .iter()
        .zip(&reference)
        .position(|(three, one)| three != one);
    assert!(
        first.is_none() && steps.len() == reference.len(),
        "at --speed {speed} three processes printed {} views, one process plays {} ticks; first \
         difference at {first:?}: three {:?} / one {:?}",
        steps.len(),
        reference.len(),
        first.map(|at| &steps[at]),
        first.map(|at| &reference[at])
    );
}

#[test]
fn three_processes_print_every_tick_one_process_plays_on_a_slow_clock() {
    every_tick_as_one_process(10);
}

fn send(stream: &mut TcpStream, message: &Value) {
    let body = serde_json::to_vec(message).expect("encodes");
    let mut bytes = u32::try_from(body.len())
        .expect("small")
        .to_be_bytes()
        .to_vec();
    bytes.extend_from_slice(&body);
    stream.write_all(&bytes).expect("the write");
}

/// One whole message within `within`; `Err` once the stream closed.
fn read_message(stream: &mut TcpStream, within: Duration) -> Result<Option<Value>, ()> {
    stream
        .set_read_timeout(Some(within.max(Duration::from_millis(1))))
        .map_err(|_| ())?;
    let mut prefix = [0_u8; 4];
    match stream.read(&mut prefix[..1]) {
        Ok(0) => return Err(()),
        Ok(_) => {}
        Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
            return Ok(None);
        }
        Err(_) => return Err(()),
    }
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|_| ())?;
    stream.read_exact(&mut prefix[1..]).map_err(|_| ())?;
    let mut body = vec![0_u8; u32::from_be_bytes(prefix) as usize];
    stream.read_exact(&mut body).map_err(|_| ())?;
    serde_json::from_slice(&body).map(Some).map_err(|_| ())
}

/// A stand-in west player says `end` to the table three envelopes into a match nobody has won;
/// the table must not take that as the match's end.
#[test]
fn end_from_a_player_does_not_end_the_table_before_a_winner() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let [table_at, east_at] = free_addresses(2)[..] else {
        unreachable!()
    };
    let west = TcpListener::bind("127.0.0.1:0").expect("a stand-in west");
    let west_at = west.local_addr().expect("bound");
    let mut east = Proc::spawn(dir.path(), EAST, east_at, &[(TABLE, table_at)], 50, 50);
    let mut table = Proc::spawn(
        dir.path(),
        TABLE,
        table_at,
        &[(WEST, west_at), (EAST, east_at)],
        50,
        50,
    );
    west.set_nonblocking(true).expect("non-blocking");
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut from_table = loop {
        match west.accept() {
            Ok((stream, _)) => break stream,
            Err(_) => {
                assert!(
                    Instant::now() < deadline,
                    "the table never connected to west"
                );
                assert!(table.wait(Duration::ZERO).is_none(), "{}", table.console());
                std::thread::sleep(POLL);
            }
        }
    };
    from_table.set_nonblocking(false).expect("blocking");
    let mut to_table = loop {
        match TcpStream::connect(table_at) {
            Ok(stream) => break stream,
            Err(_) => {
                assert!(Instant::now() < deadline, "the table never listened");
                std::thread::sleep(POLL);
            }
        }
    };
    send(
        &mut to_table,
        &json!({"hello": {"from": WEST, "to": TABLE}}),
    );
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = Arc::clone(&stop);
    let player = std::thread::spawn(move || {
        let mut envelopes = 0;
        let deadline = Instant::now() + Duration::from_secs(60);
        while Instant::now() < deadline && !stopped.load(Ordering::SeqCst) {
            match read_message(&mut from_table, Duration::from_millis(50)) {
                Ok(Some(message)) if message.get("frame").is_some() => {
                    send(&mut from_table, &json!({"settled": 1}));
                    envelopes += 1;
                    if envelopes == 3 {
                        send(&mut to_table, &json!({"end": true}));
                    }
                }
                Ok(_) => {}
                Err(()) => break,
            }
        }
        envelopes
    });
    let status = table.wait(Duration::from_secs(20));
    let console = table.console();
    stop.store(true, Ordering::SeqCst);
    let envelopes = player.join().expect("the stand-in west");
    drop(table);
    let _ = east.wait(Duration::ZERO);
    assert!(
        envelopes >= 3,
        "the stand-in west got {envelopes} envelopes:\n{console}"
    );
    let won = console.contains("the match has a winner");
    assert!(
        !(status.and_then(|status| status.code()) == Some(0) && !won),
        "the stand-in west said end after {envelopes} envelopes of a match to 50 points; the \
         table ended {} with no winner:\n{}",
        describe(status),
        console.lines().rev().take(4).collect::<Vec<_>>().join("\n")
    );
}
