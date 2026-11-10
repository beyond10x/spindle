//! A target that answers one command wrongly fails the run, naming the scenario; a declaration
//! does not hide a failure it does not name, nor a scenario it names that passed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ess_conformance::known_failures::Declaration;
use ess_conformance::{
    AdmittedSuite, ConformanceTarget, EventObservationRequest, ExternalOutcomeControl,
    ImplementationIdentity, ObservedEvent, RedeliveryRequest, ScenarioContext,
    SemanticCommandRequest, SemanticCommandResult, SemanticViewRequest, SemanticViewResult, Status,
    TargetError,
};
use ess_primitives::node::Node;
use spindle_capability::Environment;
use spindle_conformance::{Counts, HostTarget};

/// The command the double answers wrongly.
const WRONG: &str = "spindle.deployment.DeclareEnvironment";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// [`HostTarget`], except that it answers [`WRONG`] as an outcome the model does not declare.
struct Wrong(HostTarget);

impl ConformanceTarget for Wrong {
    fn identity(&self) -> Result<ImplementationIdentity, TargetError> {
        self.0.identity()
    }

    fn fixture_values(
        &self,
        scenario: &ScenarioContext,
        contract: &ess_conformance::fixtures::Contract,
    ) -> Result<BTreeMap<String, Node>, TargetError> {
        self.0.fixture_values(scenario, contract)
    }

    fn establish_entity(
        &self,
        request: ess_conformance::target::EntitySetupRequest,
    ) -> Result<(), TargetError> {
        self.0.establish_entity(request)
    }

    fn begin_scenario(&self, scenario: &ScenarioContext) -> Result<(), TargetError> {
        self.0.begin_scenario(scenario)
    }

    fn execute_command(
        &self,
        request: SemanticCommandRequest,
    ) -> Result<SemanticCommandResult, TargetError> {
        if request.command.to_string() == WRONG {
            return Ok(SemanticCommandResult::undeclared());
        }
        self.0.execute_command(request)
    }

    fn query_view(&self, request: SemanticViewRequest) -> Result<SemanticViewResult, TargetError> {
        self.0.query_view(request)
    }

    fn observe_events(
        &self,
        request: EventObservationRequest,
    ) -> Result<Vec<ObservedEvent>, TargetError> {
        self.0.observe_events(request)
    }

    fn configure_external_outcome(
        &self,
        request: ExternalOutcomeControl,
    ) -> Result<(), TargetError> {
        self.0.configure_external_outcome(request)
    }

    fn redeliver_event(&self, request: RedeliveryRequest) -> Result<(), TargetError> {
        self.0.redeliver_event(request)
    }

    fn end_scenario(&self, scenario: &ScenarioContext) -> Result<(), TargetError> {
        self.0.end_scenario(scenario)
    }
}

#[test]
fn a_failing_scenario_fails_the_gate() {
    let loaded = spindle_spec::load(&root().join("ess")).expect("ess/ loads");
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("the laptop environment reads");
    let environment = Environment::parse(&text).expect("the laptop environment parses");
    let synthesis = spindle_conformance::synthesize(&loaded).expect("the suite synthesises");
    let suite = AdmittedSuite::from_suite(&synthesis.suite).expect("the suite is admitted");

    let right = spindle_conformance::execute(
        &suite,
        &HostTarget::new(
            loaded.ir.clone(),
            spindle_conformance::in_memory(&environment),
        ),
    );
    let failing = |run: &spindle_conformance::Run| {
        run.report()
            .failures()
            .filter(|result| result.scenario.to_string().contains(WRONG))
            .count()
    };
    assert_eq!(failing(&right), 0, "{}", right.report());

    let wrong = spindle_conformance::execute(
        &suite,
        &Wrong(HostTarget::new(
            loaded.ir.clone(),
            spindle_conformance::in_memory(&environment),
        )),
    );
    assert!(!wrong.passed(), "{}", wrong.report());
    assert_eq!(wrong.exit_status(), 1);
    let mut printed = Vec::new();
    wrong.render(&mut printed).expect("rendering to memory");
    let printed = String::from_utf8(printed).expect("utf-8");
    let named = printed
        .lines()
        .filter(|line| line.starts_with("failed ") && line.contains(WRONG))
        .count();
    assert!(
        named > 0,
        "a scenario sending {WRONG} is named as failed: {printed}"
    );
    assert!(failing(&wrong) > 0, "{}", wrong.report());
    assert!(
        wrong.counts().passed < right.counts().passed,
        "{:?} against {:?}",
        wrong.counts(),
        right.counts()
    );
}

/// The scenario `ess/ess-inputs.yaml` lists under `scenarios:`, which ESS's synthesis cannot write.
const AUTHORED: &str = "spindle.placement/authored/launch-not-admitted";

