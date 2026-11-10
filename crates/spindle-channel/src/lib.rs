//! Carrying an event published by one component to the command it binds to in another.
//!
//! Routing comes from the compiled IR only: a [`ResolvedBinding`](ess_compiler::ir::ResolvedBinding)
//! with an event cause names the event, its `where` condition, the command, the mapping between
//! them, its `delivery` and its `on_failure`; the component that publishes the event is the route's
//! source and the component that accepts the command its destination. [`Router::from_ir`] reads
//! every such binding once; periodic bindings are the timer's and are not routed here.
//!
//! An event and a command are plain data, `{name, fields}`, with `fields` a JSON object keyed by
//! the declared field names. A [`Sender`] maps each event it is given into one [`Envelope`] per
//! binding and holds it in its [`Outbox`] until the producer marks the state that produced the event
//! durable; only then does [`Sender::flush`] put it on a [`Channel`]. A [`Receiver`] takes envelopes
//! off a channel and acknowledges the `at_least_once` ones; the sender redelivers an unacknowledged
//! envelope once its acknowledgement is overdue. `at_most_once` envelopes are sent once and never
//! tracked.
//!
//! Two channels implement the port: [`in_process`], a pair of queues, and [`tcp`], length-prefixed
//! JSON frames over a TCP stream.

mod channel;
mod endpoint;
pub mod in_process;
mod model;
mod outbox;
mod route;
pub mod tcp;

pub use channel::{Channel, ChannelError};
pub use endpoint::{DEFAULT_ACKNOWLEDGEMENT_TIMEOUT, Receiver, Sender, Unmet};
pub use model::{Command, DeliveryId, Envelope, Event, Frame};
pub use outbox::Outbox;
pub use route::{MappingError, Route, RouteError, Router};
