//! The rally example as three `b10x-spindle play` processes, one per component, joined by TCP
//! channels, each started by the `process` launcher from the variables the target contract sets.
//!
//! Every process a test starts is held by a guard that stops it when the guard is dropped, so a
//! failing assertion or a panic never leaves a child running; every wait has a hard bound.

use std::io::BufReader;
use std::io::Write;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};
use spindle_capability::Environment;
use spindle_channel::{Command as Invoked, DeliveryId, Envelope, Frame};
use spindle_target::{Artifact, Exit, LaunchPlan, Launcher, Peer, ProcessLauncher, Resources};
use spindle_telemetry::FrameReader;

const COMPONENTS: [&str; 3] = ["rally-table", "rally-player-west", "rally-player-east"];
/// How many times faster than real time the match clock runs, so a match takes seconds.
const SPEED: &str = "50";
/// The longest any test waits for a match to end.
const MATCH_WITHIN: Duration = Duration::from_secs(120);

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

fn parsed(name: &str) -> Environment {
    let text = std::fs::read_to_string(environment(name)).expect("the environment reads");
    Environment::parse(&text).expect("an environment")
}

/// `dir`, named relative to the crate, the working directory of the test and of every process it
/// starts, so a socket path in it fits `sun_path` however deep the tree lies.
fn relative_to_the_crate(dir: &Path) -> PathBuf {
    let inside = dir
        .canonicalize()
        .expect("the directory")
        .strip_prefix(root())
        .expect("the target directory is in the workspace")
        .to_path_buf();
    Path::new("../..").join(inside)
}

/// Loopback addresses no one listens on now, one per name.
fn free_addresses(count: usize) -> Vec<SocketAddr> {
    let listeners = (0..count)
        .map(|_| TcpListener::bind("127.0.0.1:0").expect("a loopback port"))
        .collect::<Vec<_>>();
    listeners
        .iter()
        .map(|listener| listener.local_addr().expect("bound"))
        .collect()
}

fn play_arguments(environment_file: &str) -> Vec<String> {
    vec![
        "play".to_owned(),
        spec().display().to_string(),
        "--environment".to_owned(),
        environment(environment_file).display().to_string(),
        "--target-score".to_owned(),
        "1".to_owned(),
        "--speed".to_owned(),
        SPEED.to_owned(),
    ]
}

fn plan(component: &str, listen: SocketAddr, peers: Vec<Peer>) -> LaunchPlan {
    LaunchPlan {
        instance_id: format!("{component}-0"),
        component: component.to_owned(),
        settings: Vec::new(),
        artifact: Artifact::HostBinary {
            path: PathBuf::from(env!("CARGO_BIN_EXE_b10x-spindle")),
            args: play_arguments("laptop-processes.toml"),
        },
        request: Resources::default(),
        listen,
        peers,
    }
}

/// Waits at most `within` for `instance` to end; the guard stops it if the test gives up.
fn ended(instance: &mut dyn spindle_target::Running, within: Duration) -> Exit {
    match instance.wait(within).expect("the instance is observable") {
        Some(exit) => exit,
        None => panic!(
            "{} did not end within {within:?}; console:\n{}",
            instance.instance_id(),
            console(instance)
        ),
    }
}

fn console(instance: &dyn spindle_target::Running) -> String {
    std::fs::read_to_string(instance.console()).unwrap_or_default()
}

