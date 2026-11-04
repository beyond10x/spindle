//! Which counters an instance of one component has, derived from the compiled IR.

use std::collections::BTreeSet;
use std::fmt;

use ess_compiler::EssIr;
use serde::Serialize;

use crate::port::{Carried, Failure, Timed};

/// Every counter and histogram an instance of one component reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Catalogue {
    /// The component.
    pub component: String,
    /// Its counters and histograms, in a stable order: commands, events, bindings, timers, store.
    pub counters: Vec<Counter>,
}

/// One entry of a [`Catalogue`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Counter {
    /// The dotted name, such as `command.rally.table.Tick.moved`.
    pub name: String,
    /// A counter or a histogram.
    pub kind: Kind,
    /// The part of the runtime that measures it.
    pub source: Source,
    /// What it counts, structured.
    #[serde(skip)]
    pub key: Key,
}

/// A monotonic count or a distribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A monotonic count.
    Counter,
    /// Fixed buckets; see [`DURATION_BOUNDS_US`](crate::DURATION_BOUNDS_US).
    Histogram,
}

/// The part of the runtime that measures a counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Around `spindle_kernel::execute`.
    Kernel,
    /// The store port.
    Store,
    /// The channel endpoints.
    Channel,
    /// The timer.
    Timer,
}

/// What a counter counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    /// `command.<Command>.<outcome>`: the model selected that declared outcome.
    Outcome {
        /// The command.
        command: String,
        /// The outcome.
        outcome: String,
    },
    /// `command.<Command>.failed.<kind>`: the command did not execute.
    Failed {
        /// The command.
        command: String,
        /// Why.
        failure: Failure,
    },
    /// `command.<Command>.duration_us`: how long `execute` took, in microseconds.
    Duration {
        /// The command.
        command: String,
    },
    /// `event.<Event>.appended`.
    Appended {
        /// The event.
        event: String,
    },
    /// `binding.<binding>.<sent|delivered|redelivered|dropped>`.
    Carried {
        /// The binding.
        binding: String,
        /// What happened to the envelope.
        carried: Carried,
    },
    /// `timer.<binding>.<fired|late|dropped>`.
    Timed {
        /// The binding.
        binding: String,
        /// What the timer did.
        timed: Timed,
    },
    /// `store.rows_written`.
    RowsWritten,
    /// `store.bytes_written`.
    BytesWritten,
}

impl fmt::Display for Kind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Counter => "counter",
            Self::Histogram => "histogram",
        })
    }
}

impl fmt::Display for Source {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Kernel => "kernel",
            Self::Store => "store",
            Self::Channel => "channel",
            Self::Timer => "timer",
        })
    }
}

/// The catalogue of `component` in `ir`; `None` when `ir` has no such component.
///
/// Derived as capabilities are: one counter per outcome of each command the component accepts
/// and per way the kernel can fail to execute it, a duration histogram per command, one counter
/// per event it publishes or its commands emit, the channel counters of each event binding that
/// starts or ends at it, the timer counters of each periodic binding whose command it accepts,
/// and the store counters where it owns an entity.
pub fn catalogue(ir: &EssIr, component: &str) -> Option<Catalogue> {
    let declared = ir
        .components()
        .values()
        .find(|candidate| candidate.name.as_str() == component)?;
    let mut counters = Vec::new();
    let mut add = |key: Key| {
        let (name, kind, source) = describe(&key);
        counters.push(Counter {
            name,
            kind,
            source,
            key,
        });
    };

    let mut events = declared
        .publishes
        .iter()
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>();
    for accepted in &declared.accepts {
        let command = accepted.to_string();
        for outcome in &ir.command(accepted).outcomes {
            add(Key::Outcome {
                command: command.clone(),
                outcome: outcome.name.to_string(),
            });
            events.extend(outcome.emits.iter().map(ToString::to_string));
        }
        for failure in Failure::ALL {
            if failure != Failure::UnknownCommand {
                add(Key::Failed {
                    command: command.clone(),
                    failure,
                });
            }
        }
        add(Key::Duration { command });
    }
    for event in events {
        add(Key::Appended { event });
    }

    let publishes = |event: &str| {
        declared
            .publishes
            .iter()
            .any(|published| published.to_string() == event)
    };
    let accepts = |command: &str| {
        declared
            .accepts
            .iter()
            .any(|accepted| accepted.to_string() == command)
    };
    for binding in ir.bindings().values() {
        let Some(event) = binding.cause.event() else {
            continue;
        };
        let name = binding.name.as_str();
        let sends = publishes(&event.to_string());
        let receives = accepts(&binding.command.to_string());
        for carried in Carried::ALL {
            let measured = match carried {
                Carried::Sent | Carried::Redelivered => sends,
                Carried::Delivered => receives,
                Carried::Dropped => sends || receives,
            };
            if measured {
                add(Key::Carried {
                    binding: name.to_owned(),
                    carried,
                });
            }
        }
    }
    for binding in ir.bindings().values() {
        if binding.cause.periodic().is_some() && accepts(&binding.command.to_string()) {
            for timed in Timed::ALL {
                add(Key::Timed {
                    binding: binding.name.as_str().to_owned(),
                    timed,
                });
            }
        }
    }
    if ir
        .entities()
        .values()
        .any(|entity| declared.owns.contains(&entity.domain))
    {
        add(Key::RowsWritten);
        add(Key::BytesWritten);
    }

    Some(Catalogue {
        component: declared.name.as_str().to_owned(),
        counters,
    })
}

fn describe(key: &Key) -> (String, Kind, Source) {
    match key {
        Key::Outcome { command, outcome } => (
            format!("command.{command}.{outcome}"),
            Kind::Counter,
            Source::Kernel,
        ),
        Key::Failed { command, failure } => (
            format!("command.{command}.failed.{}", failure.as_str()),
            Kind::Counter,
            Source::Kernel,
        ),
        Key::Duration { command } => (
            format!("command.{command}.duration_us"),
            Kind::Histogram,
            Source::Kernel,
        ),
        Key::Appended { event } => (
            format!("event.{event}.appended"),
            Kind::Counter,
            Source::Store,
        ),
        Key::Carried { binding, carried } => (
            format!("binding.{binding}.{}", carried.as_str()),
            Kind::Counter,
            Source::Channel,
        ),
        Key::Timed { binding, timed } => (
            format!("timer.{binding}.{}", timed.as_str()),
            Kind::Counter,
            Source::Timer,
        ),
        Key::RowsWritten => (
            "store.rows_written".to_owned(),
            Kind::Counter,
            Source::Store,
        ),
        Key::BytesWritten => (
            "store.bytes_written".to_owned(),
            Kind::Counter,
            Source::Store,
        ),
    }
}
