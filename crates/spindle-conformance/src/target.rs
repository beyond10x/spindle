//! [`HostTarget`]: every component of a system in one [`Host`], as ESS's conformance runner
//! drives it.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use ess_compiler::EssIr;
use ess_compiler::ir::{ResolvedFailure, ResolvedRefusalAction, ResolvedTypeRef, ResolvedView};
use ess_compiler::refs::{ErrorRef, EventRef, OutcomeRef};
use ess_conformance::{
    ConformanceTarget, DeclaredErrorValue, EventObservationRequest, ExternalOutcomeControl,
    ImplementationIdentity, InvocationObservationRequest, ObservedEvent, ObservedInvocation,
    RedeliveryRequest, ScenarioContext, SemanticCommandRequest, SemanticCommandResult,
    SemanticViewRequest, SemanticViewResult, TargetError,
};
use ess_domain::command::OutcomeName;
use ess_domain::types::Presence;
use ess_primitives::consistency::ConsistencyToken;
use ess_primitives::ids::CorrelationId;
use ess_primitives::node::Node;
use serde_json::{Map, Value};
use spindle_capability::Environment;
use spindle_host::{
    Answers, Cause, Clock, Escalation, Host, HostError, Placements, RETRY_ATTEMPTS, Refusal,
    SystemClock,
};
use spindle_kernel::{Decider, ExecuteError, External};

/// The outcome a scenario forced for the next invocation of its command: `(command, outcome)`.
type Forcing = Arc<Mutex<Option<(String, String)>>>;

/// Every component of the system in one [`Host`], opened afresh for each scenario.
///
/// The model is admitted on the environment once, when the target is made, and every host it
/// opens shares that model and that admission ([`Host::admitted`]); a refused admission is
/// answered by [`HostTarget::open`]. Each scenario starts from empty stores of its own, bound as
/// the environment binds them; pass it through
/// [`in_memory`](crate::in_memory) first so a run writes nothing to disk. An `external:` cause is
/// answered as the scenario forced it ([`ConformanceTarget::configure_external_outcome`]): the
/// forced outcome's cause holds and every other cause of that command does not, for the next
/// invocation of the command only, the scenario's own send or one a binding makes: the forcing
/// is consumed where its cause is answered, and lapses after any invocation of its command that
/// did not ask it. A cause nothing forced is answered by the decisions file the target was given
/// ([`HostTarget::answering`]); without one, or where it does not answer, the cause does not hold,
/// so the model selects as it would without it. Nothing forced in one scenario reaches the next.
///
/// A command sent as an actor whose `may:` does not name it is refused before it runs, as ESS's
/// interpreter refuses it ([`TargetError::NotGranted`]); one sent as no actor is not checked. A
/// step's actor and `caller` are the caller the command runs as ([`Host::submit_as`]). An entity
/// the scenario establishes is written as it rests into the store of the component that owns it
/// ([`Host::establish`]), checked by ESS first, as ESS's interpreter establishes it. Fixture
/// values and redelivery are unsupported, each with its reason, and so is a send whose
/// `on_failure: retry` binding gave up after [`RETRY_ATTEMPTS`](spindle_host::RETRY_ATTEMPTS), as
/// ESS's interpreter answers a retry that exceeds its execution budget. An escalation event a
/// binding published is observed among the send's events; a send whose escalation was unmet
/// ([`Escalation::Unmet`]) is unsupported, since ESS's interpreter fills a placeholder there. Every
/// attempt an event binding made of its command, with the input it sent, is observed as the
/// binding's invocations ([`ConformanceTarget::observe_invocations`]).
pub struct HostTarget {
    ir: Arc<EssIr>,
    /// The admission of `ir` on the environment, made once and shared by every host opened.
    admission: Result<Arc<Placements>, Refusal>,
    running: RefCell<Option<Running>>,
    forced: Forcing,
    /// What answers a cause the scenario does not force ([`HostTarget::answering`]).
    answers: Option<Answers>,
    /// How many commands this target has sent to a host.
    commands: Cell<usize>,
    /// What each command the runner asked of the scenario in progress, or the last one, answered.
    answered: RefCell<Vec<Answered>>,
    /// Whether the runner asked for state the target does not arrange.
    unarranged: Cell<bool>,
    /// The clock every host it opens decides `now` by.
    clock: Arc<dyn Clock>,
    /// The seed every host it opens mints from ([`Host::seeded`]); `None` for random UUIDs.
    mint: Option<u64>,
}

