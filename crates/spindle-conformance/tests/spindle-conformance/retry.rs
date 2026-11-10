//! An `on_failure: retry` binding, held to ESS's interpreter: the suite ESS synthesizes for the
//! `filing` fixture forces `Record` to fail once and expects the binding to retry it until
//! `Recorded` is published, and the in-process host answers every scenario as the interpreter
//! does.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ess_conformance::interpret::Interpreted;
use ess_conformance::{AdmittedSuite, Status};
use spindle_capability::Environment;
use spindle_conformance::HostTarget;

const FILING: &str = "crates/spindle-host/tests/fixtures/retry";
const ON_FAILURE: &str = "record-filing/binding/on-failure";

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

#[test]
fn the_retry_scenario_passes_on_the_host_as_on_the_interpreter() {
    let specification = spindle_spec::load(&root().join(FILING)).expect("the filing fixture loads");
    let synthesis = spindle_conformance::synthesize(&specification).expect("the suite synthesises");
    let ir = specification.ir;
    let suite = AdmittedSuite::from_suite(&synthesis.suite).expect("the suite is admitted");
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("the laptop environment reads");
    let environment =
        spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"));

    let ess = statuses(&spindle_conformance::execute(
        &suite,
        &Interpreted::for_model(ir.clone()),
    ));
    let spindle = statuses(&spindle_conformance::execute(
        &suite,
        &HostTarget::new(ir, environment),
    ));

    assert_eq!(
        ess.get(ON_FAILURE),
        Some(&Status::Passed),
        "the interpreter: {ess:?}"
    );
    assert_eq!(
        spindle.get(ON_FAILURE),
        Some(&Status::Passed),
        "the host: {spindle:?}"
    );
    assert_eq!(
        spindle.keys().collect::<Vec<_>>(),
        ess.keys().collect::<Vec<_>>()
    );
    let executed = spindle
        .iter()
        .filter(|(_, status)| **status != Status::Unsupported)
        .collect::<Vec<_>>();
    assert!(executed.len() > 1, "{spindle:?}");
    for (scenario, status) in executed {
        assert_eq!(
            Some(status),
            ess.get(scenario),
            "{scenario}: every scenario the host executes answers as the interpreter's"
        );
    }
    let skipped = spindle
        .iter()
        .filter(|(_, status)| **status == Status::Unsupported)
        .map(|(scenario, _)| scenario.as_str())
        .collect::<Vec<_>>();
    assert!(
        skipped
            .iter()
            .all(|scenario| scenario.ends_with("/binding/mapping")),
        "the host skips only what it cannot observe, the invocations a binding made: {skipped:?}"
    );
}
