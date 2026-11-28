//! A step the system takes itself, awaited by a scenario (`story:conform-awaits-system-steps`),
//! over the neutral `await-system` fixture: a caller adds agents and opens tickets, and only the
//! system routes a ticket (`Route`, `await_within: PT5S`) or closes it (`Close`, no bound).
//!
//! `Route` has a stated input (`lane`, which its `rushed` and `routed` guards read), an `Optional`
//! input (`note`, which holds the ticket back where it is present) and a `when_related:` input
//! (`agent`, refused where no agent or a busy one carries it), so a target that minted every input
//! would be refused where ESS's interpreter reaches a branch.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;

use ess_conformance::interpret::Interpreted;
use ess_conformance::occurrence_clock::CommandClock;
use ess_conformance::target::{
    AbsentInputRequest, AwaitAnswer, AwaitRequest, EntitySetupRequest, RecordedCommandCompletion,
};
use ess_conformance::{
    AdmittedSuite, CheckCode, ConformanceTarget, ElapsedObservation, ElapsedObservationRequest,
    EventDeliveryRequest, EventObservationRequest, ExternalOutcomeControl, ImplementationIdentity,
    InstantMark, InvocationObservationRequest, NowHold, ObservedEvent, ObservedInvocation,
    OrderedScan, OrderedScanRequest, RedeliveryRequest, ScenarioContext, ScenarioStep,
    SemanticCommandRequest, SemanticCommandResult, SemanticViewRequest, SemanticViewResult, Status,
    TargetError,
};
use ess_primitives::node::Node;
use spindle_capability::Environment;
use spindle_conformance::{HostTarget, Run};
use spindle_spec::Specification;

const FIXTURE: &str = "tests/fixtures/await-system";

fn specification() -> Specification {
    spindle_spec::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE))
        .expect("the fixture loads")
}

fn laptop() -> Environment {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/environments/laptop.toml");
    let text = std::fs::read_to_string(path).expect("the laptop environment reads");
    spindle_conformance::in_memory(&Environment::parse(&text).expect("it parses"))
}

/// The suite ESS synthesises from the fixture, as `conform` prepares it.
fn suite(specification: &Specification) -> AdmittedSuite {
    spindle_conformance::prepare(specification, None)
        .expect("the fixture's suite is admitted")
        .suite
}

fn host(specification: &Specification) -> HostTarget {
    HostTarget::new(specification.ir.clone(), laptop())
}

/// Every scenario of `suite` with a step of `kind`: an `await_outcome` or an `expect_not_issued`.
fn scenarios_with(suite: &AdmittedSuite, kind: fn(&ScenarioStep) -> bool) -> Vec<String> {
    suite
        .suite()
        .scenarios
        .iter()
        .filter(|(_, scenario)| scenario.steps.iter().any(kind))
        .map(|(id, _)| id.to_string())
        .collect()
}

fn awaits(step: &ScenarioStep) -> bool {
    matches!(step, ScenarioStep::AwaitOutcome { .. })
}

fn not_issued(step: &ScenarioStep) -> bool {
    matches!(step, ScenarioStep::ExpectNotIssued { .. })
}

fn status(run: &Run, id: &str) -> Status {
    run.report()
        .scenarios
        .iter()
        .find(|result| result.scenario.to_string() == id)
        .unwrap_or_else(|| panic!("`{id}` ran"))
        .status
}

fn rendered(run: &Run) -> String {
    let mut out = Vec::new();
    run.render(&mut out).expect("the run renders");
    String::from_utf8(out).expect("UTF-8")
}

/// One answer of [`ConformanceTarget::await_outcome`], as two targets are compared on it: the
/// branch, its error, the input used, the events published and the clock reading.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Seen {
    Reached {
        correlation: String,
        command: String,
        outcome: Option<String>,
        error: Option<String>,
        input: BTreeMap<String, Node>,
        events: Vec<(String, BTreeMap<String, Node>)>,
        elapsed_ms: Option<u64>,
    },
    NotIssued {
        correlation: String,
        command: String,
        elapsed_ms: Option<u64>,
    },
    Failed {
        correlation: String,
        command: String,
        error: String,
    },
}