/// The suite and the run of `ess/` against the unaltered target, and the target [`Wrong`] makes.
fn ess_runs() -> (
    AdmittedSuite,
    spindle_conformance::Run,
    spindle_conformance::Run,
) {
    let loaded = spindle_spec::load(&root().join("ess")).expect("ess/ loads");
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("the laptop environment reads");
    let environment = Environment::parse(&text).expect("the laptop environment parses");
    let synthesis = spindle_conformance::synthesize(&loaded).expect("the suite synthesises");
    let suite = AdmittedSuite::from_suite(&synthesis.suite).expect("the suite is admitted");
    let right = spindle_conformance::execute(
        &suite,
        &HostTarget::new(
            loaded.ir.clone(),
            spindle_conformance::in_memory(&environment),
        ),
    );
    let wrong = spindle_conformance::execute(
        &suite,
        &Wrong(HostTarget::new(
            loaded.ir.clone(),
            spindle_conformance::in_memory(&environment),
        )),
    );
    (suite, right, wrong)
}

/// An `ess-known-failures/1` declaration of `scenarios`, bound to `suite`'s own digests and this
/// build, as a release would write one.
fn declaration(suite: &AdmittedSuite, scenarios: &[String]) -> Declaration {
    let implementation = format!("b10x-spindle {}", env!("CARGO_PKG_VERSION"));
    let mut failures = scenarios
        .iter()
        .map(|scenario| {
            serde_json::json!({
                "reason": "declared by the test to see what the gate makes of it",
                "scenario": scenario,
                "tracking": "none",
            })
        })
        .collect::<Vec<_>>();
    failures.sort_by(|left, right| left["scenario"].as_str().cmp(&right["scenario"].as_str()));
    let text = serde_json::json!({
        "failures": failures,
        "format": "ess-known-failures/1",
        "implementation": implementation,
        "implementation_build": spindle_conformance::build(),
        "spec_digest": suite.suite().provenance.spec_digest.to_string(),
        "suite_digest": suite.digest(),
    });
    Declaration::from_json(&text.to_string()).expect("the declaration reads")
}

/// The scenarios a run failed, by id: those that ended `failed`, the only status a declaration
/// may name.
fn failed(run: &spindle_conformance::Run) -> Vec<String> {
    run.report()
        .failures()
        .filter(|result| result.status == Status::Failed)
        .map(|result| result.scenario.to_string())
        .collect()
}

/// The suite the library synthesises from `ess/` is the one `task check:conformance` runs: the
/// scenarios `ess-inputs.yaml` lists are in it, and it executes exactly the committed set, every
/// scenario passing with no declaration.
#[test]
fn the_library_runs_the_suite_the_task_holds() {
    let (suite, right, _) = ess_runs();
    let committed = Counts::committed(
        &std::fs::read_to_string(root().join("conformance.toml")).expect("the committed counts"),
    )
    .expect("the counts read");
    assert!(
        suite
            .suite()
            .scenarios
            .keys()
            .any(|id| id.to_string() == AUTHORED),
        "{AUTHORED} is in the suite"
    );
    assert_eq!(
        right.executed_scenarios(),
        committed.executed_scenarios,
        "{:?}",
        right.set_drops(&committed)
    );
    assert!(right.passed(), "{}", right.report());
    assert!(right.counts().drops(committed).is_empty());
}

/// A declared scenario that passes fails the gate: ESS refuses the declaration as stale.
#[test]
fn a_declared_scenario_that_passes_fails_the_gate() {
    let (suite, right, wrong) = ess_runs();
    let failing = failed(&wrong);
    assert!(!failing.is_empty(), "{}", wrong.report());
    let stale = declaration(&suite, &failing[..1]);
    let Err(refusal) = right.account(&suite, &stale) else {
        panic!("a declared {} that passed is accounted", failing[0]);
    };
    let refusal = refusal.to_string();
    assert!(
        refusal.contains("known-failures.stale") && refusal.contains(&failing[0]),
        "{refusal}"
    );
}

/// With a declaration of one scenario the run fails, a second scenario that fails, one the
/// declaration does not name, still fails the gate.
#[test]
fn a_second_failure_fails_the_gate_despite_the_declaration() {
    let (suite, _, wrong) = ess_runs();
    let failing = failed(&wrong)
        .into_iter()
        .filter(|scenario| scenario.contains(WRONG))
        .collect::<Vec<_>>();
    assert!(failing.len() >= 2, "{failing:?}");
    let one = declaration(&suite, &failing[..1]);

    let known = wrong
        .account(&suite, &one)
        .expect("the declaration binds the run");
    assert!(!wrong.passed_with(&known), "{}", wrong.report());
    assert!(
        known
            .accounting
            .unexpected_failed
            .iter()
            .any(|scenario| scenario.contains(WRONG) && *scenario != failing[0]),
        "{:?}",
        known.accounting.unexpected_failed
    );
    assert_eq!(known.accounting.known_failed.len(), 1);

    // Declaring every failure is what makes the same run pass: the one undeclared failure is
    // what failed it above.
    let every = declaration(&suite, &failed(&wrong));
    let known = wrong
        .account(&suite, &every)
        .expect("the declaration binds the run");
    assert!(wrong.passed_with(&known), "{:?}", known.accounting);
    assert_eq!(wrong.counts_with(&known).known_failed, failed(&wrong).len());
}
