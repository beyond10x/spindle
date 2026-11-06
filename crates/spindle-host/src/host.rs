//! The host: every component of one system in one process.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use ess_compiler::EssIr;
use ess_compiler::ir::{
    ResolvedBinding, ResolvedMappingValue, ResolvedRefusalAction, ResolvedView,
};
use ess_domain::binding::Failure;
use serde_json::{Map, Value};
use spindle_capability::{Capability, Environment};
use spindle_channel::{
    ChannelError, Envelope, Receiver, Route, RouteError, Router, Sender, in_process,
};
use spindle_kernel::{
    Change, Decider, DeclaredError, Event, ExecuteError, Executed, MemoryStore, Mint, NoExternals,
    Row, Seeded, Store, StoreError, Uuid, UuidV4,
};
use spindle_telemetry::{Counters, CountingStore, Retried, Sink};
use spindle_timer::{BindingName, Clock, CompleteError, Instant, LifetimeError, Timer};

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
    /// How many times it was run, this one included: more than 1 only for a binding that retried
    /// its earlier attempts (`on_failure: retry`, or `on_refusal` choosing `retry`).
    pub attempts: u32,
}

/// One command a tick or a binding invoked that did not execute. Nothing was written, and the
/// host does not run it again: a binding's command is given up after its last attempt
/// (at most [`RETRY_ATTEMPTS`] for a binding that retries, else 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failed {
    /// The component that was to run it.
    pub component: String,
    /// The command.
    pub command: String,
    /// Why it was invoked.
    pub cause: Cause,
    /// The kernel's error on the last attempt, as text.
    pub error: String,
    /// How many times it was run: 0 for a binding whose input was unmet, so its command was
    /// never sent.
    pub attempts: u32,
}

/// One command a tick or a binding invoked that the model refused: its outcome declares an error.
/// Nothing was written, and the host does not run it again, as for [`Failed`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefusedCommand {
    /// The component that ran it.
    pub component: String,
    /// The command.
    pub command: String,
    /// Why it was invoked.
    pub cause: Cause,
    /// The refusing outcome of the last attempt.
    pub outcome: String,
    /// The error that outcome declares.
    pub error: DeclaredError,
    /// How many times it was run.
    pub attempts: u32,
}

/// What a run starts from after a [reset](Host::reset_to); the default is an empty run.
#[derive(Default)]
pub struct World {
    /// The rows written once every store is empty, in order.
    pub rows: Vec<Seed>,
    /// What answers every `external:` cause until the next reset; `None` for the decider the
    /// host was opened with.
    pub answers: Option<Box<dyn Decider + Send>>,
    /// The seed every identity and generated value is minted from until the next reset
    /// ([`Seeded`]); `None` for random version 4 UUIDs.
    pub mint: Option<u64>,
}

impl std::fmt::Debug for World {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("World")
            .field("rows", &self.rows)
            .field("answers", &self.answers.is_some())
            .field("mint", &self.mint)
            .finish()
    }
}

/// One row of a [`World`]: the instance of `entity` (its qualified name) with `identity`, as it
/// rests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seed {
    /// The entity's qualified name.
    pub entity: String,
    /// The instance's identity.
    pub identity: Value,
    /// Its state and fields.
    pub row: Row,
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

/// How many times the host runs the command of an `on_failure: retry` binding for one envelope
/// before it gives the invocation up: the first attempt and four retries. A runtime default
/// (design § 11): ESS bounds no unbounded retry, and this host retries at once, on the calling
/// thread, with no backoff, so the next envelope is not invoked before the retry settles.
pub const RETRY_ATTEMPTS: u32 = 5;

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
    /// A view was read with parameter values it refuses ([`Host::list_with`]): one it does not
    /// declare, a required one left out, or one not of its declared type. Nothing was read.
    #[error("{view}: {detail}")]
    Parameters {
        /// The view.
        view: String,
        /// What is wrong, naming the parameter.
        detail: String,
    },
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
    /// [`Host::reset_to`] refused a world before it changed anything.
    #[error("the world was refused: {0}")]
    World(String),
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

