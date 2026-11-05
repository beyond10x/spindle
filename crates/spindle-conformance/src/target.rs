//! [`HostTarget`]: every component of a system in one [`Host`], as ESS's conformance runner
//! drives it.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use ess_compiler::EssIr;
use ess_compiler::ir::{ResolvedFailure, ResolvedTypeRef};
use ess_compiler::refs::{ErrorRef, EventRef, OutcomeRef};
use ess_conformance::{
    ConformanceTarget, DeclaredErrorValue, EventObservationRequest, ExternalOutcomeControl,
    ImplementationIdentity, ObservedEvent, RedeliveryRequest, ScenarioContext,
    SemanticCommandRequest, SemanticCommandResult, SemanticViewRequest, SemanticViewResult,
    TargetError,
};
use ess_domain::command::OutcomeName;
use ess_primitives::consistency::ConsistencyToken;
use ess_primitives::node::Node;
use serde_json::{Map, Value};
use spindle_capability::Environment;
use spindle_host::{Cause, Host, HostError, RETRY_ATTEMPTS, SystemClock};
use spindle_kernel::{Decider, ExecuteError, External};

/// The outcome a scenario forced for the next invocation of its command: `(command, outcome)`.
type Forcing = Arc<Mutex<Option<(String, String)>>>;

/// Every component of the system in one [`Host`], opened afresh for each scenario.
///
/// Each scenario starts from empty stores, bound as the environment binds them; pass it through
/// [`in_memory`](crate::in_memory) first so a run writes nothing to disk. An `external:` cause is
/// answered as the scenario forced it ([`ConformanceTarget::configure_external_outcome`]): the
/// forced outcome's cause holds and every other cause of that command does not, for the next
/// invocation of the command only, the scenario's own send or one a binding makes: the forcing
/// is consumed where its cause is answered, and lapses after any invocation of its command that
/// did not ask it. A cause nothing forced does not hold, so the model selects as it would without
/// it. Nothing forced in one scenario reaches the next.
///
/// A command sent as an actor is sent as it is: the host checks no grant, which is the caller's
/// where no component serves the actor; a step's actor and `caller` are the caller the command
/// runs as ([`Host::submit_as`]). An entity the scenario establishes, fixture values and
/// redelivery are unsupported, each with its reason, and so is a send whose `on_failure: retry`
/// binding gave up after [`RETRY_ATTEMPTS`](spindle_host::RETRY_ATTEMPTS), as ESS's interpreter
/// answers a retry that exceeds its execution budget.
pub struct HostTarget {
    ir: EssIr,
    environment: Environment,
    running: RefCell<Option<Running>>,
    forced: Forcing,
    /// How many commands this target has sent to a host.
    commands: Cell<usize>,
    /// Whether the runner asked for state the target does not arrange.
    unarranged: Cell<bool>,
}

/// The host of the scenario in progress, and every event its commands published.
struct Running {
    host: Host,
    /// How many commands have run: the consistency token of the last.
    writes: u64,
    events: Vec<ObservedEvent>,
}

/// The decider of every host the target opens: what the scenario forced, else "does not hold".
struct Forced(Forcing);

impl Decider for Forced {
    fn decide(&mut self, question: &External<'_>) -> Result<bool, String> {
        let mut forced = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let holds = forced.as_ref().is_some_and(|(command, outcome)| {
            command == question.command && outcome == question.outcome
        });
        if holds {
            // Consumed where it is answered, whichever invocation asks: the scenario's own send
            // or a binding's, as ESS's interpreter consumes it.
            *forced = None;
        }
        Ok(holds)
    }
}

impl HostTarget {
    /// A target running `ir` on `environment`.
    pub fn new(ir: EssIr, environment: Environment) -> Self {
        Self {
            ir,
            environment,
            running: RefCell::new(None),
            forced: Arc::default(),
            commands: Cell::new(0),
            unarranged: Cell::new(false),
        }
    }

    /// Opens a host as every scenario does, to say before a run whether one can be opened.
    ///
    /// # Errors
    /// The host's own refusal: admission refused, or a construct it does not run.
    pub fn open(&self) -> Result<(), HostError> {
        self.host().map(|mut host| host.stop())
    }

