//! Running every component of one ESS system in one process.
//!
//! [`Host::new`] admits each component on its own against an environment
//! ([`admit`]), opens one kernel store per component with the implementation admission bound,
//! and connects every pair of components an event binding joins with an in-process channel.
//! [`Host::submit`] runs a command on the component that accepts it; [`Host::start`] starts a
//! periodic binding with its host context, and [`Host::advance`] runs every tick due on the
//! injected [`Clock`]. Each event a command emits is carried to the commands it binds to, and each
//! of those runs on its own component, until nothing is left in flight. [`Host::view`] answers a
//! view from the store of the component that owns its source.
//!
//! [`Host::component`] runs one component on its own instead, its bindings carried over TCP links:
//! [`Host::connect`] links each peer its events lead to, [`Host::accept`] takes the links its
//! peers open, and [`Host::deliver`] runs what arrives.
//!
//! The clock is a port: the host never reads the time on its own, so a [`ManualClock`] runs a
//! system with no sleeping. [`rally`] draws the `rally` example's terminal frame from a view
//! answer.

mod admission;
mod host;
pub mod rally;
mod view;

pub use admission::{Placements, Refusal, admit};
pub use host::{Cause, DELIVERY_LIMIT, Failed, Host, HostError, Ran, RefusedCommand, Report};
pub use spindle_timer::{Clock, ManualClock, SystemClock};