/// What one command the runner asked of a [`HostTarget`] answered, as a replay prints it beside
/// the act that sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answered {
    /// It took `outcome`, carrying the declared `error` where that outcome declares one.
    Took {
        /// The name of the outcome taken.
        outcome: String,
        /// The qualified name of the declared error it carries.
        error: Option<String>,
    },
    /// It was refused by no declared outcome.
    Undeclared,
    /// Its actor may not send it.
    NotGranted,
    /// It did not run: the target answered an error.
    Failed,
}

impl Answered {
    /// What `sent` answered.
    fn of(sent: &Result<SemanticCommandResult, TargetError>) -> Self {
        match sent {
            Ok(result) => match &result.outcome {
                Some(taken) => Self::Took {
                    outcome: taken.outcome.as_str().to_owned(),
                    error: result.error.as_ref().map(|error| error.error.to_string()),
                },
                None => Self::Undeclared,
            },
            Err(TargetError::NotGranted { .. }) => Self::NotGranted,
            Err(_) => Self::Failed,
        }
    }
}

/// The host of the scenario in progress, and every event its commands published.
struct Running {
    host: Host,
    /// How many commands have run: the consistency token of the last.
    writes: u64,
    events: Vec<ObservedEvent>,
    /// Every attempt an event binding made of its command, in the order made.
    invocations: Vec<Invocation>,
}

/// One attempt an event binding made of its command, as [`Host::submit_as`] reported it, under
/// the correlation of the step that sent the command it followed from.
struct Invocation {
    binding: String,
    command: String,
    input: BTreeMap<String, Node>,
    correlation: CorrelationId,
}

/// The decider of every host the target opens: what the scenario forced, for every cause of the
/// forced command; else what the decisions file answers, where the target has one
/// ([`HostTarget::answering`]); else "does not hold".
struct Forced {
    forcing: Forcing,
    answers: Option<Answers>,
}

impl Decider for Forced {
    fn decide(&mut self, question: &External<'_>) -> Result<bool, String> {
        let mut forced = self.forcing.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((command, outcome)) = forced.as_ref()
            && command == question.command
        {
            let holds = outcome == question.outcome;
            if holds {
                // Consumed where it is answered, whichever invocation asks: the scenario's own
                // send or a binding's, as ESS's interpreter consumes it.
                *forced = None;
            }
            return Ok(holds);
        }
        // A cause neither forced nor answered by the file does not hold, as ESS's runner reads
        // a cause nothing forces.
        Ok(self
            .answers
            .as_ref()
            .is_some_and(|answers| answers.decide(&BTreeMap::new(), question).unwrap_or(false)))
    }
}

impl HostTarget {
    /// A target running `ir` on `environment`, each host on the system clock, minting random
    /// UUIDs.
    pub fn new(ir: EssIr, environment: Environment) -> Self {
        Self::clocked(ir, environment, Arc::new(SystemClock), None)
    }

    /// A target running `ir` on `environment` whose every host decides `now` by `clock` and,
    /// where `mint` is given, mints every identity and generated value from that seed, each host
    /// from its first draw ([`Host::seeded`]). The runner keeps its own clock and identities for
    /// the scenario document.
    pub fn clocked(
        ir: EssIr,
        environment: Environment,
        clock: Arc<dyn Clock>,
        mint: Option<u64>,
    ) -> Self {
        let admission = spindle_host::admit(&ir, &environment).map(Arc::new);
        Self {
            ir: Arc::new(ir),
            admission,
            running: RefCell::new(None),
            forced: Arc::default(),
            answers: None,
            commands: Cell::new(0),
            answered: RefCell::default(),
            unarranged: Cell::new(false),
            clock,
            mint,
        }
    }

