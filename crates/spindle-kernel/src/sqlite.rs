//! Instances on SQLite, through `eventlog`: `store.durable` bound to `sqlite`.
//!
//! Each instance is one event stream, keyed by its entity and its identity. A commit appends the
//! events the command emitted followed by one `spindle.row` event carrying the row as it now
//! rests, in one append, so the row and the events that changed it are recorded together or not
//! at all. Loading reads the stream's last event. The stream's version is the version a later
//! commit names, so two writers that read the same row cannot both commit. A removal appends the
//! events and one `spindle.removed` event, which loading and scanning read as no row. Several
//! writes together ([`Store::commit_all`]) are one append group: every stream's append in one
//! transaction. Every write is one append group, which also appends one `spindle.written` event to
//! the stream of each entity it writes: that stream's version is the entity's version a scan
//! answers and a change decided on a scan names ([`crate::Change::scanned`]), so a row of the
//! entity written after the scan refuses the change.
//!
//! A scan and a count answer from each entity's rows as they rest ([`Resting`]): folded from one
//! read of the feed when the store opens, then from the events appended since, read before each
//! scan or count and after each write. So a scan costs the rows of its entity and the events
//! written since the last read, never the whole log.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};

use eventlog_core::{
    AppendGroup, AtomicEventStore, CommandMeta, EventLogError, EventStore, Expected, NewEvent,
    RecordedEvent, StreamAppend, StreamId, TenantId, new_event_id, request_hash,
};
use eventlog_sqlite::SqliteEventStore;
use serde_json::Value;
use time::OffsetDateTime;

use crate::store::{
    Change, Reading, Removal, Row, Scan, Scanned, Store, StoreError, Stored, Write, first_writes,
    identity_from_key, moved,
};

/// The table prefix Spindle's events are kept under.
const PREFIX: &str = "spindle";
/// The one tenant a single Spindle process holds.
const TENANT: &str = "local";
/// The event that carries an instance's row after a change.
const ROW: &str = "spindle.row";
/// The event that ends the stream of a removed instance (`deletes:`).
const REMOVED: &str = "spindle.removed";
/// The event each write appends to the stream of an entity it writes.
const WRITTEN: &str = "spindle.written";
/// The stream type of the streams carrying an entity's version, one per entity: no qualified
/// entity name holds a colon.
const ENTITY: &str = "spindle:entity";
/// Who records the change: the runtime itself.
const ACTOR: &str = "spindle-kernel";
/// The schema version of every event Spindle appends.
const SCHEMA: u32 = 1;
/// How many feed events one read of a scan asks for.
const FEED_PAGE: usize = 500;

/// Instances in one SQLite database file.
///
/// Its reading handle ([`Store::reading`]) reads the same file on connections of its own, which
/// SQLite's write-ahead log lets read beside the writer, and shares the store's resting rows: each
/// read folds in the feed past them in commit order, to its end, so it sees a write's whole append
/// group or none of it.
pub struct SqliteStore {
    log: SqliteEventStore,
    tenant: TenantId,
    /// What drives its log's calls: its own, or the one every connection of its reading handle
    /// shares.
    runtime: Arc<tokio::runtime::Runtime>,
    path: PathBuf,
    /// Its reading handle, once one was asked for: retired when this store is dropped.
    reading: Option<Arc<SqliteReading>>,
    /// Each entity's rows as they rest, shared with every connection of its reading handle.
    resting: Arc<Resting>,
}

impl std::fmt::Debug for SqliteStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SqliteStore")
            .field("tenant", &self.tenant)
            .finish_non_exhaustive()
    }
}

