//! The host: every component of one system in one process.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use ess_compiler::EssIr;
use ess_compiler::ir::{ResolvedEntity, ResolvedMappingValue, ResolvedRefusalAction, ResolvedView};
use ess_domain::binding::Failure;
use serde_json::{Map, Value};
use spindle_capability::{Capability, Environment};
use spindle_channel::{
    ChannelError, DeliveryId, Envelope, MappingError, Receiver, Route, RouteError, Router, Sender,
    in_process,
};
use spindle_kernel::{
    Change, Committed, DeadLetter, Decider, DeclaredError, Event, ExecuteError, Executed,
    MemoryStore, Mint, NoExternals, Origin, Provenance, Reading, Row, Scanned, Seeded, Store,
    StoreError, Stored, Uuid, UuidV4,
};
use spindle_telemetry::{Counters, CountingStore, Retried, Sink};
use spindle_timer::{
    BindingName, Clock, CompleteError, Instant, LifetimeError, ManualClock, Timer,
};

use crate::admission::{Placements, Refusal, admit};

mod component;
mod range;
mod reader;
mod rows;

pub use range::{Cursor, History, HistoryCursor, Page, Revision};
pub use reader::Reader;
use rows::Counted;

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
/// (at most [`RETRY_ATTEMPTS`] for a binding that retries, else 1), and kept as a dead letter
/// where its binding did not escalate it ([`Failed::dead_letter`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failed {
    /// The component that was to run it.
    pub component: String,
    /// The command.
    pub command: String,
    /// Why it was invoked.
    pub cause: Cause,
    /// The qualified command whose run published the event a binding carried here, for a
    /// command an event binding invoked, or whose input was unmet, in this host; `None` for a
    /// tick's command, and for an envelope delivered from another process (a run split over
    /// processes, [`Host::component`]), whose publishing command this host never ran.
    pub caused_by: Option<String>,
    /// The kernel's error on the last attempt, as text.
    pub error: String,
    /// The kind of the kernel's error on the last attempt ([`ExecuteError::kind`]); `None` for a
    /// binding whose input was unmet, which no attempt ran.
    pub kind: Option<&'static str>,
    /// How many times it was run: 0 for a binding whose input was unmet, so its command was
    /// never sent.
    pub attempts: u32,
    /// The id of the dead letter its component's store keeps it as ([`Host::dead_letters`]):
    /// `None` for a command [`Host::submit`] was asked to run, one whose binding escalated it,
    /// one whose input was unmet, and one whose store keeps no dead letters or could not keep
    /// this one.
    pub dead_letter: Option<u64>,
    /// Why its component's store did not keep it as a dead letter, where it is one and that store
    /// could not keep it ([`Store::keep_dead_letter`]): the store's reason. `None` where it was
    /// kept, and where it is no dead letter.
    pub not_kept: Option<String>,
}

/// One command a tick or a binding invoked that the model refused: its outcome declares an error.
/// Nothing was written, and the host does not run it again, as for [`Failed`]; it is kept as a
/// dead letter alike ([`RefusedCommand::dead_letter`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefusedCommand {
    /// The component that ran it.
    pub component: String,
    /// The command.
    pub command: String,
    /// Why it was invoked.
    pub cause: Cause,
    /// The qualified command whose run published the event a binding carried here, as
    /// [`Failed::caused_by`].
    pub caused_by: Option<String>,
    /// The refusing outcome of the last attempt.
    pub outcome: String,
    /// The error that outcome declares.
    pub error: DeclaredError,
    /// How many times it was run.
    pub attempts: u32,
    /// The id of the dead letter its component's store keeps it as, as [`Failed::dead_letter`].
    pub dead_letter: Option<u64>,
    /// Why its component's store did not keep it as a dead letter, as [`Failed::not_kept`].
    pub not_kept: Option<String>,
}

/// What [`Host::replayed`] ran: the replay's own run, and everything the call settled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replayed {
    /// The replay's own run of the command, as the report lists it.
    pub own: Attempted,
    /// Everything the call ran: the replay's own run first in its list, then what its events led
    /// to and any envelope a peer process sent that the call settled.
    pub report: Report,
}

/// How one run of a command ended ([`Replayed::own`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attempted {
    /// It executed and was not refused.
    Ran(Ran),
    /// The model refused it.
    Refused(RefusedCommand),
    /// It did not execute.
    Failed(Failed),
}

/// A dead letter a component's store keeps ([`Host::dead_letters`]): a command a binding or a
/// tick invoked whose last attempt was refused or did not execute, and that its binding did not
/// escalate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Undelivered {
    /// The component that accepts the command, whose store keeps the dead letter.
    pub component: String,
    /// Its id in that store, which [`Host::replay`] names.
    pub id: u64,
    /// What was kept.
    pub letter: DeadLetter,
}

/// One command a binding invoked whose last attempt did not take effect and whose failure policy
/// escalates that answer (`on_failure: {escalate: ...}`, or an `escalate:` entry chosen for the
/// refusal): the component that ran it publishes the binding's escalation event once, from the
/// attempt's input, and the command is not invoked again. The event goes to that component's
/// outbox, as every event it publishes, and is not written to its store: it is not in the event
/// history ([`Host::published`]), and is lost if the process stops before it is delivered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Escalated {
    /// The binding.
    pub binding: String,
    /// The component that ran the command and publishes the event.
    pub component: String,
    /// The command.
    pub command: String,
    /// The refusing outcome of the last attempt; `None` for an attempt that did not execute.
    pub outcome: Option<String>,
    /// The escalation event.
    pub event: String,
    /// What was published.
    pub escalation: Escalation,
}

/// What an escalation published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Escalation {
    /// The event, with this payload: each field of the event the command's input holds, an
    /// optional one it does not hold left out.
    Published(Map<String, Value>),
    /// Nothing: the command's input does not hold a required field of the event, or holds one that
    /// is no value of the field's type. The escalation is unmet; this host fills no placeholder.
    Unmet(String),
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
    /// The civil time the host's stepping clock moves forward to ([`Host::stepping`]); `None`
    /// leaves the clock where it stands. A host without a stepping clock refuses a world that
    /// carries one ([`HostError::Clock`]), and one with an instant before the clock's
    /// ([`HostError::World`]), before anything changed.
    pub clock: Option<SystemTime>,
}

impl std::fmt::Debug for World {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("World")
            .field("rows", &self.rows)
            .field("answers", &self.answers.is_some())
            .field("mint", &self.mint)
            .field("clock", &self.clock)
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
    /// The civil time of its command, as RFC 3339 in UTC to the millisecond, read from the
    /// sequence the host attributed the command with ([`spindle_kernel::Origin::sequence`]), as
    /// [`Revision::at`] is; `None` for an event its store holds no command for.
    pub at: Option<String>,
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
    /// The escalations of the refused and failed commands whose binding escalates their answer,
    /// in the order escalated.
    pub escalated: Vec<Escalated>,
    /// Every attempt an event binding made of its command, in the order made, a refused or failed
    /// one included, each with the input the binding sent: what ESS's conformance runner observes
    /// as the binding's invocations. A binding whose input was unmet made none.
    pub invoked: Vec<Invoked>,
}

/// One attempt an event binding made of its command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invoked {
    /// The binding.
    pub binding: String,
    /// The component that ran the command.
    pub component: String,
    /// The command.
    pub command: String,
    /// The input the binding sent.
    pub input: Map<String, Value>,
}

/// How many escalations a host keeps for [`Host::take_escalated`] before it forgets the oldest.
pub const ESCALATIONS_KEPT: usize = DELIVERY_LIMIT;

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
/// The `identity.mint` implementation that mints from a seed ([`Seeded`]): a host whose
/// environment binds it mints from seed 0 until [`Host::seeded`] names another.
const SEEDED_MINT: &str = "seeded";

/// Why the host did not start or could not go on.
#[derive(Debug, thiserror::Error)]
pub enum HostError {
    /// The environment cannot run every component.
    #[error("{0}")]
    Refused(Refusal),
    /// A component's durable store holds rows the specification cannot read: it was last written
    /// under another specification (`story:spec-change-over-stored-rows`). Nothing was written.
    #[error(
        "refused: the store of {component} holds rows this specification cannot read{}",
        misfits.iter().map(|misfit| format!("\n  {misfit}")).collect::<String>()
    )]
    Misfit {
        /// The component.
        component: String,
        /// Every row the specification cannot read, by entity and field or state.
        misfits: Vec<spindle_kernel::Misfit>,
    },
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
    /// A store could not be opened or read; or [`Host::reset`] found a store the host was given
    /// busy ([`StoreError::Busy`]), naming the component and the step, and kept that store.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// No component accepts the command.
    #[error("no component accepts {0}")]
    NotAccepted(String),
    /// A command [`Host::submit`] was asked to run did not execute. Nothing was written, except
    /// where its store cannot say whether the commit landed ([`ExecuteError::Store`] with
    /// [`StoreError::Unknown`]): its effect may have been written.
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
    /// No entity has this name ([`Host::history`]). Nothing was read.
    #[error("no entity {0}")]
    NoEntity(String),
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
        error: Box<DeclaredError>,
    },
    /// [`Host::reset`] could not empty a component's store. A store the host was given that was
    /// busy ([`StoreError::Busy`]) is not reported here: the reset answers [`HostError::Store`]
    /// and keeps it.
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
    /// The host's clock was asked to step and does not: the host has no stepping clock
    /// ([`Host::stepping`]), or [`Host::step_clock`] named an instant before the clock's.
    /// Nothing moved and nothing was emptied.
    #[error("{0}")]
    Clock(String),
    /// A cursor names a component this host does not run ([`Host::published_after`]):
    /// one another run answered. Nothing was read.
    #[error("{0}")]
    Cursor(String),
    /// One call delivered [`DELIVERY_LIMIT`] envelopes and more were still in flight; those
    /// were discarded, and nothing runs after them.
    /// What the call ran before it stopped is committed; the commands it reported as not
    /// executed until then are in `failed`.
    #[error(
        "stopped after {deliveries} deliveries in one call; these bindings kept triggering each other: {}",
        bindings.iter().cloned().collect::<Vec<_>>().join(", ")
    )]
    Runaway {
        /// How many envelopes were delivered.
        deliveries: usize,
        /// The bindings that carried them.
        bindings: BTreeSet<String>,
        /// Every command the call reported as not executed before it stopped, an unmet
        /// binding input included ([`Failed::attempts`] 0), in the order reported.
        failed: Vec<Failed>,
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
    /// A read of the events through a [`Reader`] stopped by its flag ([`Reader::stopped_by`]) was
    /// waiting, for a store's feed or for its turn, when the flag was set, and was given up: it
    /// read nothing to answer (`story:leader-exits-promptly-on-sigterm`).
    #[error("the read was given up: the run is stopping")]
    ReadStopped,
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

/// The stores a host is given rather than opening them itself, by component ([`Host::supplied`]).
pub type Stores = BTreeMap<String, Box<dyn Store>>;

/// What [`Host::assemble`] opens a host with, besides its placements.
struct Opening {
    decider: Option<Box<dyn Decider + Send>>,
    retries: Retries,
    stores: Stores,
}

/// Where a component's store keeps its state.
enum Kept {
    /// In memory.
    Memory,
    /// In the SQLite file at this path.
    File(PathBuf),
    /// In a store the host was given ([`Host::supplied`]).
    Supplied,
}

/// A store the host was given, as a [`Store`] of its own.
struct Supplied(Box<dyn Store>);

/// A component's store while one command runs on it: each write that lands tallies the events it
/// carries, by name, where the [`CountingStore`] beneath counts them appended.
struct Tallied<'a> {
    store: &'a mut dyn Store,
    appended: Vec<String>,
}

impl Tallied<'_> {
    /// Tallies `events`, which a write that landed carried.
    fn tally(&mut self, events: &[Event]) {
        self.appended
            .extend(events.iter().map(|event| event.name.clone()));
    }
}

impl Store for Tallied<'_> {
    fn load(
        &mut self,
        entity: &str,
        identity: &Value,
    ) -> Result<Option<spindle_kernel::Stored>, StoreError> {
        self.store.load(entity, identity)
    }

    fn scan(&mut self, entity: &str) -> Result<spindle_kernel::Scan, StoreError> {
        self.store.scan(entity)
    }

    fn counts(&mut self, entities: &[String]) -> Result<BTreeMap<String, usize>, StoreError> {
        self.store.counts(entities)
    }

    fn reading(&mut self) -> Option<Arc<dyn Reading>> {
        self.store.reading()
    }

    fn attribute(&mut self, origin: Option<Origin>) {
        self.store.attribute(origin);
    }

    fn outcome(&mut self, outcome: &str) {
        self.store.outcome(outcome);
    }

    fn events_after(&mut self, position: u64, limit: usize) -> Result<Vec<Committed>, StoreError> {
        self.store.events_after(position, limit)
    }

    fn events_head(&mut self) -> Result<u64, StoreError> {
        self.store.events_head()
    }

    fn events_id(&mut self) -> Result<u64, StoreError> {
        self.store.events_id()
    }

    /// Kept for [`Host::published`], tallied as no write's: an event no write carries counts
    /// nothing appended ([`Host::emitted`]).
    fn commit_events(&mut self, events: &[Event], scanned: &[Scanned]) -> Result<(), StoreError> {
        self.store.commit_events(events, scanned)
    }

    fn forget_events(&mut self) -> Result<(), StoreError> {
        self.store.forget_events()
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.store.commit(change)?;
        self.tally(&change.events);
        Ok(())
    }

    fn remove(&mut self, removal: &spindle_kernel::Removal) -> Result<(), StoreError> {
        self.store.remove(removal)?;
        self.tally(&removal.events);
        Ok(())
    }

    /// The writes land together, then tally as each would alone.
    fn commit_all(&mut self, writes: &[spindle_kernel::Write]) -> Result<(), StoreError> {
        self.store.commit_all(writes)?;
        for write in writes {
            match write {
                spindle_kernel::Write::Commit(change) => self.tally(&change.events),
                spindle_kernel::Write::Remove(removal) => self.tally(&removal.events),
            }
        }
        Ok(())
    }
}

