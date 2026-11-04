//! The two ends a component holds: a sender for the events it publishes, a receiver for the
//! commands bound to it.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use ess_domain::binding::{Delivery, Failure};

use crate::channel::{Channel, ChannelError};
use crate::model::{DeliveryId, Envelope, Event, Frame};
use crate::outbox::Outbox;
use crate::route::Router;
use spindle_telemetry::{Carried, Sink};

/// How long an `at_least_once` envelope waits for its acknowledgement unless configured.
pub const DEFAULT_ACKNOWLEDGEMENT_TIMEOUT: Duration = Duration::from_secs(1);

struct InFlight {
    envelope: Envelope,
    sent_at: Instant,
}

/// The publishing side of a component: maps its events, holds them in the outbox until their
/// state is durable, sends them, and redelivers unacknowledged `at_least_once` envelopes.
///
/// Each destination is served on its own: a destination whose channel fails is reported in
/// [`ChannelError::Destinations`] and does not stop the call from serving the others.
///
/// The [`DeliveryId`]s a sender mints are unique within that sender only. They start again at 1
/// in a new sender, so after a restart they are not a dedup key: a receiver that must recognise a
/// command across restarts keys it on the command's own identity, as the overlay names it.
pub struct Sender {
    component: String,
    router: Router,
    outbox: Outbox,
    channels: BTreeMap<String, Box<dyn Channel>>,
    in_flight: BTreeMap<DeliveryId, InFlight>,
    retry: Vec<Envelope>,
    timeout: Duration,
    next: u64,
    sink: Sink,
}

impl Sender {
    /// A sender for `component`'s events.
    pub fn new(component: &str, router: Router) -> Result<Self, ChannelError> {
        if router.publishes(component).is_none() {
            return Err(ChannelError::UnknownComponent(component.to_owned()));
        }
        Ok(Self {
            component: component.to_owned(),
            router,
            outbox: Outbox::default(),
            channels: BTreeMap::new(),
            in_flight: BTreeMap::new(),
            retry: Vec::new(),
            timeout: DEFAULT_ACKNOWLEDGEMENT_TIMEOUT,
            next: 1,
            sink: Sink::default(),
        })
    }

    /// How long an `at_least_once` envelope may go unacknowledged before it is redelivered.
    /// `Duration::MAX` never redelivers.
    #[must_use]
    pub fn with_acknowledgement_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Records every envelope sent, redelivered or dropped into `sink`, by binding.
    #[must_use]
    pub fn with_telemetry(mut self, sink: Sink) -> Self {
        self.sink = sink;
        self
    }

    /// Carries envelopes for `component` on `channel`, replacing any channel connected to it.
    pub fn connect(&mut self, component: &str, channel: impl Channel + 'static) {
        self.channels
            .insert(component.to_owned(), Box::new(channel));
    }

    /// Maps `event`, produced by the state at log `position`, and stages one envelope per binding
    /// it triggers; returns how many.
    ///
    /// Refused, with nothing staged, when the component does not publish the event, when the event
    /// does not map, or when a binding leads to a component no channel is connected to.
    pub fn publish(&mut self, position: u64, event: Event) -> Result<usize, ChannelError> {
        let publishes = self
            .router
            .publishes(&self.component)
            .is_some_and(|events| events.contains(&event.name));
        if !publishes {
            return Err(ChannelError::NotPublished {
                component: self.component.clone(),
                event: event.name,
            });
        }
        let mapped = self.router.map(&event)?;
        if let Some((route, _)) = mapped
            .iter()
            .find(|(route, _)| !self.channels.contains_key(&route.to))
        {
            return Err(ChannelError::NotConnected {
                binding: route.binding.clone(),
                component: route.to.clone(),
            });
        }
        let mut envelopes = Vec::with_capacity(mapped.len());
        for (route, command) in mapped {
            envelopes.push(Envelope {
                id: DeliveryId(self.next),
                binding: route.binding.clone(),
                from: self.component.clone(),
                to: route.to.clone(),
                delivery: route.delivery,
                on_failure: route.on_failure,
                attempt: 1,
                command,
            });
            self.next += 1;
        }
        let staged = envelopes.len();
        for envelope in envelopes {
            self.outbox.stage(position, envelope);
        }
        Ok(staged)
    }

