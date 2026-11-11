//! Where instances rest between commands.
//!
//! The kernel reads one instance, decides, and commits one [`Change`] to that instance: the
//! runtime default of § 3a, one command on one instance. A set outcome (`instances:`) writes every
//! row it changes in one [`Store::commit_all`]. A store never decides anything; it holds rows and
//! the events that changed them.

use std::collections::BTreeMap;
use std::sync::{Arc, PoisonError, RwLock};

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
    /// Each entity the command decided on by scanning it, at the version the scan answered: a
    /// store refuses the change when another write of that entity landed in between.
    pub scanned: Vec<Scanned>,
}

/// An entity a command scanned before it decided, at the version [`Store::scan`] answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scanned {
    /// The entity, by qualified name.
    pub entity: String,
    /// The entity's version the scan answered.
    pub version: u64,
}

/// What [`Store::scan`] reads: every instance of an entity, and the entity's version.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scan {
    /// Every instance, each with its identity, ordered by the identity's canonical JSON text.
    pub rows: Vec<(Value, Stored)>,
    /// The entity's version: it grows with every write of an instance of the entity, so a
    /// [`Change`] conditioned on it ([`Change::scanned`]) is refused once another landed.
    pub version: u64,
}

impl IntoIterator for Scan {
    type Item = (Value, Stored);
    type IntoIter = std::vec::IntoIter<(Value, Stored)>;

    fn into_iter(self) -> Self::IntoIter {
        self.rows.into_iter()
    }
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

    /// The entities the write is conditioned on, each at the version it was scanned at.
    pub fn scanned(&self) -> &[Scanned] {
        match self {
            Self::Commit(change) => &change.scanned,
            Self::Remove(_) => &[],
        }
    }
}

/// The command the writes a store commits are made for, as a writer attributes them
/// ([`Store::attribute`]) and each event it committed answers it ([`Committed::origin`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    /// The command, by qualified name.
    pub command: String,
    /// Where the command falls among every command its writer ran: a host numbers the commands
    /// of every component in one order, so the events of its components' stores merge back into
    /// the order it published them.
    pub sequence: u64,
}

/// One event a store committed, as [`Store::events_after`] answers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Committed {
    /// Where the write that committed it stands in the store's commit order: every event of one
    /// write carries the same position, and a later write's is greater.
    pub position: u64,
    /// The command its write was attributed to, or `None` for a write attributed to none.
    pub origin: Option<Origin>,
    /// The event and its payload.
    pub event: Event,
}

/// What a store that does not read back the events it commits answers to
/// [`Store::events_after`].
fn no_events() -> StoreError {
    StoreError::Backend("this store does not read back the events it commits".to_owned())
}

/// The identity a [`StoreError::Conflict`] names where a scanned entity moved, rather than one
/// instance.
pub const EVERY_INSTANCE: &str = "*";

/// The conflict a write conditioned on `scanned` meets where the entity stands at `actual`.
pub(crate) fn moved(scanned: &Scanned, actual: u64) -> StoreError {
    StoreError::Conflict {
        entity: scanned.entity.clone(),
        identity: EVERY_INSTANCE.to_owned(),
        expected: Some(scanned.version),
        actual: Some(actual),
    }
}

