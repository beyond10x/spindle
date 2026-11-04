//! What an instance did, counted while it runs.
//!
//! Every effect of a spindle instance passes through the runtime, so the runtime counts it
//! without the specification saying a word about metrics: the caller of
//! `spindle_kernel::execute` records each command's outcome and duration ([`Sink::executed`]), a
//! [`CountingStore`] each committed row, its bytes and its events, the channel endpoints each
//! envelope sent, delivered, redelivered or dropped, and the timer each tick fired, late or
//! dropped.
//!
//! # The port
//!
//! Each of them holds a [`Sink`], a handle on a [`Telemetry`]. The default sink records nothing;
//! [`Counters`] is the default implementation: one atomic per counter and fixed buckets per
//! histogram, laid out once from a [`Catalogue`]. Recording is a lookup in maps built at start
//! and one atomic increment: no lock and no allocation on the command path.
//!
//! # The catalogue
//!
//! Like capabilities, the counters are derived from the compiled IR, never authored
//! ([`catalogue`]). For one component:
//!
//! | counter | one per |
//! |---|---|
//! | `command.<Command>.<outcome>` | outcome of each command it accepts |
//! | `command.<Command>.failed.<kind>` | way the kernel can fail to execute it ([`Failure`]) |
//! | `command.<Command>.duration_us` (histogram) | command it accepts |
//! | `event.<Event>.appended` | event it publishes or its commands emit |
//! | `binding.<binding>.<sent\|redelivered\|dropped>` | event binding whose event it publishes |
//! | `binding.<binding>.<delivered\|dropped>` | event binding whose command it accepts |
//! | `timer.<binding>.<fired\|late\|dropped>` | periodic binding whose command it accepts |
//! | `store.rows_written`, `store.bytes_written` | component owning an entity |
//!
//! Names use the ESS handles as they display, so a name is the name in the specification. An
//! outcome name is lower-kebab and holds no dot, so `failed.<kind>` never collides with one.

mod catalogue;
mod counters;
#[cfg(feature = "kernel")]
mod kernel;
mod port;

pub use catalogue::{Catalogue, Counter, Key, Kind, Source, catalogue};
pub use counters::{CounterValue, Counters, DURATION_BOUNDS_US, HistogramValue, Snapshot};
#[cfg(feature = "kernel")]
pub use kernel::CountingStore;
pub use port::{Carried, Failure, Outcome, Record, Sink, Telemetry, Timed};