impl Seen {
    fn of(request: &AwaitRequest, answer: &Result<AwaitAnswer, TargetError>) -> Self {
        let (correlation, command) = (request.correlation.to_string(), request.command.to_string());
        match answer {
            Ok(AwaitAnswer::Reached {
                result,
                input,
                elapsed_ms,
            }) => Self::Reached {
                correlation,
                command,
                outcome: result.outcome.as_ref().map(ToString::to_string),
                error: result.error.as_ref().map(|error| error.error.to_string()),
                input: input.clone(),
                events: result
                    .direct_events
                    .iter()
                    .map(|event| (event.event.to_string(), event.payload.clone()))
                    .collect(),
                elapsed_ms: *elapsed_ms,
            },
            Ok(AwaitAnswer::NotIssued { elapsed_ms }) => Self::NotIssued {
                correlation,
                command,
                elapsed_ms: *elapsed_ms,
            },
            Err(error) => Self::Failed {
                correlation,
                command,
                error: format!("{error:?}"),
            },
        }
    }
}

/// `inner`, every call passed on, each await answer kept; where `not_issued`, every await is
/// answered [`AwaitAnswer::NotIssued`] at its bound without asking `inner`.
struct Watched<T> {
    inner: T,
    seen: RefCell<Vec<Seen>>,
    not_issued: bool,
}

impl<T> Watched<T> {
    fn new(inner: T) -> Self {
        Self {
            inner,
            seen: RefCell::default(),
            not_issued: false,
        }
    }

    fn answering_not_issued(inner: T) -> Self {
        Self {
            not_issued: true,
            ..Self::new(inner)
        }
    }

    fn seen(&self) -> Vec<Seen> {
        self.seen.borrow().clone()
    }
}

impl<T: ConformanceTarget> ConformanceTarget for Watched<T> {
    fn fixture_values(
        &self,
        scenario: &ScenarioContext,
        contract: &ess_conformance::fixtures::Contract,
    ) -> Result<BTreeMap<String, Node>, TargetError> {
        self.inner.fixture_values(scenario, contract)
    }

    fn open_periodic(
        &self,
        request: ess_conformance::periodic::Open,
    ) -> Result<ess_conformance::periodic::Opened, TargetError> {
        self.inner.open_periodic(request)
    }

    fn observe_periodic(
        &self,
        request: ess_conformance::periodic::Observe,
    ) -> Result<ess_conformance::periodic::Observation, TargetError> {
        self.inner.observe_periodic(request)
    }

    fn close_periodic(
        &self,
        scope: ess_conformance::periodic::Scope,
    ) -> Result<ess_conformance::periodic::Closed, TargetError> {
        self.inner.close_periodic(scope)
    }

    fn observe_clock_reading(
        &self,
        request: ess_conformance::reading::ReadingObservationRequest,
    ) -> Result<ess_domain::reading::ReadingEvidence, TargetError> {
        self.inner.observe_clock_reading(request)
    }

    fn establish_entity(&self, request: EntitySetupRequest) -> Result<(), TargetError> {
        self.inner.establish_entity(request)
    }

    fn identity(&self) -> Result<ImplementationIdentity, TargetError> {
        self.inner.identity()
    }

    fn begin_scenario(&self, scenario: &ScenarioContext) -> Result<(), TargetError> {
        self.inner.begin_scenario(scenario)
    }

    fn execute_command(
        &self,
        request: SemanticCommandRequest,
    ) -> Result<SemanticCommandResult, TargetError> {
        self.inner.execute_command(request)
    }

    fn execute_command_recorded(
        &self,
        request: SemanticCommandRequest,
    ) -> RecordedCommandCompletion {
        self.inner.execute_command_recorded(request)
    }

