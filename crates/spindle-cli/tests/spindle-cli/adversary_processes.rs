//! Adversary cases for `play --component`: the rally example as three processes joined by TCP.
//!
//! Every process a case starts is held by a [`Proc`] guard that sends `SIGKILL` to its process
//! group and reaps it when dropped, so a failing assertion never leaves a child running; every
//! wait has a hard bound. Each process gets its own `TMPDIR` inside the case's directory, so the
//! state directory it creates can be looked for after it ends.
//!
//! Each case starts its processes on a loopback host of its own (`127.53.0.<n>`, [`host_of`]) at
//! ports it holds on `127.0.0.1` until it ends ([`ports`]): no other process can take a port
//! between the case's choice and the process's bind, and a connect, which leaves from
//! `127.0.0.1`, never reaches itself.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
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
/// The test runner kills a case at this age (`.config/nextest.toml`).
const KILL: Duration = Duration::from_secs(60);
/// How long the table may take to play a match once every process has started.
const MATCH_WITHIN: Duration = Duration::from_secs(40);
/// How long a player may take to end once the table has.
const PEERS_END_WITHIN: Duration = Duration::from_secs(5);
/// How long the slow peer starts after the others.
const SLOW_START: Duration = Duration::from_secs(3);
const _: () = assert!(
    SLOW_START.as_secs() + MATCH_WITHIN.as_secs() + 2 * PEERS_END_WITHIN.as_secs() < KILL.as_secs(),
    "a match is bounded below the runner's kill, so a slow one fails naming its bound"
);
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

/// Every case of this file, in order: the `n`-th listens on `127.53.0.<n>`.
const CASES: [&str; 11] = [
    "each_case_listens_on_a_loopback_host_of_its_own",
    "three_processes_play_the_match_one_process_plays_at_the_tested_speed",
    "three_processes_play_the_match_one_process_plays_on_a_fast_clock",
    "a_player_exits_non_zero_when_the_table_dies_mid_match",
    "the_table_exits_non_zero_when_both_players_die_mid_match",
    "the_table_exits_non_zero_naming_a_player_that_dies_mid_match",
    "sigterm_while_waiting_for_a_peer_ends_within_the_launchers_grace_and_leaves_no_state",
    "sigterm_mid_match_stops_every_process_and_leaves_no_state",
    "a_peer_slow_to_start_is_reached_within_the_connect_bound",
    "a_peer_that_never_listens_ends_the_process_naming_it_within_the_bound",
    "a_connection_that_opens_and_closes_before_the_table_does_not_end_a_player",
];

/// The loopback host the processes of `case` listen on.
fn host_of(case: &str) -> Ipv4Addr {
    let place = CASES
        .iter()
        .position(|listed| *listed == case)
        .unwrap_or_else(|| panic!("{case} is not in CASES"));
    Ipv4Addr::new(
        127,
        53,
        0,
        u8::try_from(place + 1).expect("fewer than 255 cases"),
    )
}

/// The addresses one case hands its processes, each on the case's own host, at a port held on
/// `127.0.0.1` for as long as this lives.
struct Ports {
    addresses: Vec<SocketAddr>,
    _held: Vec<TcpListener>,
}

/// `count` addresses for the processes of `case`.
fn ports(case: &str, count: usize) -> Ports {
    let host = IpAddr::V4(host_of(case));
    let held = (0..count)
        .map(|_| TcpListener::bind("127.0.0.1:0").expect("a loopback port"))
        .collect::<Vec<_>>();
    let addresses = held
        .iter()
        .map(|listener| SocketAddr::new(host, listener.local_addr().expect("bound").port()))
        .collect();
    Ports {
        addresses,
        _held: held,
    }
}

