//! Where instances rest between commands.
//!
//! The kernel reads one instance, decides, and commits one [`Change`] to that instance: the
//! runtime default of § 3a, one command on one instance. A set outcome (`instances:`) writes every
//! row it changes in one [`Store::commit_all`]. A store never decides anything; it holds rows and
//! the events that changed them.

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

/// One command removing one instance: `deletes:`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removal {
    /// The entity, by qualified name.
    pub entity: String,
    /// The instance's identity.
    pub identity: Value,
    /// The version read before deciding. A store refuses the removal when the instance moved in
    /// between.
    pub expected: u64,
    /// The events the command emitted, in order.
    pub events: Vec<Event>,
}

/// One write of several a command makes together ([`Store::commit_all`]): a set outcome's change
/// or removal of one row it selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Write {
    /// The instance rests on, holding this.
    Commit(Change),
    /// The instance is removed.
    Remove(Removal),
}

impl Write {
    /// The entity and identity of the instance written.
    pub(crate) fn instance(&self) -> (&str, &Value) {
        match self {
            Self::Commit(change) => (&change.entity, &change.identity),
            Self::Remove(removal) => (&removal.entity, &removal.identity),
        }
    }

    /// The version the write names: `None` for a creation.
    pub(crate) fn expected(&self) -> Option<u64> {
        match self {
            Self::Commit(change) => change.expected,
            Self::Remove(removal) => Some(removal.expected),
        }
    }
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

    /// Removes the instance `removal` names and records its events, atomically: once it returns,
    /// [`Store::load`] answers `None` for the identity, [`Store::scan`] leaves it out, and a later
    /// [`Change`] with `expected: None` may create it again.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] when the instance is absent or not at `removal.expected`, and
    /// [`StoreError::Backend`] when the store cannot be written.
    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError>;

    /// Records every write of `writes`, in order, each with its events, atomically: all of them,
    /// or, where one is refused, none. Each names the version it read, as [`Store::commit`] and
    /// [`Store::remove`] do, and a later write of an instance an earlier one wrote names the
    /// version that one left.
    ///
    /// The memory, SQLite and IndexedDB stores write them in one transaction. This default, for a
    /// store that can write one instance at a time only, first refuses a write that names a
    /// version the store does not hold, writing nothing; it then writes each in turn, and where
    /// one is still refused (another writer moved it in between), writes back, with no events,
    /// every instance it already wrote, so the rows are as they were though their versions have
    /// grown. Such a store records the events of a write with it: a caller that keeps them on the
    /// last write records none unless every write landed.
    ///
    /// # Errors
    /// [`StoreError::Conflict`] when an instance is not at the version its write names, and
    /// [`StoreError::Backend`] when the store cannot be written.
    fn commit_all(&mut self, writes: &[Write]) -> Result<(), StoreError> {
        one_at_a_time(self, writes)
    }

    /// Every instance of `entity`, each with its identity, ordered by the identity's canonical
    /// JSON text: one order on every store, whatever order the instances were written in. Each
    /// carries the row and the version [`Store::load`] would answer for it.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the store cannot be read.
    fn scan(&mut self, entity: &str) -> Result<Vec<(Value, Stored)>, StoreError>;
}

/// [`Store::commit_all`] on a store that writes one instance at a time: every write's version
/// checked first, then each written in turn, and those written undone where a later one is
/// refused.
fn one_at_a_time<S: Store + ?Sized>(store: &mut S, writes: &[Write]) -> Result<(), StoreError> {
    for write in first_writes(writes) {
        let (entity, identity) = write.instance();
        let actual = store.load(entity, identity)?.map(|stored| stored.version);
        if actual != write.expected() {
            return Err(StoreError::Conflict {
                entity: entity.to_owned(),
                identity: identity.to_string(),
                expected: write.expected(),
                actual,
            });
        }
    }
    let mut written: Vec<(&str, &Value, Option<Row>)> = Vec::with_capacity(writes.len());
    for write in writes {
        let (entity, identity) = write.instance();
        let before = store.load(entity, identity)?.map(|stored| stored.row);
        let landed = match write {
            Write::Commit(change) => store.commit(change),
            Write::Remove(removal) => store.remove(removal),
        };
        if let Err(error) = landed {
            for (entity, identity, before) in written.into_iter().rev() {
                // Best effort: the refusal is what the caller is answered, whatever this finds.
                let _ = restore(store, entity, identity, before);
            }
            return Err(error);
        }
        written.push((entity, identity, before));
    }
    Ok(())
}

