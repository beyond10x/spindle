//! The host: every component of one system in one process.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ess_compiler::EssIr;
use ess_compiler::ir::{ResolvedMappingValue, ResolvedView};
use ess_domain::binding::Failure;
use serde_json::{Map, Value};
use spindle_capability::{Capability, Environment};
use spindle_channel::{ChannelError, Receiver, RouteError, Router, Sender, in_process};
use spindle_kernel::{
    Decider, DeclaredError, Event, ExecuteError, Executed, MemoryStore, NoExternals, SqliteStore,
    Store, StoreError, UuidV4,
};
use spindle_telemetry::{Counters, CountingStore, Sink};
use spindle_timer::{BindingName, Clock, CompleteError, LifetimeError, Timer};

use crate::admission::{Placements, Refusal, admit};

mod component;

/// The channel implementation this host carries bindings on.
const IN_PROCESS: &str = "in-process";

/// Why a command ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cause {
    /// [`Host::submit`] was asked to run it.
    Submitted,
    /// A periodic binding fell due.
    Tick {
        /// The binding.
        binding: String,
    },
    /// An event binding carried it from another component.
    Delivered {
        /// The binding.
        binding: String,
    },
}

/// One command a component ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ran {
    /// The component that ran it.
    pub component: String,
    /// The command.
    pub command: String,
    /// Why it ran.
    pub cause: Cause,
    /// What it did.
    pub executed: Executed,
}

/// One command a tick or a binding invoked that did not execute. Nothing was written, and the
/// host does not run it again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failed {
    /// The component that was to run it.
    pub component: String,
    /// The command.
    pub command: String,
    /// Why it was invoked.
    pub cause: Cause,
    /// The kernel's error, as text.
    pub error: String,
}

/// One command a tick or a binding invoked that the model refused: its outcome declares an error.
/// Nothing was written, and the host does not run it again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefusedCommand {
    /// The component that ran it.
    pub component: String,
    /// The command.
    pub command: String,
    /// Why it was invoked.
    pub cause: Cause,
    /// The refusing outcome.
    pub outcome: String,
    /// The error that outcome declares.
    pub error: DeclaredError,
}

/// One event a command published: committed with the command's effect, in the order published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    /// The component that ran the command.
    pub component: String,
    /// The command.
    pub command: String,
    /// The event and its payload.
    pub event: Event,
}

/// Every command one call ran, in the order run: those that took effect, those the model refused,
/// and those that did not execute.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// The commands that executed and were not refused.
    pub ran: Vec<Ran>,
    /// The commands that did not execute.
    pub failed: Vec<Failed>,
    /// The commands whose outcome declares an error.
    pub refused: Vec<RefusedCommand>,
}

/// How many envelopes one [`Host::submit`] or [`Host::advance`] delivers before it stops as a
/// [`HostError::Runaway`]: bindings that trigger each other have no end of their own.
pub const DELIVERY_LIMIT: usize = 10_000;

/// The timer implementations this host runs: `spindle-timer`'s schedule, which the shipped
/// environments name `tokio`.
const TIMERS: &[&str] = &["tokio"];
/// The clock implementations this host runs. A test injects its own clock through
/// [`Host::new`].
const CLOCKS: &[&str] = &["system"];

