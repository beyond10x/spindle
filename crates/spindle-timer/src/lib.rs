//! Periodic bindings: when each one is due.
//!
//! A binding whose cause is `periodic` invokes its command on a fixed grid of deadlines instead of
//! in reaction to an event. [`Timer`] reads those bindings from the compiled IR and answers, for a
//! given instant, which of them are due. It yields [`Tick`]s — a binding and the command it
//! invokes — and knows nothing about how a command runs: the host turns a tick into a command,
//! and reports back with [`Timer::complete`] when that command has finished.
//!
//! The API is pull-based. The host asks [`Timer::next_deadline`] when to wake, sleeps however it
//! likes, and calls [`Timer::due`] with the instant from its [`Clock`].
//!
//! # Lifetimes
//!
//! Each periodic binding names its own host (`host.owner`, `host.authority`), so each binding has
//! its own lifetime: [`Timer::activate_binding`] starts it at an instant that anchors that
//! binding's grid, and [`Timer::stop_binding`] ends it. [`Timer::activate`], [`Timer::stop`] and
//! [`Timer::quiescent`] do the same for every periodic binding at once.
//!
//! # The profile
//!
//! ESS 0.85.0 admits exactly one periodic profile; every word in it has one variant
//! (`ess_domain::binding::periodic`). Each is honoured as ESS documents it:
//!
//! | word | value | here |
//! |---|---|---|
//! | `anchor` | `host_activation` | the grid starts at the instant the binding's lifetime is activated |
//! | `first` | `after_period` | the first deadline is one period after activation; nothing is due at activation |
//! | `cadence` | `fixed_rate` | deadline *n* is activation + *n* × period; completing a tick does not move the grid |
//! | `overlap` | `serial_per_instance` | a binding with an open tick yields no other until that tick is completed |
//! | `missed` | `one_pending_drop_excess` | deadlines that pass while a tick is open or unobserved keep one pending tick, the earliest; the rest are dropped and counted ([`Timer::dropped`]) |
//! | `lifetime` | `host_instance` | a lifetime runs from activation to acknowledged stop; it holds no durable state, and a new host instance is a new activation |
//! | `cancellation` | `stop_acknowledged` | a stop ends issuing at once; the lifetime ends only when its open tick is completed ([`Phase::Stopping`]) |
//! | `host.eligibility` | `host_boolean` | sampled by the host when it dispatches a tick; the timer yields every due tick |
//!
//! The words are matched exhaustively, so an ESS release that adds a variant or a field fails to
//! compile here until it is decided.

mod clock;

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use ess_compiler::EssIr;
use ess_compiler::ir::CommandHandle;
use ess_domain::binding::periodic::{
    Anchor, Cadence, Cancellation, Eligibility, First, HostInputContract, Lifetime, Missed,
    Overlap, Period, PeriodicCause,
};

pub use clock::{Clock, ManualClock, SystemClock};
pub use ess_domain::binding::BindingName;

/// One due occurrence of a periodic binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tick {
    /// The binding that is due.
    pub binding: BindingName,
    /// The command the binding invokes.
    pub command: CommandHandle,
    /// The grid deadline this tick stands for; never after the instant it was yielded at, and
    /// earlier when the tick was pending.
    pub deadline: Instant,
    lifetime: u64,
}

/// Where a periodic binding's lifetime stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Not activated, or stopped and acknowledged.
    Inactive,
    /// Activated: deadlines fall due.
    Active,
    /// Stopped with a tick still open; nothing more is issued, and the lifetime ends when that
    /// tick is completed.
    Stopping,
}

/// Why [`Timer::complete`] refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CompleteError {
    /// The tick's binding is not a periodic binding of this timer's specification: the tick came
    /// from another timer.
    #[error("`{0}` is not a periodic binding")]
    NotPeriodic(BindingName),
    /// The tick is not the one open: it was completed already, or belongs to an earlier lifetime.
    #[error("the tick of `{binding}` at {deadline:?} is not the open one")]
    Stale {
        /// The binding the tick belongs to.
        binding: BindingName,
        /// The deadline the stale tick stood for.
        deadline: Instant,
    },
}

/// Why activating or stopping a binding's lifetime refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LifetimeError {
    /// The binding is not a periodic binding of this specification.
    #[error("`{0}` is not a periodic binding")]
    NotPeriodic(BindingName),
    /// The binding's lifetime is active or still stopping.
    #[error("`{0}` already has a running lifetime")]
    Running(BindingName),
    /// The binding has no running lifetime to stop.
    #[error("`{0}` has no running lifetime")]
    NotRunning(BindingName),
}

