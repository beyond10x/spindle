//! Adversarial cases for the `scenarios:` list `spindle_spec::load` now reads (wave 17): an entry
//! outside the root through `..` or an absolute path is refused, as ESS refuses it, and an empty
//! list reads no scenario.

use std::fs;
use std::path::Path;

const SYSTEM: &str = "format: ess/34\nsystem: listed\nversion: v1\ndomains: []\n";

fn spec_with_scenarios(root: &Path, scenarios: &str) {
    fs::create_dir_all(root).expect("the root");
    fs::write(root.join("system.yaml"), SYSTEM).expect("write");
    fs::write(
        root.join("ess-inputs.yaml"),
        format!("format: ess-inputs/2\nspecification:\n  - system.yaml\nscenarios:{scenarios}"),
    )
    .expect("write the manifest");
}

#[test]
fn a_scenario_entry_that_climbs_out_of_the_root_is_refused() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let root = directory.path().join("spec");
    fs::write(
        directory.path().join("outside.yaml"),
        "type: ess-scenario/1\n",
    )
    .expect("write outside the root");
    spec_with_scenarios(&root, "\n  - ../outside.yaml\n");
    let loaded = spindle_spec::load(&root);
    assert!(
        loaded.is_err(),
        "a scenario entry through `..` is refused by ess; spindle read {:?}",
        loaded.map(|loaded| loaded.scenarios)
    );
}

#[test]
fn an_absolute_scenario_entry_is_refused() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let root = directory.path().join("spec");
    let outside = directory.path().join("outside.yaml");
    fs::write(&outside, "type: ess-scenario/1\n").expect("write outside the root");
    spec_with_scenarios(&root, &format!("\n  - {}\n", outside.display()));
    let loaded = spindle_spec::load(&root);
    assert!(
        loaded.is_err(),
        "an absolute scenario entry is refused by ess; spindle read {:?}",
        loaded.map(|loaded| loaded.scenarios)
    );
}

#[test]
fn an_empty_scenarios_list_reads_no_scenario() {
    let directory = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).expect("temporary directory");
    let root = directory.path().join("spec");
    spec_with_scenarios(&root, " []\n");
    fs::create_dir(root.join("scenarios")).expect("the scenarios directory");
    fs::write(
        root.join("scenarios/unlisted.yaml"),
        "type: ess-scenario/1\n",
    )
    .expect("write");
    let loaded = spindle_spec::load(&root).expect("an empty scenarios list loads");
    assert_eq!(loaded.files, vec!["system.yaml"]);
    assert!(
        loaded.scenarios.is_empty(),
        "an empty list implies no scenario: {:?}",
        loaded.scenarios
    );
}