    fn execute_command_without_input(
        &self,
        request: AbsentInputRequest,
    ) -> Result<SemanticCommandResult, TargetError> {
        self.inner.execute_command_without_input(request)
    }

    fn query_view(&self, request: SemanticViewRequest) -> Result<SemanticViewResult, TargetError> {
        self.inner.query_view(request)
    }

    fn query_view_as(
        &self,
        request: SemanticViewRequest,
        reader: &ess_conformance::scenario::ActorRef,
    ) -> Result<SemanticViewResult, TargetError> {
        self.inner.query_view_as(request, reader)
    }

    fn query_view_anonymous(
        &self,
        request: SemanticViewRequest,
    ) -> Result<SemanticViewResult, TargetError> {
        self.inner.query_view_anonymous(request)
    }

    fn observe_events(
        &self,
        request: EventObservationRequest,
    ) -> Result<Vec<ObservedEvent>, TargetError> {
        self.inner.observe_events(request)
    }

    fn configure_external_outcome(
        &self,
        request: ExternalOutcomeControl,
    ) -> Result<(), TargetError> {
        self.inner.configure_external_outcome(request)
    }

    fn configure_external_outcome_repeatedly(
        &self,
        request: ExternalOutcomeControl,
        times: std::num::NonZeroU32,
    ) -> Result<(), TargetError> {
        self.inner
            .configure_external_outcome_repeatedly(request, times)
    }

    fn redeliver_event(&self, request: RedeliveryRequest) -> Result<(), TargetError> {
        self.inner.redeliver_event(request)
    }

    fn deliver_event(&self, request: EventDeliveryRequest) -> Result<(), TargetError> {
        self.inner.deliver_event(request)
    }

    fn observe_invocations(
        &self,
        request: InvocationObservationRequest,
    ) -> Result<Vec<ObservedInvocation>, TargetError> {
        self.inner.observe_invocations(request)
    }

    fn mark_instant(&self, request: InstantMark) -> Result<(), TargetError> {
        self.inner.mark_instant(request)
    }

    fn hold_now(&self, request: NowHold) -> Result<(), TargetError> {
        self.inner.hold_now(request)
    }

    fn take_command_clock(&mut self, clock: Box<dyn CommandClock + Send>) -> bool {
        self.inner.take_command_clock(clock)
    }

    fn observe_elapsed(
        &self,
        request: ElapsedObservationRequest,
    ) -> Result<ElapsedObservation, TargetError> {
        self.inner.observe_elapsed(request)
    }

    fn scan_view(&self, request: OrderedScanRequest) -> Result<OrderedScan, TargetError> {
        self.inner.scan_view(request)
    }

    fn scan_view_as(
        &self,
        request: OrderedScanRequest,
        reader: &ess_conformance::scenario::ActorRef,
    ) -> Result<OrderedScan, TargetError> {
        self.inner.scan_view_as(request, reader)
    }

    fn scan_view_anonymous(&self, request: OrderedScanRequest) -> Result<OrderedScan, TargetError> {
        self.inner.scan_view_anonymous(request)
    }

    fn await_outcome(&self, request: AwaitRequest) -> Result<AwaitAnswer, TargetError> {
        let answer = if self.not_issued {
            Ok(AwaitAnswer::NotIssued {
                elapsed_ms: request.within.map(|within| within.millis()),
            })
        } else {
            self.inner.await_outcome(request.clone())
        };
        self.seen.borrow_mut().push(Seen::of(&request, &answer));
        answer
    }

    fn end_scenario(&self, scenario: &ScenarioContext) -> Result<(), TargetError> {
        self.inner.end_scenario(scenario)
    }
}

