//! Adversary cases for the rally on QEMU microVMs: the guest variables and what `task check:vm`
//! shows when the VM case does not run.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use spindle_qemu::{Artifact, LaunchPlan, Resources};
use spindle_target::{Launcher, ProcessLauncher};

const QEMU_MATCH: &str = "a_match_between_three_qemu_microvms_ends_with_a_winner";

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

fn text(output: &Output) -> String {
    format!(
        "status: {}\n--- stdout\n{}\n--- stderr\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Acceptance: the VM test is "skipped with its reason where a prerequisite is absent". Run as
/// `task check:vm` runs it, on a host without `qemu-system-x86_64` on `PATH`, the output must
/// name QEMU; the harness swallows what a passing case printed, so it reads `ok` and nothing else.
#[test]
fn a_skipped_qemu_rally_case_names_its_reason_in_the_gate_output() {
    let binary = rally_binary(QEMU_MATCH);
    let no_qemu = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("an empty PATH");
    let output = Command::new(&binary)
        .args(["--include-ignored", "--exact", QEMU_MATCH])
        .env("PATH", no_qemu.path())
        .output()
        .expect("the rally test binary runs");
    let shown = text(&output);
    assert!(
        shown.contains("qemu-system-x86_64"),
        "the gate shows no reason why {QEMU_MATCH} did not run:\n{shown}"
    );
}

fn plan(settings: Vec<(String, String)>, artifact: Artifact) -> LaunchPlan {
    LaunchPlan {
        instance_id: "rally-table".to_owned(),
        component: "rally-table".to_owned(),
        settings,
        artifact,
        request: Resources {
            vcpus: 0,
            memory_mib: 0,
            disk_mib: 0,
        },
        listen: "172.16.0.2:7000".parse::<SocketAddr>().expect("an address"),
        peers: Vec::new(),
    }
}

/// `guest_variables` claims to compute the variables "as the process launcher sets them for a
/// child". The process launcher refuses, at launch, a value no process environment can hold (a
/// NUL byte). `guest_variables` accepts it, so the QEMU launcher boots a VM whose init cannot
/// start the command (`cannot run …: nul byte`, `exit code=127`).
#[test]
fn guest_variables_refuse_a_value_the_process_launcher_refuses() {
    let settings = vec![("serve.side".to_owned(), "le\0ft".to_owned())];
    let scratch = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let process = ProcessLauncher::new(scratch.path().join("consoles")).launch(&plan(
        settings.clone(),
        Artifact::HostBinary {
            path: Path::new("/bin/true").to_path_buf(),
            args: Vec::new(),
        },
    ));
    assert!(
        process.is_err(),
        "the process launcher starts a child with a NUL in a variable"
    );
    let guest = spindle_qemu::guest_variables(
        &plan(
            settings,
            Artifact::Guest {
                kernel: PathBuf::from("vmlinuz"),
                initramfs: PathBuf::from("initramfs.cpio"),
                cmdline: "-- /bin/b10x-spindle".to_owned(),
            },
        ),
        None,
    );
    assert!(
        guest.is_err(),
        "the process launcher refuses this plan; guest_variables gives the guest {guest:?}"
    );
}