impl SqliteStore {
    /// Opens the database at `path`, creating it and its tables when absent, and reads its log
    /// once for the rows that rest in it.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the file cannot be opened, provisioned or read.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let store = Self::opening(path, true, runtime()?, Arc::default())?;
        // No other handle holds the rows yet: the whole log folds into them as it is read, a page
        // at a time, so no more than one page of it is held at once.
        let mut folded = Folded::default();
        folded.position = store.feed(0, |event| {
            if let Some(fold) = Fold::of(event) {
                folded.apply(fold);
            }
        })?;
        *store.resting.folded() = folded;
        Ok(store)
    }

    /// How many events of its log this store and its reading handle have read for their scans
    /// and counts: the whole log once when it opened, then only the events appended since.
    #[must_use]
    pub fn events_read(&self) -> u64 {
        self.resting.read.load(Ordering::SeqCst)
    }

    /// Opens the database at `path`, which a store opened before provisioned, on a connection of
    /// its own driven by `runtime`, sharing that store's `resting` rows: what a reading handle
    /// reads through.
    fn open_existing(
        path: &Path,
        runtime: Arc<tokio::runtime::Runtime>,
        resting: Arc<Resting>,
    ) -> Result<Self, StoreError> {
        Self::opening(path, false, runtime, resting)
    }

    fn opening(
        path: &Path,
        create: bool,
        runtime: Arc<tokio::runtime::Runtime>,
        resting: Arc<Resting>,
    ) -> Result<Self, StoreError> {
        let location = path.to_str().ok_or_else(|| {
            StoreError::Backend(format!("{} is not a UTF-8 path", path.display()))
        })?;
        let log = if create {
            runtime.block_on(SqliteEventStore::open(location, PREFIX))
        } else {
            runtime.block_on(SqliteEventStore::open_existing(location, PREFIX))
        }
        .map_err(backend)?;
        let tenant = TenantId::new(TENANT).map_err(backend)?;
        Ok(Self {
            log,
            tenant,
            runtime,
            path: path.to_owned(),
            reading: None,
            resting,
        })
    }

    fn stream(&self, entity: &str, identity: &Value) -> Result<StreamId, StoreError> {
        StreamId::new(self.tenant.clone(), entity, identity.to_string()).map_err(backend)
    }

    /// The stream's version and its last event, or `None` for a stream never written.
    fn last(&self, stream: &StreamId) -> Result<Option<(u64, RecordedEvent)>, StoreError> {
        let Some(version) = self
            .runtime
            .block_on(self.log.stream_version(stream))
            .map_err(backend)?
        else {
            return Ok(None);
        };
        let slice = self
            .runtime
            .block_on(self.log.read_stream(stream, version.saturating_sub(1), 1))
            .map_err(backend)?;
        let last = slice.events.into_iter().last().ok_or_else(|| {
            StoreError::Backend(format!("the stream {stream:?} has no event at {version}"))
        })?;
        Ok(Some((version, last)))
    }

    /// Refuses `write` where its instance stands at another version than the one it names, or
    /// at none: the conflict [`Store::commit`] names.
    fn stands_at(&mut self, write: &Write) -> Result<(), StoreError> {
        let (entity, identity) = write.instance();
        let actual = self.load(entity, identity)?.map(|stored| stored.version);
        if actual == write.expected() {
            return Ok(());
        }
        Err(StoreError::Conflict {
            entity: entity.to_owned(),
            identity: identity.to_string(),
            expected: write.expected(),
            actual,
        })
    }

    /// The stream every write of an instance of `entity` appends one event to: its version is
    /// the entity's version a scan answers.
    fn entity_stream(&self, entity: &str) -> Result<StreamId, StoreError> {
        StreamId::new(self.tenant.clone(), ENTITY, entity).map_err(backend)
    }

    /// The entity's version: how many writes of its instances landed, 0 for none.
    fn entity_version(&self, entity: &str) -> Result<u64, StoreError> {
        let stream = self.entity_stream(entity)?;
        Ok(self
            .runtime
            .block_on(self.log.stream_version(&stream))
            .map_err(backend)?
            .unwrap_or(0))
    }

    /// Refuses `scanned` where its entity no longer stands at the version the scan answered.
    fn unmoved(&self, scanned: &Scanned) -> Result<(), StoreError> {
        let actual = self.entity_version(&scanned.entity)?;
        if actual == scanned.version {
            return Ok(());
        }
        Err(moved(scanned, actual))
    }

    /// Hands every event of the tenant's feed past `position` to `each`, in commit order, a page
    /// of [`FEED_PAGE`] at a time, to the feed's end; answers the position past the last event
    /// read. Every page moves the position forward, or the read stops as a backend error.
    fn feed(
        &self,
        mut position: u64,
        mut each: impl FnMut(RecordedEvent),
    ) -> Result<u64, StoreError> {
        loop {
            let page = self
                .runtime
                .block_on(self.log.read_feed(&self.tenant, position, FEED_PAGE))
                .map_err(backend)?;
            let read = u64::try_from(page.events.len()).unwrap_or(u64::MAX);
            self.resting.read.fetch_add(read, Ordering::SeqCst);
            for event in page.events {
                each(event);
            }
            if !page.has_more {
                return Ok(page.next_position.max(position));
            }
            if page.next_position <= position {
                return Err(StoreError::Backend(format!(
                    "the feed did not move past position {position}"
                )));
            }
            position = page.next_position;
        }
    }

    /// The store's resting rows, under their lock, once every event appended past those they
    /// hold is folded in. The events are read and their rows decoded outside the lock, to the
    /// feed's end; under it, only those past the position another read may have folded to
    /// meanwhile are applied (SQLite's feed position is the event's `global_seq`). An append
    /// group commits whole and the read runs to the feed's end, so the rows hold each write
    /// group whole or not at all, and the lock is held only to apply what was read and to hand
    /// out the rows as they stand.
    fn caught_up(&self) -> Result<std::sync::MutexGuard<'_, Folded>, StoreError> {
        let start = self.resting.folded().position;
        let mut folds = Vec::new();
        let end = self.feed(start, |event| {
            let position = event.global_seq;
            if let Some(fold) = Fold::of(event) {
                folds.push((position, fold));
            }
        })?;
        let mut held = self.resting.folded();
        if end > held.position {
            let past = held.position;
            for (position, fold) in folds {
                if position > past {
                    held.apply(fold);
                }
            }
            held.position = end;
        }
        Ok(held)
    }
}