/// Why a store could not read or commit.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The instance moved between the read and the commit, or, with the identity
    /// [`EVERY_INSTANCE`], an entity the write was conditioned on ([`Change::scanned`]) moved
    /// between its scan and the commit; the versions are then the entity's.
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
    /// [`StoreError::Conflict`] when the instance is not at `change.expected`, or an entity of
    /// `change.scanned` is not at the version it names, and [`StoreError::Backend`] when the
    /// store cannot be written.
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
    /// version that one left. A write's scanned entities ([`Change::scanned`]) are checked
    /// against their versions before any write lands.
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
    /// [`StoreError::Conflict`] when an instance is not at the version its write names, or a
    /// scanned entity is not at the version a write names for it, and [`StoreError::Backend`]
    /// when the store cannot be written.
    fn commit_all(&mut self, writes: &[Write]) -> Result<(), StoreError> {
        one_at_a_time(self, writes)
    }

    /// Every instance of `entity`, each with its identity, ordered by the identity's canonical
    /// JSON text: one order on every store, whatever order the instances were written in. Each
    /// carries the row and the version [`Store::load`] would answer for it. The scan carries the
    /// entity's version too, read no later than the rows: a [`Change`] conditioned on it
    /// ([`Change::scanned`]) is refused once another write of the entity landed after the scan.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the store cannot be read.
    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError>;

    /// How many instances each of `entities` holds, by entity: as many as [`Store::scan`] answers
    /// rows for. This default scans each entity in turn; a store that holds every entity's rows
    /// at once (SQLite folds its feed into them) counts them in one read.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the store cannot be read.
    fn counts(&mut self, entities: &[String]) -> Result<BTreeMap<String, usize>, StoreError> {
        entities
            .iter()
            .map(|entity| Ok((entity.clone(), self.scan(entity)?.rows.len())))
            .collect()
    }

    /// A handle that reads what this store commits from any thread while its one writer goes on
    /// ([`Reading`]); `None`, the default, for a store only its writer reads, whose reads then
    /// wait for the writer. The memory and SQLite stores give one.
    fn reading(&mut self) -> Option<Arc<dyn Reading>> {
        None
    }

    /// Attributes the writes that follow to `origin`, until attributed again: each event they
    /// commit is read back with it ([`Committed::origin`]). `None` attributes them to no command.
    /// This default, for a store that does not read back its events, keeps nothing; a store that
    /// wraps another hands it on.
    fn attribute(&mut self, origin: Option<Origin>) {
        let _ = origin;
    }

    /// Every event committed by a write after the one at `position`, in commit order, each with
    /// its write's position and origin; `0` reads every event. The events of one write are read
    /// together or not at all, and a refused write committed none.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the store cannot be read, or, this default, does not read back
    /// the events it commits.
    fn events_after(&mut self, position: u64) -> Result<Vec<Committed>, StoreError> {
        let _ = position;
        Err(no_events())
    }
}

/// A read-only handle on what a store committed, read from any thread beside the store's one
/// writer ([`Store::reading`]).
///
/// Each read answers the store as it stood once some write had landed, never part of one: a
/// write of several instances ([`Store::commit_all`]) is seen whole or not at all. A read never
/// waits for a command to finish, and the writer never waits for a read beyond the moment it
/// takes to hand it the store's current state.
pub trait Reading: Send + Sync {
    /// Every instance of `entity`, as [`Store::scan`] answers them.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the store cannot be read.
    fn scan(&self, entity: &str) -> Result<Scan, StoreError>;

    /// How many instances each of `entities` holds, as [`Store::counts`] answers them; this
    /// default scans each entity in turn.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the store cannot be read.
    fn counts(&self, entities: &[String]) -> Result<BTreeMap<String, usize>, StoreError> {
        entities
            .iter()
            .map(|entity| Ok((entity.clone(), self.scan(entity)?.rows.len())))
            .collect()
    }

    /// The events committed after `position`, as [`Store::events_after`] answers them.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the store cannot be read, or, this default, does not read back
    /// the events it commits.
    fn events_after(&self, position: u64) -> Result<Vec<Committed>, StoreError> {
        let _ = position;
        Err(no_events())
    }
}