    /// The state through log position `through` is durable.
    pub fn mark_durable(&mut self, through: u64) {
        self.outbox.mark_durable(through);
    }

    /// Sends every envelope waiting for another attempt, then every envelope whose state is
    /// durable; returns how many were sent.
    ///
    /// Every envelope is attempted. When a send fails:
    ///
    /// - an `at_least_once` envelope was tracked before it was sent, so it is redelivered once its
    ///   acknowledgement is overdue;
    /// - an `at_most_once` envelope whose binding says `on_failure: retry` waits for the next
    ///   [`flush`](Self::flush) or [`redeliver`](Self::redeliver);
    /// - any other `at_most_once` envelope is lost.
    ///
    /// Failures come back together in [`ChannelError::Destinations`], with the count sent to the
    /// other destinations.
    pub fn flush(&mut self) -> Result<usize, ChannelError> {
        let now = Instant::now();
        let mut pending = std::mem::take(&mut self.retry);
        pending.extend(self.outbox.release());
        let mut sent = 0;
        let mut failures = Vec::new();
        for envelope in pending {
            if envelope.delivery == Delivery::AtLeastOnce {
                self.in_flight.insert(
                    envelope.id,
                    InFlight {
                        envelope: envelope.clone(),
                        sent_at: now,
                    },
                );
            }
            self.attempt(envelope, &mut sent, &mut failures);
        }
        settle(sent, failures)
    }

    /// Reads acknowledgements; returns how many settled an envelope still in flight.
    ///
    /// Waits up to `wait` on each connected channel for the first frame, then takes what is
    /// already there. An acknowledgement for an envelope already settled, or never sent, is
    /// ignored. A channel that fails is reported in [`ChannelError::Destinations`], with the
    /// count settled on the others, which are read all the same.
    pub fn collect_acknowledgements(&mut self, wait: Duration) -> Result<usize, ChannelError> {
        let mut settled = 0;
        let mut failures = Vec::new();
        for (component, channel) in &mut self.channels {
            let mut patience = wait;
            loop {
                match channel.receive(patience) {
                    Ok(None) => break,
                    Ok(Some(Frame::Acknowledge(id))) => {
                        if self.in_flight.remove(&id).is_some() {
                            settled += 1;
                        }
                    }
                    Ok(Some(Frame::Deliver(envelope))) => {
                        failures.push((
                            component.clone(),
                            ChannelError::Unexpected {
                                at: "sender",
                                frame: format!("an envelope from binding {}", envelope.binding),
                            },
                        ));
                        break;
                    }
                    Err(error) => {
                        failures.push((component.clone(), error));
                        break;
                    }
                }
                patience = Duration::ZERO;
            }
        }
        settle(settled, failures)
    }

    /// Resends every envelope waiting for another attempt, and every `at_least_once` envelope
    /// whose acknowledgement is overdue at `now`; returns how many were sent. Failures come back
    /// as [`flush`](Self::flush) returns them.
    pub fn redeliver(&mut self, now: Instant) -> Result<usize, ChannelError> {
        let mut resent = 0;
        let mut failures = Vec::new();
        for envelope in std::mem::take(&mut self.retry) {
            self.attempt(envelope, &mut resent, &mut failures);
        }
        let timeout = self.timeout;
        for flight in self.in_flight.values_mut() {
            let overdue = flight
                .sent_at
                .checked_add(timeout)
                .is_some_and(|due| due <= now);
            if !overdue {
                continue;
            }
            flight.envelope.attempt += 1;
            flight.sent_at = now;
            let envelope = flight.envelope.clone();
            let to = envelope.to.clone();
            let binding = envelope.binding.clone();
            if let Err(error) = send(&mut self.channels, envelope) {
                failures.push((to, error));
            } else {
                self.sink.carried(&binding, Carried::Redelivered);
                resent += 1;
            }
        }
        settle(resent, failures)
    }

    /// Envelopes not yet sent: held in the outbox, or waiting for another attempt.
    pub fn staged(&self) -> usize {
        self.outbox.len() + self.retry.len()
    }

    /// `at_least_once` envelopes sent and not yet acknowledged.
    pub fn unacknowledged(&self) -> usize {
        self.in_flight.len()
    }

