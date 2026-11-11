//! Adversary case for `story:state-directory-lock`: "A second `run` given the same state exits 1
//! before opening anything". `run` reading stdin locks `<path>.lock`, the path as given
//! (`state_lock::file`), so a second path naming the same SQLite file, here a symbolic link to it,
//! takes a lock file of its own and opens the database the first run holds.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const LAPTOP: &str = "examples/environments/laptop.toml";
const STARTUP: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(20);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A process killed and waited on when dropped.
struct Killed(Child);

impl Drop for Killed {
    fn drop(&mut self) {
        drop(self.0.stdin.take());
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn a_second_stdin_run_on_a_link_to_the_held_state_file_exits_1() {
    let scratch = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let file = scratch.path().join("state.sqlite");
    let first = Killed(
        Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
            .current_dir(root())
            .args(["run", "ess", "--environment", LAPTOP, "--state"])
            .arg(&file)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("b10x-spindle starts"),
    );
    let deadline = Instant::now() + STARTUP;
    while !file.exists() {
        assert!(
            Instant::now() < deadline,
            "{} is not created",
            file.display()
        );
        std::thread::sleep(POLL);
    }
    let alias = scratch.path().join("alias.sqlite");
    std::os::unix::fs::symlink(&file, &alias).expect("the link is made");

    let mut second = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(["run", "ess", "--environment", LAPTOP, "--state"])
        .arg(&alias)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("b10x-spindle starts");
    let deadline = Instant::now() + STARTUP;
    while second.try_wait().expect("waited on").is_none() {
        assert!(Instant::now() < deadline, "the second run did not exit");
        std::thread::sleep(POLL);
    }
    let second = second.wait_with_output().expect("the output is read");
    let stderr = String::from_utf8_lossy(&second.stderr);
    assert_eq!(
        second.status.code(),
        Some(1),
        "a second run on the state file process {} holds, through a link to it: {stderr}",
        first.0.id()
    );
    assert!(
        stderr.contains(&format!("process {}", first.0.id())),
        "the holder is named: {stderr}"
    );
}