/// Why the host did not start or could not go on.
#[derive(Debug, thiserror::Error)]
pub enum HostError {
    /// The environment cannot run every component.
    #[error("{0}")]
    Refused(Refusal),
    /// The specification, or a binding of the environment, uses something this host does not
    /// run.
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// The event bindings cannot be routed.
    #[error(transparent)]
    Route(#[from] RouteError),
    /// A channel failed.
    #[error(transparent)]
    Channel(#[from] ChannelError),
    /// A store could not be opened or read.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// No component accepts the command.
    #[error("no component accepts {0}")]
    NotAccepted(String),
    /// A command [`Host::submit`] was asked to run did not execute.
    #[error("{component} did not run {command}: {error}")]
    Execute {
        /// The component.
        component: String,
        /// The command.
        command: String,
        /// The kernel's error.
        error: Box<ExecuteError>,
    },
    /// No periodic binding has this name.
    #[error("no periodic binding {0}")]
    NotPeriodic(String),
    /// The host context lacks a field the periodic binding declares.
    #[error("binding {binding} needs the host context field {field}")]
    Context {
        /// The binding.
        binding: String,
        /// The field.
        field: String,
    },
    /// A periodic binding's lifetime could not be started or stopped.
    #[error(transparent)]
    Lifetime(#[from] LifetimeError),
    /// A tick could not be completed.
    #[error(transparent)]
    Complete(#[from] CompleteError),
    /// No view has this name.
    #[error("no view {0}")]
    NoView(String),
    /// The model refused a command [`Host::submit`] was asked to run: its outcome declares an
    /// error. Nothing was written.
    #[error("{component} refused {command} with {} {}", error.name, Value::Object(error.fields.clone()))]
    CommandRefused {
        /// The component.
        component: String,
        /// The command.
        command: String,
        /// The refusing outcome.
        outcome: String,
        /// The declared error.
        error: DeclaredError,
    },
    /// [`Host::reset`] could not empty a component's store.
    #[error(
        "resetting {component}: {error}; {}",
        if *restored {
            "its store is as it was before the reset"
        } else {
            "its store could not be reopened, and every read and write of it now fails"
        }
    )]
    Reset {
        /// The component.
        component: String,
        /// Why its store could not be emptied.
        error: String,
        /// Whether its store was reopened as it was before the reset.
        restored: bool,
    },
    /// One call delivered [`DELIVERY_LIMIT`] envelopes and more were still in flight; those
    /// were discarded, and nothing runs after them.
    #[error(
        "stopped after {deliveries} deliveries in one call; these bindings kept triggering each other: {}",
        bindings.iter().cloned().collect::<Vec<_>>().join(", ")
    )]
    Runaway {
        /// How many envelopes were delivered.
        deliveries: usize,
        /// The bindings that carried them.
        bindings: BTreeSet<String>,
    },
    /// The link to a peer component closed or failed before the run ended, or what was sent
    /// there did not settle in time.
    #[error("the link to {peer} {reason}")]
    Peer {
        /// The peer component, or several, comma-separated.
        peer: String,
        /// What happened to it.
        reason: String,
    },
    /// The caller asked the host to stop while it waited for its peers.
    #[error("stopped while waiting for peers")]
    Stopped,
}

/// What a host does with an event binding whose `on_failure` is `retry`: this host retries no
/// invocation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Retries {
    /// Refuse to start, as for any binding the host does not carry.
    #[default]
    Refused,
    /// Run the binding as a `drop` binding runs: a failed invocation is reported once and never
    /// run again, and an envelope whose send fails is dropped.
    Dropped,
}

/// The first event binding of `ir`, by name, whose `on_failure` a host started with `retries`
/// does not carry, and what it says: the binding [`Host::with_retries`] refuses before it opens
/// anything. `escalate` is never carried, and `retry` only under [`Retries::Dropped`].
pub fn uncarried(ir: &EssIr, retries: Retries) -> Option<(String, Failure)> {
    ir.bindings()
        .values()
        .filter(|binding| binding.cause.event().is_some())
        .find(|binding| match binding.failure {
            Failure::Drop => false,
            Failure::Retry => retries == Retries::Refused,
            Failure::Escalate => true,
        })
        .map(|binding| (binding.name.as_str().to_owned(), binding.failure))
}

/// The event bindings of `ir` whose `on_failure` is `retry`, by name.
pub fn retried(ir: &EssIr) -> Vec<String> {
    ir.bindings()
        .values()
        .filter(|binding| binding.cause.event().is_some() && binding.failure == Failure::Retry)
        .map(|binding| binding.name.as_str().to_owned())
        .collect()
}

struct Component {
    store: Box<dyn Store>,
    /// The SQLite file the store keeps its state in; `None` for a store in memory.
    file: Option<PathBuf>,
    sender: Sender,
    position: u64,
    counters: Arc<Counters>,
    sink: Sink,
}

struct Periodic {
    name: BindingName,
    command: String,
    component: String,
    context_fields: Vec<String>,
    mapping: Vec<(String, String)>,
    context: Option<Map<String, Value>>,
}

/// Every component of one system, run in one process.
///
/// Each component has its own kernel store, bound to the implementation admission chose for it.
/// Periodic bindings are scheduled by one [`Timer`] on the injected [`Clock`]; event bindings are
/// carried by in-process channels, one per pair of components, and each envelope that arrives is
/// executed as a command on the component it is addressed to. Everything runs on the calling
/// thread: a call returns once no envelope is left in flight, or stops after [`DELIVERY_LIMIT`]
/// deliveries.
pub struct Host {
    ir: EssIr,
    clock: Arc<dyn Clock>,
    placements: Placements,
    components: BTreeMap<String, Component>,
    receivers: Vec<Receiver>,
    timer: Timer,
    periodic: BTreeMap<String, Periodic>,
    remote: Option<component::Remote>,
    published: Vec<Published>,
    /// Answers the `external:` causes of every command any component runs.
    decider: Box<dyn Decider + Send>,
}

