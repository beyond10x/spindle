//! Adversarial case for wave 17: `conform` without `--suite` on a specification listing an
//! authored scenario ESS refuses exits 1, prints the refusal, and runs nothing (CHANGELOG,
//! `conform --help`).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

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
fn a_listed_scenario_ess_refuses_is_printed_and_exits_1() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("a scratch directory");
    let spec = dir.path().join("ess");
    copy(&root().join("ess"), &spec);
    std::fs::write(
        spec.join("scenarios/names-nothing.yaml"),
        "type: ess-scenario/1\n\
         domain: spindle.deployment\n\
         scenario: names-nothing\n\
         summary: Names a command the specification does not declare.\n\
         timeline:\n\
         \x20 - at: 2026-01-05T09:00:00Z\n\
         \x20   command: spindle.deployment.NoSuchCommand\n\
         \x20   input: {}\n\
         \x20   outcome: declared\n",
    )
    .expect("writing the scenario");
    let manifest = spec.join("ess-inputs.yaml");
    let mut text = std::fs::read_to_string(&manifest).expect("the manifest");
    text.push_str("  - scenarios/names-nothing.yaml\n");
    std::fs::write(&manifest, text).expect("writing the manifest");

    let report = dir.path().join("report.json");
    let output = Command::new(env!("CARGO_BIN_EXE_b10x-spindle"))
        .current_dir(root())
        .arg("conform")
        .arg(&spec)
        .args([
            "--environment",
            "examples/environments/laptop.toml",
            "--report",
        ])
        .arg(&report)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("b10x-spindle runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(1),
        "stdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("ESS refused the listed scenarios")
            && stdout.contains("names-nothing.yaml")
            && stdout.contains("NoSuchCommand"),
        "the refusal is printed, naming the file and what it names: {stdout}"
    );
    assert!(
        !report.exists(),
        "nothing ran, so no report was written: {stdout}"
    );
}