/// One `b10x-spindle play --component` process, killed with its group and reaped on drop.
struct Proc {
    name: String,
    child: Child,
    console: PathBuf,
    tmp: PathBuf,
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
            name: component.to_owned(),
            child,
            console,
            tmp,
            status: None,
        }
    }

    fn pid(&self) -> Pid {
        Pid::from_child(&self.child)
    }

    fn signal(&self, signal: Signal) {
        let _ = kill_process_group(self.pid(), signal);
    }

    /// The exit status, waiting at most `within`; `None` while it still runs.
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

    /// Every `{"view": …}` the process printed, in order.
    fn views(&self) -> Vec<Map<String, Value>> {
        self.console()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter_map(|line| line.get("view").and_then(Value::as_object).cloned())
            .collect()
    }

    /// Waits at most `within` until the process has printed `count` views.
    fn await_views(&mut self, count: usize, within: Duration) {
        let deadline = Instant::now() + within;
        while self.views().len() < count {
            assert!(
                self.wait(Duration::ZERO).is_none(),
                "{} ended before {count} views:\n{}",
                self.name,
                self.console()
            );
            assert!(
                Instant::now() < deadline,
                "{} printed no {count} views within {within:?}:\n{}",
                self.name,
                self.console()
            );
            std::thread::sleep(POLL);
        }
    }

    /// What is left in the process's `TMPDIR`.
    fn left_behind(&self) -> Vec<String> {
        std::fs::read_dir(&self.tmp)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        if self.status.is_none() {
            self.signal(Signal::KILL);
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// The three components, started at once at `ports`, the table last.
fn three(dir: &Path, ports: &Ports, target_score: u32, speed: u32) -> (Proc, Proc, Proc) {
    let addresses = &ports.addresses;
    let peers_of = |me: &str| {
        COMPONENTS
            .iter()
            .zip(addresses)
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

/// What a case prints when `what` ended with `status` after `took`: the bound it had, the time
/// it took, and the console of every process in `procs`.
fn missed(what: &str, status: Option<ExitStatus>, took: Duration, procs: &[&Proc]) -> String {
    format!(
        "{what}: {} after {took:.1?}, the bound {MATCH_WITHIN:?}\n{}",
        describe(status),
        consoles(procs)
    )
}

/// The console of every process in `procs`, each under its name and how it ended.
fn consoles(procs: &[&Proc]) -> String {
    procs
        .iter()
        .map(|proc| {
            format!(
                "--- {} ({}) ---\n{}",
                proc.name,
                describe(proc.status),
                proc.console()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

/// What a tick changes about the match, read from a `MatchState` row.
fn step(view: &Map<String, Value>) -> Value {
    json!([
        view["ball_x"],
        view["ball_y"],
        view["west_score"],
        view["east_score"],
        view.get("winner")
    ])
}

/// The match one process plays: the rally example under `Host::new` with `laptop.toml`, as
/// `play` without `--component` runs it, on a manual clock, one view per tick until a winner.
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

/// Whether `part` appears in `whole` in order, each element after the one before.
fn is_subsequence(part: &[Value], whole: &[Value]) -> Option<usize> {
    let mut at = 0;
    for (index, wanted) in part.iter().enumerate() {
        match whole[at..].iter().position(|value| value == wanted) {
            Some(found) => at += found + 1,
            None => return Some(index),
        }
    }
    None
}

fn same_match_as_one_process(case: &str, speed: u32) {
    let target_score = 1;
    let reference = one_process_match(target_score);
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let ports = ports(case, 3);
    let (mut table, mut west, mut east) = three(dir.path(), &ports, target_score, speed);
    let started = Instant::now();
    let status = table.wait(MATCH_WITHIN);
    let took = started.elapsed();
    if status.is_some() {
        let _ = (west.wait(PEERS_END_WITHIN), east.wait(PEERS_END_WITHIN));
    }
    assert_eq!(
        status.and_then(|status| status.code()),
        Some(0),
        "{}",
        missed("the table", status, took, &[&table, &west, &east])
    );
    let steps = table.views().iter().map(step).collect::<Vec<_>>();
    assert_eq!(
        steps.last(),
        reference.last(),
        "at --speed {speed} three processes end the match ({} views) another way than one \
         process does ({} ticks)",
        steps.len(),
        reference.len()
    );
    if let Some(index) = is_subsequence(&steps, &reference) {
        panic!(
            "at --speed {speed} view {index} of three processes, {}, is not in the match one \
             process plays from that point on; first views: three {:?} / one {:?}",
            steps[index],
            &steps[..steps.len().min(index + 3)],
            &reference[..reference.len().min(index + 3)]
        );
    }
}

#[test]
fn each_case_listens_on_a_loopback_host_of_its_own() {
    let source = include_str!("adversary_processes.rs");
    let mut cases = Vec::new();
    for case in source.split("#[test]\nfn ").skip(1) {
        let (name, body) = case.split_once('(').expect("a case's name");
        let body = body.split("\n}\n").next().expect("a case's body");
        assert!(
            body.contains(&format!("\"{name}\"")),
            "{name} asks for its ports under its own name"
        );
        cases.push(name);
    }
    let mut listed = CASES.to_vec();
    listed.sort_unstable();
    cases.sort_unstable();
    assert_eq!(cases, listed, "CASES lists every case of this file");
    let hosts = CASES.map(host_of);
    for (case, host) in CASES.iter().zip(&hosts) {
        assert!(
            host.is_loopback() && *host != Ipv4Addr::LOCALHOST,
            "{case} listens on {host}, which is not a loopback host of its own"
        );
        assert_eq!(
            hosts.iter().filter(|other| *other == host).count(),
            1,
            "{case} shares {host}"
        );
    }

    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let ports = ports("each_case_listens_on_a_loopback_host_of_its_own", 2);
    let [listen, table] = ports.addresses[..] else {
        panic!("two addresses");
    };
    assert!(
        TcpListener::bind((Ipv4Addr::LOCALHOST, listen.port())).is_err(),
        "the case holds 127.0.0.1:{}",
        listen.port()
    );
    let mut west = Proc::spawn(dir.path(), WEST, listen, &[(TABLE, table)], 1, 50);
    let line = format!("\"listening\":\"{listen}\"");
    let deadline = Instant::now() + Duration::from_secs(20);
    while !west.console().contains(&line) {
        assert!(
            west.wait(Duration::ZERO).is_none() && Instant::now() < deadline,
            "{WEST} at {listen}, with 127.0.0.1:{} held, printed no {line}: {}\n{}",
            listen.port(),
            describe(west.status),
            west.console()
        );
        std::thread::sleep(POLL);
    }
}

#[test]
fn three_processes_play_the_match_one_process_plays_at_the_tested_speed() {
    same_match_as_one_process(
        "three_processes_play_the_match_one_process_plays_at_the_tested_speed",
        50,
    );
}

#[test]
fn three_processes_play_the_match_one_process_plays_on_a_fast_clock() {
    same_match_as_one_process(
        "three_processes_play_the_match_one_process_plays_on_a_fast_clock",
        1000,
    );
}

#[test]
fn a_player_exits_non_zero_when_the_table_dies_mid_match() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let ports = ports("a_player_exits_non_zero_when_the_table_dies_mid_match", 3);
    let (mut table, mut west, mut east) = three(dir.path(), &ports, 9, 50);
    table.await_views(5, Duration::from_secs(60));
    assert!(
        table
            .views()
            .iter()
            .all(|view| view.get("winner").is_none()),
        "the match is still on when the table dies"
    );
    table.signal(Signal::KILL);
    let killed = table.wait(Duration::from_secs(5));
    assert!(killed.is_some(), "the table was killed");
    for player in [&mut west, &mut east] {
        let status = player.wait(Duration::from_secs(15));
        assert!(
            status.is_some_and(|status| status.code().is_some_and(|code| code != 0)),
            "{} must fail when its table dies mid-match; it ended with {}:\n{}",
            player.name,
            describe(status),
            player.console()
        );
    }
}

#[test]
fn the_table_exits_non_zero_when_both_players_die_mid_match() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let ports = ports(
        "the_table_exits_non_zero_when_both_players_die_mid_match",
        3,
    );
    let (mut table, mut west, mut east) = three(dir.path(), &ports, 9, 50);
    table.await_views(5, Duration::from_secs(60));
    west.signal(Signal::KILL);
    east.signal(Signal::KILL);
    assert!(
        west.wait(Duration::from_secs(5)).is_some(),
        "west was killed"
    );
    assert!(
        east.wait(Duration::from_secs(5)).is_some(),
        "east was killed"
    );
    let status = table.wait(Duration::from_secs(15));
    let views = table.views();
    assert!(
        status.is_some_and(|status| status.code().is_some_and(|code| code != 0)),
        "the table must fail when both players die mid-match; it ended with {} after {} views, \
         the last {:?}:\n{}",
        describe(status),
        views.len(),
        views.last().map(step),
        table
            .console()
            .lines()
            .rev()
            .take(3)
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn the_table_exits_non_zero_naming_a_player_that_dies_mid_match() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let ports = ports(
        "the_table_exits_non_zero_naming_a_player_that_dies_mid_match",
        3,
    );
    let (mut table, mut west, _east) = three(dir.path(), &ports, 9, 50);
    table.await_views(5, Duration::from_secs(60));
    west.signal(Signal::KILL);
    assert!(
        west.wait(Duration::from_secs(5)).is_some(),
        "west was killed"
    );
    let status = table.wait(Duration::from_secs(15));
    let console = table.console();
    assert!(
        status.is_some_and(|status| status.code().is_some_and(|code| code != 0)),
        "the table ended with {}:\n{console}",
        describe(status)
    );
    let last = console.lines().rev().take(5).collect::<Vec<_>>().join("\n");
    assert!(last.contains(WEST), "the error names {WEST}:\n{last}");
}

#[test]
fn sigterm_while_waiting_for_a_peer_ends_within_the_launchers_grace_and_leaves_no_state() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let ports = ports(
        "sigterm_while_waiting_for_a_peer_ends_within_the_launchers_grace_and_leaves_no_state",
        3,
    );
    let addresses = &ports.addresses;
    let mut table = Proc::spawn(
        dir.path(),
        TABLE,
        addresses[0],
        &[(WEST, addresses[1]), (EAST, addresses[2])],
        1,
        50,
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    while table.left_behind().is_empty() {
        assert!(
            Instant::now() < deadline,
            "the table made no state directory"
        );
        assert!(table.wait(Duration::ZERO).is_none(), "{}", table.console());
        std::thread::sleep(POLL);
    }
    std::thread::sleep(Duration::from_millis(300));
    table.signal(Signal::TERM);
    let status = table.wait(Duration::from_secs(5));
    if status.is_none() {
        table.signal(Signal::KILL);
        table.wait(Duration::from_secs(5));
    }
    assert_eq!(
        status.and_then(|status| status.code()),
        Some(143),
        "SIGTERM while connecting must end the table within the process launcher's 5 s grace; \
         it was {} after 5 s, and after the launcher's SIGKILL its TMPDIR holds {:?}:\n{}",
        describe(status),
        table.left_behind(),
        table.console()
    );
    assert_eq!(table.left_behind(), Vec::<String>::new());
}

#[test]
fn sigterm_mid_match_stops_every_process_and_leaves_no_state() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let ports = ports(
        "sigterm_mid_match_stops_every_process_and_leaves_no_state",
        3,
    );
    let (mut table, mut west, mut east) = three(dir.path(), &ports, 9, 50);
    table.await_views(5, Duration::from_secs(60));
    for proc in [&table, &west, &east] {
        proc.signal(Signal::TERM);
    }
    for proc in [&mut table, &mut west, &mut east] {
        let status = proc.wait(Duration::from_secs(5));
        assert_eq!(
            status.and_then(|status| status.code()),
            Some(143),
            "{}: {}\n{}",
            proc.name,
            describe(status),
            proc.console()
        );
        assert_eq!(proc.left_behind(), Vec::<String>::new(), "{}", proc.name);
    }
}

#[test]
fn a_peer_slow_to_start_is_reached_within_the_connect_bound() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let ports = ports(
        "a_peer_slow_to_start_is_reached_within_the_connect_bound",
        3,
    );
    let addresses = &ports.addresses;
    let peers_of = |me: &str| {
        COMPONENTS
            .iter()
            .zip(addresses)
            .filter(|(other, _)| **other != me)
            .map(|(other, address)| (*other, *address))
            .collect::<Vec<_>>()
    };
    let mut table = Proc::spawn(dir.path(), TABLE, addresses[0], &peers_of(TABLE), 1, 50);
    let mut east = Proc::spawn(dir.path(), EAST, addresses[2], &peers_of(EAST), 1, 50);
    std::thread::sleep(SLOW_START);
    assert!(
        table.wait(Duration::ZERO).is_none(),
        "the table ended before west started {SLOW_START:?} after it:\n{}",
        consoles(&[&table, &east])
    );
    let mut west = Proc::spawn(dir.path(), WEST, addresses[1], &peers_of(WEST), 1, 50);
    let started = Instant::now();
    let status = table.wait(MATCH_WITHIN);
    let took = started.elapsed();
    if status.is_some() {
        let _ = (west.wait(PEERS_END_WITHIN), east.wait(PEERS_END_WITHIN));
    }
    let views = table.views();
    assert_eq!(
        status.and_then(|status| status.code()),
        Some(0),
        "{}",
        missed("the table", status, took, &[&table, &west, &east])
    );
    assert!(
        views
            .last()
            .is_some_and(|view| view.get("winner").is_some()),
        "{:?}",
        views.last()
    );
}

#[test]
fn a_peer_that_never_listens_ends_the_process_naming_it_within_the_bound() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let ports = ports(
        "a_peer_that_never_listens_ends_the_process_naming_it_within_the_bound",
        2,
    );
    let addresses = &ports.addresses;
    let mut west = Proc::spawn(
        dir.path(),
        WEST,
        addresses[0],
        &[(TABLE, addresses[1])],
        1,
        50,
    );
    let status = west.wait(Duration::from_secs(45));
    let console = west.console();
    assert!(
        status.is_some_and(|status| status.code().is_some_and(|code| code != 0)),
        "{}:\n{console}",
        describe(status)
    );
    assert!(console.contains(TABLE), "{console}");
    assert_eq!(west.left_behind(), Vec::<String>::new());
}

#[test]
fn a_connection_that_opens_and_closes_before_the_table_does_not_end_a_player() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let stand_in = TcpListener::bind("127.0.0.1:0").expect("a stand-in table");
    let ports = ports(
        "a_connection_that_opens_and_closes_before_the_table_does_not_end_a_player",
        1,
    );
    let listen = ports.addresses[0];
    let mut west = Proc::spawn(
        dir.path(),
        WEST,
        listen,
        &[(TABLE, stand_in.local_addr().expect("bound"))],
        1,
        50,
    );
    let deadline = Instant::now() + Duration::from_secs(20);
    while !west.console().contains("\"listening\"") {
        assert!(Instant::now() < deadline, "{}", west.console());
        assert!(west.wait(Duration::ZERO).is_none(), "{}", west.console());
        std::thread::sleep(POLL);
    }
    drop(TcpStream::connect(listen).expect("a probe connects"));
    let status = west.wait(Duration::from_secs(2));
    assert!(
        status.is_none(),
        "a probe that connected and closed ended {} before its table ever linked: {}:\n{}",
        WEST,
        describe(status),
        west.console()
    );
}