/// The rows of every entity of a [`SqliteStore`] as they rest in its log, folded from its feed:
/// what the store and every connection of its reading handle scan and count. Memory holds each
/// entity's resting rows, never the events that led to them.
#[derive(Debug, Default)]
struct Resting {
    folded: Mutex<Folded>,
    /// How many feed events were read for it ([`SqliteStore::events_read`]).
    read: AtomicU64,
}

impl Resting {
    fn folded(&self) -> std::sync::MutexGuard<'_, Folded> {
        self.folded.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The feed folded up to a position: each entity's rows as one value a read takes as it stands,
/// so a fold changes an entity in place unless a read still holds it, and then a copy of that
/// entity alone.
#[derive(Debug, Default)]
struct Folded {
    /// The feed position past the last event folded.
    position: u64,
    entities: BTreeMap<String, Arc<Entity>>,
}

/// One entity's rows in a [`Folded`] feed.
#[derive(Debug, Clone, Default)]
struct Entity {
    /// Each instance's last `spindle.row` event, by its stream id, unless a `spindle.removed`
    /// event followed it.
    rows: BTreeMap<String, Arc<Stored>>,
    /// The streams whose last such event carried no row it could read: counted, never scanned.
    unreadable: BTreeSet<String>,
    /// Why a `spindle.row` event of the entity did not read, if one did not: a scan of the
    /// entity fails, as a read of the whole feed did.
    malformed: Option<String>,
    /// The version of the entity's `spindle:entity` stream: how many writes of it landed.
    version: u64,
}

/// What one feed event changes in the resting rows, decoded from it.
enum Fold {
    /// The entity's `spindle:entity` stream stands at this version.
    Version(String, u64),
    /// The instance of the entity rests holding this row, or why its row event did not read.
    Row(String, String, Result<Arc<Stored>, String>),
    /// The instance of the entity was removed.
    Removed(String, String),
}

impl Fold {
    /// What `event` changes: `None` for an event that is no row, removal or entity version.
    fn of(event: RecordedEvent) -> Option<Self> {
        if event.stream_type == ENTITY {
            return Some(Self::Version(event.stream_id, event.version));
        }
        if event.name == REMOVED {
            return Some(Self::Removed(event.stream_type, event.stream_id));
        }
        if event.name != ROW {
            return None;
        }
        let version = event.version;
        let row = serde_json::from_value::<Row>(event.data)
            .map(|row| Arc::new(Stored { row, version }))
            .map_err(|error| format!("a stored row: {error}"));
        Some(Self::Row(event.stream_type, event.stream_id, row))
    }
}

impl Folded {
    fn entity(&mut self, entity: &str) -> &mut Entity {
        if !self.entities.contains_key(entity) {
            self.entities.insert(entity.to_owned(), Arc::default());
        }
        Arc::make_mut(
            self.entities
                .get_mut(entity)
                .expect("the entity was inserted"),
        )
    }