/// Every JSON line of a console.
fn lines(console: &str) -> Vec<Value> {
    console
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

#[test]
fn a_match_between_three_processes_ends_with_a_winner() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let addresses = free_addresses(COMPONENTS.len());
    let mut launcher = ProcessLauncher::new(dir.path().join("consoles"));
    let mut instances = Vec::new();
    for (index, component) in COMPONENTS.iter().enumerate().rev() {
        let peers = COMPONENTS
            .iter()
            .zip(&addresses)
            .filter(|(other, _)| *other != component)
            .map(|(other, address)| Peer {
                component: (*other).to_owned(),
                address: *address,
            })
            .collect();
        instances.push(
            launcher
                .launch(&plan(component, addresses[index], peers))
                .expect("the instance starts"),
        );
    }
    let table = instances.last_mut().expect("the table");
    assert_eq!(table.instance_id(), "rally-table-0");
    let exit = ended(table.as_mut(), MATCH_WITHIN);
    let table_console = console(table.as_ref());
    assert_eq!(exit.code, Some(0), "the table:\n{table_console}");

    let views = lines(&table_console)
        .into_iter()
        .filter_map(|line| line.get("view").cloned())
        .collect::<Vec<_>>();
    let last = views
        .last()
        .and_then(Value::as_object)
        .unwrap_or_else(|| panic!("the table printed no MatchState view:\n{table_console}"));
    let winner = spindle_host::rally::winner(last)
        .unwrap_or_else(|| panic!("the last view has no winner: {last:?}"));
    assert!(["West", "East"].contains(&winner), "{last:?}");
    assert_eq!(last["state"], json!("Won"), "{last:?}");

    let ran = |console: &str, command: &str| {
        lines(console).iter().any(|line| {
            line.get("ran") == Some(&json!(command))
                && line.get("cause") == Some(&json!("delivered"))
        })
    };
    assert!(
        ran(&table_console, "rally.table.MovePaddle"),
        "a player's request crossed to the table:\n{table_console}"
    );
    for instance in &mut instances[..2] {
        let side = if instance.instance_id().contains("west") {
            "west"
        } else {
            "east"
        };
        let exit = ended(instance.as_mut(), Duration::from_secs(30));
        let text = console(instance.as_ref());
        assert_eq!(
            exit.code,
            Some(0),
            "{} ends once the table is gone:\n{text}",
            instance.instance_id()
        );
        assert!(
            ran(&text, &format!("rally.{side}.ObserveBall")),
            "the table's ball crossed to {}:\n{text}",
            instance.instance_id()
        );
    }
}