impl std::fmt::Debug for Host {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Host")
            .field("placements", &self.placements)
            .field("components", &self.components.keys().collect::<Vec<_>>())
            .field("timer", &self.timer)
            .finish_non_exhaustive()
    }
}

impl Host {
    /// Admits every component of `ir` against `environment` and opens its store.
    ///
    /// A component whose state is bound to `sqlite` keeps it in `<state>/<component>.sqlite`; one
    /// bound to `memory` keeps it in memory. No periodic binding is started yet.
    ///
    /// # Errors
    /// [`HostError::Refused`] when a component is not admitted, naming everything every component
    /// lacks; [`HostError::Unsupported`] when admission bound something this host does not run,
    /// or the specification uses a binding it does not carry, such as an event binding whose
    /// `on_failure` is not `drop` ([`uncarried`]).
    pub fn new(
        ir: EssIr,
        environment: &Environment,
        clock: Arc<dyn Clock>,
        state: &Path,
    ) -> Result<Self, HostError> {
        Self::open(ir, environment, clock, state, None, Retries::Refused)
    }

    /// [`Host::new`], with `retries` saying what becomes of an event binding whose `on_failure`
    /// is `retry` ([`retried`]): refused, as [`Host::new`] refuses it, or run as a `drop` binding
    /// runs, each failed invocation reported once and never run again. `on_failure: escalate`
    /// is refused either way.
    ///
    /// # Errors
    /// As [`Host::new`].
    pub fn with_retries(
        ir: EssIr,
        environment: &Environment,
        clock: Arc<dyn Clock>,
        state: &Path,
        retries: Retries,
    ) -> Result<Self, HostError> {
        Self::open(ir, environment, clock, state, None, retries)
    }

    /// [`Host::new`], with every `external:` cause of every component answered by `decider`, whatever
    /// implementation admission bound for it.
    ///
    /// # Errors
    /// As [`Host::new`], except that no external cause is refused.
    pub fn deciding(
        ir: EssIr,
        environment: &Environment,
        clock: Arc<dyn Clock>,
        state: &Path,
        decider: Box<dyn Decider + Send>,
    ) -> Result<Self, HostError> {
        Self::open(
            ir,
            environment,
            clock,
            state,
            Some(decider),
            Retries::Refused,
        )
    }

    fn open(
        ir: EssIr,
        environment: &Environment,
        clock: Arc<dyn Clock>,
        state: &Path,
        decider: Option<Box<dyn Decider + Send>>,
        retries: Retries,
    ) -> Result<Self, HostError> {
        let placements = admit(&ir, environment).map_err(HostError::Refused)?;
        Self::assemble(ir, clock, state, placements, IN_PROCESS, decider, retries)
    }