    fn apply(&mut self, fold: Fold) {
        match fold {
            Fold::Version(entity, version) => self.entity(&entity).version = version,
            Fold::Removed(entity, identity) => {
                let entity = self.entity(&entity);
                entity.rows.remove(&identity);
                entity.unreadable.remove(&identity);
            }
            Fold::Row(entity, identity, Ok(stored)) => {
                let entity = self.entity(&entity);
                entity.unreadable.remove(&identity);
                entity.rows.insert(identity, stored);
            }
            Fold::Row(entity, identity, Err(error)) => {
                let entity = self.entity(&entity);
                entity.rows.remove(&identity);
                entity.unreadable.insert(identity);
                entity.malformed.get_or_insert(error);
            }
        }
    }

    /// How many instances of `entity` rest.
    fn count(&self, entity: &str) -> usize {
        self.entities
            .get(entity)
            .map_or(0, |entity| entity.rows.len() + entity.unreadable.len())
    }
}

impl Entity {
    fn scan(&self) -> Result<Scan, StoreError> {
        if let Some(malformed) = &self.malformed {
            return Err(StoreError::Backend(malformed.clone()));
        }
        let rows = self
            .rows
            .iter()
            .map(|(identity, stored)| Ok((identity_from_key(identity)?, Stored::clone(stored))))
            .collect::<Result<_, StoreError>>()?;
        Ok(Scan {
            rows,
            version: self.version,
        })
    }
}

impl Store for SqliteStore {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        let stream = self.stream(entity, identity)?;
        let Some((version, last)) = self.last(&stream)? else {
            return Ok(None);
        };
        if last.name == REMOVED {
            return Ok(None);
        }
        if last.name != ROW {
            return Err(StoreError::Backend(format!(
                "the stream of {entity} {identity} does not end with its row"
            )));
        }
        let row = serde_json::from_value::<Row>(last.data)
            .map_err(|error| StoreError::Backend(format!("a stored row: {error}")))?;
        Ok(Some(Stored { row, version }))
    }

