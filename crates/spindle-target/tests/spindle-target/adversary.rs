use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rustix::process::{Pid, Signal, kill_process};
use spindle_capability::{Admission, Capability};
use spindle_target::{
    Artifact, EnvironmentAdmission, Exit, LaunchError, LaunchPlan, Launcher, Peer, Placement,
    ProcessLauncher, Refusal, ResourceKind, Resources, Target, TargetKind, TargetState, place,
};

const ROLE: &str = "SPINDLE_SETTING_ADVERSARY_ROLE";
const GRANDCHILD: &str = "ADVERSARY_GRANDCHILD";
const HELPER: &str = "adversary::adversary_helper_process";

unsafe extern "C" {
    fn signal(signum: i32, handler: usize) -> usize;
}

fn helper_args() -> Vec<String> {
    [HELPER, "--exact", "--nocapture", "--test-threads=1"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

#[test]
#[allow(clippy::zombie_processes)]
fn adversary_helper_process() {
    let Ok(role) = std::env::var(ROLE) else {
        return;
    };
    let mut out = std::io::stdout();
    if std::env::var_os(GRANDCHILD).is_none() {
        match role.as_str() {
            "ignore-term" => unsafe {
                signal(15, 1);
            },
            "parent" => {
                let grandchild = std::process::Command::new(std::env::current_exe().expect("exe"))
                    .args(helper_args())
                    .env(GRANDCHILD, "1")
                    .spawn()
                    .expect("the grandchild starts");
                writeln!(out, "grandchild {}", grandchild.id()).expect("stdout");
            }
            other => panic!("unknown role {other}"),
        }
        writeln!(out, "ready").expect("stdout");
        out.flush().expect("stdout");
    }
    loop {
        std::thread::sleep(Duration::from_secs(60));
    }
}

fn address(port: u16) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], port))
}

fn fixture_plan(instance_id: &str, settings: &[(&str, &str)]) -> LaunchPlan {
    LaunchPlan {
        instance_id: instance_id.to_owned(),
        component: "rally-table".to_owned(),
        settings: settings
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect(),
        artifact: Artifact::HostBinary {
            path: PathBuf::from(env!("CARGO_BIN_EXE_spindle-target-fixture")),
            args: Vec::new(),
        },
        request: Resources::default(),
        listen: address(7200),
        peers: Vec::new(),
    }
}

fn helper_plan(instance_id: &str, role: &str) -> LaunchPlan {
    LaunchPlan {
        artifact: Artifact::HostBinary {
            path: std::env::current_exe().expect("the test binary"),
            args: helper_args(),
        },
        ..fixture_plan(instance_id, &[("adversary_role", role)])
    }
}

