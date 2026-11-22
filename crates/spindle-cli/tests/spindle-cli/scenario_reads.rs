//! `check`, `capabilities` and `admit` read no authored scenario (`story:hot-paths-skip-unused-work`).
//!
//! 0.17.0: 8 MiB of comments in the one scenario `ess/ess-inputs.yaml` lists made each of the
//! three read 8 MiB more, with byte-identical output. Here that scenario is replaced by bytes that
//! do not read as text: the three answer as on `ess/` itself, and `conform`, which runs the
//! scenarios, still reads it and is refused.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const SCENARIO: &str = "scenarios/launch-not-admitted.yaml";
const LAPTOP: &str = "examples/environments/laptop.toml";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn spindle(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("b10x-spindle runs")
}

/// Copies the directory `from` to `to`.
fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("creating a directory");
    for entry in std::fs::read_dir(from).expect("reading a directory") {
        let entry = entry.expect("a directory entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a file type").is_dir() {
            copy(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copying a file");
        }
    }
}

#[test]
fn check_capabilities_and_admit_read_no_scenario() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a directory");
    let unreadable = directory.path().join("ess");
    copy(&root().join("ess"), &unreadable);
    std::fs::write(unreadable.join(SCENARIO), [0xff, 0xfe, 0x00, 0x80])
        .expect("the scenario is replaced");
    let unreadable = unreadable.to_str().expect("utf-8");

    for command in [
        vec!["check", "{}"],
        vec!["check", "{}", "--json"],
        vec!["capabilities", "{}", "--json"],
        vec!["capabilities", "{}", "--component", "spindle-control"],
        vec!["admit", "{}", "--environment", LAPTOP, "--json"],
    ] {
        let with = |spec: &str| -> Vec<String> {
            command
                .iter()
                .map(|argument| argument.replace("{}", spec))
                .collect()
        };
        let readable = with("ess");
        let readable = spindle(&readable.iter().map(String::as_str).collect::<Vec<_>>());
        let replaced = with(unreadable);
        let replaced = spindle(&replaced.iter().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(readable.status.code(), Some(0), "{command:?}: {readable:?}");
        assert_eq!(
            (
                replaced.status.code(),
                String::from_utf8_lossy(&replaced.stdout)
            ),
            (
                readable.status.code(),
                String::from_utf8_lossy(&readable.stdout)
            ),
            "{command:?} answered otherwise with the scenario unreadable: {}",
            String::from_utf8_lossy(&replaced.stderr)
        );
    }

    let conform = spindle(&["conform", unreadable, "--environment", LAPTOP]);
    assert_eq!(conform.status.code(), Some(1), "{conform:?}");
    let stderr = String::from_utf8_lossy(&conform.stderr);
    assert!(stderr.contains("launch-not-admitted.yaml"), "{stderr}");
}
