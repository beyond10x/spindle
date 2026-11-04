//! Where instances rest between commands.
//!
//! The kernel reads one instance, decides, and commits one [`Change`] to that instance: the
//! runtime default of § 3a, one command on one instance. A store never decides anything; it holds
//! rows and the events that changed them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Event;

/// One instance of an entity: the lifecycle state it holds and its stored fields.
///
/// The identity is the key it is stored under, not a field. A field holding nothing is absent
/// from `fields`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    /// The lifecycle state.
    pub state: String,
    /// The stored fields, by name.
    pub fields: BTreeMap<String, Value>,
}

/// A row as the store holds it, with the version a later commit must name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    /// The row.
    pub row: Row,
    /// The version a later commit must name; it grows with every change.
    pub version: u64,
}

/// What one command did to one instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// The entity, by qualified name.
    pub entity: String,
    /// The instance's identity.
    pub identity: Value,
    /// The version read before deciding, or `None` for an instance the command creates. A store
    /// refuses the change when the instance moved in between.
    pub expected: Option<u64>,
    /// The row after the command.
    pub row: Row,
    /// The events the command emitted, in order.
    pub events: Vec<Event>,
}

/// Why a store could not read or commit.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The instance moved between the read and the commit.
    #[error("{entity} {identity} is at version {actual:?}, not {expected:?}")]
    Conflict {
        /// The entity.
        entity: String,
        /// The identity, as JSON.
        identity: String,
        /// The version the change named.
        expected: Option<u64>,
        /// The version the store holds.
        actual: Option<u64>,
    },
    /// The backing implementation failed.
    #[error("store: {0}")]
    Backend(String),
}

/// The port the kernel reads and commits instances through.
pub trait Store {
    /// The instance of `entity` with this identity, if one exists.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the store cannot be read.
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError>;

    /// Records `change` atomically: the row and its events together, or neither.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] when the instance is not at `change.expected`, and
    /// [`StoreError::Backend`] when the store cannot be written.
    fn commit(&mut self, change: &Change) -> Result<(), StoreError>;

    /// Every instance of `entity`, each with its identity, ordered by the identity's canonical
    /// JSON text: one order on every store, whatever order the instances were written in. Each
    /// carries the row and the version [`Store::load`] would answer for it.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the store cannot be read.
    fn scan(&mut self, entity: &str) -> Result<Vec<(Value, Stored)>, StoreError>;
}

/// The key an instance is held under: its entity and its identity as canonical JSON text.
fn key(entity: &str, identity: &Value) -> (String, String) {
    (entity.to_owned(), identity.to_string())
}

/// The identity an instance key's canonical JSON text names.
pub(crate) fn identity_from_key(text: &str) -> Result<Value, StoreError> {
    serde_json::from_str(text)
        .map_err(|error| StoreError::Backend(format!("a stored identity {text}: {error}")))
}

/// Instances held only while the process runs: `store.ephemeral`, or `store.durable` bound to
/// `memory` in an environment that accepts losing state on restart.
#[derive(Debug, Default)]
pub struct MemoryStore {
    rows: BTreeMap<(String, String), Stored>,
    events: Vec<Event>,
}

impl MemoryStore {
    /// Every event committed so far, in commit order.
    pub fn events(&self) -> &[Event] {
        &self.events
    }
}

impl Store for MemoryStore {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        Ok(self.rows.get(&key(entity, identity)).cloned())
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        let key = key(&change.entity, &change.identity);
        let actual = self.rows.get(&key).map(|stored| stored.version);
        if actual != change.expected {
            return Err(StoreError::Conflict {
                entity: change.entity.clone(),
                identity: key.1,
                expected: change.expected,
                actual,
            });
        }
        self.rows.insert(
            key,
            Stored {
                row: change.row.clone(),
                version: actual.unwrap_or(0) + 1,
            },
        );
        self.events.extend(change.events.iter().cloned());
        Ok(())
    }

    fn scan(&mut self, entity: &str) -> Result<Vec<(Value, Stored)>, StoreError> {
        self.rows
            .iter()
            .filter(|((held, _), _)| held == entity)
            .map(|((_, identity), stored)| Ok((identity_from_key(identity)?, stored.clone())))
            .collect()
    }
}