/// [`Store::commit_all`] on a store that writes one instance at a time: every write's version
/// checked first, then each written in turn, and those written undone where a later one is
/// refused. Each scanned entity is checked before anything is written; the writes then land
/// without the condition, so one write of the entity does not refuse a later one.
fn one_at_a_time<S: Store + ?Sized>(store: &mut S, writes: &[Write]) -> Result<(), StoreError> {
    for scanned in writes.iter().flat_map(Write::scanned) {
        let actual = store.scan(&scanned.entity)?.version;
        if actual != scanned.version {
            return Err(moved(scanned, actual));
        }
    }
    let unconditioned: Vec<Write> = writes
        .iter()
        .map(|write| match write {
            Write::Commit(change) if !change.scanned.is_empty() => Write::Commit(Change {
                scanned: Vec::new(),
                ..change.clone()
            }),
            other => other.clone(),
        })
        .collect();
    let writes = unconditioned.as_slice();
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
            scanned: Vec::new(),
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
///
/// Its rows are shared with its reading handles ([`Store::reading`]), each entity's as its own
/// value: a write changes them under a lock held only while the write lands, and a read takes
/// them as they stand and reads them without the lock. Where a read still holds an entity a write
/// changes, the write changes a copy of that entity alone, so neither waits for the other and a
/// write never copies an entity it does not change. The events it committed are shared alike, in
/// chunks ([`Events`]), so a read of them never makes a write copy more than one chunk.
#[derive(Debug, Default)]
pub struct MemoryStore {
    held: Arc<RwLock<Held>>,
    /// The command the writes are attributed to ([`Store::attribute`]).
    origin: Option<Origin>,
}

/// What a [`MemoryStore`] holds: each entity's instances, by qualified name, each as one value its
/// reads share, and every event committed.
#[derive(Debug, Default)]
struct Held {
    entities: BTreeMap<String, Arc<Instances>>,
    events: Events,
}

/// How many events one chunk of [`Events`] holds before it is sealed.
const CHUNK: usize = 256;

/// Every event a [`MemoryStore`] committed, in commit order, as chunks its reads share. A sealed
/// chunk never changes again; only the open one grows, and where a read still holds it, the event
/// lands on a copy of it alone. So a read takes the chunks as they stand under the store's lock,
/// one pointer each, and reads them without it.
#[derive(Debug, Default)]
struct Events {
    sealed: Vec<Arc<Vec<Committed>>>,
    open: Arc<Vec<Committed>>,
    /// The position of the last write that committed an event.
    position: u64,
}

impl Events {
    /// Records `events`, which one write committed, at the next position, attributed to `origin`.
    fn record(&mut self, events: Vec<Event>, origin: Option<&Origin>) {
        if events.is_empty() {
            return;
        }
        self.position += 1;
        for event in events {
            if self.open.len() >= CHUNK {
                let full = std::mem::take(&mut self.open);
                self.sealed.push(full);
            }
            Arc::make_mut(&mut self.open).push(Committed {
                position: self.position,
                origin: origin.cloned(),
                event,
            });
        }
    }

    fn chunks(&self) -> Vec<Arc<Vec<Committed>>> {
        self.sealed
            .iter()
            .chain(std::iter::once(&self.open))
            .cloned()
            .collect()
    }
}

/// Every event `held` holds past `position`: its chunks taken under its lock, read without it.
fn committed_after(held: &RwLock<Held>, position: u64) -> Vec<Committed> {
    let chunks = held
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .events
        .chunks();
    chunks
        .iter()
        .filter(|chunk| chunk.last().is_some_and(|last| last.position > position))
        .flat_map(|chunk| chunk.iter())
        .filter(|committed| committed.position > position)
        .cloned()
        .collect()
}

/// The instances of one entity in a [`MemoryStore`].
#[derive(Debug, Default, Clone)]
struct Instances {
    /// Each instance's row, by its identity as canonical JSON text.
    rows: BTreeMap<String, Stored>,
    /// The last version of each removed instance, so one created again under its identity goes
    /// on from it and no commit that read the removed row matches the new one.
    removed: BTreeMap<String, u64>,
    /// How many writes of its instances landed.
    version: u64,
}

/// What one instance held before a group of writes touched it: its row and its last removed
/// version.
type Before = (Option<Stored>, Option<u64>);

impl Held {
    fn version(&self, entity: &str) -> u64 {
        self.entities
            .get(entity)
            .map_or(0, |instances| instances.version)
    }

    /// The instances of `entity`, to change: copied first where a read still holds them.
    fn changing(&mut self, entity: &str) -> &mut Instances {
        if !self.entities.contains_key(entity) {
            self.entities.insert(entity.to_owned(), Arc::default());
        }
        Arc::make_mut(
            self.entities
                .get_mut(entity)
                .expect("the entity was inserted"),
        )
    }

    /// Refuses a write conditioned on an entity that moved since it was scanned.
    fn unmoved(&self, scanned: &[Scanned]) -> Result<(), StoreError> {
        for scanned in scanned {
            let actual = self.version(&scanned.entity);
            if actual != scanned.version {
                return Err(moved(scanned, actual));
            }
        }
        Ok(())
    }

    /// Writes `change`, checking only the version of its instance; its events are recorded on
    /// `events`.
    fn put(&mut self, change: &Change, events: &mut Vec<Event>) -> Result<(), StoreError> {
        let identity = change.identity.to_string();
        let actual = self
            .entities
            .get(&change.entity)
            .and_then(|instances| instances.rows.get(&identity))
            .map(|stored| stored.version);
        if actual != change.expected {
            return Err(StoreError::Conflict {
                entity: change.entity.clone(),
                identity,
                expected: change.expected,
                actual,
            });
        }
        let instances = self.changing(&change.entity);
        let version = actual
            .or_else(|| instances.removed.get(&identity).copied())
            .unwrap_or(0)
            + 1;
        instances.rows.insert(
            identity,
            Stored {
                row: change.row.clone(),
                version,
            },
        );
        instances.version += 1;
        events.extend(change.events.iter().cloned());
        Ok(())
    }

    /// Removes the instance `removal` names; its events are recorded on `events`.
    fn remove(&mut self, removal: &Removal, events: &mut Vec<Event>) -> Result<(), StoreError> {
        let identity = removal.identity.to_string();
        let actual = self
            .entities
            .get(&removal.entity)
            .and_then(|instances| instances.rows.get(&identity))
            .map(|stored| stored.version);
        if actual != Some(removal.expected) {
            return Err(StoreError::Conflict {
                entity: removal.entity.clone(),
                identity,
                expected: Some(removal.expected),
                actual,
            });
        }
        let instances = self.changing(&removal.entity);
        instances.rows.remove(&identity);
        instances.removed.insert(identity, removal.expected);
        instances.version += 1;
        events.extend(removal.events.iter().cloned());
        Ok(())
    }

    /// Checks every write's scanned entities, then writes each in turn, keeping what each
    /// instance it touches held before and each entity's version; where one is refused, every
    /// instance and version is put back and the events recorded since dropped.
    fn commit_all(&mut self, writes: &[Write], events: &mut Vec<Event>) -> Result<(), StoreError> {
        for write in writes {
            self.unmoved(write.scanned())?;
        }
        let mut before: BTreeMap<(String, String), Before> = BTreeMap::new();
        let mut versions: BTreeMap<String, u64> = BTreeMap::new();
        let recorded = events.len();
        for write in writes {
            let (entity, identity) = write.instance();
            let key = key(entity, identity);
            if !versions.contains_key(entity) {
                versions.insert(entity.to_owned(), self.version(entity));
            }
            if let std::collections::btree_map::Entry::Vacant(vacant) = before.entry(key) {
                let instances = self.entities.get(entity);
                let held = (
                    instances.and_then(|instances| instances.rows.get(&vacant.key().1).cloned()),
                    instances.and_then(|instances| instances.removed.get(&vacant.key().1).copied()),
                );
                vacant.insert(held);
            }
            let landed = match write {
                Write::Commit(change) => self.put(change, events),
                Write::Remove(removal) => self.remove(removal, events),
            };
            if let Err(error) = landed {
                for ((entity, identity), (row, removed)) in before {
                    let instances = self.changing(&entity);
                    match row {
                        Some(row) => instances.rows.insert(identity.clone(), row),
                        None => instances.rows.remove(&identity),
                    };
                    match removed {
                        Some(version) => instances.removed.insert(identity, version),
                        None => instances.removed.remove(&identity),
                    };
                }
                for (entity, version) in versions {
                    self.changing(&entity).version = version;
                }
                events.truncate(recorded);
                return Err(error);
            }
        }
        Ok(())
    }

    fn load(&self, entity: &str, identity: &Value) -> Option<Stored> {
        self.entities
            .get(entity)?
            .rows
            .get(&identity.to_string())
            .cloned()
    }
}

impl Instances {
    fn scan(&self) -> Result<Scan, StoreError> {
        let rows = self
            .rows
            .iter()
            .map(|(identity, stored)| Ok((identity_from_key(identity)?, stored.clone())))
            .collect::<Result<_, StoreError>>()?;
        Ok(Scan {
            rows,
            version: self.version,
        })
    }
}

/// What `held` holds of `entity` now: taken under its lock, read without it. Only the entity is
/// taken, so a write of any other entity changes it in place.
fn standing(held: &RwLock<Held>, entity: &str) -> Option<Arc<Instances>> {
    held.read()
        .unwrap_or_else(PoisonError::into_inner)
        .entities
        .get(entity)
        .cloned()
}

/// Every instance of `entity` in `held`, as it stands.
fn scanned(held: &RwLock<Held>, entity: &str) -> Result<Scan, StoreError> {
    standing(held, entity).map_or_else(|| Ok(Scan::default()), |instances| instances.scan())
}

impl MemoryStore {
    /// Every event committed so far, in commit order.
    pub fn events(&self) -> Vec<Event> {
        committed_after(&self.held, 0)
            .into_iter()
            .map(|committed| committed.event)
            .collect()
    }

    /// Lands one write on what the store holds, under its lock: on each entity it changes
    /// itself where no read holds it, else on a copy of that entity that then stands in its place.
    /// The events it committed are recorded with it, under the same lock, so a read sees the
    /// write and its events together.
    fn write(
        &mut self,
        write: impl FnOnce(&mut Held, &mut Vec<Event>) -> Result<(), StoreError>,
    ) -> Result<(), StoreError> {
        let mut held = self.held.write().unwrap_or_else(PoisonError::into_inner);
        let mut events = Vec::new();
        write(&mut held, &mut events)?;
        held.events.record(events, self.origin.as_ref());
        Ok(())
    }
}

impl Store for MemoryStore {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        Ok(self
            .held
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .load(entity, identity))
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.write(|held, events| {
            held.unmoved(&change.scanned)?;
            held.put(change, events)
        })
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.write(|held, events| held.remove(removal, events))
    }

    /// Checks every write's scanned entities, then writes each in turn, keeping what each
    /// instance it touches held before; where one is refused, every instance is put back and the
    /// events recorded since dropped. A read sees all of them or none.
    fn commit_all(&mut self, writes: &[Write]) -> Result<(), StoreError> {
        self.write(|held, events| held.commit_all(writes, events))
    }

    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError> {
        scanned(&self.held, entity)
    }

    fn reading(&mut self) -> Option<Arc<dyn Reading>> {
        Some(Arc::new(MemoryReading(Arc::clone(&self.held))))
    }

    fn attribute(&mut self, origin: Option<Origin>) {
        self.origin = origin;
    }

    fn events_after(&mut self, position: u64) -> Result<Vec<Committed>, StoreError> {
        Ok(committed_after(&self.held, position))
    }
}

/// The reading handle of a [`MemoryStore`]: what it holds, read as it stands.
struct MemoryReading(Arc<RwLock<Held>>);

impl Reading for MemoryReading {
    fn scan(&self, entity: &str) -> Result<Scan, StoreError> {
        scanned(&self.0, entity)
    }

    fn events_after(&self, position: u64) -> Result<Vec<Committed>, StoreError> {
        Ok(committed_after(&self.0, position))
    }

    /// Every entity's count as one write left them all: taken under one lock.
    fn counts(&self, entities: &[String]) -> Result<BTreeMap<String, usize>, StoreError> {
        let held = self.0.read().unwrap_or_else(PoisonError::into_inner);
        Ok(entities
            .iter()
            .map(|entity| {
                let rows = held
                    .entities
                    .get(entity)
                    .map_or(0, |instances| instances.rows.len());
                (entity.clone(), rows)
            })
            .collect())
    }
}
