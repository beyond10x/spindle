//! A target that answers one command wrongly fails the run, naming the scenario.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ess_conformance::{
    AdmittedSuite, ConformanceTarget, EventObservationRequest, ExternalOutcomeControl,
    ImplementationIdentity, ObservedEvent, RedeliveryRequest, ScenarioContext,
    SemanticCommandRequest, SemanticCommandResult, SemanticViewRequest, SemanticViewResult,
    TargetError,
};
use ess_primitives::node::Node;
use spindle_capability::Environment;
use spindle_conformance::HostTarget;

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
    let synthesis = spindle_conformance::synthesize(&loaded.ir);
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

/// With the committed known-failures declaration, a second scenario that fails, one the
/// declaration does not name, still fails the gate.
#[test]
fn a_second_failure_fails_the_gate_despite_the_declaration() {
    let loaded = spindle_spec::load(&root().join("ess")).expect("ess/ loads");
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("the laptop environment reads");
    let environment = Environment::parse(&text).expect("the laptop environment parses");
    let synthesis = spindle_conformance::synthesize(&loaded.ir);
    let suite = AdmittedSuite::from_suite(&synthesis.suite).expect("the suite is admitted");
    let declaration = ess_conformance::known_failures::Declaration::from_json(
        &std::fs::read_to_string(root().join("conformance-known-failures.json"))
            .expect("the committed declaration"),
    )
    .expect("the declaration reads");

    let right = spindle_conformance::execute(
        &suite,
        &HostTarget::new(
            loaded.ir.clone(),
            spindle_conformance::in_memory(&environment),
        ),
    );
    let known = right
        .account(&suite, &declaration)
        .expect("the declaration binds the run");
    assert!(right.passed_with(&known), "{}", right.report());
    assert!(known.accounting.unexpected_failed.is_empty());

    let wrong = spindle_conformance::execute(
        &suite,
        &Wrong(HostTarget::new(
            loaded.ir.clone(),
            spindle_conformance::in_memory(&environment),
        )),
    );
    let known = wrong
        .account(&suite, &declaration)
        .expect("the declaration binds the run");
    assert!(!wrong.passed_with(&known), "{}", wrong.report());
    assert!(
        known
            .accounting
            .unexpected_failed
            .iter()
            .any(|scenario| scenario.contains(WRONG)),
        "{:?}",
        known.accounting.unexpected_failed
    );
    assert_eq!(
        known.accounting.known_failed.len(),
        declaration.failures().len()
    );
}