/// Every synthesised await scenario passes on the host, and each await the runner asked the host
/// is answered as ESS's interpreter answers it on the same steps: the same branch, error, input
/// and events, so the host chose the input in the interpreter's order (D1 A).
#[test]
fn an_awaited_system_step_is_observed() {
    let specification = specification();
    let suite = suite(&specification);
    let awaited = scenarios_with(&suite, awaits);
    for id in [
        "dispatch.desk.Route/outcome/rushed",
        "dispatch.desk.Route/outcome/routed",
        "dispatch.desk.Close/outcome/closed",
    ] {
        assert!(
            awaited.contains(&id.to_owned()),
            "`{id}` awaits: {awaited:?}"
        );
    }

    let host = Watched::new(host(&specification));
    let run = spindle_conformance::execute(&suite, &host);
    for id in &awaited {
        assert_eq!(
            status(&run, id),
            Status::Passed,
            "{id}:\n{}",
            rendered(&run)
        );
    }

    let interpreted = Watched::new(Interpreted::for_model(specification.ir.clone()));
    let reference = spindle_conformance::execute(&suite, &interpreted);
    for id in &awaited {
        assert_eq!(
            status(&reference, id),
            Status::Passed,
            "{id} on ESS's interpreter"
        );
    }

    let (seen, expected) = (host.seen(), interpreted.seen());
    let reached = |seen: &[Seen], command: &str| {
        seen.iter()
            .filter(|seen| matches!(seen, Seen::Reached { command: named, .. } if named == command))
            .count()
    };
    assert!(
        reached(&expected, "dispatch.desk.Route") >= 2
            && reached(&expected, "dispatch.desk.Close") >= 1,
        "the bounded and the unbounded command are each reached: {expected:#?}"
    );
    assert_eq!(
        seen, expected,
        "every await answered as ESS's interpreter answers it"
    );
}

/// A ticket still waiting refuses `Close` by its state, so the synthesised `expect_not_issued`
/// passes: nothing is issued, whether the command declares a bound (`Route` on a routed ticket) or
/// not (`Close` on a waiting one).
#[test]
fn a_refused_system_step_is_not_issued() {
    let specification = specification();
    let suite = suite(&specification);
    let refused = scenarios_with(&suite, not_issued);
    for id in [
        "dispatch.desk.Ticket/state/Waiting/refuses/dispatch.desk.Close",
        "dispatch.desk.Ticket/state/Routed/refuses/dispatch.desk.Route",
    ] {
        assert!(
            refused.contains(&id.to_owned()),
            "`{id}` expects nothing issued: {refused:?}"
        );
    }
    let run = spindle_conformance::execute(&suite, &host(&specification));
    for id in &refused {
        assert_eq!(
            status(&run, id),
            Status::Passed,
            "{id}:\n{}",
            rendered(&run)
        );
    }
}

/// The red side of [`an_awaited_system_step_is_observed`]: a target that answers every await
/// with nothing issued fails each await scenario under `ESS-CF-AWAIT`.
#[test]
fn an_await_answered_not_issued_fails() {
    let specification = specification();
    let suite = suite(&specification);
    let run =
        spindle_conformance::execute(&suite, &Watched::answering_not_issued(host(&specification)));
    let id = "dispatch.desk.Route/outcome/rushed";
    let result = run
        .report()
        .scenarios
        .iter()
        .find(|result| result.scenario.to_string() == id)
        .unwrap_or_else(|| panic!("`{id}` ran"));
    assert_eq!(result.status, Status::Failed, "{}", rendered(&run));
    assert!(
        result
            .checks
            .iter()
            .any(|check| check.status == Status::Failed
                && check.code == CheckCode::Await
                && check.code.to_string() == "ESS-CF-AWAIT"),
        "{}",
        rendered(&run)
    );
}

/// `conform` over the fixture skips nothing: every await scenario executes (D5 A).
#[test]
fn no_await_scenario_of_the_fixture_is_skipped() {
    let specification = specification();
    let suite = suite(&specification);
    let run = spindle_conformance::execute(&suite, &host(&specification));
    let counts = run.counts();
    assert_eq!(counts.skipped, 0, "{}", rendered(&run));
    assert_eq!(counts.executed, counts.total, "{}", rendered(&run));
    assert!(run.passed(), "{}", rendered(&run));
}
