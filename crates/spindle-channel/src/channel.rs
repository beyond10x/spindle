//! The port both channel implementations provide.

use std::time::Duration;

use crate::model::Frame;

/// One end of a link between two components.
///
/// Frames sent at one end are received, in order, at the other. Delivery guarantees are not the
/// channel's: a channel may lose a frame, and [`Sender`](crate::Sender) redelivers what an
/// `at_least_once` binding needs acknowledged.
///
/// Which errors end a channel: after [`ChannelError::Closed`] or [`ChannelError::Broken`], every
/// later call on that end fails again, with `Closed` or `Broken`. A
/// [`ChannelError::FrameTooLarge`] returned by `receive` ends it too: the stream can no longer be
/// read in step, so every later `receive` and `send` returns `Broken`. Every other error refuses
/// one frame and leaves the channel usable: a frame that does not decode
/// ([`ChannelError::Frame`]) is skipped and the next one is read, and a frame too large to send is
/// refused before a byte of it is written.
pub trait Channel: Send {
    /// Sends one frame toward the other end.
    fn send(&mut self, frame: Frame) -> Result<(), ChannelError>;

    /// The next frame from the other end, waiting at most `wait`; `None` when none arrived.
    /// `Duration::MAX` waits without a bound.
    fn receive(&mut self, wait: Duration) -> Result<Option<Frame>, ChannelError>;
}

/// Why a channel operation failed.
#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    /// The other end is gone. Terminal.
    #[error("the channel is closed")]
    Closed,
    /// The channel can no longer carry frames in step: a send timed out or failed part-way, or a
    /// frame over the limit arrived. Terminal.
    #[error("the channel is broken: {0}")]
    Broken(String),
    /// The transport failed.
    #[error("channel i/o: {0}")]
    Io(#[from] std::io::Error),
    /// A frame could not be encoded or decoded.
    #[error("channel frame: {0}")]
    Frame(String),
    /// One or more destinations failed while the others were served. `done` counts what the
    /// call achieved on the others: envelopes sent or acknowledgements settled.
    #[error("{} destination(s) failed after {done} done: {}", failures.len(), failures.iter().map(|(component, error)| format!("{component}: {error}")).collect::<Vec<_>>().join("; "))]
    Destinations {
        /// What the call achieved despite the failures.
        done: usize,
        /// Each failed destination component, with its error, in the order met.
        failures: Vec<(String, ChannelError)>,
    },
    /// A frame exceeds the size a channel accepts.
    #[error("a frame of {size} bytes exceeds the limit of {limit}")]
    FrameTooLarge {
        /// The frame's size.
        size: usize,
        /// The limit.
        limit: usize,
    },
    /// A frame arrived at the end that does not take it: an acknowledgement at a receiver, or an
    /// envelope at a sender.
    #[error("unexpected frame at {at}: {frame}")]
    Unexpected {
        /// `sender` or `receiver`.
        at: &'static str,
        /// What arrived.
        frame: String,
    },
    /// An envelope arrived for another component.
    #[error("{component} received an envelope for {to}")]
    Misrouted {
        /// The receiving component.
        component: String,
        /// The component the envelope names.
        to: String,
    },
    /// The specification has no component of this name.
    #[error("no component {0} in the specification")]
    UnknownComponent(String),
    /// The component does not publish the event it was asked to send.
    #[error("{component} does not publish {event}")]
    NotPublished {
        /// The sending component.
        component: String,
        /// The event.
        event: String,
    },
    /// A binding leads to a component no channel is connected to.
    #[error("{binding} leads to {component}, and no channel is connected to it")]
    NotConnected {
        /// The binding.
        binding: String,
        /// The destination component.
        component: String,
    },
    /// The event did not map onto its command.
    #[error(transparent)]
    Mapping(#[from] crate::route::MappingError),
}