/// The store a command runs on in a host: the running component's ([`Tallied`]), where every
/// write goes, with each read of an entity another component of this host owns (a row, a scan,
/// a count) answered from that component's store, as ESS's interpreter reads every entity from its
/// one store (`story:cross-component-related-reads`). The kernel reaches the stores of the
/// components this host runs ([`spindle_kernel::execute_reaching`]).
///
/// A write the kernel conditions on an entity it scanned ([`spindle_kernel::Write::scanned`]) is
/// conditioned on the version of the store that answered the scan: each entity another
/// component owns is checked against its owner's version, then left out of what the running
/// component's store checks, which holds no row of it. The check and the commit are two steps,
/// which no other write comes between while this host is the one writer of the owner's store: a
/// memory store, a SQLite state directory, which `run` locks, or PostgreSQL tables, which `run`
/// writes only under their lease: a second `run` stands by, and a write of one that lost the
/// lease is refused inside its own transaction (`story:several-writers-one-postgres`). Two hosts
/// writing one supplied store outside `run` can keep a stale decision
/// (`docs/design/spindle.md` § 11).
struct Routed<'a> {
    ir: &'a EssIr,
    running: Tallied<'a>,
    /// The store of every other component this host runs, by name.
    others: BTreeMap<&'a str, &'a mut dyn Store>,
}

impl Routed<'_> {
    /// The other component of this host that owns `entity`, where one does.
    fn owner(&self, entity: &str) -> Option<&str> {
        let declared = self.ir.entities().get(&entity.parse().ok()?)?;
        let owner = self
            .ir
            .components()
            .values()
            .find(|component| component.owns.contains(&declared.domain))?;
        self.others
            .get_key_value(owner.name.as_str())
            .map(|(name, _)| *name)
    }

    /// The store `entity` is read from: its owner's, where another component of this host owns
    /// it, else the running component's.
    fn source(&mut self, entity: &str) -> &mut dyn Store {
        match self.owner(entity).map(str::to_owned) {
            Some(owner) => match self.others.get_mut(owner.as_str()) {
                Some(store) => &mut **store,
                None => &mut self.running,
            },
            None => &mut self.running,
        }
    }

    /// Checks each of `scanned` another component owns against its owner's version, and answers
    /// the rest, which the running component's store checks; `None` where every one is the running
    /// component's.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] where another component's entity moved since its scan, as a store
    /// refuses a write whose scanned entity moved.
    fn unmoved(&mut self, scanned: &[Scanned]) -> Result<Option<Vec<Scanned>>, StoreError> {
        if scanned
            .iter()
            .all(|entity| self.owner(&entity.entity).is_none())
        {
            return Ok(None);
        }
        let mut own = Vec::new();
        for entity in scanned {
            match self.owner(&entity.entity).map(str::to_owned) {
                Some(owner) => {
                    let store = self
                        .others
                        .get_mut(owner.as_str())
                        .ok_or_else(|| StoreError::Backend(format!("{owner} is not running")))?;
                    let actual = store.scan(&entity.entity)?.version;
                    if actual != entity.version {
                        return Err(StoreError::Conflict {
                            entity: entity.entity.clone(),
                            identity: spindle_kernel::EVERY_INSTANCE.to_owned(),
                            expected: Some(entity.version),
                            actual: Some(actual),
                        });
                    }
                }
                None => own.push(entity.clone()),
            }
        }
        Ok(Some(own))
    }

    /// `write`, conditioned only on the running component's entities, once the others are
    /// checked ([`Self::unmoved`]).
    fn checked(
        &mut self,
        write: &spindle_kernel::Write,
    ) -> Result<spindle_kernel::Write, StoreError> {
        let mut write = write.clone();
        let scanned = match &mut write {
            spindle_kernel::Write::Commit(change) => &mut change.scanned,
            spindle_kernel::Write::Remove(removal) => &mut removal.scanned,
        };
        if let Some(own) = self.unmoved(scanned)? {
            *scanned = own;
        }
        Ok(write)
    }
}

impl Store for Routed<'_> {
    fn load(
        &mut self,
        entity: &str,
        identity: &Value,
    ) -> Result<Option<spindle_kernel::Stored>, StoreError> {
        self.source(entity).load(entity, identity)
    }

    fn scan(&mut self, entity: &str) -> Result<spindle_kernel::Scan, StoreError> {
        self.source(entity).scan(entity)
    }

    /// Counts each entity in the store it is read from, the running component's in one read.
    fn counts(&mut self, entities: &[String]) -> Result<BTreeMap<String, usize>, StoreError> {
        let (foreign, own): (Vec<String>, Vec<String>) = entities
            .iter()
            .cloned()
            .partition(|entity| self.owner(entity).is_some());
        let mut counts = self.running.counts(&own)?;
        for entity in foreign {
            let count = self.source(&entity).counts(std::slice::from_ref(&entity))?;
            counts.extend(count);
        }
        Ok(counts)
    }

    fn reading(&mut self) -> Option<Arc<dyn Reading>> {
        self.running.reading()
    }

    fn attribute(&mut self, origin: Option<Origin>) {
        self.running.attribute(origin);
    }

    fn outcome(&mut self, outcome: &str) {
        self.running.outcome(outcome);
    }

    fn events_after(&mut self, position: u64, limit: usize) -> Result<Vec<Committed>, StoreError> {
        self.running.events_after(position, limit)
    }

    fn events_head(&mut self) -> Result<u64, StoreError> {
        self.running.events_head()
    }

    fn events_id(&mut self) -> Result<u64, StoreError> {
        self.running.events_id()
    }

    fn commit_events(&mut self, events: &[Event], scanned: &[Scanned]) -> Result<(), StoreError> {
        if events.is_empty() {
            return self.running.commit_events(events, scanned);
        }
        match self.unmoved(scanned)? {
            Some(own) => self.running.commit_events(events, &own),
            None => self.running.commit_events(events, scanned),
        }
    }

    fn forget_events(&mut self) -> Result<(), StoreError> {
        self.running.forget_events()
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        match self.unmoved(&change.scanned)? {
            Some(scanned) => self.running.commit(&Change {
                scanned,
                ..change.clone()
            }),
            None => self.running.commit(change),
        }
    }

    fn remove(&mut self, removal: &spindle_kernel::Removal) -> Result<(), StoreError> {
        match self.unmoved(&removal.scanned)? {
            Some(scanned) => self.running.remove(&spindle_kernel::Removal {
                scanned,
                ..removal.clone()
            }),
            None => self.running.remove(removal),
        }
    }

    fn commit_all(&mut self, writes: &[spindle_kernel::Write]) -> Result<(), StoreError> {
        let writes = writes
            .iter()
            .map(|write| self.checked(write))
            .collect::<Result<Vec<_>, _>>()?;
        self.running.commit_all(&writes)
    }
}

impl Store for Supplied {
    fn load(
        &mut self,
        entity: &str,
        identity: &Value,
    ) -> Result<Option<spindle_kernel::Stored>, StoreError> {
        self.0.load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.0.commit(change)
    }

    fn remove(&mut self, removal: &spindle_kernel::Removal) -> Result<(), StoreError> {
        self.0.remove(removal)
    }

    fn commit_all(&mut self, writes: &[spindle_kernel::Write]) -> Result<(), StoreError> {
        self.0.commit_all(writes)
    }

    fn scan(&mut self, entity: &str) -> Result<spindle_kernel::Scan, StoreError> {
        self.0.scan(entity)
    }

    fn counts(&mut self, entities: &[String]) -> Result<BTreeMap<String, usize>, StoreError> {
        self.0.counts(entities)
    }

    fn reading(&mut self) -> Option<Arc<dyn Reading>> {
        self.0.reading()
    }

    fn attribute(&mut self, origin: Option<Origin>) {
        self.0.attribute(origin);
    }

    fn outcome(&mut self, outcome: &str) {
        self.0.outcome(outcome);
    }

    fn commit_events(&mut self, events: &[Event], scanned: &[Scanned]) -> Result<(), StoreError> {
        self.0.commit_events(events, scanned)
    }

    fn forget_events(&mut self) -> Result<(), StoreError> {
        self.0.forget_events()
    }

    fn events_after(&mut self, position: u64, limit: usize) -> Result<Vec<Committed>, StoreError> {
        self.0.events_after(position, limit)
    }

    fn events_head(&mut self) -> Result<u64, StoreError> {
        self.0.events_head()
    }

    fn events_id(&mut self) -> Result<u64, StoreError> {
        self.0.events_id()
    }

    fn keep_dead_letter(&mut self, letter: &DeadLetter) -> Result<u64, StoreError> {
        self.0.keep_dead_letter(letter)
    }

    fn dead_letters(&mut self) -> Result<Vec<(u64, DeadLetter)>, StoreError> {
        self.0.dead_letters()
    }

    fn take_dead_letter(&mut self, id: u64) -> Result<Option<DeadLetter>, StoreError> {
        self.0.take_dead_letter(id)
    }

    fn last_dead_letter_id(&mut self) -> Result<u64, StoreError> {
        self.0.last_dead_letter_id()
    }

    fn continue_dead_letter_ids(&mut self, after: u64) -> Result<(), StoreError> {
        self.0.continue_dead_letter_ids(after)
    }
}

struct Component {
    /// The store, with the rows each entity the component owns holds.
    store: Counted,
    /// Where the store keeps its state.
    kept: Kept,
    sender: Sender,
    position: u64,
    counters: Arc<Counters>,
    sink: Sink,
    /// Set while its store is lost: a reset could not empty it nor reopen it ([`Lost`]), so
    /// every read and write of it fails until a reset opens it again.
    lost: bool,
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
    /// The clock this host steps forward between commands, the one it reads
    /// ([`Host::stepping`]); `None` where it steps none.
    stepping: Option<Arc<ManualClock>>,
    placements: Arc<Placements>,
    components: BTreeMap<String, Component>,
    receivers: Vec<Receiver>,
    timer: Timer,
    periodic: BTreeMap<String, Periodic>,
    remote: Option<component::Remote>,
    /// The sequence the last command run was attributed with ([`Origin::sequence`]): the events
    /// each component's store committed merge back into the order published by it
    /// ([`Host::published`]). The host keeps no list of the events itself. Shared with its
    /// readers, which take the cursor after the last event published by it
    /// ([`Reader::published_head`]); `0` before the first command of the run.
    sequence: Arc<AtomicU64>,
    /// This run of the host, drawn at random when it opens: a [`Cursor`] marks the stores of the
    /// run it was taken in ([`Host::published_after`]).
    run: u64,
    /// Every event a store appended since the host started, by the command whose write carried it
    /// and the event: a reset keeps the counts, as it keeps the counters.
    emitted: Arc<Mutex<BTreeMap<(String, String), u64>>>,
    /// Answers the `external:` causes of every command any component runs, unless the world of
    /// the last reset carries its own answers.
    decider: Box<dyn Decider + Send>,
    /// The answers of the world the last [reset](Host::reset_to) loaded, if it carried any.
    answers: Option<Box<dyn Decider + Send>>,
    /// Mints the identities and generated values of every command any component runs.
    mint: Box<dyn Mint + Send>,
    /// The run's seed ([`Host::seeded`]): the mint a reset whose world has no mint of its own
    /// installs again from its first draw; `None` for random version 4 UUIDs.
    seed: Option<u64>,
    /// The routes of the event bindings this host may retry or escalate a failure of, by binding
    /// name: read from its own routes, never from an envelope, so a peer's frame does not decide
    /// the policy.
    answering: BTreeMap<String, Arc<Route>>,
    /// The qualified command whose run staged each envelope in flight between this host's own
    /// components, by the publishing component and the envelope's id, with what the command the
    /// envelope invokes records as its trigger ([`Provenance::Binding`]); taken when the envelope
    /// is invoked ([`Failed::caused_by`]). Empty in a run split over processes, whose bound
    /// commands record no trigger.
    staged_by: BTreeMap<(String, DeliveryId), (String, Option<Provenance>)>,
    /// Every escalation since the last [`Host::take_escalated`], at most [`ESCALATIONS_KEPT`],
    /// the oldest forgotten first: what a caller that sees only part of a [`Report`] reports.
    escalations: Vec<Escalated>,
    /// The components whose SQLite store held rows and recorded no specification when it opened
    /// ([`Host::unrecorded`]).
    unrecorded: Vec<String>,
    /// Every refused command whose dead letter its store could not keep since the last
    /// [`Host::take_unkept`], at most [`ESCALATIONS_KEPT`], the oldest forgotten first.
    unkept: Vec<RefusedCommand>,
    /// Whether the last reset changed a store's rows ([`Host::reset_changed_rows`]).
    reset_rows: bool,
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
    /// lacks; [`HostError::Unsupported`] when admission bound something this host does not run;
    /// [`HostError::Route`] when the specification uses a binding the channel does not carry, such
    /// as a bounded retry, or one that escalates into an event the component running its command
    /// does not publish. An `on_failure: retry` binding is retried ([`Retries::Retried`]); a
    /// binding that escalates a failure publishes its escalation event ([`Escalated`]).
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

    /// [`Host::deciding`], on a model and an admission made once and shared by every host opened
    /// from them: `ir` as it is shared, and `placements` as [`admit`] answered for `ir` on the
    /// environment. Nothing is cloned or admitted again; each host still opens stores of its own.
    ///
    /// # Errors
    /// As [`Host::deciding`], except that admission is not refused here: it was decided by the
    /// [`admit`] that answered `placements`.
    pub fn admitted(
        ir: Arc<EssIr>,
        placements: Arc<Placements>,
        clock: Arc<dyn Clock>,
        state: &Path,
        decider: Box<dyn Decider + Send>,
    ) -> Result<Self, HostError> {
        Self::assemble(
            ir,
            clock,
            state,
            placements,
            IN_PROCESS,
            Opening {
                decider: Some(decider),
                retries: Retries::Retried,
                stores: Stores::new(),
            },
        )
    }

