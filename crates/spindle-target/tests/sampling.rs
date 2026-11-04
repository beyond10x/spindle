//! Acceptance cases of host-side resource sampling (`docs/design/observe.md` § 4): what an
//! instance costs the host, and its share of the quota it asked for.

use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use spindle_target::{Artifact, LaunchPlan, Launcher, ProcessLauncher, ResourceSample, Resources};

const ROLE: &str = "SPINDLE_SETTING_SAMPLING_ROLE";
const MEMBER: &str = "SAMPLING_MEMBER";
const HELPER: &str = "sampling_helper_process";
const BALLAST: usize = 64 << 20;
const BURNED: f64 = 0.2;
const SAMPLE_PAUSE: Duration = Duration::from_millis(20);
const SAMPLE_STEPS: u32 = 500;

fn helper_args() -> Vec<String> {
    [HELPER, "--exact", "--nocapture", "--test-threads=1"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

/// The instance the group case launches: a leader that sleeps, and one member of its process
/// group that holds [`BALLAST`] bytes resident and spins.
#[test]
#[allow(clippy::zombie_processes)]
fn sampling_helper_process() {
    let Ok(role) = std::env::var(ROLE) else {
        return;
    };
    assert_eq!(role, "group", "unknown role {role}");
    if std::env::var_os(MEMBER).is_some() {
        let ballast = std::hint::black_box(vec![1u8; BALLAST]);
        let mut spins = 0u64;
        loop {
            spins = std::hint::black_box(
                spins.wrapping_add(u64::from(ballast[spins as usize % BALLAST])),
            );
        }
    }
    let member = std::process::Command::new(std::env::current_exe().expect("exe"))
        .args(helper_args())
        .env(MEMBER, "1")
        .spawn()
        .expect("the member starts");
    let mut out = std::io::stdout();
    writeln!(out, "leader {}", std::process::id()).expect("stdout");
    writeln!(out, "member {}", member.id()).expect("stdout");
    writeln!(out, "ready").expect("stdout");
    out.flush().expect("stdout");
    loop {
        std::thread::sleep(Duration::from_secs(60));
    }
}

fn plan(instance_id: &str, artifact: Artifact, settings: &[(&str, &str)]) -> LaunchPlan {
    LaunchPlan {
        instance_id: instance_id.to_owned(),
        component: "rally-table".to_owned(),
        settings: settings
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect(),
        artifact,
        request: Resources::default(),
        listen: SocketAddr::from(([127, 0, 0, 1], 7300)),
        peers: Vec::new(),
    }
}

fn fixture() -> Artifact {
    Artifact::HostBinary {
        path: PathBuf::from(env!("CARGO_BIN_EXE_spindle-target-fixture")),
        args: Vec::new(),
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

fn pid_named(text: &str, name: &str) -> u32 {
    text.lines()
        .find_map(|line| line.split_once(&format!("{name} ")).map(|(_, pid)| pid))
        .unwrap_or_else(|| panic!("the instance names its {name}:\n{text}"))
        .trim()
        .parse()
        .expect("a pid")
}

fn resident_bytes(pid: u32) -> u64 {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).expect("the status");
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|rest| {
            rest.trim()
                .trim_end_matches("kB")
                .trim()
                .parse::<u64>()
                .ok()
        })
        .map_or(0, |kib| kib * 1024)
}

#[test]
fn cpu_share_is_cpu_seconds_over_wall_time_and_requested_vcpus() {
    let request = Resources {
        vcpus: 2,
        memory_mib: 512,
        disk_mib: 0,
    };
    let sample = ResourceSample::against(3.0, 128 << 20, 2.0, &request);
    assert_eq!(
        sample,
        ResourceSample {
            cpu_seconds: 3.0,
            memory_bytes: 128 << 20,
            wall_seconds: 2.0,
            cpu_share: Some(0.75),
            memory_share: Some(0.25),
        }
    );

    let cpu_only = ResourceSample::against(
        1.0,
        1 << 20,
        4.0,
        &Resources {
            vcpus: 1,
            ..Resources::default()
        },
    );
    assert_eq!(cpu_only.cpu_share, Some(0.25));
    assert_eq!(
        cpu_only.memory_share, None,
        "no memory was asked for, so there is no memory quota"
    );

    let memory_only = ResourceSample::against(
        1.0,
        1 << 20,
        4.0,
        &Resources {
            memory_mib: 4,
            ..Resources::default()
        },
    );
    assert_eq!(memory_only.memory_share, Some(0.25));
    assert_eq!(
        memory_only.cpu_share, None,
        "no vCPU was asked for, so there is no CPU quota"
    );

    let at_launch = ResourceSample::against(0.0, 0, 0.0, &request);
    assert_eq!(
        at_launch.cpu_share, None,
        "no wall time has passed, so no share of it is defined"
    );
    assert_eq!(at_launch.memory_share, Some(0.0));
}

#[test]
fn a_process_instance_without_a_limit_reports_no_quota() {
    let consoles = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("tempdir");
    let mut launcher = ProcessLauncher::new(consoles.path());
    assert!(
        launcher.enforces().is_empty(),
        "the process launcher holds an instance to no limit"
    );
    let mut running = launcher
        .launch(&plan("unlimited-1", fixture(), &[]))
        .expect("starts");
    console_text(running.console(), "ready");

    let sample = running
        .resources()
        .expect("a running process instance is sampled");
    assert!(sample.memory_bytes > 0, "{sample:?}");
    assert!(sample.wall_seconds > 0.0, "{sample:?}");
    assert!(sample.cpu_seconds >= 0.0, "{sample:?}");
    assert_eq!(sample.cpu_share, None, "{sample:?}");
    assert_eq!(sample.memory_share, None, "{sample:?}");

    running.stop().expect("stops");
    assert_eq!(
        running.resources(),
        None,
        "a stopped instance has nothing left to sample"
    );
}

#[test]
fn a_process_group_is_sampled_as_the_sum_of_its_members() {
    let consoles = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("tempdir");
    let mut launcher = ProcessLauncher::new(consoles.path()).with_grace(Duration::from_millis(500));
    let artifact = Artifact::HostBinary {
        path: std::env::current_exe().expect("the test binary"),
        args: helper_args(),
    };
    let before_launch = Instant::now();
    let mut running = launcher
        .launch(&plan("group-1", artifact, &[("sampling_role", "group")]))
        .expect("starts");
    let after_launch = Instant::now();
    let text = console_text(running.console(), "ready");
    let leader = pid_named(&text, "leader");
    let member = pid_named(&text, "member");

    let mut steps = 0;
    let (sample, before_sample, after_sample) = loop {
        let before_sample = Instant::now();
        let sample = running.resources();
        let after_sample = Instant::now();
        if let Some(sample) = sample
            && sample.cpu_seconds >= BURNED
            && sample.memory_bytes >= BALLAST as u64
        {
            break (sample, before_sample, after_sample);
        }
        steps += 1;
        assert!(
            steps < SAMPLE_STEPS,
            "the group did not reach {BURNED} CPU seconds and {BALLAST} resident bytes within \
             {SAMPLE_STEPS} samples; the last was {sample:?}"
        );
        std::thread::sleep(SAMPLE_PAUSE);
    };

    let leader_resident = resident_bytes(leader);
    assert!(
        resident_bytes(member) >= BALLAST as u64,
        "the member holds its ballast"
    );
    assert!(
        sample.memory_bytes > leader_resident && leader_resident < BALLAST as u64,
        "the sleeping leader alone holds {leader_resident} bytes; the group sample holds \
         {sample:?}, so it counts the member"
    );
    let earliest = before_sample.duration_since(after_launch).as_secs_f64();
    let latest = after_sample.duration_since(before_launch).as_secs_f64();
    assert!(
        (earliest..=latest).contains(&sample.wall_seconds),
        "the wall time since launch lies in {earliest}..={latest}: {sample:?}"
    );
    assert_eq!(sample.cpu_share, None);
    assert_eq!(sample.memory_share, None);
    running.stop().expect("stops");
}