    /// The host of the components in `placements`, their bindings carried on `carried`: every
    /// pair joined in this process when it is [`IN_PROCESS`], none otherwise.
    fn assemble(
        ir: EssIr,
        clock: Arc<dyn Clock>,
        state: &Path,
        placements: Placements,
        carried: &str,
        decider: Option<Box<dyn Decider + Send>>,
        retries: Retries,
    ) -> Result<Self, HostError> {
        let answers = decider.is_some();
        if let Some((binding, failure)) = uncarried(&ir, retries) {
            return Err(HostError::Unsupported(format!(
                "binding {binding} says on_failure: {failure:?}; this host drops an invocation \
                 that fails and neither retries nor escalates it"
            )));
        }
        let router = match retries {
            Retries::Refused => Router::from_ir(&ir)?,
            Retries::Dropped => Router::from_ir(&ir)?.dropping_retries(),
        };
        let mut components = BTreeMap::new();
        for (name, bindings) in &placements {
            for binding in bindings {
                let capability = binding.capability.as_str();
                let implementation = binding.implementation.as_str();
                let runs = |known: &[&str]| known.contains(&implementation);
                let unsupported = if capability.starts_with(Capability::EXTERNAL_PREFIX) && !answers
                {
                    Some("this host answers no external cause")
                } else if capability == Capability::TIMER && !runs(TIMERS) {
                    Some("this host runs no such timer")
                } else if capability == Capability::CLOCK && !runs(CLOCKS) {
                    Some("this host runs no such clock")
                } else if capability == Capability::IDENTITY_MINT
                    && !runs(&[spindle_kernel::UuidV4::IMPLEMENTATION])
                {
                    Some("this host mints identities as uuid-v4 only")
                } else {
                    None
                };
                if let Some(why) = unsupported {
                    return Err(HostError::Unsupported(format!(
                        "{name} has {capability} bound to {implementation}; {why}"
                    )));
                }
            }
            let bound = |capability: &str| {
                bindings
                    .iter()
                    .find(|binding| binding.capability.as_str() == capability)
                    .map(|binding| binding.implementation.as_str())
            };
            if let Some(channel) = bound(Capability::CHANNEL).filter(|bound| *bound != carried) {
                return Err(HostError::Unsupported(format!(
                    "{name} has its channel bound to {channel}; this host carries bindings {carried} only"
                )));
            }
            let counters = Arc::new(Counters::new(
                spindle_telemetry::catalogue(&ir, name)
                    .ok_or_else(|| HostError::Unsupported(format!("no component {name}")))?,
            ));
            let sink = Sink::new(counters.clone());
            let file = match bound(Capability::STORE_DURABLE).or_else(|| bound("store.ephemeral")) {
                None | Some("memory") => None,
                Some("sqlite") => Some(state.join(format!("{name}.sqlite"))),
                Some(other) => {
                    return Err(HostError::Unsupported(format!(
                        "{name} has its state bound to {other}, which this host has no store for"
                    )));
                }
            };
            let store = opened(file.as_deref(), &sink)?;
            let sender = Sender::new(name, router.clone())?.with_telemetry(sink.clone());
            components.insert(
                name.clone(),
                Component {
                    store,
                    file,
                    sender,
                    position: 0,
                    counters,
                    sink,
                },
            );
        }

        let mut receivers = Vec::new();
        let links = router
            .routes()
            .iter()
            .map(|route| (route.from.clone(), route.to.clone()))
            .collect::<BTreeSet<_>>();
        for (from, to) in links.into_iter().filter(|_| carried == IN_PROCESS) {
            let (near, far) = in_process::pair();
            if let Some(component) = components.get_mut(&from) {
                component.sender.connect(&to, near);
            }
            let sink = components
                .get(&to)
                .map(|component| component.sink.clone())
                .unwrap_or_default();
            receivers.push(Receiver::new(&to, far).with_telemetry(sink));
        }

        let mut periodic = BTreeMap::new();
        for binding in ir.bindings().values() {
            let Some(cause) = binding.cause.periodic() else {
                continue;
            };
            let name = binding.name.as_str().to_owned();
            let command = binding.command.to_string();
            let component =
                accepting(&ir, &command).ok_or_else(|| HostError::NotAccepted(command.clone()))?;
            if !components.contains_key(&component) {
                continue;
            }
            if !cause.contract.host.read_fields.is_empty() {
                return Err(HostError::Unsupported(format!(
                    "binding {name} reads fresh host fields on each occurrence"
                )));
            }
            let mut mapping = Vec::new();
            for rule in &binding.mapping {
                match &rule.value {
                    ResolvedMappingValue::HostContext { field, .. } => {
                        mapping.push((rule.target.clone(), field.clone()));
                    }
                    _ => {
                        return Err(HostError::Unsupported(format!(
                            "binding {name} fills {} from something other than its host context",
                            rule.target
                        )));
                    }
                }
            }
            periodic.insert(
                name,
                Periodic {
                    name: binding.name.clone(),
                    command,
                    component,
                    context_fields: cause
                        .contract
                        .host
                        .context_fields
                        .iter()
                        .map(|field| field.name.clone())
                        .collect(),
                    mapping,
                    context: None,
                },
            );
        }

        let mut timer = Timer::new(&ir);
        for periodic in periodic.values() {
            if let Some(component) = components.get(&periodic.component) {
                timer.bind_telemetry(&periodic.name, component.sink.clone())?;
            }
        }
        Ok(Self {
            ir,
            clock,
            placements,
            components,
            receivers,
            timer,
            periodic,
            remote: None,
            published: Vec::new(),
            decider: decider.unwrap_or_else(|| Box::new(NoExternals)),
        })
    }

    /// The compiled specification the host runs.
    pub fn ir(&self) -> &EssIr {
        &self.ir
    }

    /// Every event a command published since the host started or was last [reset](Self::reset),
    /// in the order published: a command's own events in emit order, then those of the commands
    /// they led to. A refused command publishes none. Held in memory: the events of an earlier
    /// process that kept its state in SQLite are not among them.
    pub fn published(&self) -> &[Published] {
        &self.published
    }

