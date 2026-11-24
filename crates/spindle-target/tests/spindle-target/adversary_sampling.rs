//! Adversarial case for host-side sampling: `ProcessInstance::resources` promises `None` once the
//! instance has ended (`crates/spindle-target/src/process.rs`, doc of `resources`), and
//! `process_usage` treats an unreaped process as ended. A control process samples on a period
//! without calling `wait` in between, so an instance that exits between two `wait`s is a zombie
//! group leader that `process_group_usage` still counts as a member.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use spindle_target::{Artifact, LaunchPlan, Launcher, ProcessLauncher, Resources};

const STEP: Duration = Duration::from_millis(10);
const STEPS: u32 = 500;

#[test]
fn an_instance_that_ended_without_a_wait_is_not_sampled() {
    let consoles = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("tempdir");
    let mut launcher = ProcessLauncher::new(consoles.path());
    let plan = LaunchPlan {
        instance_id: "ends-1".to_owned(),
        component: "rally-table".to_owned(),
        settings: [("mode", "exit"), ("exit_code", "0")]
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value.to_owned()))
            .collect(),
        artifact: Artifact::HostBinary {
            path: PathBuf::from(env!("CARGO_BIN_EXE_spindle-target-fixture")),
            args: Vec::new(),
        },
        request: Resources::default(),
        listen: SocketAddr::from(([127, 0, 0, 1], 7300)),
        peers: Vec::new(),
    };
    let mut running = launcher.launch(&plan).expect("starts");
    let mut steps = 0;
    let console = loop {
        let console = std::fs::read_to_string(running.console()).unwrap_or_default();
        if console.contains("ready") {
            break console;
        }
        assert!(steps < STEPS, "the fixture never wrote ready");
        steps += 1;
        std::thread::sleep(STEP);
    };
    let pid = console
        .lines()
        .find_map(|line| line.strip_prefix("pid "))
        .and_then(|pid| pid.trim().parse::<u32>().ok())
        .unwrap_or_else(|| panic!("the fixture names its pid:\n{console}"));

    // The fixture exits at once. Waits until its leader is a zombie, as `/proc` shows it, before
    // the one sample (`story:ci-tests-deterministic`): a resident set of zero, which this case
    // waited for before, is read already while the process is on its way out and not yet a
    // zombie, and a sample then is rightly of a running instance. Nothing reaps it meanwhile:
    // only `wait` does.
    let mut steps = 0;
    while state(pid) != Some('Z') {
        assert!(
            steps < STEPS,
            "the fixture never became a zombie: {:?}",
            state(pid)
        );
        steps += 1;
        std::thread::sleep(STEP);
    }
    let sampled = running.resources();
    let ended = running
        .wait(Duration::ZERO)
        .expect("the instance is observed");
    assert!(ended.is_some(), "the fixture has exited by now");
    assert_eq!(
        sampled, None,
        "the instance has ended (exited, not yet reaped), and is still sampled as running"
    );
}

/// The state of process `pid` (field 3 of `/proc/<pid>/stat`), `None` once it is gone.
fn state(pid: u32) -> Option<char> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (_, rest) = stat.rsplit_once(')')?;
    rest.split_ascii_whitespace().next()?.chars().next()
}