fn console_text(console: &Path, until: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let text = std::fs::read_to_string(console).unwrap_or_default();
        if text.contains(until) || Instant::now() > deadline {
            return text;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn alive(pid: i32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat
            .rsplit_once(')')
            .map(|(_, rest)| !rest.trim_start().starts_with('Z'))
            .unwrap_or(false),
        Err(_) => false,
    }
}

fn host(ceiling: Resources) -> Target {
    Target {
        id: "host-1".to_owned(),
        kind: TargetKind::Process,
        environment: "laptop".to_owned(),
        ceiling,
        state: TargetState::Available,
    }
}

fn admitted() -> EnvironmentAdmission {
    EnvironmentAdmission {
        environment: "laptop".to_owned(),
        admission: Admission::Admitted {
            bindings: Vec::new(),
        },
    }
}

#[test]
fn wait_with_the_longest_timeout_returns_the_exit_instead_of_panicking() {
    let consoles = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("tempdir");
    let mut launcher = ProcessLauncher::new(consoles.path());
    let mut running = launcher
        .launch(&fixture_plan(
            "exits-1",
            &[("mode", "exit"), ("exit_code", "4")],
        ))
        .expect("starts");
    let exit = running
        .wait(Duration::MAX)
        .expect("waits")
        .expect("the instance exits on its own");
    assert_eq!(
        exit,
        Exit {
            code: Some(4),
            signal: None
        }
    );
}

#[test]
fn stop_with_the_longest_grace_terminates_instead_of_panicking() {
    let consoles = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("tempdir");
    let mut launcher = ProcessLauncher::new(consoles.path()).with_grace(Duration::MAX);
    let mut running = launcher
        .launch(&fixture_plan("graceful-1", &[]))
        .expect("starts");
    console_text(running.console(), "ready");
    assert_eq!(
        running.stop().expect("stops"),
        Exit {
            code: None,
            signal: Some(15)
        }
    );
}

#[test]
fn stopping_an_instance_leaves_no_process_it_started_behind() {
    let consoles = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("tempdir");
    let mut launcher = ProcessLauncher::new(consoles.path()).with_grace(Duration::from_millis(500));
    let mut running = launcher
        .launch(&helper_plan("parent-1", "parent"))
        .expect("starts");
    let text = console_text(running.console(), "ready");
    let grandchild: i32 = text
        .lines()
        .find_map(|line| line.split_once("grandchild ").map(|(_, pid)| pid))
        .unwrap_or_else(|| panic!("the instance names its child:\n{text}"))
        .trim()
        .parse()
        .expect("a pid");
    assert!(alive(grandchild), "the instance's own child runs");

    let stopped = running.stop().expect("stops");
    let deadline = Instant::now() + Duration::from_secs(2);
    while alive(grandchild) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    let survived = alive(grandchild);
    if survived {
        let _ = kill_process(Pid::from_raw(grandchild).expect("a pid"), Signal::KILL);
    }
    assert_eq!(stopped.signal, Some(15));
    assert!(
        !survived,
        "process {grandchild}, started by the stopped instance, still runs as an orphan"
    );
}

#[test]
fn a_launch_that_starts_nothing_leaves_the_previous_console_as_it_was() {
    let consoles = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("tempdir");
    let previous = consoles.path().join("table-1.log");
    std::fs::write(&previous, "the last run's crash report\n").expect("writes");

    let mut launcher = ProcessLauncher::new(consoles.path());
    let missing = LaunchPlan {
        artifact: Artifact::HostBinary {
            path: consoles.path().join("no-such-binary"),
            args: Vec::new(),
        },
        ..fixture_plan("table-1", &[])
    };
    match launcher.launch(&missing) {
        Err(LaunchError::Io { .. }) => {}
        Err(other) => panic!("expected an I/O refusal, got {other}"),
        Ok(_) => panic!("a missing binary started"),
    }
    assert_eq!(
        std::fs::read_to_string(&previous).expect("reads"),
        "the last run's crash report\n",
        "nothing started, yet the previous console was truncated"
    );
}

#[test]
fn two_peers_of_one_component_reach_the_instance() {
    let consoles = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("tempdir");
    let mut launcher = ProcessLauncher::new(consoles.path());
    let plan = LaunchPlan {
        peers: vec![
            Peer {
                component: "rally-player".to_owned(),
                address: address(7201),
            },
            Peer {
                component: "rally-player".to_owned(),
                address: address(7202),
            },
        ],
        ..fixture_plan("table-1", &[])
    };
    let mut running = match launcher.launch(&plan) {
        Ok(running) => running,
        Err(error) => panic!("two instances of one peer component are refused: {error}"),
    };
    let text = console_text(running.console(), "ready");
    running.stop().expect("stops");
    for peer in ["127.0.0.1:7201", "127.0.0.1:7202"] {
        assert!(
            text.lines()
                .any(|line| line.starts_with("env SPINDLE_PEER_") && line.contains(peer)),
            "{peer} missing from the instance's environment:\n{text}"
        );
    }
}

#[test]
fn a_child_that_ignores_sigterm_is_killed_after_the_grace() {
    let consoles = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("tempdir");
    let mut launcher = ProcessLauncher::new(consoles.path()).with_grace(Duration::from_millis(300));
    let mut running = launcher
        .launch(&helper_plan("stubborn-1", "ignore-term"))
        .expect("starts");
    console_text(running.console(), "ready");
    let started = Instant::now();
    let stopped = running.stop().expect("stops");
    let took = started.elapsed();
    assert_eq!(
        stopped,
        Exit {
            code: None,
            signal: Some(9)
        }
    );
    assert!(
        took >= Duration::from_millis(300),
        "killed before the grace: {took:?}"
    );
    assert!(took < Duration::from_secs(4), "stop took {took:?}");
    assert_eq!(running.wait(Duration::ZERO).expect("waits"), Some(stopped));
}

#[test]
fn instance_ids_that_leave_the_console_directory_are_refused() {
    let root = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("tempdir");
    let consoles = root.path().join("consoles");
    let mut launcher = ProcessLauncher::new(&consoles);
    for id in [
        "",
        ".",
        "..",
        "../escape",
        "a/b",
        "/abs",
        ".hidden",
        "a\0b",
        "a\\b",
        "é",
        " ",
        "a b",
    ] {
        match launcher.launch(&fixture_plan(id, &[])) {
            Err(LaunchError::InvalidPlan(_)) => {}
            Err(other) => panic!("{id:?}: expected InvalidPlan, got {other}"),
            Ok(_) => panic!("{id:?} was accepted as an instance id"),
        }
    }
    assert!(
        !consoles.exists(),
        "a refused plan created the console directory"
    );
    assert_eq!(
        std::fs::read_dir(root.path()).expect("reads").count(),
        0,
        "a refused plan wrote beside the console directory"
    );
}

#[test]
fn settings_and_peers_that_sanitise_to_one_variable_are_refused() {
    let consoles = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("tempdir");
    let mut launcher = ProcessLauncher::new(consoles.path());
    for pair in [
        ("match-id", "match_id"),
        ("points", "POINTS"),
        ("a=b", "a.b"),
        ("ü", "_"),
    ] {
        match launcher.launch(&fixture_plan("table-1", &[(pair.0, "1"), (pair.1, "2")])) {
            Err(LaunchError::InvalidPlan(_)) => {}
            Err(other) => panic!("{pair:?}: expected InvalidPlan, got {other}"),
            Ok(_) => panic!("{pair:?}: both settings were passed as one variable"),
        }
    }
    match launcher.launch(&fixture_plan("table-1", &[("", "1")])) {
        Err(LaunchError::InvalidPlan(_)) => {}
        Err(other) => panic!("empty setting name: expected InvalidPlan, got {other}"),
        Ok(_) => panic!("an unnamed setting was passed"),
    }
    let peers = LaunchPlan {
        peers: vec![
            Peer {
                component: "player.west".to_owned(),
                address: address(7201),
            },
            Peer {
                component: "player-west".to_owned(),
                address: address(7202),
            },
        ],
        ..fixture_plan("table-1", &[])
    };
    assert!(matches!(
        launcher.launch(&peers),
        Err(LaunchError::InvalidPlan(_))
    ));
}

#[test]
fn setting_values_and_names_cannot_inject_another_variable() {
    let consoles = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("tempdir");
    let mut launcher = ProcessLauncher::new(consoles.path());
    let mut running = launcher
        .launch(&fixture_plan(
            "table-1",
            &[
                ("x\nSPINDLE_LISTEN", "0.0.0.0:1"),
                ("../LISTEN", "0.0.0.0:2"),
                ("y", "1\nSPINDLE_LISTEN=0.0.0.0:3"),
            ],
        ))
        .expect("starts");
    let text = console_text(running.console(), "ready");
    running.stop().expect("stops");
    let listens: Vec<&str> = text
        .lines()
        .filter(|line| line.starts_with("env SPINDLE_LISTEN="))
        .collect();
    assert_eq!(listens, vec!["env SPINDLE_LISTEN=127.0.0.1:7200"], "{text}");
    assert!(
        text.lines()
            .any(|line| line == "env SPINDLE_SETTING_X_SPINDLE_LISTEN=0.0.0.0:1"),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|line| line == "env SPINDLE_SETTING____LISTEN=0.0.0.0:2"),
        "{text}"
    );
}