    /// How many commands this target has sent to a host.
    pub fn commands(&self) -> usize {
        self.commands.get()
    }

    /// Whether the runner asked for state the target does not arrange.
    pub fn unarranged(&self) -> bool {
        self.unarranged.get()
    }

    fn host(&self) -> Result<Host, HostError> {
        Host::deciding(
            self.ir.clone(),
            &self.environment,
            Arc::new(SystemClock),
            Path::new(""),
            Box::new(Forced(self.forced.clone())),
        )
    }

    /// The first field `view` declares, not as `Optional`, that a row of `rows` lacks: one no
    /// command determined, such as a list a later command sets.
    fn undetermined(&self, view: &str, rows: &[Map<String, Value>]) -> Option<String> {
        let declared = self
            .ir
            .views()
            .iter()
            .find_map(|(name, declared)| (name.to_string() == view).then_some(declared))?;
        rows.iter().find_map(|row| {
            declared
                .fields
                .iter()
                .filter(|field| !matches!(field.type_ref, ResolvedTypeRef::Optional { .. }))
                .find(|field| !row.contains_key(&field.name))
                .map(|field| field.name.clone())
        })
    }

    fn force(&self, forcing: Option<(String, String)>) {
        *self.forced.lock().unwrap_or_else(PoisonError::into_inner) = forcing;
    }

    /// Lapses what was forced for `command`, which has now been invoked.
    fn lapse(&self, command: &str) {
        let mut forced = self.forced.lock().unwrap_or_else(PoisonError::into_inner);
        if forced
            .as_ref()
            .is_some_and(|(forced, _)| forced.as_str() == command)
        {
            *forced = None;
        }
    }
}

impl ConformanceTarget for HostTarget {
    fn identity(&self) -> Result<ImplementationIdentity, TargetError> {
        Ok(ImplementationIdentity::new(
            "b10x-spindle",
            env!("CARGO_PKG_VERSION"),
        ))
    }

    fn fixture_values(
        &self,
        _scenario: &ScenarioContext,
        _contract: &ess_conformance::fixtures::Contract,
    ) -> Result<BTreeMap<String, Node>, TargetError> {
        self.unarranged.set(true);
        Err(TargetError::unsupported(
            "fixture values",
            "the in-process host provides no fixture",
        ))
    }

    fn establish_entity(
        &self,
        request: ess_conformance::target::EntitySetupRequest,
    ) -> Result<(), TargetError> {
        self.unarranged.set(true);
        Err(TargetError::unsupported(
            format!("establishing `{}`", request.entity),
            "the in-process host holds only state its commands wrote",
        ))
    }

    fn begin_scenario(&self, _scenario: &ScenarioContext) -> Result<(), TargetError> {
        self.force(None);
        let host = self
            .host()
            .map_err(|error| TargetError::unavailable("starting the host", error.to_string()))?;
        *self.running.borrow_mut() = Some(Running {
            host,
            writes: 0,
            events: Vec::new(),
        });
        Ok(())
    }

