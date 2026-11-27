//! Adversary cases for wave 29 U3 (`story:ess-pin-0-99-2`).
//!
//! ESS 0.99.2 moves the witnesses of a quantifier over an input list onto the element that decides
//! it (`onto_deciding` in `ess-conformance`'s `synthesize.rs`). The unit's own fixture holds one
//! shape: `exists` over a `List` of a one-member struct with `trimmed: {equals: ""}`. Each fixture
//! under `tests/fixtures/adversary-w29-u3` is another shape whose suite 0.99.2 synthesises
//! differently from 0.99.1 (or, for the `String` lists, the same), and the host must give every
//! scenario of it the status ESS's interpreter gives it.
//!
//! The last case holds the pin itself: every file that names the ESS release names the one
//! `ess/ess-inputs.yaml` requires, including the manifests and the lock file that
//! `check:the_ess_pin_is_one_release` does not read.

use std::path::{Path, PathBuf};

use ess_conformance::interpret::Interpreted;
use ess_conformance::{AdmittedSuite, Runner, Status};
use spindle_capability::Environment;
use spindle_conformance::HostTarget;
use spindle_spec::Specification;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn laptop() -> Environment {
    let path = manifest_dir().join("../../examples/environments/laptop.toml");
    let text = std::fs::read_to_string(path).expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

/// Synthesises the suite of `fixture`, runs it on ESS's interpreter and on the host, and asserts
/// that each scenario has the same status on both and that every one passes.
fn host_answers_as_ess(fixture: &str) {
    let dir = manifest_dir()
        .join("tests/fixtures/adversary-w29-u3")
        .join(fixture);
    let specification: Specification =
        spindle_spec::load(&dir).unwrap_or_else(|error| panic!("{fixture} loads: {error:?}"));
    let synthesis = spindle_conformance::synthesize(&specification)
        .unwrap_or_else(|error| panic!("{fixture} synthesises: {error:?}"));
    assert!(
        synthesis.refusals.is_empty(),
        "ESS refuses no scenario of {fixture}: {:#?}",
        synthesis.refusals
    );
    let suite = AdmittedSuite::from_suite(&synthesis.suite).expect("the suite is admitted");
    let interpreted: Vec<(String, Status)> = Runner::for_suite(suite.suite())
        .run_admitted(&suite, &Interpreted::for_model(specification.ir.clone()))
        .into_report()
        .scenarios
        .into_iter()
        .map(|run| (run.scenario.to_string(), run.status))
        .collect();
    let run =
        spindle_conformance::execute(&suite, &HostTarget::new(specification.ir.clone(), laptop()));
    let hosted: Vec<(String, Status)> = run
        .executed
        .scenarios
        .iter()
        .map(|result| (result.scenario.to_string(), result.status))
        .collect();
    assert_eq!(
        hosted.len(),
        2,
        "{fixture}: both scenarios run on the host: {hosted:?}"
    );
    assert_eq!(
        hosted, interpreted,
        "{fixture}: each scenario has the status on the host it has on ESS's interpreter"
    );
    for (id, status) in &hosted {
        assert_eq!(
            *status,
            Status::Passed,
            "{fixture}: {id} passes on the host"
        );
    }
}

#[test]
fn a_negated_forall_over_a_struct_list_is_answered_as_ess_answers_it() {
    host_answers_as_ess("forall-not");
}

#[test]
fn a_trimmed_starts_with_over_a_struct_list_is_answered_as_ess_answers_it() {
    host_answers_as_ess("starts");
}

#[test]
fn a_trimmed_in_over_a_struct_list_is_answered_as_ess_answers_it() {
    host_answers_as_ess("inlist");
}

#[test]
fn a_trimmed_equals_ignore_case_over_a_struct_list_is_answered_as_ess_answers_it() {
    host_answers_as_ess("ignorecase");
}

#[test]
fn a_trimmed_member_of_a_nested_struct_in_a_list_is_answered_as_ess_answers_it() {
    host_answers_as_ess("inner");
}

#[test]
fn a_quantifier_over_a_string_list_inside_a_list_element_is_answered_as_ess_answers_it() {
    host_answers_as_ess("nested-str");
}

#[test]
fn a_quantifier_over_a_struct_list_inside_a_list_element_is_answered_as_ess_answers_it() {
    host_answers_as_ess("nested-struct");
}

#[test]
fn a_conjunction_over_two_members_of_a_list_element_is_answered_as_ess_answers_it() {
    host_answers_as_ess("two-members");
}

#[test]
fn an_optional_list_of_a_struct_is_answered_as_ess_answers_it() {
    host_answers_as_ess("optional-label-list");
}

#[test]
fn a_string_list_is_answered_as_ess_answers_it() {
    host_answers_as_ess("string-list");
}

#[test]
fn an_optional_string_list_is_answered_as_ess_answers_it() {
    host_answers_as_ess("optional-string-list");
}

/// The release `ess/ess-inputs.yaml` requires.
fn required_release(root: &Path) -> String {
    let inputs = std::fs::read_to_string(root.join("ess/ess-inputs.yaml")).expect("inputs read");
    inputs
        .lines()
        .find_map(|line| line.strip_prefix("requires: ess "))
        .expect("ess-inputs.yaml requires an ess release")
        .trim()
        .to_string()
}

#[test]
fn every_manifest_lock_entry_and_generated_header_names_the_required_ess_release() {
    let root = manifest_dir().join("../..");
    let release = required_release(&root);
    let tag = format!("tag = \"{release}\"");
    let mut wrong = Vec::new();
    for manifest in [
        "Cargo.toml",
        "crates/spindle-control/Cargo.toml",
        "crates/spindle-telemetry/Cargo.toml",
    ] {
        let text = std::fs::read_to_string(root.join(manifest)).expect("manifest reads");
        let named: Vec<&str> = text
            .lines()
            .filter(|line| line.contains("github.com/beyond10x/ess\""))
            .collect();
        assert!(!named.is_empty(), "{manifest} names an ESS crate");
        for line in named {
            if !line.contains(&tag) {
                wrong.push(format!("{manifest}: {line}"));
            }
        }
    }
    let lock = std::fs::read_to_string(root.join("Cargo.lock")).expect("lock reads");
    let source = format!("source = \"git+https://github.com/beyond10x/ess?tag={release}#");
    let mut ess_sources = 0;
    for line in lock.lines() {
        if line.starts_with("source = \"git+https://github.com/beyond10x/ess?") {
            ess_sources += 1;
            if !line.starts_with(&source) {
                wrong.push(format!("Cargo.lock: {line}"));
            }
        }
    }
    assert_eq!(
        ess_sources, 9,
        "the lock holds nine ESS packages from one source"
    );
    for header in [
        "crates/spindle-observe-api/types.rs",
        "ui/src/api/generated/types.ts",
    ] {
        let text = std::fs::read_to_string(root.join(header)).expect("generated file reads");
        let first = text.lines().next().unwrap_or_default();
        if first != format!("// @generated by ESS {release}; do not edit.") {
            wrong.push(format!("{header}: {first}"));
        }
    }
    for report in [
        "crates/spindle-observe-api/types-report.json",
        "ui/src/api/generated/types-report.json",
    ] {
        let text = std::fs::read_to_string(root.join(report)).expect("report reads");
        if !text.contains(&format!("\"generator_version\": \"{release}\"")) {
            wrong.push(format!("{report}: generator_version is not {release}"));
        }
    }
    assert!(
        wrong.is_empty(),
        "every file that names the ESS release names {release}: {wrong:#?}"
    );
}
