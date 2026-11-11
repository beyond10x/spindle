//! The rows each entity of a component holds, kept as its store's writes land, so that reading
//! them ([`super::Host::rows`]) reads no store.

use std::collections::BTreeMap;

use serde_json::Value;
use spindle_kernel::{Change, Removal, Scan, Store, StoreError, Stored, Write};

/// A component's store with the rows each entity it owns holds: counted once from the store when
/// it is opened ([`Store::counts`]), then moved by each write that lands. The store refuses a
/// creation of an instance it holds, and a change or removal of one it does not
/// ([`StoreError::Conflict`]), so a creation (`expected: None`) adds one, a removal takes one
/// away and any other change leaves the count; a refused write, which lands nothing, moves
/// nothing.
pub(super) struct Counted {
    store: Box<dyn Store>,
    /// The entities counted, by qualified name.
    entities: Vec<String>,
    /// The rows each holds; `None` while they could not be counted, until they are read.
    rows: Option<BTreeMap<String, usize>>,
}

impl Counted {
    /// `store`, with the rows of each of `entities` counted from it.
    pub(super) fn new(store: Box<dyn Store>, entities: Vec<String>) -> Self {
        let mut counted = Self {
            store,
            entities,
            rows: None,
        };
        counted.recount();
        counted
    }

    /// Opens `store` in place of the one held, its rows counted from it; answers the one held.
    pub(super) fn replace(&mut self, store: Box<dyn Store>) -> Box<dyn Store> {
        let held = std::mem::replace(&mut self.store, store);
        self.recount();
        held
    }

    fn recount(&mut self) {
        self.rows = self.store.counts(&self.entities).ok();
    }

    /// The rows each counted entity holds: kept, and counted from the store only where an
    /// earlier count failed.
    ///
    /// # Errors
    /// [`StoreError`] when they had to be counted and the store could not be read.
    pub(super) fn rows(&mut self) -> Result<&BTreeMap<String, usize>, StoreError> {
        if self.rows.is_none() {
            self.rows = Some(self.store.counts(&self.entities)?);
        }
        Ok(self.rows.get_or_insert_with(BTreeMap::new))
    }

    /// Moves the count of `entity` by a write that landed: `created` adds one, else `removed`
    /// takes one away.
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
        Ok(())
    }

    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.store.remove(removal)?;
        self.landed(&removal.entity, false, true);
        Ok(())
    }

    /// The writes land together, then count as each would alone, in order.
    fn commit_all(&mut self, writes: &[Write]) -> Result<(), StoreError> {
        self.store.commit_all(writes)?;
        for write in writes {
            self.written(write);
        }
        Ok(())
    }

    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError> {
        self.store.scan(entity)
    }

    fn counts(&mut self, entities: &[String]) -> Result<BTreeMap<String, usize>, StoreError> {
        self.store.counts(entities)
    }
}
