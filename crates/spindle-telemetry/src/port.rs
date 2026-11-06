//! The port the runtime records through.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

/// Where the runtime reports what it did.
///
/// Called on the command path, so an implementation must not block and should not allocate: the
/// default, [`Counters`](crate::Counters), increments an atomic per record.
pub trait Telemetry: Send + Sync {
    /// Takes one record.
    fn record(&self, record: Record<'_>);
}

/// One thing the runtime did. Names are the specification's, as the ESS handles display them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Record<'a> {
    /// The kernel executed a command, or failed to.
    Command {
        /// The command's qualified name.
        command: &'a str,
        /// What came of it.
        outcome: Outcome<'a>,
        /// How long `execute` took.
        elapsed: Duration,
    },
    /// The store committed an event.
    Appended {
        /// The event's qualified name.
        event: &'a str,
    },
    /// The store committed a change.
    Written {
        /// Rows written.
        rows: u64,
        /// Bytes written: the JSON encoding of the rows and their events.
        bytes: u64,
    },
    /// A channel endpoint carried an envelope of an event binding.
    Carried {
        /// The binding.
        binding: &'a str,
        /// What happened to the envelope.
        carried: Carried,
    },
    /// The receiving component invoked the command of an `on_failure: retry` binding, or gave
    /// up on it.
    Retried {
        /// The binding.
        binding: &'a str,
        /// What happened to the invocation.
        retried: Retried,
    },
    /// The timer did something for a periodic binding.
    Timed {
        /// The binding.
        binding: &'a str,
        /// What it did.
        timed: Timed,
        /// How many times.
        count: u64,
    },
}

/// What came of one command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome<'a> {
    /// The model selected this declared outcome; a refusing outcome is one of these.
    Declared(&'a str),
    /// The command did not execute.
    Failed(Failure),
}

/// Why a command did not execute: the kernel's error kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Failure {
    /// No command of that name.
    UnknownCommand,
    /// A construct the kernel does not run.
    Unsupported,
    /// The input is not what the command declares.
    InvalidInput,
    /// A guard is undecidable.
    Undecidable,
    /// The decider gave no answer.
    Undecided,
    /// A determined value could not be computed.
    NoValue,
    /// A written value is not of its declared type.
    InvalidValue,
    /// An entity invariant would break.
    BrokenInvariant,
    /// The model declares no outcome for the request.
    Undeclared,
    /// The store could not read or commit.
    Store,
    /// The actor it was sent as holds no grant for it.
    NotGranted,
}

impl Failure {
    /// Every failure, in declaration order.
    pub const ALL: [Self; 11] = [
        Self::UnknownCommand,
        Self::Unsupported,
        Self::InvalidInput,
        Self::Undecidable,
        Self::Undecided,
        Self::NoValue,
        Self::InvalidValue,
        Self::BrokenInvariant,
        Self::Undeclared,
        Self::Store,
        Self::NotGranted,
    ];

    /// The kernel's name for it, as `ExecuteError::kind` gives it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnknownCommand => "unknown_command",
            Self::Unsupported => "unsupported",
            Self::InvalidInput => "invalid_input",
            Self::Undecidable => "undecidable",
            Self::Undecided => "undecided",
            Self::NoValue => "no_value",
            Self::InvalidValue => "invalid_value",
            Self::BrokenInvariant => "broken_invariant",
            Self::Undeclared => "undeclared",
            Self::Store => "store",
            Self::NotGranted => "not_granted",
        }
    }

    pub(crate) fn index(self) -> usize {
        self as usize
    }
}

/// What a channel endpoint did with one envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Carried {
    /// Sent for the first time.
    Sent,
    /// Taken off the channel at the receiving component.
    Delivered,
    /// Sent again: its acknowledgement was overdue, or its first send failed.
    Redelivered,
    /// Lost: an `at_most_once` envelope whose send failed, or one discarded at the receiver.
    Dropped,
}

impl Carried {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 4] = [
        Self::Sent,
        Self::Delivered,
        Self::Redelivered,
        Self::Dropped,
    ];

    /// Its name in a counter.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sent => "sent",
            Self::Delivered => "delivered",
            Self::Redelivered => "redelivered",
            Self::Dropped => "dropped",
        }
    }

    pub(crate) fn index(self) -> usize {
        self as usize
    }
}

/// What came of one invocation of an `on_failure: retry` binding's command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Retried {
    /// One attempt was made: the first and every retry.
    Attempted,
    /// Every attempt the retry limit allows failed; the invocation is given up.
    Exhausted,
}

impl Retried {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 2] = [Self::Attempted, Self::Exhausted];

    /// Its name in a counter.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Attempted => "attempted",
            Self::Exhausted => "exhausted",
        }
    }

    pub(crate) fn index(self) -> usize {
        self as usize
    }
}

/// What the timer did for one periodic binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Timed {
    /// A tick was yielded.
    Fired,
    /// A tick was yielded at or after the next deadline of its grid: it waited a whole period.
    Late,
    /// Deadlines dropped under `missed: one_pending_drop_excess`.
    Dropped,
}

impl Timed {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 3] = [Self::Fired, Self::Late, Self::Dropped];

    /// Its name in a counter.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fired => "fired",
            Self::Late => "late",
            Self::Dropped => "dropped",
        }
    }

    pub(crate) fn index(self) -> usize {
        self as usize
    }
}

/// A handle on a [`Telemetry`], given to the kernel's caller, the store, the channel endpoints
/// and the timer. The default records nothing.
#[derive(Clone, Default)]
pub struct Sink(Option<Arc<dyn Telemetry>>);

impl Sink {
    /// A sink recording into `telemetry`.
    pub fn new(telemetry: Arc<dyn Telemetry>) -> Self {
        Self(Some(telemetry))
    }

    /// Records `record`; does nothing for the default sink.
    pub fn record(&self, record: Record<'_>) {
        if let Some(telemetry) = &self.0 {
            telemetry.record(record);
        }
    }

    /// Records what a channel endpoint did with one envelope of `binding`.
    pub fn carried(&self, binding: &str, carried: Carried) {
        self.record(Record::Carried { binding, carried });
    }

    /// Records what came of one invocation of the retry binding `binding`.
    pub fn retried(&self, binding: &str, retried: Retried) {
        self.record(Record::Retried { binding, retried });
    }

    /// Records what the timer did, `count` times, for `binding`.
    pub fn timed(&self, binding: &str, timed: Timed, count: u64) {
        if count > 0 {
            self.record(Record::Timed {
                binding,
                timed,
                count,
            });
        }
    }
}

impl fmt::Debug for Sink {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(if self.0.is_some() {
            "Sink(recording)"
        } else {
            "Sink(none)"
        })
    }
}
