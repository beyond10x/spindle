//! The rows each entity of a component holds, kept as its store's writes land, so that reading
//! them ([`super::Host::rows`]) reads no store; and the handle its readers read the store through
//! beside the host's thread ([`super::Reader`]).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};

use serde_json::Value;
use spindle_kernel::{Change, Reading, Removal, Scan, Store, StoreError, Stored, Write};

/// A component's store with the rows each entity it owns holds: counted once from the store when
/// it is opened ([`Store::counts`]), then moved by each write that lands. The store refuses a
/// creation of an instance it holds, and a change or removal of one it does not
/// ([`StoreError::Conflict`]), so a creation (`expected: None`) adds one, a removal takes one
/// away and any other change leaves the count; a refused write, which lands nothing, moves
/// nothing.
///
/// Its readers read the counts as each write left them, a group of writes as one
/// ([`Store::commit_all`]). From [`Counted::hold`] to [`Counted::publish`], a reset, they read
/// the counts and the store as they were before it.
pub(super) struct Counted {
    store: Box<dyn Store>,
    /// The rows each counted entity holds, as the host's thread knows them; `None` while they
    /// could not be counted, until they are read.
    rows: Option<BTreeMap<String, usize>>,
    /// Whether its readers are held at the counts and store they read: from [`Counted::hold`]
    /// until [`Counted::publish`].
    held: bool,
    /// What its readers share with it: the rows, and the store's reading handle.
    shared: Arc<Shared>,
}

/// What a component's readers read beside the host's thread ([`super::Reader`]).
pub(super) struct Shared {
    /// The entities counted, by qualified name.
    pub(super) entities: Vec<String>,
    /// The rows each holds, as the last write or reset handed to its readers left them; `None`
    /// while they could not be counted.
    rows: Mutex<Option<BTreeMap<String, usize>>>,
    /// The handle a reader reads the store through ([`Store::reading`]); `None` for a store only
    /// the host's thread reads. It is the one of the store as the last command left it: a reset
    /// hands its readers the emptied store only once every store is emptied and its world loaded
    /// ([`Counted::publish`]).
    reading: RwLock<Option<Arc<dyn Reading>>>,
}

impl Shared {
    fn rows(&self) -> MutexGuard<'_, Option<BTreeMap<String, usize>>> {
        self.rows.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The rows each counted entity holds, as kept; `None` where they could not be counted.
    pub(super) fn kept(&self) -> Option<BTreeMap<String, usize>> {
        self.rows().clone()
    }

    /// The handle the store is read through beside the host's thread, where it has one.
    pub(super) fn reading(&self) -> Option<Arc<dyn Reading>> {
        self.reading
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Counted {
    /// `store`, with the rows of each of `entities` counted from it, and read beside the host's
    /// thread through its reading handle.
    pub(super) fn new(store: Box<dyn Store>, entities: Vec<String>) -> Self {
        let mut counted = Self {
            store,
            rows: None,
            held: false,
            shared: Arc::new(Shared {
                entities,
                rows: Mutex::new(None),
                reading: RwLock::new(None),
            }),
        };
        counted.rows = counted.store.counts(&counted.shared.entities).ok();
        counted.publish();
        counted
    }

    /// What its readers share with it.
    pub(super) fn shared(&self) -> Arc<Shared> {
        Arc::clone(&self.shared)
    }

    /// Holds its readers at the counts and the store they read now, whatever lands, until
    /// [`Counted::publish`].
    pub(super) fn hold(&mut self) {
        self.held = true;
    }

    /// Opens `store` in place of the one held, its rows counted from it; answers the one held.
    /// Its readers go on reading the one held, and its counts, until [`Counted::publish`].
    pub(super) fn replace(&mut self, store: Box<dyn Store>) -> Box<dyn Store> {
        self.held = true;
        let held = std::mem::replace(&mut self.store, store);
        self.rows = self.store.counts(&self.shared.entities).ok();
        held
    }

    /// Hands its readers the counts and the reading handle of the store it holds now, together,
    /// and stops holding them.
    pub(super) fn publish(&mut self) {
        let reading = self.store.reading();
        let mut rows = self.shared.rows();
        *self
            .shared
            .reading
            .write()
            .unwrap_or_else(PoisonError::into_inner) = reading;
        rows.clone_from(&self.rows);
        drop(rows);
        self.held = false;
    }

    /// Hands its readers the counts as the writes so far left them, unless they are held.
    fn handed(&self) {
        if !self.held {
            self.shared.rows().clone_from(&self.rows);
        }
    }

    /// The rows each counted entity holds: kept, and counted from the store only where an
    /// earlier count failed.
    ///
    /// # Errors
    /// [`StoreError`] when they had to be counted and the store could not be read.
    pub(super) fn rows(&mut self) -> Result<BTreeMap<String, usize>, StoreError> {
        if let Some(rows) = &self.rows {
            return Ok(rows.clone());
        }
        let counted = self.store.counts(&self.shared.entities)?;
        self.rows = Some(counted.clone());
        self.handed();
        Ok(counted)
    }

    /// Moves the count of `entity` by a write that landed: `created` adds one, else `removed`
    /// takes one away. Its readers do not see it until [`Counted::handed`].
    fn landed(&mut self, entity: &str, created: bool, removed: bool) {
        let Some(count) = self.rows.as_mut().and_then(|rows| rows.get_mut(entity)) else {
            return;
        };
        if created {
            *count = count.saturating_add(1);
        } else if removed {
            *count = count.saturating_sub(1);
        }
    }

    fn written(&mut self, write: &Write) {
        match write {
            Write::Commit(change) => self.landed(&change.entity, change.expected.is_none(), false),
            Write::Remove(removal) => self.landed(&removal.entity, false, true),
        }
    }
}

impl Store for Counted {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.store.load(entity, identity)
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.store.commit(change)?;
        self.landed(&change.entity, change.expected.is_none(), false);
        self.handed();
        Ok(())
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.store.remove(removal)?;
        self.landed(&removal.entity, false, true);
        self.handed();
        Ok(())
    }

    /// The writes land together, then count as each would alone, in order, and reach its readers
    /// as one.
    fn commit_all(&mut self, writes: &[Write]) -> Result<(), StoreError> {
        self.store.commit_all(writes)?;
        for write in writes {
            self.written(write);
        }
        self.handed();
        Ok(())
    }

    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError> {
        self.store.scan(entity)
    }

    fn counts(&mut self, entities: &[String]) -> Result<BTreeMap<String, usize>, StoreError> {
        self.store.counts(entities)
    }

    fn reading(&mut self) -> Option<Arc<dyn Reading>> {
        self.store.reading()
    }
}
