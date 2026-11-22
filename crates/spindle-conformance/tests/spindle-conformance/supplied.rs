//! A supplied suite is run as it was written, without synthesising one, and every host a target
//! opens shares one compiled model and one admission while each scenario keeps stores of its own
//! (story:conform-reuses-supplied-suite).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ess_compiler::refs::CommandRef;
use ess_conformance::{ConformanceTarget, ScenarioContext, ScenarioId, SemanticCommandRequest};
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use spindle_capability::Environment;
use spindle_conformance::{HostTarget, SynthesisError, Unprepared};
use spindle_spec::{Scenario, Specification};

const FORCING: &str = "crates/spindle-conformance/tests/fixtures/forcing";
const OTHER: &str = "crates/spindle-cli/tests/fixtures/replay";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

/// The forcing fixture, listing one authored scenario whose step takes `outcome`.
fn forcing(outcome: &str) -> Specification {
    let mut specification = spindle_spec::load(&root().join(FORCING)).expect("the fixture loads");
    specification.scenarios.push(Scenario {
        origin: "scenarios/opens.yaml".to_owned(),
        text: format!(
            "type: ess-scenario/3\n\
             domain: forcing.desk\n\
             scenario: opens\n\
             summary: A case opens.\n\
             timeline:\n\
             \x20 - at: 2026-01-05T09:00:00Z\n\
             \x20   command: forcing.desk.OpenCase\n\
             \x20   input:\n\
             \x20     case_id: c-1\n\
             \x20   outcome: {outcome}\n"
        ),
    });
    specification
}

/// The suite `ess conform synthesize --scenarios` writes for `specification`.
fn written(specification: &Specification) -> String {
    spindle_conformance::synthesize(specification)
        .expect("the suite synthesises")
        .suite
        .to_canonical_json()
        .expect("the suite renders")
}

#[test]
fn a_supplied_suite_is_not_synthesised() {
    let specification = forcing("opened");
    let text = written(&specification);

    let (prepared, syntheses) = spindle_conformance::syntheses(|| {
        spindle_conformance::prepare(&specification, Some(&text))
    });
    assert_eq!(syntheses, 0, "a supplied suite is not synthesised");
    let prepared = prepared.expect("the supplied suite is admitted");
    assert_eq!(
        prepared.suite.original_json(),
        text,
        "run as it was written"
    );
    assert!(
        prepared.refusals.is_none(),
        "no refusal report is made for a supplied suite"
    );

    let (prepared, syntheses) =
        spindle_conformance::syntheses(|| spindle_conformance::prepare(&specification, None));
    assert_eq!(syntheses, 1, "without a supplied suite, one is synthesised");
    assert!(prepared.expect("it is admitted").refusals.is_some());
}

/// ESS still checks the authored scenarios a specification lists when the suite is supplied, and
/// a refusal refuses the run as it refuses a synthesised one, without synthesising.
#[test]
fn a_refused_authored_scenario_refuses_a_supplied_suite() {
    let text = written(&forcing("opened"));
    let refused = forcing("not-an-outcome");
    let (prepared, syntheses) =
        spindle_conformance::syntheses(|| spindle_conformance::prepare(&refused, Some(&text)));
    assert_eq!(syntheses, 0);
    match prepared {
        Err(Unprepared::Refused(SynthesisError::Authored(refusals))) => {
            assert!(!refusals.is_empty());
            assert!(
                refusals.iter().any(|refusal| refusal.contains("opens")),
                "{refusals:?}"
            );
        }
        Err(other) => panic!("refused otherwise: {other}"),
        Ok(_) => panic!("a refused authored scenario was run"),
    }
}

#[test]
fn a_supplied_suite_of_another_specification_is_refused() {
    let other = spindle_spec::load(&root().join(OTHER)).expect("the fixture loads");
    let mut synthesis = ess_conformance::synthesize(&other.ir);
    synthesis.suite.select_fresh_format_for(&other.ir);
    let text = synthesis.suite.to_canonical_json().expect("it renders");
    let (prepared, syntheses) = spindle_conformance::syntheses(|| {
        spindle_conformance::prepare(&forcing("opened"), Some(&text))
    });
    assert_eq!(syntheses, 0);
    match prepared {
        Err(Unprepared::Foreign { system, .. }) => assert_eq!(system, "desk"),
        Err(other) => panic!("refused otherwise: {other}"),
        Ok(_) => panic!("a suite of another specification was run"),
    }
}

fn target() -> HostTarget {
    let ir = spindle_spec::load(&root().join(FORCING))
        .expect("the fixture loads")
        .ir;
    HostTarget::new(ir, laptop())
}

fn scenario(target: &HostTarget, name: &str) -> ScenarioContext {
    let scenario = ScenarioContext::new(
        ScenarioId::parse(&format!("forcing.desk.OpenCase/outcome/{name}")).expect("an id"),
        CorrelationId::new(format!("fresh-{name}")).expect("a correlation"),
    );
    target
        .begin_scenario(&scenario)
        .expect("the scenario opens");
    scenario
}

fn send(target: &HostTarget, scenario: &ScenarioContext, command: &str, case: &str) -> String {
    target
        .execute_command(SemanticCommandRequest {
            command: command.parse::<CommandRef>().expect("a command"),
            actor: None,
            caller: None,
            input: BTreeMap::from([("case_id".to_owned(), Node::Text(case.to_owned()))]),
            correlation: scenario.correlation.clone(),
        })
        .unwrap_or_else(|error| panic!("{command} runs: {error:?}"))
        .outcome
        .map(|outcome| outcome.outcome.as_str().to_owned())
        .unwrap_or_else(|| "<undeclared>".to_owned())
}

/// Two scenarios in sequence: the case the first opens and closes does not exist in the second,
/// which opens and closes it again.
#[test]
fn every_scenario_gets_fresh_stores() {
    let target = target();
    for name in ["first", "second"] {
        let scenario = scenario(&target, name);
        assert_eq!(
            send(&target, &scenario, "forcing.desk.OpenCase", "c-1"),
            "opened",
            "{name}"
        );
        assert_eq!(
            send(&target, &scenario, "forcing.desk.CloseCase", "c-1"),
            "closed",
            "{name}"
        );
        target.end_scenario(&scenario).expect("the scenario closes");
    }
}

/// A target admits its model once: opening it and three scenarios after it admit nothing again.
#[test]
fn admission_runs_once_per_target() {
    let ((), admissions) = spindle_host::admissions(|| {
        let target = target();
        target.open().expect("a host opens");
        for name in ["one", "two", "three"] {
            let scenario = scenario(&target, name);
            assert_eq!(
                send(&target, &scenario, "forcing.desk.OpenCase", "c-1"),
                "opened"
            );
            target.end_scenario(&scenario).expect("the scenario closes");
        }
    });
    assert_eq!(admissions, 1);
}
