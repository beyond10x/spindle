//! The aggregate scenarios ESS synthesises (`<view>/aggregate`) for a model with aggregate views,
//! run against the in-process host (`story:view-aggregate-fields`): each one the host can be asked
//! passes, so the rows it answers are the ones the suite expects.
//!
//! The model is `spindle-host`'s `tests/fixtures/aggregates`: sessions on accounts, counted,
//! totalled and averaged with and without `group_by`, behind a filter and a measure's `where:`.

use std::path::{Path, PathBuf};

use ess_conformance::{AdmittedSuite, ScenarioId, Status};
use spindle_capability::Environment;
use spindle_conformance::HostTarget;

const AGGREGATES: &str = "crates/spindle-host/tests/fixtures/aggregates";

/// The one aggregate view the host is not asked through the suite: it declares a parameter, and
/// the in-process target reads views without parameters only (`HostTarget::query_view`).
const WITH_PARAMETER: &str = "ledger.sessions.AccountTotals";

/// The aggregate view ESS 0.92.0 synthesises no scenario for, and lists among its refusals.
const UNSYNTHESISED: &str = "ledger.sessions.ClosedPerAccount";

/// The aggregate scenarios ESS 0.92.0 synthesises for the fixture, in the suite's order: one per
/// aggregate view but [`UNSYNTHESISED`].
const SYNTHESISED: [&str; 6] = [
    "ledger.sessions.AccountTotals/aggregate",
    "ledger.sessions.OpenPerAccount/aggregate",
    "ledger.sessions.OpenTotals/aggregate",
    "ledger.sessions.PerRating/aggregate",
    "ledger.sessions.PerState/aggregate",
    "ledger.sessions.Totals/aggregate",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

/// Every aggregate scenario of the suite synthesised for the fixture passes against the host,
/// but the one for the view with a parameter, which the target cannot be asked.
#[test]
fn every_aggregate_scenario_passes_on_the_host() {
    let loaded = spindle_spec::load(&root().join(AGGREGATES)).expect("the fixture loads");
    let synthesis = spindle_conformance::synthesize(&loaded).expect("the suite synthesises");
    let unsynthesised = synthesis
        .refusals
        .iter()
        .filter_map(|refusal| match &refusal.scenario {
            Some(id @ ScenarioId::Aggregate { .. }) => Some(format!("{id}: {:?}", refusal.cause)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        unsynthesised.len() == 1 && unsynthesised[0].contains(UNSYNTHESISED),
        "ESS refuses the aggregate scenario of {UNSYNTHESISED} alone: {unsynthesised:#?}"
    );
    let suite = AdmittedSuite::from_suite(&synthesis.suite).expect("the suite is admitted");
    let run = spindle_conformance::execute(&suite, &HostTarget::new(loaded.ir, laptop()));
    let aggregates = run
        .executed
        .scenarios
        .iter()
        .filter(|result| matches!(result.scenario, ScenarioId::Aggregate { .. }))
        .map(|result| (result.scenario.to_string(), result.status))
        .collect::<Vec<_>>();
    let ids = aggregates
        .iter()
        .map(|(id, _)| id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ids, SYNTHESISED,
        "the aggregate scenarios ESS synthesises for the fixture"
    );
    for (id, status) in &aggregates {
        let expected = if id.starts_with(WITH_PARAMETER) {
            Status::Unsupported
        } else {
            Status::Passed
        };
        assert_eq!(
            *status,
            expected,
            "{id}: {:?}",
            run.executed
                .scenarios
                .iter()
                .find(|result| &result.scenario.to_string() == id)
        );
    }
}