    /// Empties every component's store and forgets every published event; returns the
    /// components whose stores were emptied, by name.
    ///
    /// A store in memory is replaced by an empty one. A store in SQLite is closed, its file and
    /// the SQLite side files beside it moved aside, and an empty one opened at the same path; the
    /// moved files are removed only once the empty store is open. Periodic bindings keep running,
    /// and the counters keep their counts.
    ///
    /// # Errors
    /// [`HostError::Reset`] when a component's files cannot be moved aside or the empty store
    /// cannot be opened. Its files are moved back and its store reopened as it was
    /// (`restored: true`); where even that fails, every read and write of that component fails
    /// from then on, naming why (`restored: false`). The components emptied before it stay
    /// empty, and the published events are forgotten only when every store was emptied.
    pub fn reset(&mut self) -> Result<Vec<String>, HostError> {
        let mut emptied = Vec::new();
        for (name, component) in &mut self.components {
            if let Some(file) = component.file.clone() {
                let closed = std::mem::replace(
                    &mut component.store,
                    Box::new(Lost(format!("{name}'s store is being reset"))),
                );
                drop(closed);
                match emptied_file(&file, &component.sink) {
                    Ok(store) => component.store = store,
                    Err(error) => {
                        let restored = match opened(Some(&file), &component.sink) {
                            Ok(store) => {
                                component.store = store;
                                true
                            }
                            Err(again) => {
                                component.store = Box::new(Lost(format!(
                                    "{name}'s store was lost in a reset: {error}; reopening {} \
                                     failed: {again}",
                                    file.display()
                                )));
                                false
                            }
                        };
                        return Err(HostError::Reset {
                            component: name.clone(),
                            error,
                            restored,
                        });
                    }
                }
            } else {
                component.store = opened(None, &component.sink)?;
            }
            emptied.push(name.clone());
        }
        self.published.clear();
        Ok(emptied)
    }

    /// What admission bound for each component.
    pub fn placements(&self) -> &Placements {
        &self.placements
    }

    /// The counters of `component`, laid out by its catalogue; `None` when no such component
    /// runs here.
    pub fn counters(&self, component: &str) -> Option<&Counters> {
        self.components
            .get(component)
            .map(|component| component.counters.as_ref())
    }

    /// The names of the specification's periodic bindings.
    pub fn periodic(&self) -> Vec<String> {
        self.periodic.keys().cloned().collect()
    }

    /// Runs `command` on the component that accepts it, then everything its events lead to.
    ///
    /// # Errors
    /// [`HostError::Execute`] when the command itself does not execute,
    /// [`HostError::CommandRefused`] when the model refuses it, and [`HostError::Runaway`] when
    /// what it leads to does not settle within [`DELIVERY_LIMIT`] deliveries; a failure or a
    /// refusal further down is in the report.
    pub fn submit(
        &mut self,
        command: &str,
        input: Map<String, Value>,
    ) -> Result<Report, HostError> {
        self.submit_as(command, input, None, None)
    }

    /// [`Host::submit`], sent as `actor` and as the caller whose attributes `caller` holds
    /// ([`spindle_kernel::Invocation`]). What its events lead to is sent as no caller.
    ///
    /// # Errors
    /// As [`Host::submit`].
    pub fn submit_as(
        &mut self,
        command: &str,
        input: Map<String, Value>,
        actor: Option<&str>,
        caller: Option<&Map<String, Value>>,
    ) -> Result<Report, HostError> {
        let component = accepting(&self.ir, command)
            .ok_or_else(|| HostError::NotAccepted(command.to_owned()))?;
        let mut report = Report::default();
        match self.execute(&component, command, &input, actor, caller)? {
            Ok(Executed {
                outcome,
                error: Some(error),
                ..
            }) => {
                return Err(HostError::CommandRefused {
                    component,
                    command: command.to_owned(),
                    outcome,
                    error,
                });
            }
            Ok(executed) => report.ran.push(Ran {
                component,
                command: command.to_owned(),
                cause: Cause::Submitted,
                executed,
            }),
            Err(error) => {
                return Err(HostError::Execute {
                    component,
                    command: command.to_owned(),
                    error: Box::new(error),
                });
            }
        }
        self.settle(&mut report, &mut Deliveries::default())?;
        Ok(report)
    }

    /// Starts the lifetime of the periodic binding `binding` at the clock's current instant,
    /// with `context` as its host context: the lifetime-constant fields its mapping reads.
    ///
    /// # Errors
    /// [`HostError::NotPeriodic`], [`HostError::Context`] when `context` lacks a declared field,
    /// and [`HostError::Lifetime`] when the binding is already running.
    pub fn start(&mut self, binding: &str, context: Map<String, Value>) -> Result<(), HostError> {
        let periodic = self
            .periodic
            .get_mut(binding)
            .ok_or_else(|| HostError::NotPeriodic(binding.to_owned()))?;
        if let Some(field) = periodic
            .context_fields
            .iter()
            .find(|field| context.get(*field).is_none_or(Value::is_null))
        {
            return Err(HostError::Context {
                binding: binding.to_owned(),
                field: field.clone(),
            });
        }
        self.timer
            .activate_binding(&periodic.name, self.clock.now())?;
        periodic.context = Some(context);
        Ok(())
    }

