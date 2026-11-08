//! Recording around the kernel: what `execute` answered, and what the store committed.

use std::io;
use std::time::Duration;

use serde_json::Value;
use spindle_kernel::{Change, ExecuteError, Executed, Removal, Store, StoreError, Stored, Write};

use crate::port::{Failure, Outcome, Record, Sink};

impl From<&ExecuteError> for Failure {
    fn from(error: &ExecuteError) -> Self {
        match error {
            ExecuteError::UnknownCommand(_) => Self::UnknownCommand,
            ExecuteError::Unsupported { .. } => Self::Unsupported,
            ExecuteError::InvalidInput(_) => Self::InvalidInput,
            ExecuteError::Undecidable { .. } => Self::Undecidable,
            ExecuteError::Undecided { .. } => Self::Undecided,
            ExecuteError::NoValue { .. } => Self::NoValue,
            ExecuteError::InvalidValue(_) => Self::InvalidValue,
            ExecuteError::BrokenInvariant { .. } => Self::BrokenInvariant,
            ExecuteError::Undeclared { .. } => Self::Undeclared,
            ExecuteError::Store(_) => Self::Store,
            ExecuteError::NotGranted { .. } => Self::NotGranted,
        }
    }
}

impl Sink {
    /// Records what `spindle_kernel::execute` answered for `command`, and how long it took.
    pub fn executed(
        &self,
        command: &str,
        result: &Result<Executed, ExecuteError>,
        elapsed: Duration,
    ) {
        let outcome = match result {
            Ok(executed) => Outcome::Declared(&executed.outcome),
            Err(error) => Outcome::Failed(Failure::from(error)),
        };
        self.record(Record::Command {
            command,
            outcome,
            elapsed,
        });
    }
}

/// A [`Store`] that records every change it commits: `store.rows_written`,
/// `store.bytes_written` and `event.<Event>.appended`.
///
/// Bytes are the length of the JSON encoding of the row and of each event, the same for every
/// backend; they are counted into a writer that keeps no bytes.
#[derive(Debug)]
pub struct CountingStore<S> {
    inner: S,
    sink: Sink,
}

impl<S: Store> CountingStore<S> {
    /// Counts what `inner` commits into `sink`.
    pub fn new(inner: S, sink: Sink) -> Self {
        Self { inner, sink }
    }

    /// The wrapped store.
    pub fn inner(&self) -> &S {
        &self.inner
    }

    /// The wrapped store, mutably.
    pub fn inner_mut(&mut self) -> &mut S {
        &mut self.inner
    }
}

impl<S: Store> Store for CountingStore<S> {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.inner.load(entity, identity)
    }

    fn scan(&mut self, entity: &str) -> Result<spindle_kernel::Scan, StoreError> {
        self.inner.scan(entity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.inner.commit(change)?;
        self.committed(change);
        Ok(())
    }

    /// The writes land in `inner` together, then count as each would alone.
    fn commit_all(&mut self, writes: &[Write]) -> Result<(), StoreError> {
        self.inner.commit_all(writes)?;
        for write in writes {
            match write {
                Write::Commit(change) => self.committed(change),
                Write::Remove(removal) => self.removed(removal),
            }
        }
        Ok(())
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.inner.remove(removal)?;
        self.removed(removal);
        Ok(())
    }
}

impl<S: Store> CountingStore<S> {
    /// Counts `change`, which `inner` committed.
    fn committed(&self, change: &Change) {
        let mut length = Length::default();
        let encoded = serde_json::to_writer(&mut length, &change.row).is_ok()
            && change
                .events
                .iter()
                .all(|event| serde_json::to_writer(&mut length, event).is_ok());
        if encoded {
            self.sink.record(Record::Written {
                rows: 1,
                bytes: length.0,
            });
        }
        for event in &change.events {
            self.sink.record(Record::Appended { event: &event.name });
        }
    }

    /// Counts `removal`, which `inner` committed: one row written, its bytes those of its
    /// events, since no row rests.
    fn removed(&self, removal: &Removal) {
        let mut length = Length::default();
        let encoded = removal
            .events
            .iter()
            .all(|event| serde_json::to_writer(&mut length, event).is_ok());
        if encoded {
            self.sink.record(Record::Written {
                rows: 1,
                bytes: length.0,
            });
        }
        for event in &removal.events {
            self.sink.record(Record::Appended { event: &event.name });
        }
    }
}

/// A writer that counts the bytes written to it and keeps none.
#[derive(Debug, Default)]
struct Length(u64);

impl io::Write for Length {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self.0.saturating_add(bytes.len() as u64);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
