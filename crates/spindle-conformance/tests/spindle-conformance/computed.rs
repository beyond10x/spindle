//! The scenarios ESS synthesises for a model with computed view fields (`expr:`), run against the
//! in-process host (`story:view-computed-fields`): each one passes where ESS's own interpreter
//! passes it, so every computed value the suite expects of a view row is the one the host answers.
//!
//! The model is `spindle-host`'s `tests/fixtures/computed`: entries on teams, read by views that
//! compute figures from each entry, one beside a field read through a reference.

use std::path::{Path, PathBuf};

use ess_conformance::interpret::Interpreted;
use ess_conformance::{AdmittedSuite, Runner, ScenarioStep, Status, ViewExpectation};
use spindle_capability::Environment;
use spindle_conformance::HostTarget;

const COMPUTED: &str = "crates/spindle-host/tests/fixtures/computed";

/// The scenarios ESS 0.92.0 synthesises for the fixture, in the suite's order.
const SYNTHESISED: [&str; 4] = [
    "league.entries.Enter/outcome/entered",
    "league.entries.Records/reference/team_id",
    "league.teams.AddTeam/outcome/added",
    "league.teams.DropTeam/outcome/dropped",
];

/// The scenario that asserts computed view fields: each entry it enters is read back through
/// every view over entries, the computed fields among the fields it expects.
const ASSERTS_COMPUTED: &str = "league.entries.Enter/outcome/entered";

/// The scenario ESS 0.92.0's own interpreter fails on the fixture: it enters each entry without
/// the optional `team_id` and expects the reference to read the team row nonetheless. It reads
/// no computed field, and the host answers it as the interpreter does.
const FAILS_ON_THE_INTERPRETER: &str = "league.entries.Records/reference/team_id";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

/// Every scenario of the suite synthesised for the fixture has the status on the host that it
/// has on ESS's interpreter: [`ASSERTS_COMPUTED`] and the others pass, and the one the
/// interpreter fails fails on both alike.
#[test]
fn every_scenario_over_computed_views_passes_on_the_host_as_on_the_interpreter() {
    let loaded = spindle_spec::load(&root().join(COMPUTED)).expect("the fixture loads");
    let synthesis = spindle_conformance::synthesize(&loaded).expect("the suite synthesises");
    assert!(
        synthesis.refusals.is_empty(),
        "ESS refuses no scenario of the fixture: {:#?}",
        synthesis.refusals
    );
    let computed = loaded
        .ir
        .views()
        .values()
        .flat_map(|view| view.computed.keys().cloned())
        .collect::<Vec<_>>();
    let (_, scenario) = synthesis
        .suite
        .scenarios
        .iter()
        .find(|(id, _)| id.to_string() == ASSERTS_COMPUTED)
        .expect("the entered scenario is synthesised");
    let asserted = scenario
        .steps
        .iter()
        .filter_map(|step| match step {
            ScenarioStep::ExpectView {
                expectation: ViewExpectation::Contains { fields },
                ..
            }
            | ScenarioStep::EventuallyView {
                expectation: ViewExpectation::Contains { fields },
                ..
            } => Some(fields),
            _ => None,
        })
        .flat_map(|fields| fields.keys().filter(|name| computed.contains(name)))
        .collect::<std::collections::BTreeSet<_>>();
    assert!(
        !asserted.is_empty(),
        "{ASSERTS_COMPUTED} expects computed fields of a view row: {scenario:#?}"
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
    assert_eq!(
        hosted.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
        SYNTHESISED,
        "the scenarios ESS synthesises for the fixture"
    );
    assert_eq!(
        hosted, interpreted,
        "each scenario has the status on the host it has on ESS's interpreter"
    );
    for (id, status) in &hosted {
        let expected = if id == FAILS_ON_THE_INTERPRETER {
            Status::Failed
        } else {
            Status::Passed
        };
        assert_eq!(
            *status,
            expected,
            "{id}: {:#?}",
            run.executed
                .scenarios
                .iter()
                .find(|result| &result.scenario.to_string() == id)
        );
    }
    eprintln!("computed fields {ASSERTS_COMPUTED} expects: {asserted:?}");
}