    /// [`Host::new`], where a component whose state admission bound to an implementation this
    /// host has no store of its own for (neither `memory` nor `sqlite`) keeps it in the store
    /// `stores` holds for it, such as a browser's IndexedDB store; every `external:` cause is
    /// answered by `decider`, where one is given, as [`Host::deciding`] answers it.
    ///
    /// A reset empties a supplied store by removing every instance it holds, each with the version
    /// it holds; the store stays the one supplied. Where the store is busy at a step of the reset
    /// ([`StoreError::Busy`]), the reset answers that error as [`HostError::Store`], naming the
    /// component and the step, and keeps the store, the rows removed before that step removed.
    /// Where a step fails otherwise, the reset answers [`HostError::Reset`] (`restored: false`)
    /// and every read and write of that component fails from then on, naming why. A store left
    /// over in `stores`, of a component that does not run here or whose state is bound to
    /// `memory` or `sqlite`, is dropped.
    ///
    /// # Errors
    /// As [`Host::new`], and [`HostError::Unsupported`] for a component whose state is bound to
    /// an implementation `stores` holds no store for.
    pub fn supplied(
        ir: EssIr,
        environment: &Environment,
        clock: Arc<dyn Clock>,
        decider: Option<Box<dyn Decider + Send>>,
        stores: Stores,
    ) -> Result<Self, HostError> {
        let placements = admit(&ir, environment).map_err(HostError::Refused)?;
        Self::assemble(
            Arc::new(ir),
            clock,
            Path::new(""),
            Arc::new(placements),
            IN_PROCESS,
            Opening {
                decider,
                retries: Retries::Retried,
                stores,
            },
        )
    }

