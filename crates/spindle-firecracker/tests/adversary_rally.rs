//! Adversary cases for the rally VM tests on Firecracker (`tests/rally.rs`): what `task check:vm`
//! shows when a case does not run.
//!
//! Each case runs the `rally` test binary of this crate the way `task check:vm` does
//! (`--include-ignored`, output captured by the test harness) and reads what it prints and how it
//! exits. None of them boots a VM: every run lacks a prerequisite or a guest binary.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIRECRACKER_MATCH: &str = "a_match_between_three_firecracker_microvms_ends_with_a_winner";

/// The newest `rally-<hash>` test binary next to this one that lists `test`.
fn rally_binary(test: &str) -> PathBuf {
    let me = std::env::current_exe().expect("this test binary");
    let deps = me.parent().expect("the deps directory");
    let mut found: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(deps)
        .expect("the deps directory reads")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("rally-") && !name.contains('.'))
        })
        .filter(|path| {
            Command::new(path)
                .arg("--list")
                .output()
                .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).contains(test))
        })
        .filter_map(|path| Some((path.metadata().ok()?.modified().ok()?, path)))
        .collect();
    found.sort();
    found
        .pop()
        .map(|(_, path)| path)
        .unwrap_or_else(|| panic!("no rally test binary lists {test}: build it with --test rally"))
}

fn run(binary: &Path, test: &str, configure: impl FnOnce(&mut Command)) -> Output {
    let mut command = Command::new(binary);
    command.args(["--include-ignored", "--exact", test]);
    configure(&mut command);
    command.output().expect("the rally test binary runs")
}

fn text(output: &Output) -> String {
    format!(
        "status: {}\n--- stdout\n{}\n--- stderr\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Acceptance: each VM test is "skipped with its reason where a prerequisite is absent". Run as
/// `task check:vm` runs it, a case without `SPINDLE_FIRECRACKER_KERNEL` must say so; the harness
/// swallows what a passing case printed, so the run reads `ok` and names no reason.
#[test]
fn a_skipped_firecracker_rally_case_names_its_reason_in_the_gate_output() {
    let binary = rally_binary(FIRECRACKER_MATCH);
    let output = run(&binary, FIRECRACKER_MATCH, |command| {
        command.env_remove("SPINDLE_FIRECRACKER_KERNEL");
    });
    let shown = text(&output);
    assert!(
        shown.contains("SPINDLE_FIRECRACKER_KERNEL"),
        "the gate shows no reason why {FIRECRACKER_MATCH} did not run:\n{shown}"
    );
}

/// Acceptance names four prerequisites a VM test may skip on: `/dev/kvm`, QEMU, the Firecracker
/// binary or its kernel. The guest binaries are not one of them: `task check:vm` does not build
/// them, so on a checkout with every prerequisite the case must not pass without booting a VM.
#[test]
fn missing_guest_binaries_do_not_pass_the_firecracker_rally_case() {
    if std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/kvm")
        .is_err()
    {
        panic!("/dev/kvm is a prerequisite of this case's premise: run it on a KVM host");
    }
    let binary = rally_binary(FIRECRACKER_MATCH);
    let empty = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("an empty directory");
    let present = std::env::current_exe().expect("a file that exists");
    let output = run(&binary, FIRECRACKER_MATCH, |command| {
        command
            .env("SPINDLE_FIRECRACKER", &present)
            .env("SPINDLE_FIRECRACKER_KERNEL", &present)
            .env("SPINDLE_GUEST_BIN_DIR", empty.path());
    });
    assert!(
        !output.status.success(),
        "{FIRECRACKER_MATCH} passed with every prerequisite present and no guest binary, \
         without booting a VM:\n{}",
        text(&output)
    );
}