/// The schedules of every periodic binding of one specification.
#[derive(Debug)]
pub struct Timer {
    schedules: BTreeMap<BindingName, Schedule>,
}

#[derive(Debug)]
struct Schedule {
    command: CommandHandle,
    period: Period,
    lifetime: u64,
    dropped: u64,
    run: Run,
}

#[derive(Debug)]
enum Run {
    Inactive,
    Active(Grid),
    Stopping { open: Instant },
}

#[derive(Debug)]
struct Grid {
    activation: Instant,
    next: u64,
    pending: Option<Instant>,
    open: Option<Instant>,
}

impl Timer {
    /// The periodic bindings of `ir`, none of them activated.
    pub fn new(ir: &EssIr) -> Self {
        let schedules = ir
            .bindings()
            .values()
            .filter_map(|binding| {
                let periodic = binding.cause.periodic()?;
                let schedule = Schedule {
                    command: binding.command.clone(),
                    period: honoured(&periodic.contract),
                    lifetime: 0,
                    dropped: 0,
                    run: Run::Inactive,
                };
                Some((binding.name.clone(), schedule))
            })
            .collect();
        Self { schedules }
    }

    /// The periodic bindings of `ir`, every one activated at `clock.now()`.
    pub fn activate(ir: &EssIr, clock: &impl Clock) -> Self {
        let mut timer = Self::new(ir);
        let at = clock.now();
        for schedule in timer.schedules.values_mut() {
            schedule.start(at);
        }
        timer
    }

    /// Starts a new lifetime of `binding`, its grid anchored at `at`.
    ///
    /// Refused while the binding's previous lifetime is still running. The dropped count starts
    /// again at zero.
    pub fn activate_binding(
        &mut self,
        binding: &BindingName,
        at: Instant,
    ) -> Result<(), LifetimeError> {
        let schedule = self.schedule(binding)?;
        if !matches!(schedule.run, Run::Inactive) {
            return Err(LifetimeError::Running(binding.clone()));
        }
        schedule.start(at);
        Ok(())
    }

    /// Stops issuing ticks for `binding` and discards its pending tick.
    ///
    /// With no tick open the lifetime ends at once; otherwise it is [`Phase::Stopping`] until
    /// that tick is completed. Stopping a binding that is already stopping is not an error.
    pub fn stop_binding(&mut self, binding: &BindingName) -> Result<(), LifetimeError> {
        let schedule = self.schedule(binding)?;
        match &schedule.run {
            Run::Inactive => Err(LifetimeError::NotRunning(binding.clone())),
            Run::Stopping { .. } => Ok(()),
            Run::Active(grid) => {
                schedule.run = match grid.open {
                    Some(open) => Run::Stopping { open },
                    None => Run::Inactive,
                };
                Ok(())
            }
        }
    }

    /// Stops every running lifetime, as [`Self::stop_binding`] does for one.
    pub fn stop(&mut self) {
        let running: Vec<BindingName> = self
            .schedules
            .iter()
            .filter(|(_, schedule)| !matches!(schedule.run, Run::Inactive))
            .map(|(name, _)| name.clone())
            .collect();
        for name in running {
            let _ = self.stop_binding(&name);
        }
    }

    /// Whether no binding has a running lifetime: every one inactive.
    pub fn quiescent(&self) -> bool {
        self.schedules
            .values()
            .all(|schedule| matches!(schedule.run, Run::Inactive))
    }

    /// Where `binding`'s lifetime stands; `None` when it is not periodic.
    pub fn phase(&self, binding: &BindingName) -> Option<Phase> {
        self.schedules
            .get(binding)
            .map(|schedule| match schedule.run {
                Run::Inactive => Phase::Inactive,
                Run::Active(_) => Phase::Active,
                Run::Stopping { .. } => Phase::Stopping,
            })
    }

    /// The periodic bindings this timer schedules, by name.
    pub fn bindings(&self) -> impl Iterator<Item = &BindingName> {
        self.schedules.keys()
    }

    /// The ticks due at `now`, at most one per binding, in binding-name order.
    ///
    /// Only active bindings with no open tick yield, and only a tick whose deadline is not after
    /// `now`. Each yielded tick is open until [`Self::complete`] reports it finished.
    pub fn due(&mut self, now: Instant) -> Vec<Tick> {
        let mut ticks = Vec::new();
        for (name, schedule) in &mut self.schedules {
            let Run::Active(grid) = &mut schedule.run else {
                continue;
            };
            schedule.dropped = schedule
                .dropped
                .saturating_add(grid.observe(schedule.period, now));
            if grid.open.is_some() {
                continue;
            }
            if let Some(deadline) = grid.pending.filter(|deadline| *deadline <= now) {
                grid.pending = None;
                grid.open = Some(deadline);
                ticks.push(Tick {
                    binding: name.clone(),
                    command: schedule.command.clone(),
                    deadline,
                    lifetime: schedule.lifetime,
                });
            }
        }
        ticks
    }

