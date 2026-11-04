//! The data a channel carries.

use ess_domain::binding::{Delivery, Failure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// An event a component published: its qualified name and its fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// The event's qualified name, as the specification declares it.
    pub name: String,
    /// Its fields, keyed by declared field name.
    pub fields: Map<String, Value>,
}

/// A command one binding invokes: its qualified name and its input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    /// The command's qualified name.
    pub name: String,
    /// Its input, keyed by declared input name.
    pub fields: Map<String, Value>,
}

/// Identifies one envelope across its redeliveries.
///
/// Unique within one [`Sender`](crate::Sender) only: a new sender, as after a restart, counts
/// from 1 again, so the id is not a dedup key across restarts or across senders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeliveryId(pub u64);

/// One command on its way to the component that accepts it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    /// The same on every redelivery, so a receiver can recognise one.
    pub id: DeliveryId,
    /// Which binding produced it.
    pub binding: String,
    /// The component that published the event.
    pub from: String,
    /// The component that accepts the command.
    pub to: String,
    /// The binding's delivery: whether the receiver acknowledges it.
    pub delivery: Delivery,
    /// The binding's failure policy, which the receiver applies when the command fails.
    pub on_failure: Failure,
    /// 1 on the first send, one more on each redelivery.
    pub attempt: u32,
    /// The command to invoke.
    pub command: Command,
}

/// What travels on a [`Channel`](crate::Channel): an envelope toward the receiver, or an
/// acknowledgement back toward the sender.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Frame {
    /// An envelope for the receiving component.
    Deliver(Envelope),
    /// The receiver has taken the envelope with this id.
    Acknowledge(DeliveryId),
}
