//! Adversary cases for `b10x-spindle deploy`: what is left running when it is ended by a signal
//! it does not handle.
//!
//! Every process a case finds is killed by the case before it returns.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const STARTUP: Duration = Duration::from_secs(60);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the workspace")
}

/// The pids whose parent is `parent`, read from `/proc`.
fn children_of(parent: u32) -> Vec<u32> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir("/proc").expect("/proc").flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        let Some((_, rest)) = stat.rsplit_once(')') else {
            continue;
        };
        let ppid = rest.split_whitespace().nth(1).and_then(|p| p.parse().ok());
        if ppid == Some(parent) {
            found.push(pid);
        }
    }
    found
}

fn alive(pid: u32) -> bool {
    let Some(pid) = rustix::process::Pid::from_raw(pid as i32) else {
        return false;
    };
    if rustix::process::test_kill_process(pid).is_err() {
        return false;
    }
    std::fs::read_to_string(format!("/proc/{}/stat", pid.as_raw_nonzero()))
        .map(|stat| {
            stat.rsplit_once(')')
                .and_then(|(_, rest)| rest.split_whitespace().next().map(|state| state != "Z"))
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

/// Kills the process groups of `pids` (each instance leads its own group) when dropped.
struct Reaper(Vec<u32>);

impl Drop for Reaper {
    fn drop(&mut self) {
        for pid in &self.0 {
            if let Some(pid) = rustix::process::Pid::from_raw(*pid as i32) {
                let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
            }
        }
    }
}

/// The telemetry socket goes in the work directory, made under `TMPDIR`; when that path is too
/// long for `sun_path`, the fallback directory is made under `TMPDIR` too, so it is too long as
/// well and `deploy` exits 1 before it serves anything.
#[test]
fn deploy_serves_when_tmpdir_is_long() {
    let tmp = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let long = tmp
        .path()
        .join("a-temporary-directory-whose-path-is-long-but-legal-0123456789");
    std::fs::create_dir_all(&long).expect("the long directory");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(tmp.path())
        .env("TMPDIR", &long)
        .arg("deploy")
        .arg(root().join("examples/rally/ess"))
        .arg("--environment")
        .arg(root().join("examples/environments/laptop-processes.toml"))
        .arg("--placement")
        .arg(root().join("examples/placements/rally-processes.toml"))
        .arg("--port")
        .arg("0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("deploy starts");
    let stdout = child.stdout.take().expect("stdout");
    let (lines, received) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if lines.send(line).is_err() {
                return;
            }
        }
    });
    let first = received.recv_timeout(STARTUP);
    let instances = children_of(child.id());
    let _reaper = Reaper(instances);
    let _ = rustix::process::kill_process(
        rustix::process::Pid::from_child(&child),
        rustix::process::Signal::TERM,
    );
    let output = child.wait_with_output().expect("deploy ends");
    assert!(
        first.is_ok(),
        "with TMPDIR {} ({} bytes) deploy served nothing and ended {:?}: {}",
        long.display(),
        long.as_os_str().len(),
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Closing the terminal `deploy` runs in sends it SIGHUP. The instances lead process groups of
/// their own, so the hangup reaches only `deploy`; it does not handle SIGHUP, dies, and every
/// instance it launched keeps running with no control process.
#[test]
fn deploy_ended_by_sighup_leaves_no_instance_running() {
    let tmp = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(tmp.path())
        .env("TMPDIR", std::env::temp_dir())
        .arg("deploy")
        .arg(root().join("examples/rally/ess"))
        .arg("--environment")
        .arg(root().join("examples/environments/laptop-processes.toml"))
        .arg("--placement")
        .arg(root().join("examples/placements/rally-processes.toml"))
        .arg("--port")
        .arg("0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("deploy starts");
    let stdout = child.stdout.take().expect("stdout");
    let (lines, received) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if lines.send(line).is_err() {
                return;
            }
        }
    });
    if received.recv_timeout(STARTUP).is_err() {
        let _ = child.kill();
        let _ = child.wait();
        panic!("deploy printed nothing within {STARTUP:?}");
    }
    let instances = children_of(child.id());
    let reaper = Reaper(instances.clone());
    assert_eq!(instances.len(), 3, "deploy launched {instances:?}");

    let pid = rustix::process::Pid::from_child(&child);
    rustix::process::kill_process(pid, rustix::process::Signal::HUP).expect("SIGHUP");
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Ok(Some(status)) = child.try_wait() {
            break status;
        }
        assert!(Instant::now() < deadline, "deploy did not end on SIGHUP");
        std::thread::sleep(Duration::from_millis(20));
    };
    std::thread::sleep(Duration::from_millis(500));
    let survivors = instances
        .iter()
        .copied()
        .filter(|pid| alive(*pid))
        .collect::<Vec<_>>();
    drop(reaper);
    assert!(
        survivors.is_empty(),
        "deploy ended ({status:?}) and left instances {survivors:?} of {instances:?} running"
    );
}