    fn execute_command(
        &self,
        request: SemanticCommandRequest,
    ) -> Result<SemanticCommandResult, TargetError> {
        let command = request.command.to_string();
        let json = |fields: &BTreeMap<String, Node>| {
            fields
                .iter()
                .map(|(field, value)| {
                    serde_json::to_value(value)
                        .map(|value| (field.clone(), value))
                        .map_err(|error| {
                            TargetError::unavailable(
                                format!("sending `{field}`"),
                                error.to_string(),
                            )
                        })
                })
                .collect::<Result<Map<String, Value>, _>>()
        };
        let input = json(&request.input)?;
        let caller = request.caller.as_ref().map(json).transpose()?;
        let actor = request.actor.as_ref().map(ToString::to_string);
        let mut running = self.running.borrow_mut();
        let running = running
            .as_mut()
            .ok_or_else(|| TargetError::unavailable(&command, "no scenario is open"))?;
        self.commands.set(self.commands.get() + 1);
        running.writes += 1;
        let written = written(running.writes)?;
        let submitted = running
            .host
            .submit_as(&command, input, actor.as_deref(), caller.as_ref());
        self.lapse(&command);
        if let Ok(report) = &submitted {
            for invoked in invoked(report) {
                self.lapse(invoked);
            }
            if let Some(binding) = exhausted(&self.ir, report) {
                return Err(TargetError::unsupported(
                    format!("binding `{binding}`"),
                    format!(
                        "its retry gave up after {RETRY_ATTEMPTS} attempts; ESS's interpreter \
                         answers an unbounded retry that never takes effect as unsupported: it \
                         exceeds the finite execution budget"
                    ),
                ));
            }
        }
        match submitted {
            Ok(report) => {
                let submitted = report
                    .ran
                    .first()
                    .ok_or_else(|| TargetError::unavailable(&command, "the host ran nothing"))?;
                let mut result = SemanticCommandResult::took(outcome(
                    &request.command,
                    &submitted.executed.outcome,
                )?)
                .with_consistency(written);
                for event in &submitted.executed.events {
                    result = result.emitting(observed(event, &request)?);
                }
                for ran in &report.ran {
                    for event in &ran.executed.events {
                        running.events.push(observed(event, &request)?);
                    }
                }
                Ok(result)
            }
            Err(HostError::CommandRefused {
                outcome: taken,
                error,
                ..
            }) => {
                let reference = error.name.parse::<ErrorRef>().map_err(|parse| {
                    TargetError::unavailable(&command, format!("error `{}`: {parse}", error.name))
                })?;
                let mut declared = DeclaredErrorValue::new(reference);
                for (field, value) in &error.fields {
                    declared = declared.with(field.clone(), node(value)?);
                }
                Ok(
                    SemanticCommandResult::took(outcome(&request.command, &taken)?)
                        .with_error(declared)
                        .with_consistency(written),
                )
            }
            Err(HostError::Execute { error, .. }) => match *error {
                ExecuteError::Undeclared { .. } => Ok(SemanticCommandResult::undeclared()),
                ExecuteError::Unsupported { construct } => Err(TargetError::unsupported(
                    format!("running `{command}`"),
                    construct,
                )),
                other => Err(TargetError::unavailable(
                    format!("running `{command}`"),
                    other.to_string(),
                )),
            },
            Err(HostError::Unsupported(construct)) => Err(TargetError::unsupported(
                format!("running `{command}`"),
                construct,
            )),
            Err(other) => Err(TargetError::unavailable(
                format!("running `{command}`"),
                other.to_string(),
            )),
        }
    }

