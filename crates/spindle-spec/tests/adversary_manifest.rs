//! Adversarial cases for `ess-inputs.yaml`: an entry `ess specify validate` refuses must be refused
//! here too (AGENTS.md, "A document `ess specify validate` refuses must be refused here too").
//! ESS 0.86 (`ess-cli/src/input_discovery.rs`, `manifest_selection` and `manifest_inputs`) refuses
//! an entry with a `..` or empty segment, an absolute entry, an entry through a symlink, an entry
//! listed twice, and a manifest format other than `ess-inputs/1` or `ess-inputs/2`.

use std::fs;
use std::path::Path;

const SYSTEM: &str = "format: ess/34\nsystem: listed\nversion: v1\ndomains: []\n";

fn manifest(root: &Path, entries: &[&str]) {
    let mut text = String::from("format: ess-inputs/2\nspecification:\n");
    for entry in entries {
        text.push_str(&format!("  - {entry}\n"));
    }
    text.push_str("scenarios: []\n");
    fs::write(root.join("ess-inputs.yaml"), text).expect("write the manifest");
}

#[test]
fn an_entry_that_climbs_out_of_the_root_is_refused() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let root = directory.path().join("spec");
    fs::create_dir(&root).expect("the root");
    fs::write(directory.path().join("outside.yaml"), SYSTEM).expect("write outside the root");
    manifest(&root, &["../outside.yaml"]);
    let loaded = spindle_spec::load(&root);
    assert!(
        loaded.is_err(),
        "`../outside.yaml` escapes the specification root, which ess refuses; spindle read {:?}",
        loaded.map(|loaded| loaded.files)
    );
}

#[test]
fn an_absolute_entry_is_refused() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let root = directory.path().join("spec");
    fs::create_dir(&root).expect("the root");
    let outside = directory.path().join("outside.yaml");
    fs::write(&outside, SYSTEM).expect("write outside the root");
    manifest(&root, &[outside.to_str().expect("a UTF-8 path")]);
    let loaded = spindle_spec::load(&root);
    assert!(
        loaded.is_err(),
        "an absolute entry is no root-relative path, which ess refuses; spindle read {:?}",
        loaded.map(|loaded| loaded.files)
    );
}

#[test]
fn an_entry_through_a_symlink_is_refused() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let root = directory.path().join("spec");
    fs::create_dir(&root).expect("the root");
    let outside = directory.path().join("outside.yaml");
    fs::write(&outside, SYSTEM).expect("write outside the root");
    std::os::unix::fs::symlink(&outside, root.join("system.yaml")).expect("the symlink");
    manifest(&root, &["system.yaml"]);
    let loaded = spindle_spec::load(&root);
    assert!(
        loaded.is_err(),
        "a listed entry that is a symlink is refused by ess; spindle read {:?}",
        loaded.map(|loaded| loaded.files)
    );
}

#[test]
fn an_entry_listed_twice_is_refused() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let root = directory.path();
    fs::write(root.join("system.yaml"), SYSTEM).expect("write");
    manifest(root, &["system.yaml", "system.yaml"]);
    let loaded = spindle_spec::load(root);
    assert!(
        loaded.is_err(),
        "ess refuses an entry that repeats a path; spindle read {:?}",
        loaded.map(|loaded| loaded.files)
    );
}

#[test]
fn an_unknown_manifest_format_is_refused() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let root = directory.path();
    fs::write(root.join("system.yaml"), SYSTEM).expect("write");
    fs::write(
        root.join("ess-inputs.yaml"),
        "format: ess-inputs/9\nspecification:\n  - system.yaml\nscenarios: []\n",
    )
    .expect("write");
    let loaded = spindle_spec::load(root);
    assert!(
        loaded.is_err(),
        "ess reads ess-inputs/1 and ess-inputs/2 only; spindle read {:?}",
        loaded.map(|loaded| loaded.files)
    );
}