    /// Appends the change's events and its row, as a group of one write ([`Store::commit_all`]).
    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        self.commit_all(&[Write::Commit(change.clone())])
    }

    /// Appends the removal's events and a `spindle.removed` event: the stream's end, which
    /// [`Store::load`] and [`Store::scan`] read as no row.
    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        self.commit_all(&[Write::Remove(removal.clone())])
    }

    /// Appends every write's events and its `spindle.row` or `spindle.removed` event as one
    /// append group: one transaction over every stream, each at the version its write names. A
    /// write naming a version its instance does not stand at is refused before anything is
    /// appended, naming that instance; a stream moved in between refuses the whole group. The
    /// group appends one `spindle.written` event to the stream of each entity it writes, and of
    /// each entity a write was conditioned on ([`Change::scanned`]), that one at the version the
    /// scan answered: a write of the entity landed since refuses the group.
    fn commit_all(&mut self, writes: &[Write]) -> Result<(), StoreError> {
        if writes.is_empty() {
            return Ok(());
        }
        let scanned: Vec<&Scanned> = writes.iter().flat_map(Write::scanned).collect();
        for scanned in &scanned {
            self.unmoved(scanned)?;
        }
        let firsts = first_writes(writes);
        for write in &firsts {
            self.stands_at(write)?;
        }
        let mut marks = BTreeMap::<&str, Expected>::new();
        for scanned in &scanned {
            let expected = match scanned.version {
                0 => Expected::NoStream,
                version => Expected::Exact(version),
            };
            marks.insert(&scanned.entity, expected);
        }
        for write in writes {
            marks.entry(write.instance().0).or_insert(Expected::Any);
        }
        let mut appends = Vec::with_capacity(writes.len() + marks.len());
        for (entity, expected) in marks {
            appends.push(StreamAppend {
                stream: self.entity_stream(entity)?,
                expected,
                events: vec![
                    NewEvent::new(WRITTEN, SCHEMA, Value::Object(serde_json::Map::new()))
                        .map_err(backend)?,
                ],
            });
        }
        for write in writes {
            let (entity, identity) = write.instance();
            let stream = self.stream(entity, identity)?;
            let expected = match write.expected() {
                Some(version) => Expected::Exact(version),
                None => match self.last(&stream)? {
                    Some((version, last)) if last.name == REMOVED => Expected::Exact(version),
                    _ => Expected::NoStream,
                },
            };
            let (events, end) = match write {
                Write::Commit(change) => (&change.events, row_event(&change.row)?),
                Write::Remove(removal) => (&removal.events, removed_event()?),
            };
            appends.push(StreamAppend {
                stream,
                expected,
                events: recorded(events, end)?,
            });
        }
        let entries: Vec<&Vec<NewEvent>> = appends.iter().map(|append| &append.events).collect();
        let group = AppendGroup {
            tenant: self.tenant.clone(),
            meta: meta(request_hash(&entries).map_err(backend)?),
            appends,
        };
        match self.runtime.block_on(self.log.append_group(&group)) {
            Ok(_) => {
                // The group landed: its rows are folded in now, so a read finds no more to fold.
                // Where that read fails, the next scan or count folds them in or fails itself.
                drop(self.caught_up());
                Ok(())
            }
            Err(EventLogError::Conflict { .. }) => {
                // Another writer moved a stream between the check above and the append: name
                // the first entity or instance no longer where its write found it.
                for scanned in &scanned {
                    self.unmoved(scanned)?;
                }
                for write in &firsts {
                    self.stands_at(write)?;
                }
                Err(StoreError::Backend(
                    "another writer moved an instance while the writes were appended".to_owned(),
                ))
            }
            Err(other) => Err(backend(other)),
        }
    }

    /// Answers, for each stream of `entity`, its last `spindle.row` event: the row as it rests and
    /// the stream version a later commit names; a `spindle.removed` event after it leaves the
    /// stream out, until a later row. The version is that of the entity's `spindle:entity`
    /// stream. Both come from the store's resting rows once the events appended since they were
    /// last read are folded in ([`Resting`]): the rows and the version of one position of the
    /// feed, as a read of the whole feed answers them there. Only the entity is taken under the
    /// lock; its rows are copied out without it.
    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError> {
        let standing = self.caught_up()?.entities.get(entity).cloned();
        standing.map_or_else(|| Ok(Scan::default()), |entity| entity.scan())
    }

    /// Counts, for every entity of `entities`, the streams whose last `spindle.row` or
    /// `spindle.removed` event is a row, from the store's resting rows as [`Store::scan`] reads
    /// them: every entity at one position of the feed.
    fn counts(&mut self, entities: &[String]) -> Result<BTreeMap<String, usize>, StoreError> {
        let folded = self.caught_up()?;
        Ok(entities
            .iter()
            .map(|entity| (entity.clone(), folded.count(entity)))
            .collect())
    }

    /// Reads the same file on connections of its own, which share one runtime and the store's
    /// resting rows: one opened with the handle, more as reads that run at once need them, up to
    /// as many as the machine runs threads at once (at least 2). Every call answers the same
    /// handle; `None` where no connection could be opened.
    fn reading(&mut self) -> Option<Arc<dyn Reading>> {
        if self.reading.is_none() {
            let runtime = runtime().ok()?;
            let resting = Arc::clone(&self.resting);
            let first =
                Self::open_existing(&self.path, Arc::clone(&runtime), Arc::clone(&resting)).ok()?;
            let cap = std::thread::available_parallelism()
                .map_or(2, std::num::NonZero::get)
                .max(2);
            self.reading = Some(Arc::new(SqliteReading {
                path: self.path.clone(),
                runtime,
                resting,
                cap,
                connections: Mutex::new(Connections {
                    idle: vec![first],
                    live: 1,
                    waiting: 0,
                }),
                returned: Condvar::new(),
                retired: AtomicBool::new(false),
            }));
        }
        self.reading
            .clone()
            .map(|reading| reading as Arc<dyn Reading>)
    }
}