/// The first write of each instance in `writes`, in order: those whose named version is the one
/// the store holds before any of them lands.
pub(crate) fn first_writes(writes: &[Write]) -> Vec<&Write> {
    let mut seen = std::collections::BTreeSet::new();
    writes
        .iter()
        .filter(|write| {
            let (entity, identity) = write.instance();
            seen.insert(key(entity, identity))
        })
        .collect()
}

/// Writes the instance of `entity` under `identity` back to `before`, with no events: the row it
/// held, or, where it held none, no row.
fn restore<S: Store + ?Sized>(
    store: &mut S,
    entity: &str,
    identity: &Value,
    before: Option<Row>,
) -> Result<(), StoreError> {
    let now = store.load(entity, identity)?.map(|stored| stored.version);
    match (before, now) {
        (Some(row), expected) => store.commit(&Change {
            entity: entity.to_owned(),
            identity: identity.clone(),
            expected,
            row,
            events: Vec::new(),
        }),
        (None, Some(expected)) => store.remove(&Removal {
            entity: entity.to_owned(),
            identity: identity.clone(),
            expected,
            events: Vec::new(),
        }),
        (None, None) => Ok(()),
    }
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
    /// The last version of each removed instance, so one created again under its identity goes
    /// on from it and no commit that read the removed row matches the new one.
    removed: BTreeMap<(String, String), u64>,
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
        let version = actual
            .or_else(|| self.removed.get(&key).copied())
            .unwrap_or(0)
            + 1;
        self.rows.insert(
            key,
            Stored {
                row: change.row.clone(),
                version,
            },
        );
        self.events.extend(change.events.iter().cloned());
        Ok(())
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        let key = key(&removal.entity, &removal.identity);
        let actual = self.rows.get(&key).map(|stored| stored.version);
        if actual != Some(removal.expected) {
            return Err(StoreError::Conflict {
                entity: removal.entity.clone(),
                identity: key.1,
                expected: Some(removal.expected),
                actual,
            });
        }
        self.rows.remove(&key);
        self.removed.insert(key, removal.expected);
        self.events.extend(removal.events.iter().cloned());
        Ok(())
    }

    /// Writes each in turn, keeping what each instance it touches held before; where one is
    /// refused, every instance is put back and the events recorded since dropped.
    fn commit_all(&mut self, writes: &[Write]) -> Result<(), StoreError> {
        type Held = (Option<Stored>, Option<u64>);
        let mut before: BTreeMap<(String, String), Held> = BTreeMap::new();
        let recorded = self.events.len();
        for write in writes {
            let (entity, identity) = write.instance();
            let key = key(entity, identity);
            if let std::collections::btree_map::Entry::Vacant(vacant) = before.entry(key) {
                let held = (
                    self.rows.get(vacant.key()).cloned(),
                    self.removed.get(vacant.key()).copied(),
                );
                vacant.insert(held);
            }
            let landed = match write {
                Write::Commit(change) => self.commit(change),
                Write::Remove(removal) => self.remove(removal),
            };
            if let Err(error) = landed {
                for (key, (row, removed)) in before {
                    match row {
                        Some(row) => self.rows.insert(key.clone(), row),
                        None => self.rows.remove(&key),
                    };
                    match removed {
                        Some(version) => self.removed.insert(key, version),
                        None => self.removed.remove(&key),
                    };
                }
                self.events.truncate(recorded);
                return Err(error);
            }
        }
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
