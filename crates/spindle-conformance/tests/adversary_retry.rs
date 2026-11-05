//! Adversary cases for `on_failure: retry`, held to ESS's interpreter on every fixture in the
//! repository that declares such a binding, not only the one the unit wrote: a scenario the host
//! executes must answer as the interpreter answers it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ess_conformance::interpret::Interpreted;
use ess_conformance::{AdmittedSuite, Status};
use spindle_capability::Environment;
use spindle_conformance::HostTarget;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn statuses(run: &spindle_conformance::Run) -> BTreeMap<String, Status> {
    run.report()
        .scenarios
        .iter()
        .map(|result| (result.scenario.to_string(), result.status))
        .collect()
}

/// Every scenario whose status differs between the interpreter and the host, where the host
/// executed it.
fn divergent(fixture: &str) -> Vec<(String, Status, Option<Status>)> {
    let ir = spindle_spec::load(&root().join(fixture))
        .expect("the fixture loads")
        .ir;
    let synthesis = spindle_conformance::synthesize(&ir);
    let suite = AdmittedSuite::from_suite(&synthesis.suite).expect("the suite is admitted");
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("the laptop environment reads");
    let environment =
        spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"));
    let interpreted = spindle_conformance::execute(&suite, &Interpreted::for_model(ir.clone()));
    for result in &interpreted.report().scenarios {
        if result.status != Status::Passed {
            let diagnostics = result.diagnostics().collect::<Vec<_>>();
            eprintln!("  ess {}: {diagnostics:?}", result.scenario);
        }
    }
    let ess = statuses(&interpreted);
    let spindle = statuses(&spindle_conformance::execute(
        &suite,
        &HostTarget::new(ir, environment),
    ));
    eprintln!("{fixture}\n  ess:     {ess:?}\n  spindle: {spindle:?}");
    spindle
        .into_iter()
        .filter(|(_, status)| *status != Status::Unsupported)
        .filter(|(scenario, status)| ess.get(scenario) != Some(status))
        .map(|(scenario, status)| {
            let interpreted = ess.get(&scenario).copied();
            (scenario, status, interpreted)
        })
        .collect()
}

#[test]
fn adversary_the_notice_fixture_answers_as_the_interpreter() {
    let divergent = divergent("crates/spindle-host/tests/fixtures/adversary/notice");
    assert!(
        divergent.is_empty(),
        "(scenario, host, interpreter): {divergent:?}"
    );
}

#[test]
fn adversary_the_retried_fixture_answers_as_the_interpreter() {
    let divergent = divergent("crates/spindle-cli/tests/fixtures/retried");
    assert!(
        divergent.is_empty(),
        "(scenario, host, interpreter): {divergent:?}"
    );
}
