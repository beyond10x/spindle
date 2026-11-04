//! Envelopes held until the state that produced them is durable.

use crate::model::Envelope;

/// Envelopes staged against the log position of the state that produced them.
///
/// Nothing is released before its position is marked durable; then it is released in the order
/// staged. The outbox itself is memory: surviving a crash is the producer's log's job, which
/// replays the events of every durable position that was not yet sent.
#[derive(Debug, Default)]
pub struct Outbox {
    staged: Vec<(u64, Envelope)>,
    durable: Option<u64>,
}

impl Outbox {
    /// Holds `envelope` until `position` is durable.
    pub fn stage(&mut self, position: u64, envelope: Envelope) {
        self.staged.push((position, envelope));
    }

    /// Every position up to and including `through` is durable. Never moves backwards.
    pub fn mark_durable(&mut self, through: u64) {
        self.durable = Some(self.durable.map_or(through, |durable| durable.max(through)));
    }

    /// Takes every envelope whose state is durable, in the order staged.
    pub fn release(&mut self) -> Vec<Envelope> {
        let Some(durable) = self.durable else {
            return Vec::new();
        };
        let (released, held) = std::mem::take(&mut self.staged)
            .into_iter()
            .partition::<Vec<_>, _>(|(position, _)| *position <= durable);
        self.staged = held;
        released.into_iter().map(|(_, envelope)| envelope).collect()
    }

    /// How many envelopes are held.
    pub fn len(&self) -> usize {
        self.staged.len()
    }

    /// Whether nothing is held.
    pub fn is_empty(&self) -> bool {
        self.staged.is_empty()
    }
}
