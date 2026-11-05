//! Capabilities: what a specification needs from the runtime, and whether an environment has it.
//!
//! A specification says what an application means: entities, commands, guards, events, bindings,
//! views. It never says how state is stored, how a timer fires or how a message crosses a process.
//! Those are capabilities, and Spindle derives them from the specification ([`derive`]). An author
//! never writes a capability down, so a specification cannot claim to need less than it does.
//!
//! An [`Environment`] says which implementations provide which capabilities where Spindle runs, in
//! order of preference. [`admit`] binds every required capability to the first implementation the
//! environment offers, or refuses and names every capability that has none. The same
//! specification can therefore run on a laptop with an in-process channel and in a microVM with a
//! TCP one, and admission is the only step that differs.

mod admission;
mod derive;
mod environment;
pub mod mint;

pub use admission::{Admission, Binding, admit};
pub use derive::{Capability, Reason, Requirements, derive};
pub use environment::{Environment, EnvironmentError, FORMAT as ENVIRONMENT_FORMAT};