/// A runtime of its own for a store's log calls: one thread, no driver, so it holds no
/// descriptor.
fn runtime() -> Result<Arc<tokio::runtime::Runtime>, StoreError> {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .map(Arc::new)
        .map_err(|error| StoreError::Backend(error.to_string()))
}

/// The store is closed: its reading handle opens no further connection by its path, which a
/// reset may move aside and open again empty, and goes on reading the file it opened.
impl Drop for SqliteStore {
    fn drop(&mut self) {
        if let Some(reading) = &self.reading {
            reading.retired.store(true, Ordering::SeqCst);
        }
    }
}

/// How many connections a [`SqliteReading`] keeps open beyond those its reads hold, when no read
/// waits for one.
const IDLE: usize = 2;

/// The reading handle of a [`SqliteStore`]: the file, and its connections. Reads that run at
/// once each read on a connection of their own, up to `cap` of them; a read beyond that waits for
/// one handed back. Once no read waits, at most [`IDLE`] are kept for the next reads and the rest
/// closed.
struct SqliteReading {
    path: PathBuf,
    /// What drives every connection's log calls.
    runtime: Arc<tokio::runtime::Runtime>,
    /// The store's resting rows, which every connection reads and folds the feed into.
    resting: Arc<Resting>,
    /// How many connections may be open at once.
    cap: usize,
    connections: Mutex<Connections>,
    /// Signalled whenever a connection is handed back or given up.
    returned: Condvar,
    /// The store this handle reads was closed ([`Drop`]): the file at its path may be another
    /// now, so reads take turns on the connections already open.
    retired: AtomicBool,
}

/// The connections of a [`SqliteReading`].
struct Connections {
    /// Those no read holds now.
    idle: Vec<SqliteStore>,
    /// How many are open, idle or held, or being opened.
    live: usize,
    /// How many reads wait for one.
    waiting: usize,
}

impl SqliteReading {
    /// Runs `read` on a connection no other read holds ([`SqliteReading::connection`]); hands it
    /// back for a read that waits, or keeps it idle up to [`IDLE`], and closes it otherwise or
    /// where the read failed.
    fn with<T>(
        &self,
        read: impl FnOnce(&mut SqliteStore) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let mut connection = self.connection()?;
        let read = read(&mut connection);
        let mut connections = self.lock();
        let mut closed = Vec::new();
        if read.is_ok() {
            connections.idle.push(connection);
        } else {
            connections.live -= 1;
            closed.push(connection);
        }
        // A read woken for a connection counts as waiting until it runs again, so a burst can
        // leave more idle than any read waits for: the last read of a burst closes them.
        let kept = IDLE.max(connections.waiting);
        while connections.idle.len() > kept {
            if let Some(connection) = connections.idle.pop() {
                connections.live -= 1;
                closed.push(connection);
            }
        }
        drop(connections);
        self.returned.notify_all();
        drop(closed);
        read
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connections> {
        self.connections
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Waits for a connection to be handed back or given up.
    fn wait<'a>(
        &self,
        mut connections: std::sync::MutexGuard<'a, Connections>,
    ) -> std::sync::MutexGuard<'a, Connections> {
        connections.waiting += 1;
        let mut connections = self
            .returned
            .wait(connections)
            .unwrap_or_else(PoisonError::into_inner);
        connections.waiting -= 1;
        connections
    }