    fn query_view(&self, request: SemanticViewRequest) -> Result<SemanticViewResult, TargetError> {
        let view = request.view.to_string();
        if !request.params.is_empty() {
            return Err(TargetError::unsupported(
                format!("`{view}` read with parameters"),
                "the in-process host lists views without parameters only",
            ));
        }
        let mut running = self.running.borrow_mut();
        let running = running
            .as_mut()
            .ok_or_else(|| TargetError::unavailable(&view, "no scenario is open"))?;
        let rows = match running.host.list(&view) {
            Ok(rows) => rows,
            Err(HostError::Unsupported(construct)) => {
                return Err(TargetError::unsupported(
                    format!("reading `{view}`"),
                    construct,
                ));
            }
            Err(other) => {
                return Err(TargetError::unavailable(
                    format!("reading `{view}`"),
                    other.to_string(),
                ));
            }
        };
        if let Some(field) = self.undetermined(&view, &rows) {
            return Err(TargetError::unsupported(
                format!("reading `{view}`"),
                format!(
                    "the model determines no value of `{field}` for a row it holds, so no \
                     complete row of `{view}` can be answered"
                ),
            ));
        }
        rows.iter()
            .map(|row| {
                row.iter()
                    .map(|(field, value)| node(value).map(|value| (field.clone(), value)))
                    .collect::<Result<BTreeMap<_, _>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()
            .map(SemanticViewResult::of)
    }

    fn observe_events(
        &self,
        request: EventObservationRequest,
    ) -> Result<Vec<ObservedEvent>, TargetError> {
        Ok(self
            .running
            .borrow()
            .as_ref()
            .map(|running| {
                running
                    .events
                    .iter()
                    .filter(|event| {
                        event.event == request.event
                            && event.correlation.as_ref() == Some(&request.correlation)
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default())
    }

    fn configure_external_outcome(
        &self,
        request: ExternalOutcomeControl,
    ) -> Result<(), TargetError> {
        self.force(Some((
            request.force.command.to_string(),
            request.force.outcome.as_str().to_owned(),
        )));
        Ok(())
    }

    fn redeliver_event(&self, request: RedeliveryRequest) -> Result<(), TargetError> {
        Err(TargetError::unsupported(
            format!("redelivering `{}`", request.event),
            "the in-process host delivers each event once",
        ))
    }

    fn end_scenario(&self, _scenario: &ScenarioContext) -> Result<(), TargetError> {
        if let Some(mut running) = self.running.borrow_mut().take() {
            running.host.stop();
        }
        Ok(())
    }
}

/// Every command one submission invoked: the one sent and each a binding invoked, run, refused or
/// failed. Each is an invocation a forced outcome of its command lapses at, asked or not.
fn invoked(report: &spindle_host::Report) -> impl Iterator<Item = &str> {
    report
        .ran
        .iter()
        .map(|ran| ran.command.as_str())
        .chain(
            report
                .refused
                .iter()
                .map(|refused| refused.command.as_str()),
        )
        .chain(report.failed.iter().map(|failed| failed.command.as_str()))
}

/// The first `on_failure: retry` binding (no `retry:` bound) whose invocation in `report` the host
/// gave up: refused or not executed by the kernel on its last attempt. A binding whose input was
/// unmet (0 attempts) never reached the kernel and is not one.
fn exhausted<'a>(ir: &EssIr, report: &'a spindle_host::Report) -> Option<&'a str> {
    let retries = |binding: &str| {
        ir.bindings().values().any(|declared| {
            declared.name.as_str() == binding
                && matches!(declared.on_failure(), ResolvedFailure::Retry)
        })
    };
    report
        .refused
        .iter()
        .map(|refused| &refused.cause)
        .chain(
            report
                .failed
                .iter()
                .filter(|failed| failed.attempts > 0)
                .map(|failed| &failed.cause),
        )
        .find_map(|cause| match cause {
            Cause::Delivered { binding } if retries(binding) => Some(binding.as_str()),
            _ => None,
        })
}

/// The token of the `count`th command run.
///
/// Every command's change and everything its events lead to is committed before
/// [`Host::submit`] returns, and every view is read from those same stores, so any read made after
/// a command is at least as new as it: the token only names the write.
fn written(count: u64) -> Result<ConsistencyToken, TargetError> {
    ConsistencyToken::new(format!("write-{count}"))
        .map_err(|error| TargetError::unavailable("naming a write", error.to_string()))
}

/// The outcome `taken` of `command`, as the suite names it.
fn outcome(
    command: &ess_compiler::refs::CommandRef,
    taken: &str,
) -> Result<OutcomeRef, TargetError> {
    OutcomeName::new(taken)
        .map(|name| OutcomeRef::new(command.clone(), name))
        .map_err(|error| {
            TargetError::unavailable(format!("`{command}` took `{taken}`"), error.to_string())
        })
}

/// One event a command published, in the scenario the request belongs to.
fn observed(
    event: &spindle_kernel::Event,
    request: &SemanticCommandRequest,
) -> Result<ObservedEvent, TargetError> {
    let reference = event.name.parse::<EventRef>().map_err(|error| {
        TargetError::unavailable(format!("publishing `{}`", event.name), error.to_string())
    })?;
    let mut observed = ObservedEvent::new(reference).in_activity(request.correlation.clone());
    for (field, value) in &event.payload {
        observed = observed.with(field.clone(), node(value)?);
    }
    Ok(observed)
}

/// `value` as ESS's value, read exactly: an integer as the integer.
fn node(value: &Value) -> Result<Node, TargetError> {
    serde_json::from_value::<Node>(value.clone())
        .map_err(|error| TargetError::unsupported(format!("the value {value}"), error.to_string()))
}