    /// This target, answering an `external:` cause the scenario does not force from `answers`, a
    /// decisions file as `run --decisions` reads it: every host it opens asks them where no
    /// forcing of the cause's command is pending, and a cause they do not answer does not hold.
    #[must_use]
    pub fn answering(self, answers: Answers) -> Self {
        Self {
            answers: Some(answers),
            ..self
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

    /// What each command of the scenario in progress, or of the last one, answered, in the order
    /// the runner asked them; the [`SemanticCommandResult`] each returned is unchanged.
    pub fn answered(&self) -> Vec<Answered> {
        self.answered.borrow().clone()
    }

    /// Whether the runner asked for state the target does not arrange.
    pub fn unarranged(&self) -> bool {
        self.unarranged.get()
    }

    fn host(&self) -> Result<Host, HostError> {
        let placements = self.admission.clone().map_err(HostError::Refused)?;
        let host = Host::admitted(
            Arc::clone(&self.ir),
            placements,
            Arc::clone(&self.clock),
            Path::new(""),
            Box::new(Forced {
                forcing: self.forced.clone(),
                answers: self.answers.clone(),
            }),
        )?;
        match self.mint {
            Some(seed) => host.seeded(seed),
            None => Ok(host),
        }
    }

    /// The first field `view` declares, not as `Optional`, that a row of `rows` lacks: one no
    /// command determined, such as a list a later command sets.
    fn undetermined(&self, view: &str, rows: &[Map<String, Value>]) -> Option<String> {
        let declared = self.declared(view)?;
        rows.iter().find_map(|row| {
            declared
                .fields
                .iter()
                .filter(|field| !matches!(field.type_ref, ResolvedTypeRef::Optional { .. }))
                .find(|field| !row.contains_key(&field.name))
                .map(|field| field.name.clone())
        })
    }

    /// The view the model declares by the name `view`.
    fn declared(&self, view: &str) -> Option<&ResolvedView> {
        self.ir
            .views()
            .iter()
            .find_map(|(name, declared)| (name.to_string() == view).then_some(declared))
    }

    /// Answers each `Optional` field `view` declares that a row of `rows` leaves out as ESS's
    /// interpreter answers it (`ess-conformance` `src/interpret/views.rs`, `query`, at ESS
    /// 0.99.2): `null`, unless the field carries `presence: omitted_when_absent`. A field read
    /// through a reference is `null` whatever its presence, as the interpreter reads one whose
    /// reference names no row, or whose row holds nothing or `null` for it. A row of an aggregate
    /// view is a group, not a source row, and is answered as the host answers it.
    ///
    /// The host's own projection ([`Host::listed_with`]) keeps ESS's published contract, which
    /// declares such a field optional and not nullable, and leaves it out; only this target
    /// answers it as the interpreter does. The host's rows leave out a field the row held as
    /// `null`, so it reads as one the row does not hold: answered `null`, as the interpreter keeps
    /// a stored `null`, except under `omitted_when_absent`, where it stays left out and the
    /// interpreter answers the stored `null`.
    fn absent_as_ess(&self, view: &str, rows: &mut [Map<String, Value>]) {
        let Some(declared) = self.declared(view) else {
            return;
        };
        if declared.aggregation.is_some() {
            return;
        }
        let null = declared
            .fields
            .iter()
            .filter(|field| field.type_ref.is_optional())
            .filter(|field| {
                declared.related_read(&field.name).is_some()
                    || field.naming.presence != Some(Presence::OmittedWhenAbsent)
            })
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>();
        for row in rows {
            for field in &null {
                row.entry(*field).or_insert(Value::Null);
            }
        }
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

    /// Sends `request` to the host of the scenario in progress: what
    /// [`ConformanceTarget::execute_command`] answers.
    fn send(&self, request: SemanticCommandRequest) -> Result<SemanticCommandResult, TargetError> {
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
        let not_granted = matches!(
            &submitted,
            Err(HostError::Execute { error, .. })
                if matches!(**error, ExecuteError::NotGranted { .. })
        );
        if !not_granted {
            self.lapse(&command);
        }
        if let Ok(report) = &submitted {
            for invoked in invoked(report) {
                self.lapse(invoked);
            }
            for attempt in &report.invoked {
                let mut input = BTreeMap::new();
                for (field, value) in &attempt.input {
                    input.insert(field.clone(), node(value)?);
                }
                running.invocations.push(Invocation {
                    binding: attempt.binding.clone(),
                    command: attempt.command.clone(),
                    input,
                    correlation: request.correlation.clone(),
                });
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
            if let Some((escalated, why)) =
                report
                    .escalated
                    .iter()
                    .find_map(|escalated| match &escalated.escalation {
                        Escalation::Unmet(why) => Some((escalated, why)),
                        Escalation::Published(_) => None,
                    })
            {
                return Err(TargetError::unsupported(
                    format!("binding `{}`", escalated.binding),
                    format!(
                        "{why}; ESS's interpreter publishes the escalation with a placeholder for \
                         each field the input lacks, and this host publishes nothing"
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
                // The response, as the kernel answered it: each value read as a node, an integer
                // kept exact. A value that cannot be read is named by its field alone, since a
                // one-time value must reach no message.
                if let Some(response) = &submitted.executed.response {
                    let mut answered = BTreeMap::new();
                    for (field, value) in response {
                        let read = serde_json::from_value::<Node>(value.clone()).map_err(|_| {
                            TargetError::unsupported(
                                format!("the response field `{field}` of `{command}`"),
                                "its value is not one ESS reads",
                            )
                        })?;
                        answered.insert(field.clone(), read);
                    }
                    result.response = Some(answered);
                }
                for ran in &report.ran {
                    for event in &ran.executed.events {
                        running.events.push(observed(event, &request)?);
                    }
                }
                for escalated in &report.escalated {
                    if let Escalation::Published(payload) = &escalated.escalation {
                        let event = spindle_kernel::Event {
                            name: escalated.event.clone(),
                            payload: payload.clone(),
                        };
                        running.events.push(observed(&event, &request)?);
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
                ExecuteError::NotGranted { actor } => Err(TargetError::not_granted(Some(actor))),
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
        let established = match self.running.borrow_mut().as_mut() {
            Some(running) => crate::establish::establish(&self.ir, &mut running.host, &request),
            None => Err(TargetError::unavailable(
                "entity setup",
                "no scenario is open",
            )),
        };
        if established.is_err() {
            self.unarranged.set(true);
        }
        established
    }

    fn begin_scenario(&self, _scenario: &ScenarioContext) -> Result<(), TargetError> {
        self.force(None);
        self.answered.borrow_mut().clear();
        let host = self
            .host()
            .map_err(|error| TargetError::unavailable("starting the host", error.to_string()))?;
        *self.running.borrow_mut() = Some(Running {
            host,
            writes: 0,
            events: Vec::new(),
            invocations: Vec::new(),
        });
        Ok(())
    }

    fn execute_command(
        &self,
        request: SemanticCommandRequest,
    ) -> Result<SemanticCommandResult, TargetError> {
        let sent = self.send(request);
        self.answered.borrow_mut().push(Answered::of(&sent));
        sent
    }

    fn query_view(&self, request: SemanticViewRequest) -> Result<SemanticViewResult, TargetError> {
        let view = request.view.to_string();
        let params = request
            .params
            .iter()
            .map(|(name, value)| {
                serde_json::to_value(value)
                    .map(|value| (name.clone(), value))
                    .map_err(|error| {
                        TargetError::unsupported(
                            format!("the parameter `{name}` of `{view}`"),
                            error.to_string(),
                        )
                    })
            })
            .collect::<Result<Map<String, Value>, _>>()?;
        let mut running = self.running.borrow_mut();
        let running = running
            .as_mut()
            .ok_or_else(|| TargetError::unavailable(&view, "no scenario is open"))?;
        let mut listed = match running.host.listed_with(&view, &params) {
            Ok(listed) => listed,
            // ESS's interpreter refuses a read with a parameter the view does not declare, a
            // required one left out or one not of its type as unsupported (`interpret/views.rs`,
            // `query`), and a page or size it refuses (one without the other, a negative one, a
            // page below `first_page`, an offset that overflows) the same way.
            Err(HostError::Parameters { detail, .. }) => {
                return Err(TargetError::unsupported(
                    format!("reading `{view}`"),
                    detail,
                ));
            }
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
        if let Some(field) = self.undetermined(&view, &listed.rows) {
            return Err(TargetError::unsupported(
                format!("reading `{view}`"),
                format!(
                    "the model determines no value of `{field}` for a row it holds, so no \
                     complete row of `{view}` can be answered"
                ),
            ));
        }
        self.absent_as_ess(&view, &mut listed.rows);
        let rows = listed
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|(field, value)| node(value).map(|value| (field.clone(), value)))
                    .collect::<Result<BTreeMap<_, _>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        // A view declaring `paging: {total: true}` answers how many rows its filter keeps beside
        // the page, as ESS's interpreter sets `SemanticViewResult::total`.
        Ok(match listed.total {
            Some(total) => SemanticViewResult::of(rows).with_total(total),
            None => SemanticViewResult::of(rows),
        })
    }

    /// Every attempt `request.binding` made of `request.command` under the request's correlation
    /// since the scenario began, a refused or failed one included, each with the input the binding
    /// sent ([`spindle_host::Report::invoked`]); none read, drained or filtered by success.
    fn observe_invocations(
        &self,
        request: InvocationObservationRequest,
    ) -> Result<Vec<ObservedInvocation>, TargetError> {
        let (binding, command) = (request.binding.to_string(), request.command.to_string());
        Ok(self
            .running
            .borrow()
            .as_ref()
            .map(|running| {
                running
                    .invocations
                    .iter()
                    .filter(|invocation| {
                        invocation.binding == binding
                            && invocation.command == command
                            && invocation.correlation == request.correlation
                    })
                    .map(|invocation| ObservedInvocation {
                        binding: request.binding.clone(),
                        command: request.command.clone(),
                        input: invocation.input.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default())
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

/// The first binding whose invocation in `report` the host gave up while its policy still said
/// retry: refused or not executed by the kernel on its last attempt, under `on_failure: retry` (no
/// `retry:` bound) or under an `on_refusal` policy whose choice for that last attempt (its refusal,
/// or the fallback for a failure with no declared outcome) is an unbounded retry. A binding whose
/// input was unmet (0 attempts) never reached the kernel and is not one.
fn exhausted<'a>(ir: &EssIr, report: &'a spindle_host::Report) -> Option<&'a str> {
    let retries = |binding: &str, refusal: Option<&str>| {
        ir.bindings().values().any(|declared| {
            declared.name.as_str() == binding
                && match declared.on_failure() {
                    ResolvedFailure::Retry => true,
                    ResolvedFailure::ByRefusal { policy } => {
                        let outcome = refusal.and_then(|refusal| OutcomeName::new(refusal).ok());
                        matches!(
                            policy.select(outcome.as_ref()),
                            ResolvedRefusalAction::Retry { bound: None }
                        )
                    }
                    _ => false,
                }
        })
    };
    report
        .refused
        .iter()
        .map(|refused| (&refused.cause, Some(refused.outcome.as_str())))
        .chain(
            report
                .failed
                .iter()
                .filter(|failed| failed.attempts > 0)
                .map(|failed| (&failed.cause, None)),
        )
        .find_map(|(cause, refusal)| match cause {
            Cause::Delivered { binding } if retries(binding, refusal) => Some(binding.as_str()),
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
