use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn spindle(arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(arguments)
        .output()
        .expect("b10x-spindle runs")
}

#[test]
fn play_refuses_an_environment_without_a_timer() {
    let output = spindle(&[
        "play",
        "examples/rally/ess",
        "--environment",
        "crates/spindle-cli/tests/fixtures/no-timer.toml",
    ]);
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("refused on laptop-without-timer"),
        "{stdout}"
    );
    let missing = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("  ")?.split_once(": "))
        .collect::<Vec<_>>();
    assert_eq!(missing, [("rally-table", "timer")], "{stdout}");
    assert!(!stdout.contains("wins"), "nothing was played: {stdout}");
}

#[test]
fn play_refuses_a_match_to_no_points_with_the_declared_error() {
    let output = spindle(&[
        "play",
        "examples/rally/ess",
        "--environment",
        "examples/environments/laptop.toml",
        "--target-score",
        "0",
    ]);
    assert_ne!(output.status.code(), Some(0), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("rally.table.TargetScoreTooLow"), "{stderr}");
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("needs"),
        "no frame was drawn: {output:?}"
    );
}

/// Every entry under `directory`, recursively.
fn entries(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(directory).expect("the directory reads") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            found.extend(entries(&path));
        }
        found.push(path);
    }
    found
}

#[test]
fn play_stopped_by_a_signal_removes_its_state_and_exits_with_the_signal_status() {
    for (signal, status) in [
        (rustix::process::Signal::TERM, 143),
        (rustix::process::Signal::INT, 130),
    ] {
        let temporary =
            tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a temporary directory");
        let mut child = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
            .current_dir(root())
            .env("TMPDIR", temporary.path())
            .args([
                "play",
                "examples/rally/ess",
                "--environment",
                "examples/environments/laptop.toml",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("b10x-spindle starts");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !entries(temporary.path()).iter().any(|path| {
            path.extension()
                .is_some_and(|extension| extension == "sqlite")
        }) {
            assert!(
                Instant::now() < deadline,
                "play kept no state under TMPDIR within 10 s"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        rustix::process::kill_process(rustix::process::Pid::from_child(&child), signal)
            .expect("the signal is sent");
        let deadline = Instant::now() + Duration::from_secs(5);
        let exited = loop {
            if let Some(exited) = child.try_wait().expect("the child is observable") {
                break exited;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("play did not stop within 5 s of {signal:?}");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let output = child.wait_with_output().expect("the output reads");
        assert_eq!(exited.code(), Some(status), "{signal:?}: {output:?}");
        assert_eq!(
            entries(temporary.path()),
            Vec::<PathBuf>::new(),
            "{signal:?} left state behind"
        );
    }
}
