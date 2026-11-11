//! The rows each entity of a component holds, kept as its store's writes land, so that reading
//! them ([`super::Host::rows`]) reads no store; and the handle its readers read the store through
//! beside the host's thread ([`super::Reader`]).

use std::collections::BTreeMap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, RwLock};

use serde_json::Value;
use spindle_kernel::{
    Change, Committed, Event, Origin, Position, Reading, Removal, Scan, Scanned, Store, StoreError,
    Stored, Write,
};

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
    /// How many times a store was opened in place of the first: a store opened in place of another
    /// starts its positions over ([`super::Cursor`]).
    generation: u64,
    /// For a store a reset empties in place (one the host was given, such as PostgreSQL), which
    /// its readers cannot go on reading as it was: the gate its reads pass ([`Gate`]).
    gate: Option<Arc<Gate>>,
}

/// What keeps the readers of a store a reset empties in place from reading it partly reset:
/// closed from [`Counted::hold`] to [`Counted::publish`]. A read waits while it is closed, and
/// closing it waits for the reads already running, so a read answers the store before the reset
/// or after it, never between. Only a reset closes it.
#[derive(Default)]
struct Gate {
    state: Mutex<Passing>,
    changed: Condvar,
}

/// Whether a [`Gate`] is closed, and how many reads are passing it.
#[derive(Default)]
struct Passing {
    closed: bool,
    reads: usize,
}

impl Gate {
    fn lock(&self) -> MutexGuard<'_, Passing> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Waits while the gate is closed, then counts a read passing until the answer is dropped.
    fn enter(&self) -> Passed<'_> {
        let mut state = self.lock();
        while state.closed {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        state.reads += 1;
        Passed(self)
    }

    /// Closes the gate, then waits for every read passing it to end.
    fn close(&self) {
        let mut state = self.lock();
        state.closed = true;
        while state.reads > 0 {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    fn open(&self) {
        self.lock().closed = false;
        self.changed.notify_all();
    }
}

/// One read passing a [`Gate`], until dropped.
struct Passed<'a>(&'a Gate);

impl Drop for Passed<'_> {
    fn drop(&mut self) {
        let mut state = self.0.lock();
        state.reads -= 1;
        drop(state);
        self.0.changed.notify_all();
    }
}

/// A reading handle whose reads pass a [`Gate`].
struct Gated {
    reading: Arc<dyn Reading>,
    gate: Arc<Gate>,
}

impl Reading for Gated {
    fn scan(&self, entity: &str) -> Result<Scan, StoreError> {
        let _passed = self.gate.enter();
        self.reading.scan(entity)
    }

    fn counts(&self, entities: &[String]) -> Result<BTreeMap<String, usize>, StoreError> {
        let _passed = self.gate.enter();
        self.reading.counts(entities)
    }

    fn events_after(&self, position: u64, limit: usize) -> Result<Vec<Committed>, StoreError> {
        let _passed = self.gate.enter();
        self.reading.events_after(position, limit)
    }

    fn events_head(&self) -> Result<u64, StoreError> {
        let _passed = self.gate.enter();
        self.reading.events_head()
    }

    /// The position, read past the gate for as long as it is held: a reset waits for it.
    fn at(&self, entities: &[String]) -> Result<Box<dyn Position + '_>, StoreError> {
        let passed = self.gate.enter();
        let position = self.reading.at(entities)?;
        Ok(Box::new(GatedPosition {
            position,
            _passed: passed,
        }))
    }
}

/// A position of a [`Gated`] handle, holding its passage of the gate until dropped.
struct GatedPosition<'a> {
    position: Box<dyn Position + 'a>,
    _passed: Passed<'a>,
}

impl Position for GatedPosition<'_> {
    fn scan(&self, entity: &str) -> Result<Scan, StoreError> {
        self.position.scan(entity)
    }

    fn load(&self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.position.load(entity, identity)
    }

    fn holds(&self) -> Result<bool, StoreError> {
        self.position.holds()
    }
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
    /// ([`Counted::publish`]), with the generation of that store ([`Counted::generation`]).
    reading: RwLock<(u64, Option<Arc<dyn Reading>>)>,
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
        self.handed().1
    }

    /// The generation of the store its readers read ([`Counted::generation`]) and its handle,
    /// where it has one, as one reset handed both.
    pub(super) fn handed(&self) -> (u64, Option<Arc<dyn Reading>>) {
        self.reading
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Counted {
    /// `store`, with the rows of each of `entities` counted from it, and read beside the host's
    /// thread through its reading handle.
    /// `in_place`: a reset empties the store where it is rather than opening another in its
    /// place, so its readers pass a [`Gate`] a reset closes.
    pub(super) fn new(store: Box<dyn Store>, entities: Vec<String>, in_place: bool) -> Self {
        let mut counted = Self {
            store,
            rows: None,
            held: false,
            shared: Arc::new(Shared {
                entities,
                rows: Mutex::new(None),
                reading: RwLock::new((0, None)),
            }),
            generation: 0,
            gate: in_place.then(Arc::default),
        };
        counted.rows = counted.store.counts(&counted.shared.entities).ok();
        counted.publish();
        counted
    }

    /// What its readers share with it.
    pub(super) fn shared(&self) -> Arc<Shared> {
        Arc::clone(&self.shared)
    }

    /// The generation of the store it holds: 0 for the store it was opened with, one more for
    /// each store opened in place of the one before ([`Counted::replace`]).
    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    /// Holds its readers at the counts and the store they read now, whatever lands, until
    /// [`Counted::publish`]. A store a reset empties in place cannot be read as it was meanwhile:
    /// its gate closes, once the reads already running ended, and its reads wait for the reset.
    pub(super) fn hold(&mut self) {
        self.held = true;
        if let Some(gate) = &self.gate {
            gate.close();
        }
    }

    /// Opens `store` in place of the one held, its rows counted from it; answers the one held.
    /// Its readers go on reading the one held, and its counts, until [`Counted::publish`].
    pub(super) fn replace(&mut self, store: Box<dyn Store>) -> Box<dyn Store> {
        self.held = true;
        self.generation = self.generation.wrapping_add(1);
        let held = std::mem::replace(&mut self.store, store);
        self.rows = self.store.counts(&self.shared.entities).ok();
        held
    }

    /// Hands its readers the counts and the reading handle of the store it holds now, together,
    /// and stops holding them.
    pub(super) fn publish(&mut self) {
        let reading = self.store.reading().map(|reading| match &self.gate {
            Some(gate) => Arc::new(Gated {
                reading,
                gate: Arc::clone(gate),
            }) as Arc<dyn Reading>,
            None => reading,
        });
        let mut rows = self.shared.rows();
        *self
            .shared
            .reading
            .write()
            .unwrap_or_else(PoisonError::into_inner) = (self.generation, reading);
        rows.clone_from(&self.rows);
        drop(rows);
        self.held = false;
        if let Some(gate) = &self.gate {
            gate.open();
        }
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

    fn attribute(&mut self, origin: Option<Origin>) {
        self.store.attribute(origin);
    }

    fn commit_events(&mut self, events: &[Event], scanned: &[Scanned]) -> Result<(), StoreError> {
        self.store.commit_events(events, scanned)
    }

    fn forget_events(&mut self) -> Result<(), StoreError> {
        self.store.forget_events()
    }

    fn events_after(&mut self, position: u64, limit: usize) -> Result<Vec<Committed>, StoreError> {
        self.store.events_after(position, limit)
    }

    fn events_head(&mut self) -> Result<u64, StoreError> {
        self.store.events_head()
    }
}