    /// One send; on failure, an `at_most_once` envelope under `on_failure: retry` waits for the
    /// next attempt.
    fn attempt(
        &mut self,
        envelope: Envelope,
        sent: &mut usize,
        failures: &mut Vec<(String, ChannelError)>,
    ) {
        let again = (envelope.delivery == Delivery::AtMostOnce
            && envelope.on_failure == Failure::Retry)
            .then(|| envelope.clone());
        let to = envelope.to.clone();
        let binding = envelope.binding.clone();
        let first = envelope.attempt == 1;
        let tracked = envelope.delivery == Delivery::AtLeastOnce;
        match send(&mut self.channels, envelope) {
            Ok(()) => {
                self.sink.carried(
                    &binding,
                    if first {
                        Carried::Sent
                    } else {
                        Carried::Redelivered
                    },
                );
                *sent += 1;
            }
            Err(error) => {
                match again {
                    Some(mut envelope) => {
                        envelope.attempt += 1;
                        self.retry.push(envelope);
                    }
                    None if !tracked => self.sink.carried(&binding, Carried::Dropped),
                    None => {}
                }
                failures.push((to, error));
            }
        }
    }
}

fn send(
    channels: &mut BTreeMap<String, Box<dyn Channel>>,
    envelope: Envelope,
) -> Result<(), ChannelError> {
    match channels.get_mut(&envelope.to) {
        Some(channel) => channel.send(Frame::Deliver(envelope)),
        None => Err(ChannelError::NotConnected {
            binding: envelope.binding,
            component: envelope.to,
        }),
    }
}

fn settle(done: usize, failures: Vec<(String, ChannelError)>) -> Result<usize, ChannelError> {
    if failures.is_empty() {
        Ok(done)
    } else {
        Err(ChannelError::Destinations { done, failures })
    }
}

/// The receiving side of a component: takes envelopes addressed to it and acknowledges the
/// `at_least_once` ones once their command was taken.
///
/// A redelivered envelope carries the id it was first sent with; recognising a duplicate is the
/// receiving component's, as the design's step semantics say.
pub struct Receiver {
    component: String,
    channel: Box<dyn Channel>,
    sink: Sink,
}

impl Receiver {
    /// A receiver for `component` on `channel`.
    pub fn new(component: &str, channel: impl Channel + 'static) -> Self {
        Self {
            component: component.to_owned(),
            channel: Box::new(channel),
            sink: Sink::default(),
        }
    }

    /// Records every envelope delivered or discarded into `sink`, by binding.
    #[must_use]
    pub fn with_telemetry(mut self, sink: Sink) -> Self {
        self.sink = sink;
        self
    }

    /// The next envelope, waiting at most `wait`; `None` when none arrived.
    pub fn receive(&mut self, wait: Duration) -> Result<Option<Envelope>, ChannelError> {
        let envelope = self.take(wait)?;
        if let Some(envelope) = &envelope {
            self.sink.carried(&envelope.binding, Carried::Delivered);
        }
        Ok(envelope)
    }

    /// Takes every envelope already waiting off the channel without handing it on; returns how
    /// many. Each is recorded as dropped, not delivered.
    pub fn discard(&mut self) -> Result<usize, ChannelError> {
        let mut discarded = 0;
        while let Some(envelope) = self.take(Duration::ZERO)? {
            self.sink.carried(&envelope.binding, Carried::Dropped);
            discarded += 1;
        }
        Ok(discarded)
    }

    fn take(&mut self, wait: Duration) -> Result<Option<Envelope>, ChannelError> {
        match self.channel.receive(wait)? {
            None => Ok(None),
            Some(Frame::Deliver(envelope)) if envelope.to == self.component => Ok(Some(envelope)),
            Some(Frame::Deliver(envelope)) => Err(ChannelError::Misrouted {
                component: self.component.clone(),
                to: envelope.to,
            }),
            Some(Frame::Acknowledge(id)) => Err(ChannelError::Unexpected {
                at: "receiver",
                frame: format!("an acknowledgement of {}", id.0),
            }),
        }
    }

    /// Acknowledges an `at_least_once` envelope; does nothing for an `at_most_once` one.
    pub fn acknowledge(&mut self, envelope: &Envelope) -> Result<(), ChannelError> {
        match envelope.delivery {
            Delivery::AtLeastOnce => self.channel.send(Frame::Acknowledge(envelope.id)),
            Delivery::AtMostOnce => Ok(()),
        }
    }
}