    /// A connection no other read holds: an idle one, else a new one while fewer than `cap` are
    /// open, else the next one handed back. A new one is opened only while the store is open, and
    /// kept only where it still was once the connection opened: the path names the store's own
    /// file until the store is closed, and the file is moved aside only after that.
    fn connection(&self) -> Result<SqliteStore, StoreError> {
        let mut connections = self.lock();
        loop {
            if let Some(connection) = connections.idle.pop() {
                return Ok(connection);
            }
            if self.retired.load(Ordering::SeqCst) {
                if connections.live == 0 {
                    return Err(StoreError::Backend(
                        "the store was closed while it was read".to_owned(),
                    ));
                }
                connections = self.wait(connections);
                continue;
            }
            if connections.live >= self.cap {
                connections = self.wait(connections);
                continue;
            }
            connections.live += 1;
            drop(connections);
            let opened = SqliteStore::open_existing(
                &self.path,
                Arc::clone(&self.runtime),
                Arc::clone(&self.resting),
            );
            let open = !self.retired.load(Ordering::SeqCst);
            connections = self.lock();
            match opened {
                Ok(connection) if open => return Ok(connection),
                Ok(_) => {
                    connections.live -= 1;
                    self.returned.notify_all();
                }
                Err(error) => {
                    connections.live -= 1;
                    self.returned.notify_all();
                    if open {
                        return Err(error);
                    }
                }
            }
        }
    }
}

impl Reading for SqliteReading {
    fn scan(&self, entity: &str) -> Result<Scan, StoreError> {
        self.with(|connection| connection.scan(entity))
    }

    fn counts(&self, entities: &[String]) -> Result<BTreeMap<String, usize>, StoreError> {
        self.with(|connection| connection.counts(entities))
    }
}

/// The `spindle.row` event carrying `row` as it rests.
fn row_event(row: &Row) -> Result<NewEvent, StoreError> {
    let row = serde_json::to_value(row).map_err(|error| StoreError::Backend(error.to_string()))?;
    NewEvent::new(ROW, SCHEMA, row).map_err(backend)
}

/// The `spindle.removed` event ending a removed instance's stream.
fn removed_event() -> Result<NewEvent, StoreError> {
    NewEvent::new(REMOVED, SCHEMA, Value::Object(serde_json::Map::new())).map_err(backend)
}

/// `events` as the log records them, followed by `end`.
fn recorded(events: &[crate::Event], end: NewEvent) -> Result<Vec<NewEvent>, StoreError> {
    let mut recorded = events
        .iter()
        .map(|event| {
            NewEvent::new(
                event.name.clone(),
                SCHEMA,
                Value::Object(event.payload.clone()),
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(backend)?;
    recorded.push(end);
    Ok(recorded)
}

/// What every append records about the command it is for: the runtime as actor, a fresh
/// idempotency key, and `request_hash`.
fn meta(request_hash: String) -> CommandMeta {
    CommandMeta {
        idempotency_key: new_event_id(),
        request_hash,
        subject: ACTOR.to_owned(),
        actor: ACTOR.to_owned(),
        request_id: new_event_id(),
        trace_id: new_event_id(),
        causation_id: None,
        causation_depth: 0,
        occurred_at: OffsetDateTime::now_utc(),
        claim: None,
    }
}

fn backend(error: EventLogError) -> StoreError {
    StoreError::Backend(error.to_string())
}
