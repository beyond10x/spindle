//! Adversary cases for the in-process conformance target: what a forced external outcome reaches,
//! and whether planted defects in what the host answers turn the gate red.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ess_compiler::refs::{CommandRef, OutcomeRef};
use ess_conformance::interpret::Interpreted;
use ess_conformance::{
    AdmittedSuite, ConformanceTarget, EventObservationRequest, ExternalOutcomeControl,
    ImplementationIdentity, ObservedEvent, RedeliveryRequest, ScenarioContext, ScenarioId,
    SemanticCommandRequest, SemanticCommandResult, SemanticViewRequest, SemanticViewResult,
    TargetError,
};
use ess_domain::command::OutcomeName;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use spindle_capability::Environment;
use spindle_conformance::{Counts, HostTarget};

const RELAY: &str = "crates/spindle-conformance/tests/fixtures/adversary-relay";
const REVIEW: &str = "relay.desk.Review";
const REQUEST: &str = "relay.request.Request";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn laptop() -> Environment {
    let text = std::fs::read_to_string(root().join("examples/environments/laptop.toml"))
        .expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

fn context(id: &str) -> ScenarioContext {
    ScenarioContext::new(
        ScenarioId::parse(&format!("{REVIEW}/outcome/{id}")).expect("a scenario id"),
        CorrelationId::new(format!("adversary-{id}")).expect("a correlation"),
    )
}

fn send(
    target: &impl ConformanceTarget,
    scenario: &ScenarioContext,
    command: &str,
    field: &str,
    value: &str,
) -> String {
    let mut input = BTreeMap::new();
    input.insert(field.to_owned(), Node::Text(value.to_owned()));
    let result = target
        .execute_command(SemanticCommandRequest {
            command: command.parse::<CommandRef>().expect("a command"),
            actor: None,
            caller: None,
            input,
            correlation: scenario.correlation.clone(),
        })
        .unwrap_or_else(|error| panic!("{command} runs: {error:?}"));
    result
        .outcome
        .map(|outcome| outcome.outcome.as_str().to_owned())
        .unwrap_or_else(|| "<undeclared>".to_owned())
}

fn force(target: &impl ConformanceTarget, scenario: &ScenarioContext, outcome: &str) {
    target
        .configure_external_outcome(ExternalOutcomeControl {
            force: OutcomeRef::new(
                REVIEW.parse::<CommandRef>().expect("a command"),
                OutcomeName::new(outcome).expect("an outcome name"),
            ),
            correlation: scenario.correlation.clone(),
        })
        .expect("the target takes the forcing");
}

/// The scenario forces `Review/flagged`, then sends `Request`, whose event the `relay-reviews`
/// binding delivers to `Review`, then sends `Review` itself: the outcome of that last `Review`.
fn after_a_bound_invocation(target: &impl ConformanceTarget) -> String {
    let scenario = context("flagged");
    target
        .begin_scenario(&scenario)
        .expect("the scenario opens");
    force(target, &scenario, "flagged");
    assert_eq!(
        send(target, &scenario, REQUEST, "request_id", "r-1"),
        "requested"
    );
    let direct = send(target, &scenario, REVIEW, "review_id", "r-2");
    target.end_scenario(&scenario).expect("the scenario closes");
    direct
}

fn relay() -> ess_compiler::EssIr {
    spindle_spec::load(&root().join(RELAY))
        .expect("the relay fixture loads")
        .ir
}

/// ESS's interpreter consumes a forced outcome at the next invocation of its command, a binding's
/// invocation included (`interpret/bindings.rs` `attempt` goes through `execute_command`), so the
/// direct `Review` after it is not forced. Spindle lapses the forcing only for the command the
/// scenario sent, so the binding's `Review` is forced and the forcing is still there for the
/// next one.
#[test]
fn a_forced_outcome_lapses_after_the_invocation_a_binding_makes() {
    let ess = after_a_bound_invocation(&Interpreted::for_model(relay()));
    let spindle = after_a_bound_invocation(&HostTarget::new(relay(), laptop()));
    assert_eq!(
        spindle, ess,
        "the Review sent after the binding's Review: Spindle answered `{spindle}`, ESS's \
         interpreter `{ess}`"
    );
}

/// A forcing a scenario did not consume does not reach the next scenario.
#[test]
fn a_forcing_left_unconsumed_does_not_reach_the_next_scenario() {
    let target = HostTarget::new(relay(), laptop());
    let first = context("flagged");
    target.begin_scenario(&first).expect("the scenario opens");
    force(&target, &first, "flagged");
    target.end_scenario(&first).expect("the scenario closes");

    let second = context("passed");
    target.begin_scenario(&second).expect("the scenario opens");
    assert_eq!(send(&target, &second, REVIEW, "review_id", "r-1"), "passed");
    target.end_scenario(&second).expect("the scenario closes");
}

/// A forcing holds for the next direct invocation of its command and not for the one after.
#[test]
fn a_forcing_holds_for_the_next_invocation_only() {
    let target = HostTarget::new(relay(), laptop());
    let scenario = context("flagged");
    target
        .begin_scenario(&scenario)
        .expect("the scenario opens");
    force(&target, &scenario, "flagged");
    assert_eq!(
        send(&target, &scenario, REVIEW, "review_id", "r-1"),
        "flagged"
    );
    assert_eq!(
        send(&target, &scenario, REVIEW, "review_id", "r-2"),
        "passed"
    );
    target.end_scenario(&scenario).expect("the scenario closes");
}

// ---- planted defects, run through the gate's own arithmetic ------------------------------------

/// What the planted target gets wrong.
#[derive(Clone, Copy)]
enum Plant {
    /// Every command publishes nothing.
    NoEvents,
    /// Every view read answers one row fewer.
    RowDropped,
}

struct Planted(HostTarget, Plant);

impl ConformanceTarget for Planted {
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
        let mut result = self.0.execute_command(request)?;
        if matches!(self.1, Plant::NoEvents) {
            result.direct_events.clear();
        }
        Ok(result)
    }
    fn query_view(&self, request: SemanticViewRequest) -> Result<SemanticViewResult, TargetError> {
        let mut result = self.0.query_view(request)?;
        if matches!(self.1, Plant::RowDropped) {
            result.rows.pop();
        }
        Ok(result)
    }
    fn observe_events(
        &self,
        request: EventObservationRequest,
    ) -> Result<Vec<ObservedEvent>, TargetError> {
        if matches!(self.1, Plant::NoEvents) {
            return Ok(Vec::new());
        }
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

/// Whether `task check:conformance` would pass a run against `target`, as the task runs it: no
/// known-failures declaration, every executed scenario passed, and the committed counts held, the
/// executed set and the totals both.
fn gate_passes(target: &impl ConformanceTarget) -> bool {
    let loaded = spindle_spec::load(&root().join("ess")).expect("ess/ loads");
    let synthesis = spindle_conformance::synthesize(&loaded).expect("the suite synthesises");
    let suite = AdmittedSuite::from_suite(&synthesis.suite).expect("the suite is admitted");
    let committed = Counts::committed(
        &std::fs::read_to_string(root().join("conformance.toml")).expect("the committed counts"),
    )
    .expect("the counts read");
    let run = spindle_conformance::execute(&suite, target);
    run.passed() && run.set_drops(&committed).is_empty() && run.counts().drops(committed).is_empty()
}

fn planted(plant: Plant) -> Planted {
    let loaded = spindle_spec::load(&root().join("ess")).expect("ess/ loads");
    Planted(HostTarget::new(loaded.ir, laptop()), plant)
}

#[test]
fn the_gate_passes_the_unplanted_target() {
    let loaded = spindle_spec::load(&root().join("ess")).expect("ess/ loads");
    assert!(gate_passes(&HostTarget::new(loaded.ir, laptop())));
}

#[test]
fn a_host_that_publishes_no_event_fails_the_gate() {
    assert!(!gate_passes(&planted(Plant::NoEvents)));
}

#[test]
fn a_host_that_drops_a_view_row_fails_the_gate() {
    assert!(!gate_passes(&planted(Plant::RowDropped)));
}