    /// Stops every running periodic binding.
    pub fn stop(&mut self) {
        self.timer.stop();
    }

    /// The earliest instant at which [`Self::advance`] can run a tick; `None` when no periodic
    /// binding is running.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.timer.next_deadline()
    }

    /// Runs every tick due at the clock's current instant, each followed by everything its events
    /// lead to.
    ///
    /// The host context's eligibility (`host_boolean`) holds for every running binding: a binding
    /// whose lifetime was started is eligible until it is stopped.
    ///
    /// # Errors
    /// [`HostError`] when a channel or the timer fails, and [`HostError::Runaway`] when the ticks'
    /// consequences do not settle within [`DELIVERY_LIMIT`] deliveries; the open tick is
    /// completed all the same. A command that does not execute or is refused is in the report.
    pub fn advance(&mut self) -> Result<Report, HostError> {
        let mut report = Report::default();
        let mut deliveries = Deliveries::default();
        for tick in self.timer.due(self.clock.now()) {
            let binding = tick.binding.as_str().to_owned();
            let periodic = self
                .periodic
                .get(&binding)
                .ok_or_else(|| HostError::NotPeriodic(binding.clone()))?;
            let context = periodic.context.clone().unwrap_or_default();
            let input = periodic
                .mapping
                .iter()
                .filter_map(|(target, field)| {
                    context
                        .get(field)
                        .map(|value| (target.clone(), value.clone()))
                })
                .collect::<Map<_, _>>();
            let component = periodic.component.clone();
            let command = periodic.command.clone();
            let settled = self
                .run(
                    &component,
                    &command,
                    &input,
                    Cause::Tick { binding },
                    &mut report,
                )
                .and_then(|()| self.settle(&mut report, &mut deliveries));
            self.timer.complete(&tick)?;
            settled?;
        }
        Ok(report)
    }

    /// The row `view` answers for the instance `identity` names; `None` when no instance has it,
    /// or when the view's filter does not hold for it.
    ///
    /// The row is read from the store of the component that owns the view's source. A view's
    /// `consistency` changes nothing here: every component's store is in this process, so an
    /// `eventual` view reads the same store a `read_your_writes` one does.
    ///
    /// # Errors
    /// [`HostError::NoView`], [`HostError::Unsupported`] for a view [`Host::list`] cannot answer,
    /// and [`HostError::Store`] when the store cannot be read.
    pub fn view(
        &mut self,
        view: &str,
        identity: &Value,
    ) -> Result<Option<Map<String, Value>>, HostError> {
        let (declared, owner) = readable(&self.ir, view)?;
        let entity = self.ir.entity(&declared.source).name.to_string();
        let stored = store_of(&mut self.components, &owner)?.load(&entity, identity)?;
        Ok(stored
            .filter(|stored| crate::view::holds(declared, &stored.row))
            .map(|stored| crate::view::project(&self.ir, declared, identity, &stored.row)))
    }

    /// Every row `view` holds: each instance of its source its filter keeps, projected, in the
    /// order its `order_by` ranks them, ties and a view without `order_by` by identity.
    ///
    /// The filter read is one over the lifecycle state alone, as the compiler parsed it:
    /// `state == <State>` and `!=`, `in` and `none_of` over states, combined by `any`, `all` and
    /// `not`, and evaluated by ESS. Read from the
    /// same store as [`Host::view`], for either consistency.
    ///
    /// # Errors
    /// [`HostError::NoView`]; [`HostError::Unsupported`], naming the construct, for a view that
    /// declares parameters, computed fields, an aggregation, paging, fields read through a
    /// reference, a default row for an absent identity, or any other filter; and
    /// [`HostError::Store`] when the store cannot be read.
    pub fn list(&mut self, view: &str) -> Result<Vec<Map<String, Value>>, HostError> {
        let (declared, owner) = readable(&self.ir, view)?;
        let entity = self.ir.entity(&declared.source).name.to_string();
        let mut rows = store_of(&mut self.components, &owner)?
            .scan(&entity)?
            .into_iter()
            .filter(|(_, stored)| crate::view::holds(declared, &stored.row))
            .map(|(identity, stored)| {
                let row = crate::view::project(&self.ir, declared, &identity, &stored.row);
                (identity, row)
            })
            .collect::<Vec<_>>();
        crate::view::order(&self.ir, declared, &mut rows);
        Ok(rows.into_iter().map(|(_, row)| row).collect())
    }

    /// Executes one command on `component` and sends the events it emitted.
    ///
    /// The outer error is the host's own; the inner one is the command's.
    fn execute(
        &mut self,
        component: &str,
        command: &str,
        input: &Map<String, Value>,
        actor: Option<&str>,
        caller: Option<&Map<String, Value>>,
    ) -> Result<Result<Executed, ExecuteError>, HostError> {
        let held = self
            .components
            .get_mut(component)
            .ok_or_else(|| HostError::NotAccepted(command.to_owned()))?;
        let started = Instant::now();
        let clock = &self.clock;
        let result = spindle_kernel::execute_as(
            &self.ir,
            held.store.as_mut(),
            self.decider.as_mut(),
            &mut UuidV4,
            &|| clock.wall(),
            &spindle_kernel::Invocation {
                command,
                input,
                actor,
                caller,
            },
        );
        held.sink.executed(command, &result, started.elapsed());
        let executed = match result {
            Ok(executed) => executed,
            Err(error) => return Ok(Err(error)),
        };
        self.published
            .extend(executed.events.iter().map(|event| Published {
                component: component.to_owned(),
                command: command.to_owned(),
                event: event.clone(),
            }));
        if !executed.events.is_empty() {
            held.position += 1;
            for event in &executed.events {
                held.sender.publish(
                    held.position,
                    spindle_channel::Event {
                        name: event.name.clone(),
                        fields: event.payload.clone(),
                    },
                )?;
            }
            held.sender.mark_durable(held.position);
            held.sender.flush()?;
        }
        Ok(Ok(executed))
    }

    fn run(
        &mut self,
        component: &str,
        command: &str,
        input: &Map<String, Value>,
        cause: Cause,
        report: &mut Report,
    ) -> Result<(), HostError> {
        match self.execute(component, command, input, None, None)? {
            Ok(Executed {
                outcome,
                error: Some(error),
                ..
            }) => report.refused.push(RefusedCommand {
                component: component.to_owned(),
                command: command.to_owned(),
                cause,
                outcome,
                error,
            }),
            Ok(executed) => report.ran.push(Ran {
                component: component.to_owned(),
                command: command.to_owned(),
                cause,
                executed,
            }),
            Err(error) => report.failed.push(Failed {
                component: component.to_owned(),
                command: command.to_owned(),
                cause,
                error: error.to_string(),
            }),
        }
        Ok(())
    }

    /// Delivers every envelope in flight, and every one those commands send, until none is left
    /// or the call has delivered [`DELIVERY_LIMIT`]; then discards what is still in flight and
    /// stops as a [`HostError::Runaway`].
    fn settle(
        &mut self,
        report: &mut Report,
        deliveries: &mut Deliveries,
    ) -> Result<(), HostError> {
        if self.remote.is_some() {
            return self.settle_remote(report, deliveries, true);
        }
        loop {
            let mut delivered = false;
            for index in 0..self.receivers.len() {
                while let Some(envelope) = self.receivers[index].receive(Duration::ZERO)? {
                    if deliveries.count >= DELIVERY_LIMIT {
                        self.discard()?;
                        return Err(HostError::Runaway {
                            deliveries: deliveries.count,
                            bindings: std::mem::take(&mut deliveries.bindings),
                        });
                    }
                    deliveries.count += 1;
                    deliveries.bindings.insert(envelope.binding.clone());
                    delivered = true;
                    self.run(
                        &envelope.to,
                        &envelope.command.name,
                        &envelope.command.fields,
                        Cause::Delivered {
                            binding: envelope.binding.clone(),
                        },
                        report,
                    )?;
                    self.receivers[index].acknowledge(&envelope)?;
                }
            }
            if !delivered {
                break;
            }
        }
        for component in self.components.values_mut() {
            component.sender.collect_acknowledgements(Duration::ZERO)?;
        }
        Ok(())
    }

    /// Takes every envelope still in flight off its channel without running it.
    fn discard(&mut self) -> Result<(), HostError> {
        for receiver in &mut self.receivers {
            receiver.discard()?;
        }
        Ok(())
    }
}