    /// The earliest instant at which [`Self::due`] can yield a tick without a completion first.
    ///
    /// `None` when nothing can: no binding is active with its tick closed. An instant in the past
    /// means a tick is due now.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.schedules
            .values()
            .filter_map(|schedule| match &schedule.run {
                Run::Active(grid) if grid.open.is_none() => grid
                    .pending
                    .or_else(|| grid.deadline(schedule.period, grid.next)),
                _ => None,
            })
            .min()
    }

    /// Reports that `tick` has finished, closing exactly that tick.
    ///
    /// A tick that is not the open one — completed already, or from an earlier lifetime — is
    /// refused as [`CompleteError::Stale`] and changes nothing, so a duplicate report never
    /// closes the tick that followed it.
    pub fn complete(&mut self, tick: &Tick) -> Result<(), CompleteError> {
        let schedule = self
            .schedules
            .get_mut(&tick.binding)
            .ok_or_else(|| CompleteError::NotPeriodic(tick.binding.clone()))?;
        let open = match &schedule.run {
            Run::Inactive => None,
            Run::Active(grid) => grid.open,
            Run::Stopping { open } => Some(*open),
        };
        if tick.lifetime != schedule.lifetime || open != Some(tick.deadline) {
            return Err(CompleteError::Stale {
                binding: tick.binding.clone(),
                deadline: tick.deadline,
            });
        }
        match &mut schedule.run {
            Run::Active(grid) => grid.open = None,
            Run::Stopping { .. } => schedule.run = Run::Inactive,
            Run::Inactive => {}
        }
        Ok(())
    }

    /// How many deadlines of `binding` were dropped under `missed: one_pending_drop_excess` in its
    /// current or last lifetime.
    ///
    /// `None` when `binding` is not periodic. Counted when [`Self::due`] observes them.
    pub fn dropped(&self, binding: &BindingName) -> Option<u64> {
        self.schedules.get(binding).map(|schedule| schedule.dropped)
    }

    fn schedule(&mut self, binding: &BindingName) -> Result<&mut Schedule, LifetimeError> {
        self.schedules
            .get_mut(binding)
            .ok_or_else(|| LifetimeError::NotPeriodic(binding.clone()))
    }
}

impl Schedule {
    fn start(&mut self, at: Instant) {
        self.lifetime += 1;
        self.dropped = 0;
        self.run = Run::Active(Grid {
            activation: at,
            next: 1,
            pending: None,
            open: None,
        });
    }
}

impl Grid {
    fn deadline(&self, period: Period, ordinal: u64) -> Option<Instant> {
        let offset = period.deadline(0, ordinal)?;
        self.activation.checked_add(Duration::from_millis(offset))
    }

    /// Takes in every deadline passed by `now`; returns how many were dropped.
    fn observe(&mut self, period: Period, now: Instant) -> u64 {
        let elapsed = now.saturating_duration_since(self.activation).as_millis();
        let passed = elapsed / u128::from(period.milliseconds());
        let latest = u64::try_from(passed).unwrap_or(u64::MAX);
        if latest < self.next {
            return 0;
        }
        let mut missed = latest - self.next + 1;
        if self.pending.is_none() {
            self.pending = Some(self.deadline(period, self.next).unwrap_or(now));
            missed -= 1;
        }
        self.next = latest.saturating_add(1);
        missed
    }
}

fn honoured(contract: &PeriodicCause) -> Period {
    let PeriodicCause {
        every,
        anchor,
        first,
        cadence,
        overlap,
        missed,
        lifetime,
        cancellation,
        host,
    } = contract;
    let HostInputContract { eligibility, .. } = host;
    match anchor {
        Anchor::HostActivation => {}
    }
    match first {
        First::AfterPeriod => {}
    }
    match cadence {
        Cadence::FixedRate => {}
    }
    match overlap {
        Overlap::SerialPerInstance => {}
    }
    match missed {
        Missed::OnePendingDropExcess => {}
    }
    match lifetime {
        Lifetime::HostInstance => {}
    }
    match cancellation {
        Cancellation::StopAcknowledged => {}
    }
    match eligibility {
        Eligibility::HostBoolean => {}
    }
    *every
}