/// What a host does with an event binding whose `on_failure` is `retry`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Retries {
    /// Retry a failed invocation, refused or not executed, at once until it takes effect or
    /// [`RETRY_ATTEMPTS`] attempts were made; then report it once, as given up.
    #[default]
    Retried,
    /// Run the binding as a `drop` binding runs: a failed invocation is reported once and never
    /// run again, and an envelope whose send fails is dropped.
    Dropped,
}

/// The first event binding of `ir`, by name, whose `on_failure` a host does not carry, and what
/// it says: the binding [`Host::new`] refuses before it opens anything. Only `escalate` is not
/// carried, universal or chosen for some refusal (`on_refusal`).
pub fn uncarried(ir: &EssIr) -> Option<(String, Failure)> {
    ir.bindings()
        .values()
        .filter(|binding| binding.cause.event().is_some())
        .find(|binding| escalates(binding))
        .map(|binding| (binding.name.as_str().to_owned(), Failure::Escalate))
}

/// Whether `binding` escalates some failure.
fn escalates(binding: &ResolvedBinding) -> bool {
    match &binding.refusal_policy {
        Some(policy) => policy
            .actions()
            .any(|action| matches!(action, ResolvedRefusalAction::Escalate { .. })),
        None => binding.failure == Failure::Escalate,
    }
}