/// A child process killed and reaped when dropped.
struct Guard(Child);

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn the_same_bundle_runs_in_one_process_with_channel_in_process() {
    let laptop = parsed("laptop.toml");
    let processes = parsed("laptop-processes.toml");
    assert_eq!(laptop.provides["channel"], ["in-process"]);
    assert_eq!(processes.provides["channel"], ["tcp"]);
    let mut aligned = processes.clone();
    aligned
        .provides
        .insert("channel".to_owned(), laptop.provides["channel"].clone());
    assert_eq!(aligned, laptop, "the documents differ only in channel");

    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let out = dir.path().join("play.out");
    let mut guard = Guard(
        Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
            .args(play_arguments("laptop.toml"))
            .env_remove("SPINDLE_COMPONENT")
            .env_remove("SPINDLE_LISTEN")
            .stdin(Stdio::null())
            .stdout(std::fs::File::create(&out).expect("the output file"))
            .stderr(Stdio::piped())
            .spawn()
            .expect("b10x-spindle starts"),
    );
    let deadline = Instant::now() + MATCH_WITHIN;
    let status = loop {
        if let Some(status) = guard.0.try_wait().expect("the child is observable") {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "play did not end within {MATCH_WITHIN:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    let stdout = std::fs::read_to_string(&out).expect("the output reads");
    assert_eq!(
        status.code(),
        Some(0),
        "{}",
        stdout.lines().last().unwrap_or_default()
    );
    assert!(
        stdout
            .lines()
            .any(|line| line == "West wins" || line == "East wins"),
        "play ended with a winner: {}",
        stdout.lines().rev().take(3).collect::<Vec<_>>().join(" / ")
    );
}

#[test]
fn a_component_process_without_its_peers_is_refused() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let listen = free_addresses(1)[0];
    let mut launcher = ProcessLauncher::new(dir.path().join("consoles"));
    let mut table = launcher
        .launch(&plan("rally-table", listen, Vec::new()))
        .expect("the instance starts");
    let exit = ended(table.as_mut(), Duration::from_secs(30));
    let text = console(table.as_ref());
    assert!(exit.code.is_some_and(|code| code != 0), "{exit:?}:\n{text}");
    for (peer, variable) in [
        ("rally-player-west", "SPINDLE_PEER_RALLY_PLAYER_WEST"),
        ("rally-player-east", "SPINDLE_PEER_RALLY_PLAYER_EAST"),
    ] {
        assert!(
            text.contains(peer) && text.contains(variable),
            "the refusal names {peer} and {variable}:\n{text}"
        );
    }
    assert!(
        !lines(&text).iter().any(|line| line.get("view").is_some()),
        "nothing was played:\n{text}"
    );
}

/// Writes one link message: a 4-byte big-endian length and that many bytes of JSON.
fn say(stream: &mut TcpStream, message: &Value) {
    let body = serde_json::to_vec(message).expect("encodes");
    let size = u32::try_from(body.len()).expect("small");
    stream.write_all(&size.to_be_bytes()).expect("writes");
    stream.write_all(&body).expect("writes");
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

#[test]
fn a_component_process_reports_telemetry_when_given_an_endpoint() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let socket = relative_to_the_crate(dir.path()).join("telemetry.sock");
    let collector = UnixListener::bind(&socket).expect("the collector listens");
    collector
        .set_nonblocking(true)
        .expect("a non-blocking collector");
    let table = TcpListener::bind("127.0.0.1:0").expect("a stand-in table listens");
    let listen = free_addresses(1)[0];
    let mut launcher = ProcessLauncher::new(dir.path().join("consoles"))
        .with_telemetry(&socket)
        .expect("the endpoint fits");
    let mut west = launcher
        .launch(&plan(
            "rally-player-west",
            listen,
            vec![Peer {
                component: "rally-table".to_owned(),
                address: table.local_addr().expect("bound"),
            }],
        ))
        .expect("the instance starts");

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut ball = loop {
        match TcpStream::connect(listen) {
            Ok(stream) => break stream,
            Err(_) => {
                assert!(
                    Instant::now() < deadline,
                    "the player never listened:\n{}",
                    console(west.as_ref())
                );
                assert!(
                    west.wait(Duration::from_millis(20))
                        .expect("observable")
                        .is_none(),
                    "the player ended:\n{}",
                    console(west.as_ref())
                );
            }
        }
    };
    say(
        &mut ball,
        &json!({"hello": {"from": "rally-table", "to": "rally-player-west"}}),
    );
    let envelope = Frame::Deliver(Envelope {
        id: DeliveryId(1),
        binding: "west-watches".to_owned(),
        from: "rally-table".to_owned(),
        to: "rally-player-west".to_owned(),
        delivery: ess_domain::binding::Delivery::AtMostOnce,
        on_failure: ess_domain::binding::Failure::Drop,
        attempt: 1,
        command: Invoked {
            name: "rally.west.ObserveBall".to_owned(),
            fields: object(json!({"match_id": "m1", "ball_x": 8, "ball_y": 4, "paddle_y": 4})),
        },
    });
    say(&mut ball, &json!({ "frame": envelope }));

    let stream = loop {
        match collector.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    Instant::now() < deadline,
                    "the player never connected to the collector:\n{}",
                    console(west.as_ref())
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("accept: {error}"),
        }
    };
    stream.set_nonblocking(false).expect("a blocking stream");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a read timeout");
    let mut frames = FrameReader::new(BufReader::new(stream));
    let loaded = spindle_spec::load(&spec()).expect("rally compiles");
    let catalogue =
        spindle_telemetry::catalogue(&loaded.ir, "rally-player-west").expect("a component");
    let expected = catalogue
        .counters
        .iter()
        .map(|counter| counter.name.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let counted = "command.rally.west.ObserveBall.first-sight";
    assert!(expected.iter().any(|name| name == counted), "{expected:?}");
    let mut seen = false;
    for _ in 0..50 {
        let frame = frames
            .next_frame()
            .expect("a read")
            .expect("a frame while the player runs");
        assert_eq!(frame.instance_id, "rally-player-west-0");
        let names = frame
            .counters
            .iter()
            .map(|counter| counter.name.clone())
            .chain(
                frame
                    .histograms
                    .iter()
                    .map(|histogram| histogram.name.clone()),
            )
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            names, expected,
            "the frame carries the player's own counters and histograms"
        );
        if frame
            .counters
            .iter()
            .any(|counter| counter.name == counted && counter.value == 1)
        {
            seen = true;
            break;
        }
    }
    assert!(
        seen,
        "the delivered ball was counted:\n{}",
        console(west.as_ref())
    );
    drop(ball);
    west.stop().expect("the player stops");
}