/// What one call has delivered so far.
#[derive(Debug, Default)]
struct Deliveries {
    count: usize,
    bindings: BTreeSet<String>,
}

/// A component's store, counted into `sink`: in SQLite at `file`, in memory without one.
fn opened(file: Option<&Path>, sink: &Sink) -> Result<Box<dyn Store>, HostError> {
    Ok(match file {
        None => Box::new(CountingStore::new(MemoryStore::default(), sink.clone())),
        Some(file) => Box::new(CountingStore::new(SqliteStore::open(file)?, sink.clone())),
    })
}

/// The store of a component whose store is closed: every read and write fails, naming why, so
/// the component never answers from a store its binding does not name.
struct Lost(String);

impl Store for Lost {
    fn load(&mut self, _: &str, _: &Value) -> Result<Option<spindle_kernel::Stored>, StoreError> {
        Err(StoreError::Backend(self.0.clone()))
    }

    fn commit(&mut self, _: &spindle_kernel::Change) -> Result<(), StoreError> {
        Err(StoreError::Backend(self.0.clone()))
    }

    fn scan(&mut self, _: &str) -> Result<Vec<(Value, spindle_kernel::Stored)>, StoreError> {
        Err(StoreError::Backend(self.0.clone()))
    }
}

/// The SQLite files a store at `file` keeps: the database and its side files.
fn sqlite_files(file: &Path) -> Vec<PathBuf> {
    ["", "-wal", "-shm", "-journal"]
        .into_iter()
        .map(|suffix| {
            let mut path = file.as_os_str().to_owned();
            path.push(suffix);
            PathBuf::from(path)
        })
        .collect()
}