#[test]
fn ceilings_compare_each_resource_at_its_boundaries() {
    let admitted = admitted();
    let zero = host(Resources::default());
    assert_eq!(
        place(&zero, &Resources::default(), &admitted),
        Placement::Placed {
            request: Resources::default()
        }
    );

    let max = host(Resources {
        vcpus: u32::MAX,
        memory_mib: u64::MAX,
        disk_mib: u64::MAX,
    });
    let all = Resources {
        vcpus: u32::MAX,
        memory_mib: u64::MAX,
        disk_mib: u64::MAX,
    };
    assert_eq!(
        place(&max, &all, &admitted),
        Placement::Placed { request: all }
    );

    let one_below = host(Resources {
        vcpus: u32::MAX - 1,
        memory_mib: u64::MAX - 1,
        disk_mib: u64::MAX,
    });
    assert_eq!(
        place(&one_below, &all, &admitted),
        Placement::Refused {
            reasons: vec![
                Refusal::AboveCeiling {
                    resource: ResourceKind::Vcpus,
                    requested: u64::from(u32::MAX),
                    ceiling: u64::from(u32::MAX - 1),
                },
                Refusal::AboveCeiling {
                    resource: ResourceKind::Memory,
                    requested: u64::MAX,
                    ceiling: u64::MAX - 1,
                },
            ]
        }
    );

    let disk_only = Resources {
        disk_mib: 1,
        ..Resources::default()
    };
    assert_eq!(
        place(&zero, &disk_only, &admitted),
        Placement::Refused {
            reasons: vec![Refusal::AboveCeiling {
                resource: ResourceKind::Disk,
                requested: 1,
                ceiling: 0,
            }]
        }
    );

    let mut retired = host(Resources::default());
    retired.state = TargetState::Retired;
    let refused = EnvironmentAdmission {
        environment: "laptop".to_owned(),
        admission: Admission::Refused {
            missing: vec![Capability::new("clock")],
        },
    };
    assert_eq!(
        place(&retired, &Resources::default(), &admitted),
        Placement::Refused {
            reasons: vec![Refusal::TargetRetired]
        }
    );
    assert_eq!(
        place(&host(Resources::default()), &Resources::default(), &refused),
        Placement::Refused {
            reasons: vec![Refusal::NotAdmitted {
                missing: vec![Capability::new("clock")]
            }]
        }
    );
}
