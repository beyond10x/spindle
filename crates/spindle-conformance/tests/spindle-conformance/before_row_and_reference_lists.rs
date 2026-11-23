//! The scenarios ESS synthesises for an external refusal answered before the row
//! (`story:external-refusal-before-row`) and for a `when_related:` guard over a list of references
//! (`story:when-related-over-reference-list`), run against the in-process host: every one
//! executes, and each has the status on the host that it has on ESS's own interpreter, passed.
//!
//! The models are `spindle-kernel`'s `tests/fixtures/before-row` and
//! `tests/fixtures/reference-list`.

use std::path::{Path, PathBuf};

use ess_conformance::interpret::Interpreted;
use ess_conformance::{AdmittedSuite, Runner, Status};
use spindle_capability::Environment;
use spindle_conformance::HostTarget;

const BEFORE_ROW: &str = "crates/spindle-kernel/tests/fixtures/before-row";
const REFERENCE_LIST: &str = "crates/spindle-kernel/tests/fixtures/reference-list";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

/// Runs the suite ESS synthesises for `fixture` on the interpreter and on the host; checks that
/// every scenario passes on both, and that `commands` each have a scenario of their own.
fn passes_as_on_the_interpreter(fixture: &str, commands: &[&str]) {
    let loaded = spindle_spec::load(&root().join(fixture)).expect("the fixture loads");
    let synthesis = spindle_conformance::synthesize(&loaded).expect("the suite synthesises");
    assert!(
        synthesis.refusals.is_empty(),
        "ESS refuses no scenario of {fixture}: {:#?}",
        synthesis.refusals
    );
    let suite = AdmittedSuite::from_suite(&synthesis.suite).expect("the suite is admitted");
    let interpreted = Runner::for_suite(suite.suite())
        .run_admitted(&suite, &Interpreted::for_model(loaded.ir.clone()))
        .into_report()
        .scenarios
        .into_iter()
        .map(|run| (run.scenario.to_string(), run.status))
        .collect::<Vec<_>>();
    let run = spindle_conformance::execute(&suite, &HostTarget::new(loaded.ir, laptop()));
    let hosted = run
        .executed
        .scenarios
        .iter()
        .map(|result| (result.scenario.to_string(), result.status))
        .collect::<Vec<_>>();
    eprintln!("{fixture}: {hosted:#?}");
    assert_eq!(
        hosted, interpreted,
        "{fixture}: each scenario has the status on the host it has on ESS's interpreter"
    );
    for (id, status) in &hosted {
        assert_eq!(
            *status,
            Status::Passed,
            "{fixture} {id}: {:#?}",
            run.executed
                .scenarios
                .iter()
                .find(|result| &result.scenario.to_string() == id)
        );
    }
    for command in commands {
        assert!(
            hosted
                .iter()
                .any(|(id, _)| id.starts_with(&format!("{command}/"))),
            "{fixture}: a scenario of {command} executes: {hosted:#?}"
        );
    }
}

#[test]
fn every_scenario_of_an_external_refusal_before_the_row_passes_on_the_host() {
    passes_as_on_the_interpreter(BEFORE_ROW, &["lobby.desk.SignIn"]);
}

#[test]
fn every_scenario_of_a_guard_over_a_list_of_references_passes_on_the_host() {
    passes_as_on_the_interpreter(
        REFERENCE_LIST,
        &["workshop.bench.AssembleKit", "workshop.bench.ReviseKit"],
    );
}