    /// [`Host::with_retries`], or [`Host::deciding_with_retries`] where `decider` is given, where a
    /// component whose state admission bound to an implementation this host has no store of its
    /// own for keeps it in the store `stores` holds for it, as [`Host::supplied`] keeps it: the
    /// PostgreSQL store `run` opens for each component bound to `postgres`.
    ///
    /// # Errors
    /// As [`Host::supplied`].
    pub fn with_stores(
        ir: EssIr,
        environment: &Environment,
        clock: Arc<dyn Clock>,
        state: &Path,
        decider: Option<Box<dyn Decider + Send>>,
        retries: Retries,
        stores: Stores,
    ) -> Result<Self, HostError> {
        let placements = admit(&ir, environment).map_err(HostError::Refused)?;
        Self::assemble(
            Arc::new(ir),
            clock,
            state,
            Arc::new(placements),
            IN_PROCESS,
            Opening {
                decider,
                retries,
                stores,
            },
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
        Self::assemble(
            Arc::new(ir),
            clock,
            state,
            Arc::new(placements),
            IN_PROCESS,
            Opening {
                decider,
                retries,
                stores: Stores::new(),
            },
        )
    }

    /// The host of the components in `placements`, their bindings carried on `carried`: every
    /// pair joined in this process when it is [`IN_PROCESS`], none otherwise.
    fn assemble(
        ir: Arc<EssIr>,
        clock: Arc<dyn Clock>,
        state: &Path,
        placements: Arc<Placements>,
        carried: &str,
        opening: Opening,
    ) -> Result<Self, HostError> {
        let Opening {
            decider,
            retries,
            mut stores,
        } = opening;
        let answers = decider.is_some();
        let router = match retries {
            Retries::Retried => Router::from_shared(&ir)?,
            Retries::Dropped => Router::from_shared(&ir)?.dropping_retries(),
        };
        let answering = router
            .routes()
            .iter()
            .filter(|route| route.retries() || route.escalation.is_some())
            .map(|route| (route.binding.clone(), Arc::new(route.clone())))
            .collect();
        let mut components = BTreeMap::new();
        let mut files: Option<Files> = None;
        for (name, bindings) in placements.iter() {
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
                    && !runs(&[spindle_kernel::UuidV4::IMPLEMENTATION, SEEDED_MINT])
                {
                    Some("this host mints identities as uuid-v4 or seeded only")
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
            let (store, kept): (Box<dyn Store>, Kept) =
                match bound(Capability::STORE_DURABLE).or_else(|| bound("store.ephemeral")) {
                    None | Some("memory") => (opened(None, &sink)?, Kept::Memory),
                    Some("sqlite") if cfg!(feature = "sqlite") => {
                        let file = state.join(format!("{name}.sqlite"));
                        // Every component's file is checked before any records, at the first.
                        let mut checked = match files.take() {
                            Some(checked) => checked,
                            None => Files::checked(&ir, state, &placements)?,
                        };
                        let store = checked.take(name, &file, &sink)?;
                        files = Some(checked);
                        (store, Kept::File(file))
                    }
                    Some(other) => match stores.remove(name) {
                        Some(store) => (
                            Box::new(CountingStore::new(Supplied(store), sink.clone())),
                            Kept::Supplied,
                        ),
                        None => {
                            return Err(HostError::Unsupported(format!(
                                "{name} has its state bound to {other}, which this host has no \
                                 store for"
                            )));
                        }
                    },
                };
            let sender = Sender::new(name, router.clone())?.with_telemetry(sink.clone());
            let store = Counted::new(store, owned(&ir, name), matches!(kept, Kept::Supplied));
            components.insert(
                name.clone(),
                Component {
                    store,
                    kept,
                    sender,
                    position: 0,
                    counters,
                    sink,
                    lost: false,
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
        let seeded = placements.iter().any(|(_, bindings)| {
            bindings.iter().any(|binding| {
                binding.capability.as_str() == Capability::IDENTITY_MINT
                    && binding.implementation.as_str() == SEEDED_MINT
            })
        });
        let host = Self {
            ir,
            clock,
            stepping: None,
            placements,
            components,
            receivers,
            timer,
            periodic,
            remote: None,
            sequence: Arc::new(AtomicU64::new(0)),
            run: Uuid::new_v4().as_u64_pair().0.max(1),
            emitted: Arc::default(),
            decider: decider.unwrap_or_else(|| Box::new(NoExternals)),
            answers: None,
            mint: Box::new(UuidV4),
            seed: None,
            answering,
            staged_by: BTreeMap::new(),
            escalations: Vec::new(),
            unrecorded: files.map(|files| files.unrecorded).unwrap_or_default(),
            unkept: Vec::new(),
            reset_rows: false,
        };
        if seeded { host.seeded(0) } else { Ok(host) }
    }

    /// This host, minting every identity and generated value from `seed` ([`Seeded`]) from its
    /// first command on, where it minted random version 4 UUIDs or, on an environment binding
    /// `identity.mint` to `seeded`, from seed 0: the same commands mint the same identities. It
    /// never mints an identity an instance in a store of the host already holds, as a store kept
    /// by an earlier run holds them ([`Seeded::avoiding_identities`]). The seed is the run's: a
    /// reset whose world carries a mint of its own mints from that until the next reset, and one
    /// whose world has none installs this seed again, from its first draw ([`Host::reset_to`]).
    ///
    /// # Errors
    /// [`HostError::Store`] when a store cannot be read for the identities it holds.
    pub fn seeded(mut self, seed: u64) -> Result<Self, HostError> {
        let held = self.held_identities()?;
        self.seed = Some(seed);
        self.mint = Box::new(Seeded::new(seed).avoiding_identities(held.iter()));
        Ok(self)
    }

    /// This host, reading `clock` and stepping it forward between commands
    /// ([`Host::step_clock`], a world's [`World::clock`]): every guard and view that reads `now`,
    /// every generated `Timestamp` and every command's time read it from then on. A
    /// [`Reader`] taken before this call reads the clock the host was opened with.
    #[must_use]
    pub fn stepping(mut self, clock: Arc<ManualClock>) -> Self {
        self.clock = Arc::clone(&clock) as Arc<dyn Clock>;
        self.stepping = Some(clock);
        self
    }

    /// The civil time the host's stepping clock stands at ([`Host::stepping`]).
    ///
    /// # Errors
    /// [`HostError::Clock`] where the host has no stepping clock.
    pub fn clock_at(&self) -> Result<SystemTime, HostError> {
        self.stepping
            .as_ref()
            .map(|clock| clock.wall())
            .ok_or_else(unstepped)
    }

    /// Moves the host's stepping clock ([`Host::stepping`]) forward to the civil time `at`, and
    /// answers where it stands then; an instant the clock stands at moves nothing. The commands
    /// run after it read `at`, and their sequences ([`spindle_kernel::Origin::sequence`]) never
    /// go back, so neither do event times and cursors.
    ///
    /// # Errors
    /// [`HostError::Clock`] where the host has no stepping clock, or `at` is before the clock's
    /// civil time; the clock does not move.
    pub fn step_clock(&mut self, at: SystemTime) -> Result<SystemTime, HostError> {
        let clock = self.stepping.as_ref().ok_or_else(unstepped)?;
        let by = at.duration_since(clock.wall()).map_err(|_| {
            HostError::Clock(format!(
                "the clock stands at {}; it moves forward only",
                rfc3339(clock.wall()).unwrap_or_default()
            ))
        })?;
        clock.advance(by);
        Ok(clock.wall())
    }

    /// Every identity an instance in the store of a running component holds, each entity read
    /// from the store of the component that owns it.
    fn held_identities(&mut self) -> Result<Vec<Value>, HostError> {
        let mut held = Vec::new();
        for (name, component) in &mut self.components {
            for entity in owned(&self.ir, name) {
                let scan = component.store.scan(&entity)?;
                held.extend(scan.rows.into_iter().map(|(identity, _)| identity));
            }
        }
        Ok(held)
    }

    /// The compiled specification the host runs.
    pub fn ir(&self) -> &EssIr {
        &self.ir
    }

    /// Writes `row` as the instance of `entity` (its qualified name) with `identity` into the
    /// store of the component that owns the entity's domain, as it rests, with no command: the
    /// state a scenario arranges. Nothing is published and no binding runs. The row is checked
    /// against the entity's invariants first ([`spindle_kernel::at_rest`]). The identity is keyed
    /// in the one spelling a command's is ([`spindle_kernel::one_spelling`]): `1.0` as `1`.
    ///
    /// # Errors
    /// [`HostError::Unsupported`] for an entity the model does not declare or no running
    /// component owns; [`HostError::Execute`] when the row breaks an invariant of its entity;
    /// [`HostError::Store`], a [`StoreError::Conflict`], when the instance already exists, and
    /// any other store failure.
    pub fn establish(&mut self, entity: &str, identity: &Value, row: Row) -> Result<(), HostError> {
        // Keyed in the one spelling a command's identity is, so a command finds the row by any
        // spelling of a number ESS reads as one value (`spindle_kernel::one_spelling`).
        let identity = spindle_kernel::one_spelling(identity);
        let owner = self.resting(entity, &identity, &row)?;
        store_of(&mut self.components, &owner)?.commit(&Change {
            entity: entity.to_owned(),
            identity,
            expected: None,
            row,
            events: Vec::new(),
            scanned: Vec::new(),
        })?;
        Ok(())
    }

    /// The identity of every instance of `entity` (its qualified name) the store of the component
    /// that owns it holds, in the order the store scans them ([`spindle_kernel::Scan`]: by the
    /// identity's canonical JSON text).
    ///
    /// # Errors
    /// [`HostError::NoEntity`] where the model declares no `entity`,
    /// [`HostError::Unsupported`] where no running component owns it, and [`HostError::Store`]
    /// where the store cannot be read.
    pub fn identities(&mut self, entity: &str) -> Result<Vec<Value>, HostError> {
        let owner = holding(&self.ir, entity)?;
        Ok(store_of(&mut self.components, &owner)?
            .scan(entity)?
            .rows
            .into_iter()
            .map(|(identity, _)| identity)
            .collect())
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

    /// Every escalation since the last call, in the order escalated ([`Escalated`]), each returned
    /// once; at most [`ESCALATIONS_KEPT`], the oldest forgotten first. Each is in the [`Report`]
    /// of the call that made it too; this is for a caller handed only part of that report.
    pub fn take_escalated(&mut self) -> Vec<Escalated> {
        std::mem::take(&mut self.escalations)
    }

    /// Every refused command since the last call whose dead letter its store could not keep
    /// ([`RefusedCommand::not_kept`]), in the order refused, each returned once; at most
    /// [`ESCALATIONS_KEPT`], the oldest forgotten first. Each is in the [`Report`] of the call
    /// that ran it too; this is for a caller handed only the commands that did not execute, such
    /// as a served run, which prints it.
    pub fn take_unkept(&mut self) -> Vec<RefusedCommand> {
        std::mem::take(&mut self.unkept)
    }

    /// Every dead letter the components' stores keep and no replay took ([`Undelivered`]),
    /// component by component in name order, each component's by id: each command a binding or a
    /// tick invoked whose last attempt was refused or did not execute, and that its binding did
    /// not escalate (`--drop-retries`'s drops included; a binding whose input was unmet ran no
    /// attempt and keeps none). Read from the stores, so a store in SQLite or PostgreSQL answers
    /// those of an earlier process too; a reset forgets them with the events. A store that keeps
    /// no dead letters answers none.
    ///
    /// # Errors
    /// [`HostError::Store`] when a store cannot be read.
    pub fn dead_letters(&mut self) -> Result<Vec<Undelivered>, HostError> {
        let mut letters = Vec::new();
        for (name, component) in &mut self.components {
            letters.extend(
                component
                    .store
                    .dead_letters()?
                    .into_iter()
                    .map(|(id, letter)| Undelivered {
                        component: name.clone(),
                        id,
                        letter,
                    }),
            );
        }
        Ok(letters)
    }

    /// Runs the dead letter `id` of `component` once more and everything its events lead to, and
    /// answers what ran ([`Report`]), as [`Host::replayed`] answers it without telling the
    /// replay's own run apart. The store records the letter taken before the command runs
    /// ([`Store::take_dead_letter`]), so it is replayed at most once, after a restart too. The
    /// command runs with the input it was kept with, once, without its binding's retries or
    /// escalation, attributed to its binding as its first run was ([`Provenance::Binding`] with no
    /// trigger, or [`Provenance::Tick`] for a periodic binding's); a replay that is refused or
    /// does not execute is kept as a new dead letter. `None`, and nothing runs, where no dead
    /// letter with this id waits in that component: never kept, replayed already, forgotten by a
    /// reset, or no component of that name.
    ///
    /// # Errors
    /// [`HostError::Store`] when the store cannot be read or record the take, and as
    /// [`Host::submit`] for what the command leads to.
    pub fn replay(&mut self, component: &str, id: u64) -> Result<Option<Report>, HostError> {
        Ok(self
            .replayed(component, id)?
            .map(|replayed| replayed.report))
    }

    /// [`Host::replay`], answering the replay's own run ([`Replayed::own`]) apart from what
    /// settled after it: taken from the report before anything else is delivered, so an envelope
    /// a peer process sent meanwhile, settled in the same call, is never taken for it.
    ///
    /// # Errors
    /// As [`Host::replay`].
    pub fn replayed(&mut self, component: &str, id: u64) -> Result<Option<Replayed>, HostError> {
        let Some(held) = self.components.get_mut(component) else {
            return Ok(None);
        };
        let Some(letter) = held.store.take_dead_letter(id)? else {
            return Ok(None);
        };
        let binding = letter.binding;
        let cause = match letter.event {
            Some(_) => Cause::Delivered { binding },
            None => Cause::Tick { binding },
        };
        let mut report = Report::default();
        self.attempt(
            component,
            &letter.command,
            &letter.input,
            cause,
            None,
            None,
            &mut report,
        )?;
        let own = if let Some(ran) = report.ran.first() {
            Attempted::Ran(ran.clone())
        } else if let Some(refused) = report.refused.first() {
            Attempted::Refused(refused.clone())
        } else if let Some(failed) = report.failed.first() {
            Attempted::Failed(failed.clone())
        } else {
            return Err(HostError::Store(StoreError::Backend(format!(
                "the replay of dead letter {id} of {component} reported no run of its command"
            ))));
        };
        self.settle(&mut report, &mut Deliveries::default())?;
        Ok(Some(Replayed { own, report }))
    }

    /// Every event a command committed since its component's store was last
    /// [reset](Self::reset), in the order published: a command's own events in emit order, then
    /// those of the commands they led to. A refused command publishes none. An event no write of
    /// a row carries (one of an outcome without a subject, that preserves its subject, or of a set
    /// outcome that selected no row) is committed to its store on its own
    /// ([`Store::commit_events`]).
    ///
    /// Read from each component's store ([`Store::events_after`]), not held by the host: a store
    /// in SQLite or PostgreSQL answers the events of an earlier process too. The stores' events
    /// merge by the sequence the host attributed each command with ([`Origin::sequence`]), its
    /// civil time in microseconds or one past the last, whichever is later; an event its store
    /// holds no command for is answered with an empty `command`.
    ///
    /// # Errors
    /// [`HostError::Store`] when a store cannot be read, or does not read back its events.
    pub fn published(&mut self) -> Result<Vec<Published>, HostError> {
        let mut committed = Vec::with_capacity(self.components.len());
        for (name, component) in &mut self.components {
            committed.push((name.clone(), component.store.events_after(0, usize::MAX)?));
        }
        Ok(merged(committed))
    }

    /// The events published after `after`, at most `limit`, in the order [`Host::published`]
    /// lists them, and the cursor after the last of them ([`Page`]); with `after` at the end,
    /// none and the same cursor, or, for a cursor taken before the host was opened again, a
    /// cursor of this run at the same place. Each component's store is read only after the
    /// position the cursor marks for it ([`Store::events_after`]), so a read with the cursor at
    /// the end reads no event, however many the stores hold. Where a reset replaced a store since
    /// the cursor was taken, it is read from its start once. A cursor taken before the host was
    /// opened again reads each store whose log is the one it marks ([`Store::events_id`]) after
    /// its position, whatever sequences this run's commands take, so on a fixed clock too; a
    /// store of another log (a new memory store, a SQLite file a reset created again) is read
    /// from its start. The cursor answered names each store's log as read after its events, so
    /// a mark past a store's start always names one. A cursor of an earlier release names no
    /// log, and is read as that release read it: every store from its start, the events up to
    /// its last skipped by their commands' sequences. A cursor taken before `state move` names
    /// the SQLite file's log, which the PostgreSQL copy does not keep: it reads the moved events
    /// again.
    ///
    /// # Errors
    /// [`HostError::Cursor`] where `after` names a component this host does not run, and
    /// [`HostError::Store`] where a store cannot be read.
    pub fn published_after(&mut self, after: &Cursor, limit: usize) -> Result<Page, HostError> {
        let stores = self
            .components
            .iter()
            .map(|(name, component)| (name.clone(), component.store.generation()))
            .collect::<Vec<_>>();
        let components = std::cell::RefCell::new(&mut self.components);
        range::range(
            self.run,
            &stores,
            after,
            limit,
            |name, position, wanted| {
                Ok(store_of(&mut components.borrow_mut(), name)?.events_after(position, wanted)?)
            },
            |name| Ok(store_of(&mut components.borrow_mut(), name)?.events_id()?),
        )
    }

    /// The cursor after the last event published, read without reading any event: each
    /// component's store answers only the position it is at ([`Store::events_head`]). A read after
    /// it ([`Host::published_after`]) answers every event published after this call, and none
    /// before. Read in a later run (after a restart), it answers what was committed after each
    /// store's head, whatever sequences that run's commands take, and every event of a store
    /// whose log is not the one it marks ([`Store::events_id`]). Where a reset in this run
    /// replaced a store since, that store is read from its start once, and the events of the
    /// commands run before the last one this run ran, or, before the first, before the civil
    /// time of the call, are skipped.
    ///
    /// # Errors
    /// [`HostError::Store`] where a store cannot be read.
    pub fn published_head(&mut self) -> Result<Cursor, HostError> {
        let sequence = head_sequence(self.sequence.load(Ordering::SeqCst), self.clock.wall());
        let mut stores = Vec::with_capacity(self.components.len());
        for (name, component) in &mut self.components {
            let generation = component.store.generation();
            // The head before the log's id: a head past the start never comes with log 0.
            let head = component.store.events_head()?;
            stores.push((name.clone(), generation, component.store.events_id()?, head));
        }
        Ok(range::head(self.run, &stores, sequence))
    }

    /// The committed events of the instance of `entity` (a qualified entity name) with
    /// `identity` after `after`, oldest first, at most `limit` write by write, and the cursor
    /// after them ([`History`]): every event of every write that wrote the instance, a group of
    /// writes included (`affects:`, a set outcome), each with its position, its command's time,
    /// the command, its outcome and its cause ([`Revision`]). Read from the store of the
    /// component that owns the entity ([`Store::events_after`]), filtered by the instances each
    /// write names ([`spindle_kernel::Committed::instances`]): a write a release before 0.23.0
    /// stored names none, so it is in no history. An instance no command wrote reads empty.
    ///
    /// # Errors
    /// [`HostError::NoEntity`] where the model declares no `entity`, and [`HostError::Store`]
    /// where the store cannot be read.
    pub fn history(
        &mut self,
        entity: &str,
        identity: &Value,
        after: &HistoryCursor,
        limit: usize,
    ) -> Result<History, HostError> {
        let owner = holding(&self.ir, entity)?;
        let generation = self
            .components
            .get(&owner)
            .map(|component| component.store.generation())
            .ok_or_else(|| HostError::Unsupported(format!("{owner} is not running")))?;
        let store = store_of(&mut self.components, &owner)?;
        range::history(
            (self.run, generation),
            after,
            (entity, identity),
            limit,
            civil,
            |position, wanted| Ok(store.events_after(position, wanted)?),
        )
    }

    /// How many events each command appended since the host started, by command and event,
    /// counted where a component's store commits them, as its `event.<Event>.appended` counter
    /// is: a set outcome that publishes an event once per changed row counts each row's, and an
    /// event no write carries, one of an outcome without a subject or of a set outcome that
    /// selected no row, counts nothing. Its counts add up to each event's appended counter; a
    /// [reset](Self::reset) keeps them.
    pub fn emitted(&self) -> BTreeMap<(String, String), u64> {
        self.emitted
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// What reads this host's committed state from any thread while this one runs commands
    /// ([`Reader`]): its views, where a store gives a reading handle, its events published, its
    /// rows, counters and events appended. Every reader of a host reads the same state, through
    /// every reset.
    pub fn reader(&self) -> Reader {
        Reader {
            ir: Arc::clone(&self.ir),
            clock: Arc::clone(&self.clock),
            placements: Arc::clone(&self.placements),
            components: Arc::new(
                self.components
                    .iter()
                    .map(|(name, component)| {
                        (
                            name.clone(),
                            reader::Read {
                                counters: Arc::clone(&component.counters),
                                shared: component.store.shared(),
                            },
                        )
                    })
                    .collect(),
            ),
            emitted: Arc::clone(&self.emitted),
            run: self.run,
            sequence: Arc::clone(&self.sequence),
            stop: None,
        }
    }

    /// Empties every component's store and forgets every published event; returns the
    /// components whose stores were emptied, by name.
    ///
    /// A store in memory is replaced by an empty one. A store in SQLite is closed, its file and
    /// the SQLite side files beside it moved aside, and an empty one opened at the same path; the
    /// moved files are removed only once the empty store is open. A store the host was given is
    /// emptied in place and records that its events so far are forgotten
    /// ([`Store::forget_events`]), so a host opened on it later does not read them either.
    /// The dead letters are forgotten with the events, and their ids are not given again: an
    /// empty store continues after the last id the store it replaces answered
    /// ([`Store::continue_dead_letter_ids`]), so a replay naming a forgotten one runs nothing.
    /// Periodic bindings keep running, and the counters keep their counts. The answers and
    /// the mint of the world an earlier [`Host::reset_to`] loaded are dropped; the run's seed
    /// ([`Host::seeded`]) mints again from its first draw.
    ///
    /// # Errors
    /// [`HostError::Reset`] when a component's files cannot be moved aside or the empty store
    /// cannot be opened. Its files are moved back and its store reopened as it was
    /// (`restored: true`); where even that fails, every read and write of that component fails
    /// from then on, naming why (`restored: false`). A store the host was given that is busy
    /// ([`StoreError::Busy`]) while it is emptied or records that its events are forgotten is
    /// kept, the rows removed before then removed, and the reset answers
    /// [`HostError::Store`] naming the component and the step; whether it changed rows before
    /// then is [`Host::reset_changed_rows`]. One that fails otherwise is lost alike
    /// (`restored: false`). The components emptied before it stay empty, their events forgotten
    /// with them; the others keep theirs.
    pub fn reset(&mut self) -> Result<Vec<String>, HostError> {
        self.reset_to(World::default())
    }

    /// [`Host::reset`], then `world` loaded: each of its rows written where its entity rests, as
    /// [`Host::establish`] writes it, every external cause answered by its answers until the next
    /// reset, and every identity and generated value minted by its mint until then. A world
    /// without answers leaves causes to the decider the host was opened with, and one without a
    /// mint mints from the run's seed again, from its first draw ([`Host::seeded`]), or random
    /// version 4 UUIDs on a host given no seed. A seeded mint never answers a UUID a row's identity
    /// holds or ends in, nor one whose low 53 bits are a row's `Integer` identity
    /// ([`Seeded::avoiding_identities`]).
    ///
    /// Every row is checked before any store is emptied: its entity declared and owned by a
    /// running component, its invariants held, its identity one its store can key (read from the
    /// store as it is), and no two rows one instance. A world's [`World::clock`] is checked
    /// before them. The answers and the mint are taken, and the host's stepping clock moved
    /// forward to the world's clock ([`Host::step_clock`]), only once every row is written.
    ///
    /// # Errors
    /// [`HostError::Clock`] for a world that carries a clock on a host without a stepping clock
    /// ([`Host::stepping`]), and [`HostError::World`] for one whose clock is before the
    /// clock's civil time or a row that does not pass, each before anything changed, but
    /// [`HostError::Store`] where a store was busy ([`StoreError::Busy`]) while a row was read
    /// from it, also before anything changed; then as [`Host::reset`], and as
    /// [`Host::establish`] for a row a store did not take, the rows before it written and the
    /// answers, mint and clock of the last world kept.
    pub fn reset_to(&mut self, world: World) -> Result<Vec<String>, HostError> {
        self.reset_rows = false;
        if let Some(at) = world.clock {
            let stands = self.clock_at()?;
            if at < stands {
                return Err(HostError::World(format!(
                    "clock: the clock stands at {}; a world moves it forward only",
                    rfc3339(stands).unwrap_or_default()
                )));
            }
        }
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
                .map_err(|error| match error {
                    // A busy store refused the read before anything changed: the world is not
                    // at fault, and the reset can be sent again
                    // (`story:reset-on-a-busy-store-keeps-the-store`).
                    StoreError::Busy(detail) => HostError::Store(StoreError::Busy(format!(
                        "resetting {owner}: reading row {index}, {} {}: {detail}",
                        seed.entity, seed.identity
                    ))),
                    error => HostError::World(format!(
                        "row {index}: {owner}'s store cannot hold {} {}: {error}",
                        seed.entity, seed.identity
                    )),
                })?;
            owners.push(owner);
        }
        let held = world.mint.or(self.seed).map(|seed| {
            Seeded::new(seed).avoiding_identities(world.rows.iter().map(|seed| &seed.identity))
        });
        for component in self.components.values_mut() {
            component.store.hold();
        }
        let loaded = self.load(world.rows, owners);
        // Its readers read the emptied stores, and the world in them, only now: until here they
        // read every store as the last command left it.
        for component in self.components.values_mut() {
            component.store.publish();
        }
        let emptied = loaded?;
        if let Some(at) = world.clock {
            self.step_clock(at)?;
        }
        self.answers = world.answers;
        self.mint = held.map_or_else(
            || Box::new(UuidV4) as Box<dyn Mint + Send>,
            |seeded| Box::new(seeded),
        );
        Ok(emptied)
    }

    /// Empties every store, then writes each row of a world into the store of its owner. A row
    /// whose outcome is unknown ([`StoreError::Unknown`]) may have landed, so it counts as written
    /// ([`Host::reset_changed_rows`]); the store is kept.
    fn load(&mut self, rows: Vec<Seed>, owners: Vec<String>) -> Result<Vec<String>, HostError> {
        let emptied = self.empty()?;
        for (index, (seed, owner)) in rows.into_iter().zip(owners).enumerate() {
            let step = format!("writing row {index}, {} {}", seed.entity, seed.identity);
            store_of(&mut self.components, &owner)?
                .commit(&Change {
                    entity: seed.entity,
                    identity: seed.identity,
                    expected: None,
                    row: seed.row,
                    events: Vec::new(),
                    scanned: Vec::new(),
                })
                .map_err(|error| match error {
                    // A busy write names the component and the step, as every busy step of a
                    // reset does (`story:reset-on-a-busy-store-keeps-the-store`, D2 A).
                    StoreError::Busy(detail) => {
                        StoreError::Busy(format!("resetting {owner}: {step}: {detail}"))
                    }
                    error => {
                        if matches!(error, StoreError::Unknown(_)) {
                            self.reset_rows = true;
                        }
                        error
                    }
                })?;
            self.reset_rows = true;
        }
        Ok(emptied)
    }

    fn empty(&mut self) -> Result<Vec<String>, HostError> {
        let mut emptied = Vec::new();
        for (name, component) in &mut self.components {
            if matches!(component.kept, Kept::Supplied) {
                let forgotten = removed_all(&self.ir, &mut component.store, &mut self.reset_rows)
                    .and_then(|()| {
                        component
                            .store
                            .forget_events()
                            .map_err(|error| ("forgetting its events".to_owned(), error))
                    });
                if let Err((step, error)) = forgotten {
                    // A busy store refused the step before it began to commit, so it holds what
                    // it held before that step: it is kept, and the reset can be sent again
                    // (`story:reset-on-a-busy-store-keeps-the-store`). Any other error loses it.
                    let error = match error {
                        StoreError::Busy(detail) => {
                            return Err(HostError::Store(StoreError::Busy(format!(
                                "resetting {name}: {step}: {detail}"
                            ))));
                        }
                        error => format!("{step}: {error}"),
                    };
                    component.store.replace(Box::new(Lost(format!(
                        "{name}'s store was lost in a reset: {error}"
                    ))));
                    component.lost = true;
                    return Err(HostError::Reset {
                        component: name.clone(),
                        error,
                        restored: false,
                    });
                }
            } else if let Kept::File(file) = &component.kept {
                let file = file.clone();
                let after = last_dead_letter_id(name, component)?;
                let closed = component
                    .store
                    .replace(Box::new(Lost(format!("{name}'s store is being reset"))));
                drop(closed);
                match emptied_file(&file, &self.ir, name, &component.sink, after) {
                    Ok(store) => {
                        component.store.replace(store);
                        component.lost = false;
                        self.reset_rows = true;
                    }
                    Err(error) => {
                        let restored = match opened(Some(&file), &component.sink) {
                            Ok(store) => {
                                component.store.replace(store);
                                component.lost = false;
                                true
                            }
                            Err(again) => {
                                component.store.replace(Box::new(Lost(format!(
                                    "{name}'s store was lost in a reset: {error}; reopening {} \
                                     failed: {again}",
                                    file.display()
                                ))));
                                component.lost = true;
                                self.reset_rows = true;
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
                let after = last_dead_letter_id(name, component)?;
                let mut store = opened(None, &component.sink)?;
                store
                    .continue_dead_letter_ids(after)
                    .map_err(|error| HostError::Reset {
                        component: name.clone(),
                        error: format!("continuing the dead letter ids after {after}: {error}"),
                        restored: true,
                    })?;
                component.store.replace(store);
                component.lost = false;
                self.reset_rows = true;
            }
            emptied.push(name.clone());
        }
        Ok(emptied)
    }

    /// What admission bound for each component.
    pub fn placements(&self) -> &Placements {
        &self.placements
    }

    /// Whether the last [`Host::reset_to`] changed the rows of any store: removed or wrote a row,
    /// or emptied or lost a store kept in a file or in memory. `false` after a reset refused
    /// before it emptied anything, as for its world or its clock, and after one whose stores
    /// refused its first removal, as a store whose lease another process took refuses every write,
    /// or was busy at it; forgetting a store's events alone changes no row. A removal or a world
    /// row whose outcome is unknown ([`StoreError::Unknown`]) may have landed, so it counts as one
    /// that did: `true` (`story:unknown-commit-answered-as-unknown`). A reset a busy store held
    /// ([`StoreError::Busy`]) keeps that store whichever this answers: `false`, nothing was
    /// reset; `true`, the rows changed before it stay changed and the reset can be sent again
    /// (`story:reset-on-a-busy-store-keeps-the-store`). Where it is `false`, a caller that
    /// removed something beside the stores for the reset puts it back
    /// (`story:seed-record-after-a-component-is-removed`).
    pub fn reset_changed_rows(&self) -> bool {
        self.reset_rows
    }

    /// The components whose SQLite store held rows and recorded no specification when the host
    /// opened it, as a store an earlier release wrote: each now records this specification,
    /// recorded without a check (`story:spec-change-over-stored-rows`, D2).
    pub fn unrecorded(&self) -> &[String] {
        &self.unrecorded
    }

    /// The counters of `component`, laid out by its catalogue; `None` when no such component
    /// runs here.
    pub fn counters(&self, component: &str) -> Option<&Counters> {
        self.components
            .get(component)
            .map(|component| component.counters.as_ref())
    }

    /// The rows each entity holds now, by entity name in the IR's order, as the store of the
    /// component that owns its domain holds them; an entity no component here owns is left out.
    /// A reset leaves the counters as they are and empties these.
    ///
    /// They are kept, not read: each store's rows are counted once when it is opened (one read of
    /// a SQLite store's feed), then moved by each write that lands, so reading them reads no store
    /// ([`Store::counts`]). Only where that count failed are they counted again when read.
    ///
    /// They see only the writes this host lands: a store has one writer, as a state directory has
    /// one Spindle process and PostgreSQL tables the one `run` that holds their lease
    /// (`story:several-writers-one-postgres`). A second writer on the same store, another host on
    /// the same SQLite files or on a shared supplied store, moves its rows without moving these,
    /// until the store is next opened.
    ///
    /// # Errors
    /// [`HostError::Store`] when the rows had to be counted and a store could not be read, as a
    /// store a failed reset lost cannot.
    pub fn rows(&mut self) -> Result<Vec<(String, usize)>, HostError> {
        let ir = Arc::clone(&self.ir);
        let mut rows = Vec::new();
        for entity in ir.entities().values() {
            let Some(component) = ir
                .components()
                .values()
                .find(|component| component.owns.contains(&entity.domain))
                .and_then(|owner| self.components.get_mut(owner.name.as_str()))
            else {
                continue;
            };
            let name = entity.name.to_string();
            let held = component
                .store
                .rows()?
                .get(&name)
                .copied()
                .unwrap_or_default();
            rows.push((name, held));
        }
        Ok(rows)
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
        self.submit_refusing(command, input, actor, caller, &BTreeMap::new())
    }

    /// [`Host::submit_as`], refusing each outcome of `command` that `refused` names, by outcome
    /// name with what the caller does not serve of it, where the model selects it:
    /// [`HostError::Execute`] with [`spindle_kernel::ExecuteError::Unsupported`] naming the
    /// outcome, and nothing is written ([`spindle_kernel::execute_refusing`]). What its events lead
    /// to is not refused.
    ///
    /// # Errors
    /// As [`Host::submit`].
    pub fn submit_refusing(
        &mut self,
        command: &str,
        input: Map<String, Value>,
        actor: Option<&str>,
        caller: Option<&Map<String, Value>>,
        refused: &BTreeMap<String, String>,
    ) -> Result<Report, HostError> {
        let cause = Provenance::Request(spindle_kernel::Sender {
            actor: actor.map(str::to_owned),
            attributes: caller.cloned().unwrap_or_default(),
            request: None,
        });
        self.submit_caused(command, input, actor, caller, refused, cause)
    }

    /// [`Host::submit_refusing`], each event the command commits recording `cause` as what caused
    /// it ([`Origin::cause`]): the request or seed entry that sent it, with its caller and its
    /// request id, which should name `actor` and `caller` as sent. Each command its events lead to
    /// records the binding that carried the event, and `cause` as what caused the command that
    /// published it ([`Provenance::Binding`]).
    ///
    /// # Errors
    /// As [`Host::submit`].
    pub fn submit_caused(
        &mut self,
        command: &str,
        input: Map<String, Value>,
        actor: Option<&str>,
        caller: Option<&Map<String, Value>>,
        refused: &BTreeMap<String, String>,
        cause: Provenance,
    ) -> Result<Report, HostError> {
        let component = accepting(&self.ir, command)
            .ok_or_else(|| HostError::NotAccepted(command.to_owned()))?;
        let mut report = Report::default();
        match self.execute(&component, command, &input, actor, caller, refused, cause)? {
            Ok(Executed {
                outcome,
                error: Some(error),
                ..
            }) => {
                return Err(HostError::CommandRefused {
                    component,
                    command: command.to_owned(),
                    outcome,
                    error: Box::new(error),
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
        self.unmet(&component, command, &mut report);
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
    /// The row, and every row a reference names, is read by its identity in the one spelling a
    /// command keys it by ([`spindle_kernel::one_spelling`]): `1.0` and `1` read one row, which
    /// answers its identity so.
    ///
    /// The row is read from the store of the component that owns the view's source. A view's
    /// `consistency` changes nothing here: every component's store is in this process, so an
    /// `eventual` view reads the same store a `read_your_writes` one does.
    ///
    /// A field read through a reference (`field: {related: …}`) answers the row the reference
    /// names as it stands now, loaded by its identity ([`Store::load`]) from the store of the
    /// component that owns its entity, that component's or another's; it is left out where the
    /// reference is empty or names no row, a removed one included. A filter that reads through a
    /// reference reads the same row, loaded before the filter is evaluated.
    ///
    /// A view that declares parameters is read as [`Host::list_with`] reads it with no parameter
    /// values, so a required parameter is refused as left out. A computed field (`expr:`) is
    /// answered as [`Host::list_with`] answers it.
    ///
    /// # Errors
    /// [`HostError::NoView`], [`HostError::Unsupported`] for a view [`Host::list`] cannot answer,
    /// for a computed field ESS determines no value of for the row, and for an aggregate view,
    /// whose rows are groups and none an instance's,
    /// [`HostError::Parameters`] for a view with a required parameter, and [`HostError::Store`]
    /// when the store cannot be read.
    pub fn view(
        &mut self,
        view: &str,
        identity: &Value,
    ) -> Result<Option<Map<String, Value>>, HostError> {
        let (declared, owner) = readable(&self.ir, view)?;
        if declared.aggregation.is_some() {
            return Err(HostError::Unsupported(format!(
                "view {view} declares an aggregation: its rows are groups, read as a list, and \
                 none is the row of one identity"
            )));
        }
        let parameters = crate::view::parameters(&self.ir, declared, &Map::new())
            .map_err(|detail| HostError::Parameters {
                view: view.to_owned(),
                detail,
            })?
            .facts;
        let entity = self.ir.entity(&declared.source).name.to_string();
        // Read by the one spelling a command keys the row by: `1.0` and `1` name one row.
        let identity = &spindle_kernel::one_spelling(identity);
        let stored = store_of(&mut self.components, &owner)?.load(&entity, identity)?;
        let Some(stored) = stored else {
            return Ok(None);
        };
        let now = read_instant(declared, self.clock.as_ref());
        let mut far = Far::default();
        let referenced = far.referenced(
            &self.ir,
            declared,
            &mut loading_from(&mut self.components),
            identity,
            &stored.row,
        )?;
        if !held(
            &self.ir,
            declared,
            identity,
            &stored.row,
            &referenced,
            &parameters,
            now,
        )? {
            return Ok(None);
        }
        crate::view::project(&self.ir, declared, identity, &stored.row, &referenced)
            .map(Some)
            .map_err(|detail| HostError::Unsupported(format!("view {view}: {detail}")))
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
    /// The filter is evaluated by ESS, as the compiler parsed it, over the row, the parameters
    /// (each read as `param.<name>`) and the rows its references name (each field read as
    /// `<via>.<field>`). The filters read are those whose every leaf is one of:
    ///
    /// - a field compared with a literal of its type by `==` or `!=`, or by `<`, `<=`, `>` or `>=`
    ///   where ESS orders the type (a number, or text other than a `Duration`):
    ///   `archived == false`, `priority >= 3`, `state == Open`, `person_id.state == Active`; a
    ///   `Timestamp` field ordered against `now`, `now - 1h` or `now + 5m` (`issued_at < now`),
    ///   read against this host's clock, taken once per read and only for such a filter. A field
    ///   is an observable field of the source (the state and the identity among them), or one of
    ///   the row a reference names, read through it;
    /// - such a field listed `in:` or `none_of:` literals of its type;
    /// - such a field compared with `==` or `!=` to a parameter or another such field
    ///   (`book_id == param.id`, `person_id.name == holder`), or ordered against one where ESS
    ///   orders both and they are of one scalar kind (`started_at > person_id.joined_at`);
    /// - `defined(param.<name>)` or `defined(<field>)`;
    /// - a parameter compared with a literal of its type (`param.label == ""`,
    ///   `param.direction == all`), the count of a list parameter among them
    ///   (`param.kinds.count == 0`);
    /// - a text field, a text parameter or an element of a list parameter tested by
    ///   `equals_ignore_case` or `in_ignore_case` against text literals, alone or under `trimmed`;
    ///
    /// leaves combined by `any`, `all` and `not`, or under `exists` or `forall` over a list
    /// parameter, a list field of the row, a map field of the row (its values in key order, each
    /// of the map's value type, a `Timestamp` ordered by its instant) or a list member of an outer
    /// list field's element (`exists w in g.words` inside `exists g in slots`), a list field's
    /// element (and a member of it, `g.id == param.group`) read as a field is and also compared
    /// with `==` or `!=` to an outer binder. A filter unknown for a row, such as `==` with an
    /// `Optional` parameter left out, refuses the read, as ESS's interpreter refuses it;
    /// one unknown only because a path it reads through a reference reads no row (the reference
    /// is empty or names no row) does not hold the row, as the interpreter hides it.
    /// Read from the same store as [`Host::view`], for either consistency. A view that reads
    /// through a reference, by a field or by its filter, loads each row a reference names once per
    /// read, by its identity, however many rows name it, and never scans the referenced entity.
    ///
    /// A view declaring aggregate fields (`aggregate: {count: {}}`, `sum`, `avg` and the rest,
    /// with or without `group_by`, each with or without its own `where:`) answers the rows ESS's
    /// interpreter answers over the instances its filter keeps ([`crate::view::aggregate`]): one
    /// per group in the order of the group keys, and without `group_by` one row even over no
    /// instance. It reads the same one scan of the source's rows as any view.
    ///
    /// A computed field (`expr:`) answers, in each row, what ESS's evaluator computes over the
    /// source row's own fields, as ESS's interpreter answers it ([`crate::view::project`]):
    /// `null` where an `Optional` operand is absent or a divisor is zero outside `coalesce`, an
    /// `Integer` as a JSON integer, any other value as ESS writes it.
    ///
    /// A view declaring `paging:` answers the page its two paging parameters ask for, as ESS
    /// 0.99.1's interpreter cuts it (`story:view-paging`): with neither given, every row; with
    /// both, `size` rows after the first `(page - first_page) * size` of the rows in their order.
    /// Both may be left out whatever type they are declared at. [`Host::listed_with`] also answers
    /// how many rows the filter keeps, where the view declares `total: true`.
    ///
    /// # Errors
    /// [`HostError::NoView`]; [`HostError::Unsupported`], naming the construct, for a view that
    /// declares a default row for an absent identity,
    /// an aggregation beside a field read through a reference, or any other filter, a measure's
    /// `where:` included; [`HostError::Parameters`], naming the parameter, for a value the view
    /// does not declare, a required one left out or one not of its declared type or newtype, and
    /// for a page given without its size or a size without its page, a negative one, a page below
    /// `first_page` or an offset past what a read can count, before anything is read;
    /// [`HostError::Unsupported`] for a filter unknown for a row, for an
    /// aggregate ESS determines no value of, and for a computed field ESS determines no value of
    /// for a row (one unknown, absent where the field is required, or with no spelling of its
    /// type); and [`HostError::Store`] when the store cannot be read.
    pub fn list_with(
        &mut self,
        view: &str,
        parameters: &Map<String, Value>,
    ) -> Result<Vec<Map<String, Value>>, HostError> {
        self.listed_with(view, parameters).map(|listed| listed.rows)
    }

    /// The rows [`Host::list_with`] answers, and, where the view declares
    /// `paging: {total: true}`, how many rows its filter keeps, counted before the page is cut, as
    /// ESS's interpreter counts them ([`Listed`]).
    ///
    /// # Errors
    /// As [`Host::list_with`].
    pub fn listed_with(
        &mut self,
        view: &str,
        parameters: &Map<String, Value>,
    ) -> Result<Listed, HostError> {
        let listing = listing(&self.ir, self.clock.as_ref(), view, parameters)?;
        let scan = store_of(&mut self.components, &listing.owner)?.scan(&listing.entity)?;
        listing.rows(
            &self.ir,
            view,
            scan,
            &mut loading_from(&mut self.components),
        )
    }

    /// Executes one command on `component` and sends the events it emitted, refusing each outcome
    /// `refused` names ([`spindle_kernel::execute_refusing`]). The command reads each entity
    /// another component of this host owns from that component's store ([`Routed`],
    /// [`spindle_kernel::execute_reaching`]).
    ///
    /// Its writes are attributed to the command and `cause` ([`Origin`]); each envelope it stages
    /// between this host's own components keeps the command and what the command it invokes
    /// records as its trigger ([`Provenance::trigger`]).
    ///
    /// The outer error is the host's own; the inner one is the command's.
    #[allow(clippy::too_many_arguments)]
    fn execute(
        &mut self,
        component: &str,
        command: &str,
        input: &Map<String, Value>,
        actor: Option<&str>,
        caller: Option<&Map<String, Value>>,
        refused: &BTreeMap<String, String>,
        cause: Provenance,
    ) -> Result<Result<Executed, ExecuteError>, HostError> {
        let reached: BTreeSet<String> = self.components.keys().cloned().collect();
        let mut held = None;
        let mut others: BTreeMap<&str, &mut dyn Store> = BTreeMap::new();
        for (name, running) in &mut self.components {
            if name == component {
                held = Some(running);
            } else {
                others.insert(name.as_str(), &mut running.store);
            }
        }
        let held = held.ok_or_else(|| HostError::NotAccepted(command.to_owned()))?;
        let started = Instant::now();
        let clock = &self.clock;
        let sequence = following(self.sequence.load(Ordering::SeqCst), clock.wall());
        self.sequence.store(sequence, Ordering::SeqCst);
        let trigger = cause.trigger();
        held.store.attribute(Some(Origin {
            command: command.to_owned(),
            sequence,
            cause: Some(cause),
        }));
        let mut routed = Routed {
            ir: &self.ir,
            running: Tallied {
                store: &mut held.store,
                appended: Vec::new(),
            },
            others,
        };
        let result = spindle_kernel::execute_reaching(
            &self.ir,
            &mut routed,
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
            refused,
            &reached,
        );
        let appended = routed.running.appended;
        held.store.attribute(None);
        held.sink.executed(command, &result, started.elapsed());
        let mut emitted = self.emitted.lock().unwrap_or_else(PoisonError::into_inner);
        for event in appended {
            let count = emitted.entry((command.to_owned(), event)).or_default();
            *count = count.saturating_add(1);
        }
        drop(emitted);
        let executed = match result {
            Ok(executed) => executed,
            Err(error) => return Ok(Err(error)),
        };
        let sent: Vec<&Event> = executed
            .events
            .iter()
            .filter(|event| published_anywhere(&self.ir, &event.name))
            .collect();
        if !sent.is_empty() {
            held.position += 1;
            let first = held.sender.next_delivery();
            for event in sent {
                held.sender.publish(
                    held.position,
                    spindle_channel::Event {
                        name: event.name.clone(),
                        fields: event.payload.clone(),
                    },
                )?;
            }
            if self.remote.is_none() {
                let DeliveryId(after) = held.sender.next_delivery();
                for id in first.0..after {
                    self.staged_by.insert(
                        (component.to_owned(), DeliveryId(id)),
                        (command.to_owned(), trigger.clone()),
                    );
                }
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
        self.attempt(component, command, input, cause, None, None, report)
    }

    /// Runs the command `envelope` carries; where this host's own route of `envelope.binding`
    /// retries (the envelope's `on_failure` is the sender's word), again at once after each
    /// attempt that is refused or does not execute and that the route answers with
    /// [`Failure::Retry`] ([`Route::answer`]: `on_failure: retry`, or the refusal's policy under
    /// `on_refusal`), until one takes effect, the route drops one, or [`RETRY_ATTEMPTS`] were
    /// made, each attempt counted on the receiving component. Where the route answers the last
    /// attempt with [`Failure::Escalate`], the receiving component publishes the binding's
    /// escalation event ([`Escalated`]). A command it reports as refused or not executed names
    /// the command whose run in this host staged `envelope`; one delivered from another process
    /// names none.
    fn invoke(&mut self, envelope: &Envelope, report: &mut Report) -> Result<(), HostError> {
        let route = self.answering.get(&envelope.binding).cloned();
        let staged = self.staged_by.remove(&(envelope.from.clone(), envelope.id));
        self.attempt(
            &envelope.to,
            &envelope.command.name,
            &envelope.command.fields,
            Cause::Delivered {
                binding: envelope.binding.clone(),
            },
            staged,
            route.as_deref(),
            report,
        )
    }

    /// Runs `command` once, or, when `route` is the binding's route and retries, again after each
    /// attempt the route answers with [`Failure::Retry`], up to [`RETRY_ATTEMPTS`] in all; reports
    /// the attempt that took effect, or the last one, a refused or failed one as caused by the
    /// command `staged` names, and, where `route` answers that last one with
    /// [`Failure::Escalate`], publishes its escalation event from `component`
    /// ([`Route::escalate`], [`Escalated`]). Each attempt is attributed to `cause`
    /// ([`Provenance`]): a tick's binding, or a delivery's binding and the trigger `staged` names,
    /// none known where nothing staged it.
    #[allow(clippy::too_many_arguments)]
    fn attempt(
        &mut self,
        component: &str,
        command: &str,
        input: &Map<String, Value>,
        cause: Cause,
        staged: Option<(String, Option<Provenance>)>,
        route: Option<&Route>,
        report: &mut Report,
    ) -> Result<(), HostError> {
        let retried = route.filter(|route| route.retries());
        let (caused_by, trigger) = staged.unzip();
        let provenance = match &cause {
            Cause::Tick { binding } => Provenance::Tick {
                binding: binding.clone(),
            },
            Cause::Delivered { binding } => Provenance::Binding {
                binding: binding.clone(),
                trigger: trigger.flatten().map(Box::new),
            },
            Cause::Submitted => Provenance::Request(spindle_kernel::Sender::default()),
        };
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
            if let Cause::Delivered { binding } = &cause {
                report.invoked.push(Invoked {
                    binding: binding.clone(),
                    component: component.to_owned(),
                    command: command.to_owned(),
                    input: input.clone(),
                });
            }
            let result = self.execute(
                component,
                command,
                input,
                None,
                None,
                &BTreeMap::new(),
                provenance.clone(),
            )?;
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
            let escalation = route.zip(refusal).and_then(|(route, refusal)| {
                let escalation = route.escalate(&self.ir, refusal, input)?;
                Some((route, refusal.map(str::to_owned), escalation))
            });
            // The last attempt of a bound command that did not take effect and that its binding
            // did not escalate: kept as a dead letter (story:dead-letters-kept-and-replayed).
            let lettered = |outcome: Option<&str>, error: String| {
                let binding = match &cause {
                    Cause::Tick { binding } | Cause::Delivered { binding } => binding,
                    Cause::Submitted => return None,
                };
                escalation.is_none().then(|| DeadLetter {
                    binding: binding.clone(),
                    event: matches!(cause, Cause::Delivered { .. })
                        .then(|| bound_event(&self.ir, binding))
                        .flatten(),
                    command: command.to_owned(),
                    input: input.clone(),
                    outcome: outcome.map(str::to_owned),
                    error,
                    attempts,
                })
            };
            match result {
                Ok(Executed {
                    outcome,
                    error: Some(error),
                    ..
                }) => {
                    let letter = lettered(Some(&outcome), error.name.clone());
                    let (dead_letter, not_kept) = self.keep(component, letter);
                    report.refused.push(RefusedCommand {
                        component: component.to_owned(),
                        command: command.to_owned(),
                        cause,
                        caused_by,
                        outcome,
                        error,
                        attempts,
                        dead_letter,
                        not_kept,
                    });
                    if let Some(refused) = report
                        .refused
                        .last()
                        .filter(|refused| refused.not_kept.is_some())
                    {
                        if self.unkept.len() >= ESCALATIONS_KEPT {
                            self.unkept.remove(0);
                        }
                        self.unkept.push(refused.clone());
                    }
                }
                Ok(executed) => report.ran.push(Ran {
                    component: component.to_owned(),
                    command: command.to_owned(),
                    cause,
                    executed,
                    attempts,
                }),
                Err(error) => {
                    let letter = lettered(None, error.kind().to_owned());
                    let (dead_letter, not_kept) = self.keep(component, letter);
                    report.failed.push(Failed {
                        component: component.to_owned(),
                        command: command.to_owned(),
                        cause,
                        caused_by,
                        error: error.to_string(),
                        kind: Some(error.kind()),
                        attempts,
                        dead_letter,
                        not_kept,
                    });
                }
            }
            if let Some((route, outcome, escalation)) = escalation {
                let (event, escalation) = match escalation {
                    Ok(event) => {
                        let (name, payload) = (event.name.clone(), event.fields.clone());
                        self.publish(component, command, event, provenance.trigger())?;
                        (name, Escalation::Published(payload))
                    }
                    Err(unmet) => {
                        let name = match &unmet {
                            MappingError::EscalationUnmet { event, .. } => event.clone(),
                            _ => route.escalation.clone().unwrap_or_default(),
                        };
                        (name, Escalation::Unmet(unmet.to_string()))
                    }
                };
                let escalated = Escalated {
                    binding: route.binding.clone(),
                    component: component.to_owned(),
                    command: command.to_owned(),
                    outcome,
                    event,
                    escalation,
                };
                if self.escalations.len() >= ESCALATIONS_KEPT {
                    self.escalations.remove(0);
                }
                self.escalations.push(escalated.clone());
                report.escalated.push(escalated);
            }
            self.unmet(component, command, report);
            return Ok(());
        }
    }

    /// Keeps `letter`, where the command is one, in the store of `component`
    /// ([`Store::keep_dead_letter`]): its id, or why the store did not keep it, for the command's
    /// report ([`Failed::dead_letter`], [`Failed::not_kept`]). Neither for no letter.
    fn keep(
        &mut self,
        component: &str,
        letter: Option<DeadLetter>,
    ) -> (Option<u64>, Option<String>) {
        let Some(letter) = letter else {
            return (None, None);
        };
        let Some(held) = self.components.get_mut(component) else {
            return (None, Some(format!("no component {component} keeps it")));
        };
        match held.store.keep_dead_letter(&letter) {
            Ok(id) => (Some(id), None),
            Err(error) => (None, Some(error.to_string())),
        }
    }

    /// Publishes `event`, which `command` did not emit, from `component` through its sender, as
    /// [`Self::execute`] publishes what a command emits: staged at a position of its own, marked
    /// durable at once because no store holds it, and sent. Between this host's own components,
    /// each envelope it stages records `command` and `trigger` ([`Self::staged_by`]).
    fn publish(
        &mut self,
        component: &str,
        command: &str,
        event: spindle_channel::Event,
        trigger: Option<Provenance>,
    ) -> Result<(), HostError> {
        let held = self
            .components
            .get_mut(component)
            .ok_or_else(|| HostError::NotAccepted(command.to_owned()))?;
        held.position += 1;
        let first = held.sender.next_delivery();
        held.sender.publish(held.position, event)?;
        if self.remote.is_none() {
            let DeliveryId(after) = held.sender.next_delivery();
            for id in first.0..after {
                self.staged_by.insert(
                    (component.to_owned(), DeliveryId(id)),
                    (command.to_owned(), trigger.clone()),
                );
            }
        }
        held.sender.mark_durable(held.position);
        held.sender.flush()?;
        Ok(())
    }

    /// Reports every binding the events `component` just published, running `command`, triggered
    /// whose input was unmet as a command that did not execute: its command was never sent
    /// ([`spindle_channel::Unmet`]), and `command` caused it.
    fn unmet(&mut self, component: &str, command: &str, report: &mut Report) {
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
                caused_by: Some(command.to_owned()),
                error: unmet.error.to_string(),
                kind: None,
                attempts: 0,
                dead_letter: None,
                not_kept: None,
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
                            failed: std::mem::take(&mut report.failed),
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

    /// How many envelopes this host holds the staging command of, for [`Failed::caused_by`]:
    /// those staged between its own components and not yet invoked. Read by tests only.
    #[doc(hidden)]
    pub fn staged_envelopes(&self) -> usize {
        self.staged_by.len()
    }

    /// Takes every envelope still in flight off its channel without running it.
    fn discard(&mut self) -> Result<(), HostError> {
        for receiver in &mut self.receivers {
            receiver.discard()?;
        }
        self.staged_by.clear();
        Ok(())
    }
}

/// What one call has delivered so far.
#[derive(Debug, Default)]
struct Deliveries {
    count: usize,
    bindings: BTreeSet<String>,
}

/// The sequence the next command is attributed with, after `last`: the civil time `wall` in
/// microseconds since the Unix epoch, or one past `last`, whichever is later. So a command's
/// sequence is greater than every one this host attributed before it, and, as long as the civil
/// time does not go back between two processes, than every one an earlier process on the same
/// stores attributed.
fn following(last: u64, wall: std::time::SystemTime) -> u64 {
    micros(wall).max(last.saturating_add(1))
}

/// The sequence a cursor after the last event published is taken at ([`range::head`]), where
/// `last` is the sequence of the last command started: that sequence, or, before the first
/// command, the civil time `wall` in microseconds, which every earlier command's sequence is
/// below as long as the civil time does not go back. At least 1.
pub(crate) fn head_sequence(last: u64, wall: std::time::SystemTime) -> u64 {
    if last == 0 { micros(wall) } else { last }.max(1)
}

/// The civil time `wall` in microseconds since the Unix epoch, `0` before it.
fn micros(wall: std::time::SystemTime) -> u64 {
    wall.duration_since(std::time::SystemTime::UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_micros()).unwrap_or(u64::MAX)
        })
}

/// The events of every component, each component's in its store's commit order, merged into the
/// order published: by the sequence each command was attributed with ([`Origin::sequence`]). An
/// event its store holds no command for keeps the sequence of the event before it in its store,
/// and a component's events never pass each other, whatever their sequences. Each event's `at`
/// is its own command's sequence as a civil time ([`Published::at`]), `None` for such an event.
pub(crate) fn merged(committed: Vec<(String, Vec<Committed>)>) -> Vec<Published> {
    let mut keyed = Vec::with_capacity(committed.iter().map(|(_, events)| events.len()).sum());
    for (index, (component, events)) in committed.into_iter().enumerate() {
        let mut sequence = 0;
        for (at, committed) in events.into_iter().enumerate() {
            let commanded = committed.origin.as_ref().map(|origin| origin.sequence);
            let command = match committed.origin {
                Some(origin) => {
                    sequence = sequence.max(origin.sequence);
                    origin.command
                }
                None => String::new(),
            };
            keyed.push((
                (sequence, index, at),
                Published {
                    component: component.clone(),
                    command,
                    at: commanded.and_then(civil),
                    event: committed.event,
                },
            ));
        }
    }
    keyed.sort_by_key(|(key, _)| *key);
    keyed.into_iter().map(|(_, published)| published).collect()
}

/// Removes every instance `store` holds of every entity `ir` declares, each at the version it
/// holds, so that the store answers as an empty one does; otherwise the step that failed and the
/// store's error, which tells a busy store from one that failed otherwise. Sets `removed` once a
/// removal landed, or one whose outcome is unknown ([`StoreError::Unknown`]), which may have.
fn removed_all(
    ir: &EssIr,
    store: &mut dyn Store,
    removed: &mut bool,
) -> Result<(), (String, StoreError)> {
    for entity in ir.entities().keys() {
        let entity = entity.to_string();
        let held = store
            .scan(&entity)
            .map_err(|error| (format!("reading {entity}"), error))?;
        for (identity, stored) in held {
            store
                .remove(&spindle_kernel::Removal {
                    entity: entity.clone(),
                    identity: identity.clone(),
                    expected: stored.version,
                    events: Vec::new(),
                    scanned: Vec::new(),
                })
                .map_err(|error| {
                    if matches!(error, StoreError::Unknown(_)) {
                        *removed = true;
                    }
                    (format!("removing {entity} {identity}"), error)
                })?;
            *removed = true;
        }
    }
    Ok(())
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

/// The SQLite stores of a host's components that keep their state in a file, each opened and its
/// rows checked against the specification the host serves before any records it
/// (`story:spec-change-over-stored-rows`): a misfit in one leaves every record as it was.
#[derive(Default)]
struct Files {
    /// Each checked store, by component, recorded.
    #[cfg(feature = "sqlite")]
    checked: BTreeMap<String, spindle_kernel::SqliteStore>,
    /// The components whose store held rows and recorded no specification ([`Host::unrecorded`]).
    unrecorded: Vec<String>,
}

impl Files {
    /// The store of each component of `placements` whose state is bound to `sqlite`, in its file
    /// under `state`, checked against the entities of `ir` the component owns
    /// ([`spindle_kernel::LogStore::check`]); once every one fits, each whose record differs
    /// records the specification ([`spindle_kernel::LogStore::record`]).
    ///
    /// # Errors
    /// [`HostError::Misfit`] for the first store holding a row the specification cannot read,
    /// nothing recorded; [`HostError::Store`] where a store cannot be opened, read or written.
    fn checked(ir: &EssIr, state: &Path, placements: &Placements) -> Result<Self, HostError> {
        #[cfg_attr(not(feature = "sqlite"), allow(unused_mut))]
        let mut files = Self::default();
        #[cfg(feature = "sqlite")]
        {
            let mut fitting = Vec::new();
            for (name, bindings) in placements.iter() {
                let bound = |capability: &str| {
                    bindings
                        .iter()
                        .find(|binding| binding.capability.as_str() == capability)
                        .map(|binding| binding.implementation.as_str())
                };
                if bound(Capability::STORE_DURABLE).or_else(|| bound("store.ephemeral"))
                    != Some("sqlite")
                {
                    continue;
                }
                let mut store =
                    spindle_kernel::SqliteStore::open(&state.join(format!("{name}.sqlite")))?;
                let shapes = spindle_kernel::Shapes::owned(ir, name);
                match store.check(&shapes)? {
                    spindle_kernel::Fit::Same => {}
                    spindle_kernel::Fit::Fits { unrecorded } => {
                        if unrecorded {
                            files.unrecorded.push(name.clone());
                        }
                        fitting.push((name.clone(), shapes));
                    }
                    spindle_kernel::Fit::Misfits(misfits) => {
                        return Err(HostError::Misfit {
                            component: name.clone(),
                            misfits,
                        });
                    }
                }
                files.checked.insert(name.clone(), store);
            }
            for (name, shapes) in fitting {
                if let Some(store) = files.checked.get_mut(&name) {
                    store.record(&shapes)?;
                }
            }
        }
        #[cfg(not(feature = "sqlite"))]
        let _ = (ir, state, placements);
        Ok(files)
    }

    /// The checked store of the component `name`, counted into `sink`; one opened at `file`
    /// where none was checked.
    fn take(&mut self, name: &str, file: &Path, sink: &Sink) -> Result<Box<dyn Store>, HostError> {
        #[cfg(feature = "sqlite")]
        if let Some(store) = self.checked.remove(name) {
            return Ok(Box::new(CountingStore::new(store, sink.clone())));
        }
        let _ = name;
        opened(Some(file), sink)
    }
}

/// The empty SQLite store a reset opens at `file` for the component `name`, counted into `sink`,
/// recording the specification of `ir` ([`spindle_kernel::LogStore::fit`]).
fn recorded_empty(
    file: &Path,
    ir: &EssIr,
    name: &str,
    sink: &Sink,
) -> Result<Box<dyn Store>, HostError> {
    #[cfg(feature = "sqlite")]
    {
        let mut store = spindle_kernel::SqliteStore::open(file)?;
        if let spindle_kernel::Fit::Misfits(misfits) =
            store.fit(&spindle_kernel::Shapes::owned(ir, name))?
        {
            return Err(HostError::Misfit {
                component: name.to_owned(),
                misfits,
            });
        }
        Ok(Box::new(CountingStore::new(store, sink.clone())))
    }
    #[cfg(not(feature = "sqlite"))]
    {
        let _ = (ir, name);
        opened(Some(file), sink)
    }
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

    fn scan(&mut self, _: &str) -> Result<spindle_kernel::Scan, StoreError> {
        Err(StoreError::Backend(self.0.clone()))
    }

    fn keep_dead_letter(&mut self, _: &DeadLetter) -> Result<u64, StoreError> {
        Err(StoreError::Backend(self.0.clone()))
    }

    fn dead_letters(&mut self) -> Result<Vec<(u64, DeadLetter)>, StoreError> {
        Err(StoreError::Backend(self.0.clone()))
    }

    fn take_dead_letter(&mut self, _: u64) -> Result<Option<DeadLetter>, StoreError> {
        Err(StoreError::Backend(self.0.clone()))
    }

    fn last_dead_letter_id(&mut self) -> Result<u64, StoreError> {
        Err(StoreError::Backend(self.0.clone()))
    }

    fn continue_dead_letter_ids(&mut self, _: u64) -> Result<(), StoreError> {
        Err(StoreError::Backend(self.0.clone()))
    }
}

/// The last id `store`, the store of `component` a reset replaces, answered for a dead letter
/// ([`Store::last_dead_letter_id`]), which the empty store replacing it continues after: a
/// replay naming a dead letter the reset forgot then finds none. A store that cannot answer it
/// is kept as it is, and the reset fails naming why: as it was before the reset, or, for a store
/// an earlier reset lost, still lost.
fn last_dead_letter_id(name: &str, component: &mut Component) -> Result<u64, HostError> {
    component
        .store
        .last_dead_letter_id()
        .map_err(|error| HostError::Reset {
            component: name.to_owned(),
            error: if component.lost {
                format!(
                    "reading the last dead letter id: {error}; its store was lost in an earlier \
                     reset and is still lost"
                )
            } else {
                format!("reading the last dead letter id: {error}")
            },
            restored: !component.lost,
        })
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
/// one is opened, recording the specification of `ir` for the component `name`, and told to
/// continue the dead letter ids after `after`, the last id the closed store answered
/// ([`Store::continue_dead_letter_ids`]), and only then are the moved files removed. On failure
/// every file is where it was and the error names what failed; a file that could not be moved
/// back is named with where it is.
fn emptied_file(
    file: &Path,
    ir: &EssIr,
    name: &str,
    sink: &Sink,
    after: u64,
) -> Result<Box<dyn Store>, String> {
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
    let continued = recorded_empty(file, ir, name, sink)
        .map_err(|error| format!("opening an empty store at {}: {error}", file.display()))
        .and_then(|mut store| match store.continue_dead_letter_ids(after) {
            Ok(()) => Ok(store),
            Err(error) => Err(format!(
                "continuing the dead letter ids after {after} in the empty store at {}: {error}",
                file.display()
            )),
        });
    match continued {
        Ok(store) => {
            for path in &moved {
                let _ = std::fs::remove_file(aside(path));
            }
            Ok(store)
        }
        Err(why) => {
            for path in sqlite_files(file) {
                let _ = std::fs::remove_file(path);
            }
            Err(restore(&moved, why))
        }
    }
}

/// The qualified event the event binding `binding` is caused by; `None` for a periodic binding
/// and one `ir` does not declare.
fn bound_event(ir: &EssIr, binding: &str) -> Option<String> {
    ir.bindings()
        .values()
        .find(|bound| bound.name.as_str() == binding)
        .and_then(|bound| bound.cause.event())
        .map(ToString::to_string)
}

/// Whether any component lists `event` under `publishes:`. An event no component publishes is
/// committed with the write and listed by [`Host::published`], and is not handed to the channel:
/// no route has it as its event ([`Router::build`]). An event another component publishes is
/// handed over, and [`Sender::publish`] refuses it.
fn published_anywhere(ir: &EssIr, event: &str) -> bool {
    ir.components().values().any(|declared| {
        declared
            .publishes
            .iter()
            .any(|published| published.to_string() == event)
    })
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
    let owner = owner_of(ir, ir.entity(&declared.source))?;
    for reference in &declared.references {
        owner_of(ir, ir.entity(&reference.entity))?;
    }
    Ok((declared, owner))
}

/// The component that owns `entity`'s domain.
///
/// # Errors
/// [`HostError::Unsupported`] where none does.
/// The component whose store holds the instances of `entity`, a qualified entity name.
fn holding(ir: &EssIr, entity: &str) -> Result<String, HostError> {
    let declared = ir
        .entities()
        .values()
        .find(|declared| declared.name.to_string() == entity)
        .ok_or_else(|| HostError::NoEntity(entity.to_owned()))?;
    owner_of(ir, declared)
}

/// The civil time a command attributed with `sequence` ran at ([`Origin::sequence`]: its civil
/// time in microseconds, or one past the last command's), as RFC 3339 in UTC to the
/// millisecond; `None` past the years RFC 3339 spells.
fn civil(sequence: u64) -> Option<String> {
    let at = std::time::SystemTime::UNIX_EPOCH.checked_add(Duration::from_micros(sequence))?;
    rfc3339(at)
}

/// The civil time `at` as RFC 3339 in UTC to the millisecond it falls in, as ESS spells an
/// instant ([`ess_primitives::Rfc3339Instant::to_rfc3339`]: `2026-02-01T00:00:00Z`,
/// `2026-02-01T00:00:00.5Z`); `None` past the years RFC 3339 spells.
pub fn rfc3339(at: SystemTime) -> Option<String> {
    crate::view::instant(at).map(ess_primitives::Rfc3339Instant::to_rfc3339)
}

/// The civil time an RFC 3339 `date-time` names (`2026-02-01T00:00:00Z`,
/// `2026-02-01T01:00:00+01:00`, `2026-02-01T00:00:00.5Z`), its offset and its fraction of a second
/// included, read by ESS's own reader ([`ess_primitives::Rfc3339Instant`]); `None` for a text that
/// is no RFC 3339 `date-time`.
pub fn instant(text: &str) -> Option<SystemTime> {
    let parsed = ess_primitives::Rfc3339Instant::parse_rfc3339(text)?;
    let seconds = parsed.epoch_seconds();
    let nanos = nanos(&parsed.to_rfc3339());
    let whole = Duration::from_secs(seconds.unsigned_abs());
    let second = if seconds >= 0 {
        SystemTime::UNIX_EPOCH.checked_add(whole)?
    } else {
        SystemTime::UNIX_EPOCH.checked_sub(whole)?
    };
    second.checked_add(Duration::from_nanos(nanos))
}

/// The instant a stepping clock is set to from the RFC 3339 `date-time` `text` (`--clock`,
/// `POST /api/clock`, a reset world's `"clock"`): [`instant`], floored to the millisecond, as the
/// run answers where its clock stands ([`rfc3339`]), so an instant the run answered sets the clock
/// to where it stands.
///
/// # Errors
/// Why `text` sets no clock: no RFC 3339 `date-time`, or an instant before
/// 1970-01-01T00:00:00Z, the first instant a clock stands at.
pub fn clock_instant(text: &str) -> Result<SystemTime, String> {
    let at = instant(text).ok_or_else(|| {
        format!("expected an RFC 3339 instant such as 2026-02-01T00:00:00Z: {text}")
    })?;
    let since = at.duration_since(SystemTime::UNIX_EPOCH).map_err(|_| {
        format!("{text} is before 1970-01-01T00:00:00Z, the first instant a clock stands at")
    })?;
    let millis = Duration::from_millis(u64::try_from(since.as_millis()).unwrap_or(u64::MAX));
    SystemTime::UNIX_EPOCH
        .checked_add(millis)
        .ok_or_else(|| format!("{text} is past the instants a clock stands at"))
}

/// The nanoseconds of the fraction of a second `spelled` names, an instant as
/// [`ess_primitives::Rfc3339Instant::to_rfc3339`] spells it: in UTC, its fraction's trailing zeros
/// trimmed (`.5Z` is 500 000 000), 0 without a fraction. The fraction is the one part the instant
/// answers only in its spelling.
fn nanos(spelled: &str) -> u64 {
    let Some((_, fraction)) = spelled.split_once('.') else {
        return 0;
    };
    let digits = fraction.trim_end_matches('Z');
    if digits.is_empty() || digits.len() > 9 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return 0;
    }
    format!("{digits:0<9}").parse().unwrap_or(0)
}

/// The refusal of a step on a host without a stepping clock.
fn unstepped() -> HostError {
    HostError::Clock(
        "this host's clock does not step; a run started with --clock steps its clock".to_owned(),
    )
}

fn owner_of(ir: &EssIr, entity: &ResolvedEntity) -> Result<String, HostError> {
    ir.components()
        .values()
        .find(|component| component.owns.contains(&entity.domain))
        .map(|component| component.name.as_str().to_owned())
        .ok_or_else(|| HostError::Unsupported(format!("no component owns {}", entity.domain)))
}

/// What loads the row an instance rests with, by the component that owns its entity, the entity
/// and the identity: the host's stores on its thread ([`loading_from`]), the positions of a read
/// beside it ([`Reader::list_with`]).
type Load<'l> = dyn FnMut(&str, &str, &Value) -> Result<Option<Stored>, HostError> + 'l;

/// Loads from the store of the running component that owns the entity ([`Store::load`]).
fn loading_from(
    components: &mut BTreeMap<String, Component>,
) -> impl FnMut(&str, &str, &Value) -> Result<Option<Stored>, HostError> + '_ {
    |owner, entity, identity| Ok(store_of(components, owner)?.load(entity, identity)?)
}

/// The rows one read of a view reads through a reference, by a field or by its filter: each loaded
/// once, by its
/// entity and identity, through what loads the rows of the component that owns the entity
/// ([`Load`]), and held for the rest of the read, so a read loads each row it names once however
/// many of its rows name it, and never scans the referenced entity. A row that is gone is held as
/// gone.
#[derive(Default)]
struct Far {
    loaded: BTreeMap<(String, String), Option<(Value, Row)>>,
}

impl Far {
    /// The rows the references of `declared` name from the source row `row`, the instance
    /// `identity` names ([`crate::view::references`]), each loaded through `load`.
    ///
    /// # Errors
    /// [`HostError::Store`] when a store cannot be read; [`HostError::Unsupported`] where no
    /// component owns a referenced entity, or the one that does is not running.
    fn referenced<'f>(
        &'f mut self,
        ir: &'f EssIr,
        declared: &'f ResolvedView,
        load: &mut Load<'_>,
        identity: &Value,
        row: &Row,
    ) -> Result<crate::view::Referenced<'f>, HostError> {
        let named = crate::view::references(ir, declared, identity, row)
            .into_iter()
            .map(|(via, entity, far)| {
                // Loaded by the one spelling a command keys the row by, whatever spelling the
                // referencing field holds (`spindle_kernel::one_spelling`).
                let far = spindle_kernel::one_spelling(&far);
                (via, entity, (entity.name.to_string(), far.to_string()), far)
            })
            .collect::<Vec<_>>();
        for (_, entity, key, far) in &named {
            if self.loaded.contains_key(key) {
                continue;
            }
            let held =
                load(&owner_of(ir, entity)?, &key.0, far)?.map(|stored| (far.clone(), stored.row));
            self.loaded.insert(key.clone(), held);
        }
        let loaded = &self.loaded;
        Ok(named
            .into_iter()
            .filter_map(|(via, _, key, _)| {
                let (identity, row) = loaded.get(&key)?.as_ref()?;
                Some((via, (identity, row)))
            })
            .collect())
    }
}

/// The rows one read of a view answers ([`Host::listed_with`], [`Reader::listed_with`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listed {
    /// The rows, in the view's order: the page a paged read asks for, or every row the view
    /// holds.
    pub rows: Vec<Map<String, Value>>,
    /// How many rows the view's filter keeps, counted before the page is cut, where the view
    /// declares `paging: {total: true}`; `None` for every other view.
    pub total: Option<u64>,
}

/// A read of a view, checked before any store is read: the view, the component that owns its
/// source, the source entity, the parameter values read, the slice of its rows a paged read
/// answers, and the instant its filter holds rows against. [`Host::list_with`] and
/// [`Reader::list_with`] read through it.
struct Listing<'ir> {
    declared: &'ir ResolvedView,
    owner: String,
    entity: String,
    parameters: ess_primitives::FactStore,
    slice: Option<crate::view::Slice>,
    now: Option<ess_primitives::Rfc3339Instant>,
}

/// The read of `view` with `parameters` on a host running `ir` on `clock`, as
/// [`Host::list_with`] checks it before anything is read.
fn listing<'ir>(
    ir: &'ir EssIr,
    clock: &dyn Clock,
    view: &str,
    parameters: &Map<String, Value>,
) -> Result<Listing<'ir>, HostError> {
    let (declared, owner) = readable(ir, view)?;
    let given = crate::view::parameters(ir, declared, parameters).map_err(|detail| {
        HostError::Parameters {
            view: view.to_owned(),
            detail,
        }
    })?;
    Ok(Listing {
        declared,
        owner,
        entity: ir.entity(&declared.source).name.to_string(),
        parameters: given.facts,
        slice: given.slice,
        now: read_instant(declared, clock),
    })
}

impl Listing<'_> {
    /// Every row of `scan`, the source's instances, the view holds, projected, in its order
    /// ([`Listing::kept`]), and, where the view declares `paging: {total: true}`, how many they are;
    /// of a paged read, only the page its slice asks for, cut after the rows are counted and
    /// ordered, as ESS's interpreter cuts it (`ess-conformance` `src/interpret/views.rs` at ESS
    /// 0.99.1, `query`).
    ///
    /// # Errors
    /// As [`Listing::kept`]; [`HostError::Unsupported`] where the total is past `u64`.
    fn rows(
        &self,
        ir: &EssIr,
        view: &str,
        scan: spindle_kernel::Scan,
        load: &mut Load<'_>,
    ) -> Result<Listed, HostError> {
        let rows = self.kept(ir, view, scan, load)?;
        let total = self
            .declared
            .paging
            .as_ref()
            .filter(|paging| paging.total)
            .map(|_| u64::try_from(rows.len()))
            .transpose()
            .map_err(|_| {
                HostError::Unsupported(format!("view {view}: its total is past what is counted"))
            })?;
        let rows = match self.slice {
            Some(slice) => slice.cut(rows),
            None => rows,
        };
        Ok(Listed { rows, total })
    }

    /// Every row of `scan`, the source's instances, the view holds: projected, in its order. The
    /// rows it reads through a reference, by a field or by its filter, are loaded through `load`
    /// ([`Far`]).
    fn kept(
        &self,
        ir: &EssIr,
        view: &str,
        scan: spindle_kernel::Scan,
        load: &mut Load<'_>,
    ) -> Result<Vec<Map<String, Value>>, HostError> {
        if let Some(aggregation) = &self.declared.aggregation {
            let mut kept = Vec::new();
            for (identity, stored) in scan {
                if held(
                    ir,
                    self.declared,
                    &identity,
                    &stored.row,
                    &crate::view::Referenced::new(),
                    &self.parameters,
                    self.now,
                )? {
                    kept.push((identity, stored.row));
                }
            }
            return crate::view::aggregate(
                ir,
                self.declared,
                aggregation,
                &kept,
                &self.parameters,
                self.now,
            )
            .map_err(|detail| HostError::Unsupported(format!("view {view}: {detail}")));
        }
        let mut rows = Vec::new();
        let mut far = Far::default();
        for (identity, stored) in scan {
            let referenced = far.referenced(ir, self.declared, load, &identity, &stored.row)?;
            if held(
                ir,
                self.declared,
                &identity,
                &stored.row,
                &referenced,
                &self.parameters,
                self.now,
            )? {
                let row =
                    crate::view::project(ir, self.declared, &identity, &stored.row, &referenced)
                        .map_err(|detail| {
                            HostError::Unsupported(format!("view {view}: {detail}"))
                        })?;
                rows.push((identity, row));
            }
        }
        crate::view::order(ir, self.declared, &mut rows);
        Ok(rows.into_iter().map(|(_, row)| row).collect())
    }
}

/// Whether `declared` holds `row` for `parameters`, `referenced` the rows its
/// references name ([`crate::view::holds`]); a row that is not a value of its entity's fields is
/// refused [`HostError::Unsupported`], naming it.
fn held(
    ir: &EssIr,
    declared: &ResolvedView,
    identity: &Value,
    row: &Row,
    referenced: &crate::view::Referenced<'_>,
    parameters: &ess_primitives::FactStore,
    now: Option<ess_primitives::Rfc3339Instant>,
) -> Result<bool, HostError> {
    crate::view::holds(ir, declared, identity, row, referenced, parameters, now)
        .map_err(|detail| HostError::Unsupported(format!("view {}: {detail}", declared.name)))
}

/// The instant a read of `declared` holds its rows against: the clock's civil time, read once,
/// where the view's filter, or the `where:` of one of its aggregates, orders a field against `now`
/// ([`crate::view::reads_now`]), and `None` for any other view, so a read reads the clock only
/// where what it evaluates names the current time.
fn read_instant(
    declared: &ResolvedView,
    clock: &dyn Clock,
) -> Option<ess_primitives::Rfc3339Instant> {
    let conditions = declared
        .aggregation
        .iter()
        .flat_map(|aggregation| aggregation.functions.values())
        .filter_map(|measure| measure.r#where.as_ref());
    declared
        .filter
        .iter()
        .chain(conditions)
        .any(crate::view::reads_now)
        .then(|| crate::view::instant(clock.wall()))
        .flatten()
}

/// The entities whose domain the component `name` owns, by qualified name: those whose rows its
/// store counts.
fn owned(ir: &EssIr, name: &str) -> Vec<String> {
    let Some(component) = ir
        .components()
        .values()
        .find(|component| component.name.as_str() == name)
    else {
        return Vec::new();
    };
    ir.entities()
        .values()
        .filter(|entity| component.owns.contains(&entity.domain))
        .map(|entity| entity.name.to_string())
        .collect()
}

/// The store of the running component `owner`.
fn store_of<'a>(
    components: &'a mut BTreeMap<String, Component>,
    owner: &str,
) -> Result<&'a mut dyn Store, HostError> {
    components
        .get_mut(owner)
        .map(|component| &mut component.store as &mut dyn Store)
        .ok_or_else(|| HostError::Unsupported(format!("{owner} is not running")))
}

#[cfg(test)]
mod tests {
    //! [`Routed`], the store a command runs on in a host
    //! (`story:cross-component-related-reads`), on `spindle-kernel`'s `metering-split` fixture:
    //! `meter` owns charges, `holds` owns holds.

    use super::*;
    use serde_json::json;

    const HOLD: &str = "metering.holds.Hold";
    const CHARGE: &str = "metering.usage.Charge";

    fn ir() -> EssIr {
        spindle_spec::load(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../spindle-kernel/tests/fixtures/metering-split"),
        )
        .expect("the fixture compiles")
        .ir
    }

    fn create(entity: &str, identity: &str, state: &str, scanned: Vec<Scanned>) -> Change {
        Change {
            entity: entity.to_owned(),
            identity: json!(identity),
            expected: None,
            row: Row {
                state: state.to_owned(),
                fields: BTreeMap::new(),
            },
            events: Vec::new(),
            scanned,
        }
    }

    /// `meter`'s store, routed beside `holds`'s.
    fn routed<'a>(
        ir: &'a EssIr,
        meter: &'a mut MemoryStore,
        holds: &'a mut MemoryStore,
    ) -> Routed<'a> {
        Routed {
            ir,
            running: Tallied {
                store: meter,
                appended: Vec::new(),
            },
            others: BTreeMap::from([("holds", holds as &mut dyn Store)]),
        }
    }

    /// A hold is read from the holds' store, a charge decided on that scan lands in the meter's
    /// store, and once another write of the holds lands after the scan the next such charge is
    /// refused as a conflict naming the holds' entity, writing nothing.
    #[test]
    fn a_write_decided_on_another_components_scan_is_conditioned_on_its_owner() {
        let ir = ir();
        let mut meter = MemoryStore::default();
        let mut holds = MemoryStore::default();
        holds
            .commit(&create(HOLD, "h1", "Active", Vec::new()))
            .expect("the hold is placed");

        let scan = routed(&ir, &mut meter, &mut holds)
            .scan(HOLD)
            .expect("the holds are read");
        assert_eq!(scan.rows.len(), 1, "read from the holds' store");
        assert!(meter.scan(HOLD).expect("read").rows.is_empty());
        let scanned = vec![Scanned {
            entity: HOLD.to_owned(),
            version: scan.version,
        }];
        routed(&ir, &mut meter, &mut holds)
            .commit(&create(CHARGE, "c1", "Billed", scanned.clone()))
            .expect("a charge decided on the holds as they stand lands");
        assert!(meter.load(CHARGE, &json!("c1")).expect("read").is_some());

        holds
            .commit(&create(HOLD, "h2", "Active", Vec::new()))
            .expect("another hold is placed");
        let moved =
            routed(&ir, &mut meter, &mut holds).commit(&create(CHARGE, "c2", "Billed", scanned));
        assert!(
            matches!(&moved, Err(StoreError::Conflict { entity, .. }) if entity == HOLD),
            "{moved:?}"
        );
        assert!(meter.load(CHARGE, &json!("c2")).expect("read").is_none());
    }
}