/// The event bindings of `ir` that retry a failure, by name: `on_failure: retry`, or a policy
/// chosen per refusal that retries some refusal or its fallback.
pub fn retried(ir: &EssIr) -> Vec<String> {
    ir.bindings()
        .values()
        .filter(|binding| binding.cause.event().is_some())
        .filter(|binding| match &binding.refusal_policy {
            Some(policy) => policy
                .actions()
                .any(|action| matches!(action, ResolvedRefusalAction::Retry { .. })),
            None => binding.failure == Failure::Retry,
        })
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
    ir: Arc<EssIr>,
    clock: Arc<dyn Clock>,
    placements: Placements,
    components: BTreeMap<String, Component>,
    receivers: Vec<Receiver>,
    timer: Timer,
    periodic: BTreeMap<String, Periodic>,
    remote: Option<component::Remote>,
    published: Vec<Published>,
    /// Answers the `external:` causes of every command any component runs, unless the world of
    /// the last reset carries its own answers.
    decider: Box<dyn Decider + Send>,
    /// The answers of the world the last [reset](Host::reset_to) loaded, if it carried any.
    answers: Option<Box<dyn Decider + Send>>,
    /// Mints the identities and generated values of every command any component runs.
    mint: Box<dyn Mint + Send>,
    /// The routes of the event bindings this host may retry, by binding name: read from its own
    /// routes, never from an envelope, so a peer's frame does not decide the policy.
    retrying: BTreeMap<String, Arc<Route>>,
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
    /// `on_failure` is `escalate` ([`uncarried`]). An `on_failure: retry` binding is retried
    /// ([`Retries::Retried`]).
    pub fn new(
        ir: EssIr,
        environment: &Environment,
        clock: Arc<dyn Clock>,
        state: &Path,
    ) -> Result<Self, HostError> {
        Self::open(ir, environment, clock, state, None, Retries::Retried)
    }

    /// [`Host::new`], with `retries` saying what becomes of an event binding whose `on_failure`
    /// is `retry` ([`retried`]): retried, as [`Host::new`] retries it, or run as a `drop` binding
    /// runs, each failed invocation reported once and never run again.
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
            Retries::Retried,
        )
    }

    /// [`Host::deciding`], with `retries` as [`Host::with_retries`] takes it.
    ///
    /// # Errors
    /// As [`Host::deciding`].
    pub fn deciding_with_retries(
        ir: EssIr,
        environment: &Environment,
        clock: Arc<dyn Clock>,
        state: &Path,
        decider: Box<dyn Decider + Send>,
        retries: Retries,
    ) -> Result<Self, HostError> {
        Self::open(ir, environment, clock, state, Some(decider), retries)
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
        if let Some((binding, failure)) = uncarried(&ir) {
            return Err(HostError::Unsupported(format!(
                "binding {binding} says on_failure: {failure:?}; this host drops or retries an \
                 invocation that fails and does not escalate it"
            )));
        }
        let ir = Arc::new(ir);
        let router = match retries {
            Retries::Retried => Router::from_shared(&ir)?,
            Retries::Dropped => Router::from_shared(&ir)?.dropping_retries(),
        };
        let retrying = router
            .routes()
            .iter()
            .filter(|route| route.retries())
            .map(|route| (route.binding.clone(), Arc::new(route.clone())))
            .collect();
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
                Some("sqlite") if cfg!(feature = "sqlite") => {
                    Some(state.join(format!("{name}.sqlite")))
                }
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
            answers: None,
            mint: Box::new(UuidV4),
            retrying,
        })
    }

    /// The compiled specification the host runs.
    pub fn ir(&self) -> &EssIr {
        &self.ir
    }

    /// Writes `row` as the instance of `entity` (its qualified name) with `identity` into the
    /// store of the component that owns the entity's domain, as it rests, with no command: the
    /// state a scenario arranges. Nothing is published and no binding runs. The row is checked
    /// against the entity's invariants first ([`spindle_kernel::at_rest`]).
    ///
    /// # Errors
    /// [`HostError::Unsupported`] for an entity the model does not declare or no running
    /// component owns; [`HostError::Execute`] when the row breaks an invariant of its entity;
    /// [`HostError::Store`], a [`StoreError::Conflict`], when the instance already exists, and
    /// any other store failure.
    pub fn establish(&mut self, entity: &str, identity: &Value, row: Row) -> Result<(), HostError> {
        let owner = self.resting(entity, identity, &row)?;
        store_of(&mut self.components, &owner)?.commit(&Change {
            entity: entity.to_owned(),
            identity: identity.clone(),
            expected: None,
            row,
            events: Vec::new(),
        })?;
        Ok(())
    }

    /// The running component whose store `row` of `entity` rests in, once the row is checked
    /// against the entity's invariants; the errors of [`Host::establish`] before it writes.
    fn resting(&self, entity: &str, identity: &Value, row: &Row) -> Result<String, HostError> {
        let declared = self
            .ir
            .entities()
            .iter()
            .find_map(|(name, declared)| (name.to_string() == entity).then_some(declared))
            .ok_or_else(|| HostError::Unsupported(format!("no entity {entity}")))?;
        let owner = self
            .ir
            .components()
            .values()
            .find(|component| component.owns.contains(&declared.domain))
            .map(|component| component.name.as_str().to_owned())
            .ok_or_else(|| {
                HostError::Unsupported(format!("no component owns {}", declared.domain))
            })?;
        if !self.components.contains_key(&owner) {
            return Err(HostError::Unsupported(format!("{owner} is not running")));
        }
        spindle_kernel::at_rest(&self.ir, entity, identity, row).map_err(|error| {
            HostError::Execute {
                component: owner.clone(),
                command: format!("establishing {entity}"),
                error: Box::new(error),
            }
        })?;
        Ok(owner)
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
    /// and the counters keep their counts. The answers and the mint of the world an earlier
    /// [`Host::reset_to`] loaded are dropped.
    ///
    /// # Errors
    /// [`HostError::Reset`] when a component's files cannot be moved aside or the empty store
    /// cannot be opened. Its files are moved back and its store reopened as it was
    /// (`restored: true`); where even that fails, every read and write of that component fails
    /// from then on, naming why (`restored: false`). The components emptied before it stay
    /// empty, and the published events are forgotten only when every store was emptied.
    pub fn reset(&mut self) -> Result<Vec<String>, HostError> {
        self.reset_to(World::default())
    }

    /// [`Host::reset`], then `world` loaded: each of its rows written where its entity rests, as
    /// [`Host::establish`] writes it, every external cause answered by its answers until the next
    /// reset, and every identity and generated value minted by its mint until then. A world
    /// without answers leaves causes to the decider the host was opened with, and one without a
    /// mint mints random version 4 UUIDs. A seeded mint never answers a UUID a row's identity
    /// holds, or ends in ([`Seeded::avoiding`]).
    ///
    /// Every row is checked before any store is emptied: its entity declared and owned by a
    /// running component, its invariants held, its identity one its store can key (read from the
    /// store as it is), and no two rows one instance. The answers and the mint are taken only once
    /// every row is written.
    ///
    /// # Errors
    /// [`HostError::World`] for a row that does not pass, before anything changed; then as
    /// [`Host::reset`], and as [`Host::establish`] for a row a store did not take, the rows
    /// before it written and the answers and mint of the last world kept.
    pub fn reset_to(&mut self, world: World) -> Result<Vec<String>, HostError> {
        let mut owners = Vec::with_capacity(world.rows.len());
        let mut instances = BTreeSet::new();
        for (index, seed) in world.rows.iter().enumerate() {
            let owner = self
                .resting(&seed.entity, &seed.identity, &seed.row)
                .map_err(|error| HostError::World(format!("row {index}: {error}")))?;
            if !instances.insert((seed.entity.as_str(), seed.identity.to_string())) {
                return Err(HostError::World(format!(
                    "row {index}: {} {} is already a row of this world",
                    seed.entity, seed.identity
                )));
            }
            store_of(&mut self.components, &owner)?
                .load(&seed.entity, &seed.identity)
                .map_err(|error| {
                    HostError::World(format!(
                        "row {index}: {owner}'s store cannot hold {} {}: {error}",
                        seed.entity, seed.identity
                    ))
                })?;
            owners.push(owner);
        }
        let held = world
            .rows
            .iter()
            .filter_map(|seed| seed.identity.as_str())
            .filter_map(|text| {
                let tail = text.len().checked_sub(36).and_then(|at| text.get(at..))?;
                Uuid::parse_str(tail).ok()
            })
            .collect::<Vec<_>>();
        let emptied = self.empty()?;
        for (seed, owner) in world.rows.into_iter().zip(owners) {
            store_of(&mut self.components, &owner)?.commit(&Change {
                entity: seed.entity,
                identity: seed.identity,
                expected: None,
                row: seed.row,
                events: Vec::new(),
            })?;
        }
        self.answers = world.answers;
        self.mint = world.mint.map_or_else(
            || Box::new(UuidV4) as Box<dyn Mint + Send>,
            |seed| Box::new(Seeded::new(seed).avoiding(held)),
        );
        Ok(emptied)
    }

    fn empty(&mut self) -> Result<Vec<String>, HostError> {
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
                component: component.clone(),
                command: command.to_owned(),
                cause: Cause::Submitted,
                executed,
                attempts: 1,
            }),
            Err(error) => {
                return Err(HostError::Execute {
                    component,
                    command: command.to_owned(),
                    error: Box::new(error),
                });
            }
        }
        self.unmet(&component, &mut report);
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
    /// A view that declares parameters is read as [`Host::list_with`] reads it with no parameter
    /// values, so a required parameter is refused as left out.
    ///
    /// # Errors
    /// [`HostError::NoView`], [`HostError::Unsupported`] for a view [`Host::list`] cannot answer,
    /// [`HostError::Parameters`] for a view with a required parameter, and [`HostError::Store`]
    /// when the store cannot be read.
    pub fn view(
        &mut self,
        view: &str,
        identity: &Value,
    ) -> Result<Option<Map<String, Value>>, HostError> {
        let (declared, owner) = readable(&self.ir, view)?;
        let parameters =
            crate::view::parameters(&self.ir, declared, &Map::new()).map_err(|detail| {
                HostError::Parameters {
                    view: view.to_owned(),
                    detail,
                }
            })?;
        let entity = self.ir.entity(&declared.source).name.to_string();
        let stored = store_of(&mut self.components, &owner)?.load(&entity, identity)?;
        let Some(stored) = stored else {
            return Ok(None);
        };
        Ok(
            held(&self.ir, view, declared, identity, &stored.row, &parameters)?
                .then(|| crate::view::project(&self.ir, declared, identity, &stored.row)),
        )
    }

    /// Every row `view` holds: [`Host::list_with`] with no parameter values.
    ///
    /// # Errors
    /// As [`Host::list_with`]; a view with a required parameter is refused
    /// [`HostError::Parameters`].
    pub fn list(&mut self, view: &str) -> Result<Vec<Map<String, Value>>, HostError> {
        self.list_with(view, &Map::new())
    }

    /// Every row `view` holds for the parameter values `parameters`, by declared parameter name:
    /// each instance of its source its filter keeps, projected, in the order its `order_by` ranks
    /// them, ties and a view without `order_by` by identity.
    ///
    /// The filter is evaluated by ESS, as the compiler parsed it, over the row and the parameters
    /// (each read as `param.<name>`). The filters read are those whose leaves compare the state
    /// with a state (`state == <State>` and `!=`, `in` and `none_of` over states), compare a
    /// parameter by `==` or `!=` with the state, the identity or a field, or ask whether a
    /// parameter is `defined`, combined by `any`, `all` and `not`. A filter unknown for a row, such
    /// as `==` with an `Optional` parameter left out, refuses the read, as ESS's interpreter
    /// refuses it. Read from the same store as [`Host::view`], for either consistency.
    ///
    /// # Errors
    /// [`HostError::NoView`]; [`HostError::Unsupported`], naming the construct, for a view that
    /// declares computed fields, an aggregation, paging, fields read through a reference, a
    /// default row for an absent identity, or any other filter; [`HostError::Parameters`],
    /// naming the parameter, for a value the view does not declare, a required one left out or
    /// one not of its declared type or newtype, before anything is read;
    /// [`HostError::Unsupported`] for a filter unknown for a row; and [`HostError::Store`] when
    /// the store cannot be read.
    pub fn list_with(
        &mut self,
        view: &str,
        parameters: &Map<String, Value>,
    ) -> Result<Vec<Map<String, Value>>, HostError> {
        let (declared, owner) = readable(&self.ir, view)?;
        let parameters =
            crate::view::parameters(&self.ir, declared, parameters).map_err(|detail| {
                HostError::Parameters {
                    view: view.to_owned(),
                    detail,
                }
            })?;
        let entity = self.ir.entity(&declared.source).name.to_string();
        let mut rows = Vec::new();
        for (identity, stored) in store_of(&mut self.components, &owner)?.scan(&entity)? {
            if held(
                &self.ir,
                view,
                declared,
                &identity,
                &stored.row,
                &parameters,
            )? {
                let row = crate::view::project(&self.ir, declared, &identity, &stored.row);
                rows.push((identity, row));
            }
        }
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
            match &mut self.answers {
                Some(answers) => answers.as_mut(),
                None => self.decider.as_mut(),
            },
            self.mint.as_mut(),
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

    /// Runs one command a tick invoked, once.
    fn run(
        &mut self,
        component: &str,
        command: &str,
        input: &Map<String, Value>,
        cause: Cause,
        report: &mut Report,
    ) -> Result<(), HostError> {
        self.attempt(component, command, input, cause, None, report)
    }

    /// Runs the command `envelope` carries; where this host's own route of `envelope.binding`
    /// retries (the envelope's `on_failure` is the sender's word), again at once after each
    /// attempt that is refused or does not execute and that the route answers with
    /// [`Failure::Retry`] ([`Route::answer`]: `on_failure: retry`, or the refusal's policy under
    /// `on_refusal`), until one takes effect, the route drops one, or [`RETRY_ATTEMPTS`] were
    /// made, each attempt counted on the receiving component.
    fn invoke(&mut self, envelope: &Envelope, report: &mut Report) -> Result<(), HostError> {
        let retried = self.retrying.get(&envelope.binding).cloned();
        self.attempt(
            &envelope.to,
            &envelope.command.name,
            &envelope.command.fields,
            Cause::Delivered {
                binding: envelope.binding.clone(),
            },
            retried.as_deref(),
            report,
        )
    }

    /// Runs `command` once, or, when `retried` is the binding's route, again after each attempt
    /// the route answers with [`Failure::Retry`], up to [`RETRY_ATTEMPTS`] in all; reports the
    /// attempt that took effect, or the last one.
    fn attempt(
        &mut self,
        component: &str,
        command: &str,
        input: &Map<String, Value>,
        cause: Cause,
        retried: Option<&Route>,
        report: &mut Report,
    ) -> Result<(), HostError> {
        let sink = self
            .components
            .get(component)
            .map(|held| held.sink.clone())
            .unwrap_or_default();
        let mut attempts = 0;
        loop {
            attempts += 1;
            if let Some(route) = retried {
                sink.retried(&route.binding, Retried::Attempted);
            }
            let result = self.execute(component, command, input, None, None)?;
            let refusal = match &result {
                Ok(Executed { error: None, .. }) => None,
                Ok(Executed { outcome, .. }) => Some(Some(outcome.as_str())),
                Err(_) => Some(None),
            };
            let again = retried.filter(|route| {
                refusal.is_some_and(|refusal| route.answer(refusal) == Failure::Retry)
            });
            if again.is_some() && attempts < RETRY_ATTEMPTS {
                continue;
            }
            if let Some(route) = again {
                sink.retried(&route.binding, Retried::Exhausted);
            }
            match result {
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
                    attempts,
                }),
                Ok(executed) => report.ran.push(Ran {
                    component: component.to_owned(),
                    command: command.to_owned(),
                    cause,
                    executed,
                    attempts,
                }),
                Err(error) => report.failed.push(Failed {
                    component: component.to_owned(),
                    command: command.to_owned(),
                    cause,
                    error: error.to_string(),
                    attempts,
                }),
            }
            self.unmet(component, report);
            return Ok(());
        }
    }

    /// Reports every binding the events `component` just published triggered whose input was unmet
    /// as a command that did not execute: its command was never sent ([`spindle_channel::Unmet`]).
    fn unmet(&mut self, component: &str, report: &mut Report) {
        let Some(held) = self.components.get_mut(component) else {
            return;
        };
        report
            .failed
            .extend(held.sender.take_unmet().into_iter().map(|unmet| Failed {
                component: unmet.to,
                command: unmet.command,
                cause: Cause::Delivered {
                    binding: unmet.binding,
                },
                error: unmet.error.to_string(),
                attempts: 0,
            }));
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
                    self.invoke(&envelope, report)?;
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
        #[cfg(feature = "sqlite")]
        Some(file) => Box::new(CountingStore::new(
            spindle_kernel::SqliteStore::open(file)?,
            sink.clone(),
        )),
        #[cfg(not(feature = "sqlite"))]
        Some(file) => {
            return Err(HostError::Unsupported(format!(
                "{} is a SQLite store, which this build leaves out",
                file.display()
            )));
        }
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

    fn remove(&mut self, _: &spindle_kernel::Removal) -> Result<(), StoreError> {
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
    if let Some(construct) = crate::view::unsupported(ir, declared) {
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

/// Whether `declared`, the view `view`, holds `row` for `parameters` ([`crate::view::holds`]); a
/// row that is not a value of its source's fields is refused [`HostError::Unsupported`], naming it.
fn held(
    ir: &EssIr,
    view: &str,
    declared: &ResolvedView,
    identity: &Value,
    row: &Row,
    parameters: &ess_primitives::FactStore,
) -> Result<bool, HostError> {
    crate::view::holds(ir, declared, identity, row, parameters)
        .map_err(|detail| HostError::Unsupported(format!("view {view}: {detail}")))
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