/// Where a reset moves `path` while the empty store is opened.
fn aside(path: &Path) -> PathBuf {
    let mut moved = path.as_os_str().to_owned();
    moved.push(".reset");
    PathBuf::from(moved)
}

/// An empty SQLite store at `file`, whose store is closed: its files are moved aside, an empty
/// one is opened, and only then are the moved files removed. On failure every file is where it
/// was and the error names what failed; a file that could not be moved back is named with where
/// it is.
fn emptied_file(file: &Path, sink: &Sink) -> Result<Box<dyn Store>, String> {
    let mut moved = Vec::new();
    let restore = |moved: &[PathBuf], why: String| {
        let mut why = why;
        for path in moved {
            if let Err(error) = std::fs::rename(aside(path), path) {
                why.push_str(&format!(
                    "; {} could not be moved back from {}: {error}",
                    path.display(),
                    aside(path).display()
                ));
            }
        }
        why
    };
    for path in sqlite_files(file) {
        match std::fs::rename(&path, aside(&path)) {
            Ok(()) => moved.push(path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(restore(
                    &moved,
                    format!("moving {} aside: {error}", path.display()),
                ));
            }
        }
    }
    match opened(Some(file), sink) {
        Ok(store) => {
            for path in &moved {
                let _ = std::fs::remove_file(aside(path));
            }
            Ok(store)
        }
        Err(error) => {
            for path in sqlite_files(file) {
                let _ = std::fs::remove_file(path);
            }
            Err(restore(
                &moved,
                format!("opening an empty store at {}: {error}", file.display()),
            ))
        }
    }
}

/// The component that accepts `command`.
fn accepting(ir: &EssIr, command: &str) -> Option<String> {
    ir.components()
        .values()
        .find(|component| {
            component
                .accepts
                .iter()
                .any(|accepted| accepted.to_string() == command)
        })
        .map(|component| component.name.as_str().to_owned())
}

/// The view named `view`, where this host can answer it, and the component that owns its source.
/// Whether a host running `ir` answers the view `view`, asked before anything is read: the
/// check [`Host::view`] and [`Host::list`] make first.
///
/// # Errors
/// As [`Host::list`] refuses the view: [`HostError::NoView`], or [`HostError::Unsupported`]
/// naming the construct it does not read.
pub fn reads(ir: &EssIr, view: &str) -> Result<(), HostError> {
    readable(ir, view).map(|_| ())
}

fn readable<'ir>(ir: &'ir EssIr, view: &str) -> Result<(&'ir ResolvedView, String), HostError> {
    let declared = ir
        .views()
        .iter()
        .find_map(|(name, declared)| (name.to_string() == view).then_some(declared))
        .ok_or_else(|| HostError::NoView(view.to_owned()))?;
    if let Some(construct) = crate::view::unsupported(declared) {
        return Err(HostError::Unsupported(format!(
            "view {view} declares {construct}"
        )));
    }
    let entity = ir.entity(&declared.source);
    let owner = ir
        .components()
        .values()
        .find(|component| component.owns.contains(&entity.domain))
        .map(|component| component.name.as_str().to_owned())
        .ok_or_else(|| HostError::Unsupported(format!("no component owns {}", entity.domain)))?;
    Ok((declared, owner))
}

/// The store of the running component `owner`.
fn store_of<'a>(
    components: &'a mut BTreeMap<String, Component>,
    owner: &str,
) -> Result<&'a mut dyn Store, HostError> {
    components
        .get_mut(owner)
        .map(|component| component.store.as_mut() as &mut dyn Store)
        .ok_or_else(|| HostError::Unsupported(format!("{owner} is not running")))
}
