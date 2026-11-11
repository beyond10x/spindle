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
//! entity written after the scan refuses the change. The event names the instances the write
//! left, each at its stream's version after it (schema 2: `{"rows": {stream: version},
//! "removed": {stream: version}}`), so a scan can read the entity's stream and one event per
//! instance rather than the whole log. The store of 0.10.0 wrote it empty (schema 1), and before
//! 0.7.0 wrote none. A SQLite file, which any release may have written, is read whole once, when
//! the store opens, into each entity's rows as they rest ([`RestingRows`]), which its scans and
//! counts answer from, folding in only the events appended since; a scan of a PostgreSQL store
//! reads the entity's stream, unless the log is marked as holding such writes ([`MARKS`]) or the
//! entity's stream holds one.
//!
//! The encoding is the store's, not SQLite's: [`LogStore`] keeps it on any eventlog event store
//! that appends groups and answers the tenant's whole log ([`Log`]), and the PostgreSQL store
//! (`PostgresStore`, the `postgres` feature) is the same store on PostgreSQL.

use std::collections::{BTreeMap, BTreeSet};
#[cfg(feature = "sqlite")]
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(feature = "sqlite")]
use std::sync::atomic::AtomicU64;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "sqlite")]
use std::sync::{Condvar, Mutex, PoisonError};

#[cfg(feature = "sqlite")]
use eventlog_core::EventStore;
use eventlog_core::{
    AppendGroup, CommandMeta, EventLogError, Expected, MAX_READ_LIMIT, NewEvent, Read, ReadResult,
    RecordedEvent, StreamAppend, StreamId, TenantId, new_event_id, request_hash,
};
#[cfg(feature = "sqlite")]
use eventlog_sqlite::SqliteEventStore;
use serde_json::Value;
use time::OffsetDateTime;

use crate::store::{
    Change, Reading, Removal, Row, Scan, Scanned, Store, StoreError, Stored, Write, first_writes,
    identity_from_key, moved,
};

/// The table prefix Spindle's events are kept under.
pub(crate) const PREFIX: &str = "spindle";
/// The one tenant a single Spindle process holds.
pub(crate) const TENANT: &str = "local";
/// The event that carries an instance's row after a change.
const ROW: &str = "spindle.row";
/// The event that ends the stream of a removed instance (`deletes:`).
pub(crate) const REMOVED: &str = "spindle.removed";
/// The event each write appends to the stream of an entity it writes.
pub(crate) const WRITTEN: &str = "spindle.written";
/// The schema of a `spindle.written` event that names the instances its write left
/// ([`Written`]). One of schema [`SCHEMA`], as the store of 0.10.0 wrote it, names none.
pub(crate) const WRITTEN_SCHEMA: u32 = 2;
/// The stream type of the streams carrying an entity's version, one per entity: no qualified
/// entity name holds a colon.
pub(crate) const ENTITY: &str = "spindle:entity";
/// The stream type and id of the stream whose presence marks a log holding writes that name no
/// instances (written before 0.7.0, or by 0.10.0 and earlier): `state move` appends an event to
/// it before it copies such a write into a PostgreSQL store. A scan of a marked log reads the
/// whole log.
pub(crate) const MARKS: (&str, &str) = ("spindle:log", "unnamed-writes");
/// Who records the change: the runtime itself.
const ACTOR: &str = "spindle-kernel";
/// The schema version of every event Spindle appends but `spindle.written` ([`WRITTEN_SCHEMA`]).
const SCHEMA: u32 = 1;
/// The most instances' rows a scan of the entity's stream reads in one
/// [`eventlog_core::EventStore::read_many`] call, which a provider may answer with one statement.
const READ_BATCH: usize = 1_000;
/// How many it reads in its first call, before it has seen how large the entity's rows are.
const FIRST_READ_BATCH: usize = 64;
/// About how many bytes of rows one call reads, by the largest row read so far ([`weight`]): a
/// statement reading rows of a mebibyte 1,000 at a time outlasts the store's deadline.
const READ_BYTES: usize = 32 << 20;
/// How many feed events one read of a SQLite scan asks for.
#[cfg(feature = "sqlite")]
const FEED_PAGE: usize = 500;

/// What a scan reads the log through: every event of the tenant, in commit order.
pub(crate) type Visit<'a> = dyn FnMut(RecordedEvent) -> Result<(), StoreError> + 'a;

mod sealed {
    use std::sync::Arc;

    use eventlog_core::{AtomicEventStore, TenantId};

    use super::{LogStore, Visit};
    use crate::store::{Reading, StoreError};

    /// An eventlog event store a [`super::LogStore`] keeps instances in: it appends groups, and
    /// hands every event of a tenant to `visit` in commit order, from one consistent point of
    /// the log, so that every write it holds is whole and the entity versions it counts are the
    /// versions of the rows it holds.
    pub trait Log: AtomicEventStore {
        /// Whether the log may hold writes that name no instances with no mark saying so
        /// ([`super::MARKS`]): then every scan reads the whole log ([`Log::each_event`]).
        const UNMARKED: bool;

        /// Every event of `tenant`, in commit order, to `visit`, blocking on `runtime`.
        ///
        /// # Errors
        /// The first error `visit` returns, or [`StoreError::Backend`] when the log cannot be
        /// read.
        fn each_event(
            &self,
            runtime: &tokio::runtime::Runtime,
            tenant: &TenantId,
            visit: &mut Visit<'_>,
        ) -> Result<(), StoreError>;

        /// A new reading handle on `store` ([`crate::Store::reading`]), or `None` where none
        /// can be had: SQLite reads on connections of its own ([`super::SqliteReading`]), a
        /// PostgreSQL store through its own pool ([`LogStore::shared`]).
        fn reading(store: &LogStore<Self>) -> Option<Arc<dyn Reading>>
        where
            Self: Sized;
    }
}

pub(crate) use sealed::Log;

/// Instances in an eventlog event store: one SQLite database file ([`SqliteStore`]) or one
/// component's tables of a PostgreSQL database (`PostgresStore`).
pub struct LogStore<L> {
    /// The event store, shared with the store's reading handle where that reads through it.
    log: Arc<L>,
    tenant: TenantId,
    /// What drives the log's calls: the store's own, shared with its reading handle.
    runtime: Arc<tokio::runtime::Runtime>,
    /// The file a SQLite store keeps, which its reading handle opens connections to.
    #[cfg_attr(not(feature = "sqlite"), allow(dead_code))]
    path: Option<PathBuf>,
    /// Its reading handle, once one was asked for.
    reading: Option<Arc<dyn Reading>>,
    /// Set when the store is closed ([`Drop`]): a SQLite reading handle then opens no further
    /// connection by its path, which a reset may move aside and open again empty.
    retired: Arc<AtomicBool>,
    /// The rows each entity rests with, folded from the feed of a SQLite file: what the store and
    /// every connection of its reading handle scan and count. Set by [`SqliteStore::open`] alone,
    /// since the fold relies on SQLite's single writer ([`RestingRows`]); `None` on PostgreSQL.
    #[cfg(feature = "sqlite")]
    rows: Option<Arc<RestingRows>>,
}

/// The store is closed: its reading handle opens no further connection by its path.
impl<L> Drop for LogStore<L> {
    fn drop(&mut self) {
        self.retired.store(true, Ordering::SeqCst);
    }
}

/// Instances in one SQLite database file.
#[cfg(feature = "sqlite")]
pub type SqliteStore = LogStore<SqliteEventStore>;

impl<L> std::fmt::Debug for LogStore<L> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LogStore")
            .field("tenant", &self.tenant)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "sqlite")]
impl LogStore<SqliteEventStore> {
    /// Opens the database at `path`, creating it and its tables when absent, and reads its log
    /// once for the rows that rest in it.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the file cannot be opened, provisioned or read.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let mut store = Self::opening(path, true, sqlite_runtime()?, None)?;
        let rows = Arc::new(RestingRows::default());
        // No other handle holds the rows yet: the whole log folds into them as it is read, a page
        // at a time, so no more than one page of it is held at once.
        let mut folded = Folded::default();
        folded.position = store.feed(&rows, 0, |event| {
            if let Some(fold) = Fold::of(event) {
                folded.apply(fold);
            }
        })?;
        *rows.folded() = folded;
        store.rows = Some(rows);
        Ok(store)
    }

    /// How many events of its log this store and its reading handle have read for their scans
    /// and counts: the whole log once when it opened, then only the events appended since.
    #[must_use]
    pub fn events_read(&self) -> u64 {
        self.rows
            .as_ref()
            .map_or(0, |rows| rows.read.load(Ordering::SeqCst))
    }

    /// Opens the database at `path`, which a store opened before provisioned, on a connection of
    /// its own driven by `runtime`, sharing that store's `rows`: what a reading handle reads
    /// through.
    fn open_existing(
        path: &Path,
        runtime: Arc<tokio::runtime::Runtime>,
        rows: Option<Arc<RestingRows>>,
    ) -> Result<Self, StoreError> {
        Self::opening(path, false, runtime, rows)
    }

    fn opening(
        path: &Path,
        create: bool,
        runtime: Arc<tokio::runtime::Runtime>,
        rows: Option<Arc<RestingRows>>,
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
        let mut store = Self::on(log, runtime, Some(path.to_owned()))?;
        store.rows = rows;
        Ok(store)
    }
}

/// A runtime of its own for a SQLite store's log calls: one thread, no driver, so it holds no
/// descriptor.
#[cfg(feature = "sqlite")]
fn sqlite_runtime() -> Result<Arc<tokio::runtime::Runtime>, StoreError> {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .map(Arc::new)
        .map_err(|error| StoreError::Backend(error.to_string()))
}

impl<L: Log> LogStore<L> {
    /// The store on `log`, every call blocking on `runtime`; `path` is the file a SQLite store
    /// keeps.
    pub(crate) fn on(
        log: L,
        runtime: Arc<tokio::runtime::Runtime>,
        path: Option<PathBuf>,
    ) -> Result<Self, StoreError> {
        let tenant = TenantId::new(TENANT).map_err(backend)?;
        Ok(Self {
            log: Arc::new(log),
            tenant,
            runtime,
            path,
            reading: None,
            retired: Arc::default(),
            #[cfg(feature = "sqlite")]
            rows: None,
        })
    }

    /// Another store on the same event store, tenant and runtime, as the reading handle of a
    /// store whose event store reads from any thread through a pool of its own (PostgreSQL):
    /// its reads take the pool's connections one statement at a time beside the writer's, and
    /// open none of their own.
    #[cfg(feature = "postgres")]
    pub(crate) fn shared(&self) -> Self {
        Self {
            log: Arc::clone(&self.log),
            tenant: self.tenant.clone(),
            runtime: Arc::clone(&self.runtime),
            path: None,
            reading: None,
            retired: Arc::default(),
            #[cfg(feature = "sqlite")]
            rows: None,
        }
    }

    /// The event store and the runtime its calls block on: what a state move appends through.
    ///
    /// # Errors
    /// [`StoreError::Backend`] where a reading handle still reads the event store.
    #[cfg(all(feature = "sqlite", feature = "postgres"))]
    pub(crate) fn into_parts(self) -> Result<(L, Arc<tokio::runtime::Runtime>), StoreError> {
        let log = Arc::clone(&self.log);
        let runtime = Arc::clone(&self.runtime);
        drop(self);
        let log = Arc::try_unwrap(log).map_err(|_| {
            StoreError::Backend("the store is still read through a reading handle".to_owned())
        })?;
        Ok((log, runtime))
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
    /// at none: the conflict [`Store::commit`] names. Otherwise answers the version its stream
    /// ends at with a `spindle.removed` event, or `None` where it holds a row or was never
    /// written: the version a write naming none appends at.
    fn stands_at(&self, write: &Write) -> Result<Option<u64>, StoreError> {
        let (entity, identity) = write.instance();
        let (actual, removed) = match self.last(&self.stream(entity, identity)?)? {
            None => (None, None),
            Some((version, last)) if last.name == REMOVED => (None, Some(version)),
            Some((version, last)) if last.name == ROW => (Some(version), None),
            Some(_) => {
                return Err(StoreError::Backend(format!(
                    "the stream of {entity} {identity} does not end with its row"
                )));
            }
        };
        if actual == write.expected() {
            return Ok(removed);
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
}

impl<L: Log> Store for LogStore<L> {
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
    /// scan answered: a write of the entity landed since refuses the group. Each names the
    /// instances of its entity the group writes, at the version each stream stands at once the
    /// group lands ([`Written`]).
    fn commit_all(&mut self, writes: &[Write]) -> Result<(), StoreError> {
        if writes.is_empty() {
            return Ok(());
        }
        let scanned: Vec<&Scanned> = writes.iter().flat_map(Write::scanned).collect();
        for scanned in &scanned {
            self.unmoved(scanned)?;
        }
        let firsts = first_writes(writes);
        // Where each instance's stream ends removed before the group: one read per instance,
        // which a write naming no version appends after.
        let mut ends_removed = BTreeMap::<(&str, String), Option<u64>>::new();
        for write in &firsts {
            let removed = self.stands_at(write)?;
            let (entity, identity) = write.instance();
            ends_removed.insert((entity, identity.to_string()), removed);
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
        let mut written = BTreeMap::<&str, Written>::new();
        let mut instances = Vec::with_capacity(writes.len());
        for write in writes {
            let (entity, identity) = write.instance();
            let stream = self.stream(entity, identity)?;
            let expected = match write.expected() {
                Some(version) => Expected::Exact(version),
                None => match ends_removed.get(&(entity, identity.to_string())) {
                    Some(Some(version)) => Expected::Exact(*version),
                    _ => Expected::NoStream,
                },
            };
            let (events, end, removed) = match write {
                Write::Commit(change) => (&change.events, row_event(&change.row)?, false),
                Write::Remove(removal) => (&removal.events, removed_event()?, true),
            };
            let events = recorded(events, end)?;
            // The append lands at the version it names, or the group is refused.
            let before = match expected {
                Expected::Exact(version) => version,
                _ => 0,
            };
            let after = before.saturating_add(events.len() as u64);
            written
                .entry(entity)
                .or_default()
                .note(stream.stream_id(), after, removed);
            instances.push(StreamAppend {
                stream,
                expected,
                events,
            });
        }
        let mut appends = Vec::with_capacity(instances.len() + marks.len());
        for (entity, expected) in marks {
            let body = written.remove(entity).unwrap_or_default().body();
            appends.push(StreamAppend {
                stream: self.entity_stream(entity)?,
                expected,
                events: vec![NewEvent::new(WRITTEN, WRITTEN_SCHEMA, body).map_err(backend)?],
            });
        }
        appends.extend(instances);
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
                #[cfg(feature = "sqlite")]
                if let Some(rows) = &self.rows {
                    drop(self.caught_up(rows));
                }
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
            // PostgreSQL's lock table ran out: the group locks each stream it appends to.
            Err(EventLogError::Backend(why)) if why.contains("SQLSTATE 53200") => {
                Err(StoreError::Backend(format!(
                    "{why}: one write of {} streams locks more than PostgreSQL's lock table holds \
                     (out of shared memory); raise the server's max_locks_per_transaction, which \
                     takes a restart",
                    group.appends.len()
                )))
            }
            Err(other) => Err(backend(other)),
        }
    }

    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError> {
        self.scanned(entity)
    }

    fn counts(&mut self, entities: &[String]) -> Result<BTreeMap<String, usize>, StoreError> {
        self.counted(entities)
    }

    /// Asked once, and the same handle answered after: a SQLite store's reads on connections
    /// of its own ([`SqliteReading`]), a PostgreSQL store's through its own pool
    /// ([`LogStore::shared`]).
    fn reading(&mut self) -> Option<Arc<dyn Reading>> {
        if self.reading.is_none() {
            self.reading = L::reading(self);
        }
        self.reading.clone()
    }
}

/// The reading handle of a store whose event store reads from any thread through a pool of its
/// own: another [`LogStore`] on the same event store and runtime ([`LogStore::shared`]).
#[cfg(feature = "postgres")]
pub(crate) struct SharedReading<L>(pub(crate) LogStore<L>);

#[cfg(feature = "postgres")]
impl<L: Log> Reading for SharedReading<L> {
    fn scan(&self, entity: &str) -> Result<Scan, StoreError> {
        self.0.scanned(entity)
    }

    fn counts(&self, entities: &[String]) -> Result<BTreeMap<String, usize>, StoreError> {
        self.0.counted(entities)
    }
}

impl<L: Log> LogStore<L> {
    /// Reads the entity's stream and one event per instance ([`LogStore::scan_named`]), or, on a
    /// log that may hold writes naming no instances, the whole log ([`LogStore::scan_log`]): a
    /// SQLite file, or a PostgreSQL store marked as holding them ([`MARKS`]). A store holding its
    /// resting rows (SQLite, [`RestingRows`]) answers from them, the events appended since its
    /// last read folded in; only the entity is taken under their lock, its rows copied out
    /// without it.
    fn scanned(&self, entity: &str) -> Result<Scan, StoreError> {
        #[cfg(feature = "sqlite")]
        if let Some(rows) = &self.rows {
            let standing = self.caught_up(rows)?.entities.get(entity).cloned();
            return standing.map_or_else(|| Ok(Scan::default()), |entity| entity.scan());
        }
        if L::UNMARKED || self.marked()? {
            return self.scan_log(entity);
        }
        self.scan_named(entity)
    }

    /// How many instances each of `entities` holds, as [`Store::scan`] answers them: from each
    /// entity's own stream alone ([`LogStore::resting`]), reading no row, or, for a log that may
    /// hold writes naming no instances and for an entity whose stream holds one, from one read
    /// of the whole log for all of them ([`LogStore::count_log`]), where a scan of each reads the
    /// whole log again. A store holding its resting rows counts them, every entity at one
    /// position of the feed.
    fn counted(&self, entities: &[String]) -> Result<BTreeMap<String, usize>, StoreError> {
        #[cfg(feature = "sqlite")]
        if let Some(rows) = &self.rows {
            let folded = self.caught_up(rows)?;
            return Ok(entities
                .iter()
                .map(|entity| (entity.clone(), folded.count(entity)))
                .collect());
        }
        if L::UNMARKED || self.marked()? {
            return self.count_log(entities);
        }
        let mut counts = BTreeMap::new();
        let mut unnamed = Vec::new();
        for entity in entities {
            match self.resting(entity)? {
                Some((resting, _)) => {
                    counts.insert(entity.clone(), resting.len());
                }
                None => unnamed.push(entity.clone()),
            }
        }
        if !unnamed.is_empty() {
            counts.extend(self.count_log(&unnamed)?);
        }
        Ok(counts)
    }
}

impl<L: Log> LogStore<L> {
    /// Whether the log is marked as holding writes that name no instances ([`MARKS`]).
    fn marked(&self) -> Result<bool, StoreError> {
        let (kind, id) = MARKS;
        let mark = StreamId::new(self.tenant.clone(), kind, id).map_err(backend)?;
        Ok(self
            .runtime
            .block_on(self.log.stream_version(&mark))
            .map_err(backend)?
            .is_some())
    }

    /// Reads the entity's stream, page by page, to its end: its last version is the entity's
    /// version the scan answers, and its `spindle.written` events, in order, name each instance
    /// whose row rests after them and the version of its stream there ([`Written`]). Then reads
    /// each such instance's event at exactly that version: its `spindle.row` event, the row as it
    /// rested once the write the version counts last landed. Those reads go up to [`READ_BATCH`]
    /// to one [`eventlog_core::EventStore::read_many`] call, which a provider may answer with one
    /// statement rather than one per read, and about [`READ_BYTES`] of rows to one call by the
    /// largest row read so far; a call that outlasts the store's deadline is read again in halves.
    ///
    /// Every read is a bounded read of committed events and holds no lock. It is consistent all
    /// the same:
    /// a stream's events never change once appended and its versions land in order, so the pages
    /// read are the stream's first versions however many writes land meanwhile; and the write a
    /// `spindle.written` event belongs to appended the instance's events in the same group, so
    /// the event at the version it names is there to read, and later writes of the instance are
    /// at later versions, which the scan does not read. So the rows are exactly the rows of the
    /// writes the version counts. What a scan holds grows with the instances whose row rests,
    /// never with the log.
    ///
    /// An entity whose stream holds a `spindle.written` event that names no instances (schema
    /// 1, as the store of 0.10.0 wrote it) is scanned from the whole log instead
    /// ([`LogStore::scan_log`]).
    fn scan_named(&self, entity: &str) -> Result<Scan, StoreError> {
        let Some((resting, version)) = self.resting(entity)? else {
            return self.scan_log(entity);
        };
        let resting: Vec<(String, u64)> = resting.into_iter().collect();
        let mut rows = Vec::with_capacity(resting.len());
        let mut size = FIRST_READ_BATCH;
        let mut largest = 0;
        while rows.len() < resting.len() {
            let batch = &resting[rows.len()..resting.len().min(rows.len() + size)];
            let reads = batch
                .iter()
                .map(|(key, at)| {
                    Ok(Read::Stream {
                        stream: StreamId::new(self.tenant.clone(), entity, key).map_err(backend)?,
                        after_version: at.saturating_sub(1),
                        limit: 1,
                    })
                })
                .collect::<Result<Vec<_>, StoreError>>()?;
            let answered = match self.runtime.block_on(self.log.read_many(&reads)) {
                Ok(answered) => answered,
                // Rows larger than those read so far outlasted the store's deadline: read
                // fewer at once.
                Err(EventLogError::Deadline { .. }) if batch.len() > 1 => {
                    size = batch.len() / 2;
                    continue;
                }
                Err(error) => return Err(backend(error)),
            };
            if answered.len() != batch.len() {
                return Err(StoreError::Backend(format!(
                    "{} reads of instances of {entity} answered {}",
                    batch.len(),
                    answered.len()
                )));
            }
            for ((key, at), answer) in batch.iter().zip(answered) {
                let at = *at;
                let ReadResult::Stream(slice) = answer else {
                    return Err(StoreError::Backend(format!(
                        "the read of the stream of {entity} {key} answered no stream"
                    )));
                };
                let event = slice
                    .events
                    .into_iter()
                    .next()
                    .filter(|event| event.version == at && event.name == ROW)
                    .ok_or_else(|| {
                        StoreError::Backend(format!(
                            "the stream of {entity} {key} holds no row at version {at}, which \
                             the entity's stream names"
                        ))
                    })?;
                largest = largest.max(weight(&event.data));
                let row = serde_json::from_value::<Row>(event.data)
                    .map_err(|error| StoreError::Backend(format!("a stored row: {error}")))?;
                rows.push((identity_from_key(key)?, Stored { row, version: at }));
            }
            size = (READ_BYTES / largest.max(1)).clamp(1, READ_BATCH);
        }
        Ok(Scan { rows, version })
    }

    /// Reads the entity's stream, page by page, to its end ([`LogStore::scan_named`]): each
    /// instance whose row rests, by stream id at the version of its stream there, and the
    /// entity's version. `None` where the stream holds a `spindle.written` event that names no
    /// instances (schema 1).
    fn resting(&self, entity: &str) -> Result<Option<Resting>, StoreError> {
        let stream = self.entity_stream(entity)?;
        let mut resting = BTreeMap::<String, u64>::new();
        let mut version = 0;
        loop {
            let page = self
                .runtime
                .block_on(self.log.read_stream(&stream, version, MAX_READ_LIMIT))
                .map_err(backend)?;
            let read = page.events.len();
            for event in page.events {
                if event.version != version + 1 {
                    return Err(StoreError::Backend(format!(
                        "the stream of {entity} skips from version {version} to {}",
                        event.version
                    )));
                }
                version = event.version;
                let Some(written) = Written::read(&event, entity)? else {
                    return Ok(None);
                };
                written.apply(&mut resting);
            }
            if page.end_of_stream || read == 0 {
                return Ok(Some((resting, version)));
            }
        }
    }

    /// Reads the tenant's log once ([`Log::each_event`]), as [`LogStore::scan_log`] reads it, and
    /// keeps for each stream of every entity of `entities` only whether its last `spindle.row` or
    /// `spindle.removed` event is a row: one read for every entity, where a scan of each reads the
    /// whole log again.
    fn count_log(&self, entities: &[String]) -> Result<BTreeMap<String, usize>, StoreError> {
        let counted = entities.iter().collect::<BTreeSet<_>>();
        let mut rests = BTreeMap::<(String, String), bool>::new();
        self.log
            .each_event(&self.runtime, &self.tenant, &mut |event: RecordedEvent| {
                if (event.name == ROW || event.name == REMOVED)
                    && counted.contains(&event.stream_type)
                {
                    rests.insert((event.stream_type, event.stream_id), event.name == ROW);
                }
                Ok(())
            })?;
        let mut counts = entities
            .iter()
            .map(|entity| (entity.clone(), 0))
            .collect::<BTreeMap<_, _>>();
        for ((entity, _), rests) in rests {
            if !rests {
                continue;
            }
            if let Some(count) = counts.get_mut(&entity) {
                *count += 1;
            }
        }
        Ok(counts)
    }

    /// Reads the tenant's log in commit order ([`Log::each_event`]) and keeps, for each stream of
    /// `entity`, its last `spindle.row` event: the row as it rests and the stream version a later
    /// commit names. A `spindle.removed` event after it leaves the stream out, until a later row.
    /// The entity's version is the version of its last `spindle.written` event in the same read:
    /// each write appends one in the append group of its rows, so the version counts exactly the
    /// writes whose rows the scan holds. The scan of a log that may hold writes naming no
    /// instances.
    fn scan_log(&self, entity: &str) -> Result<Scan, StoreError> {
        let mut latest = BTreeMap::<String, Stored>::new();
        let mut version = 0;
        self.log
            .each_event(&self.runtime, &self.tenant, &mut |event: RecordedEvent| {
                if event.stream_type == ENTITY && event.stream_id == entity {
                    version = event.version;
                    return Ok(());
                }
                if event.stream_type != entity {
                    return Ok(());
                }
                if event.name == REMOVED {
                    latest.remove(&event.stream_id);
                    return Ok(());
                }
                if event.name != ROW {
                    return Ok(());
                }
                let row = serde_json::from_value::<Row>(event.data)
                    .map_err(|error| StoreError::Backend(format!("a stored row: {error}")))?;
                latest.insert(
                    event.stream_id,
                    Stored {
                        row,
                        version: event.version,
                    },
                );
                Ok(())
            })?;
        let rows = latest
            .into_iter()
            .map(|(identity, stored)| Ok((identity_from_key(&identity)?, stored)))
            .collect::<Result<_, StoreError>>()?;
        Ok(Scan { rows, version })
    }
}

/// SQLite has one writer, so its feed holds every committed event: a scan reads it page by page,
/// every page moving the position forward or the read stopping as a backend error. A file may
/// hold writes of any release, back to those before 0.7.0 recorded no entity's version, and none
/// is marked: a store reads the feed whole once, when it opens, into its resting rows
/// ([`RestingRows`]), and scans and counts from them.
#[cfg(feature = "sqlite")]
impl Log for SqliteEventStore {
    const UNMARKED: bool = true;

    fn each_event(
        &self,
        runtime: &tokio::runtime::Runtime,
        tenant: &TenantId,
        visit: &mut Visit<'_>,
    ) -> Result<(), StoreError> {
        let mut position = 0;
        loop {
            let page = runtime
                .block_on(self.read_feed(tenant, position, FEED_PAGE))
                .map_err(backend)?;
            for event in page.events {
                visit(event)?;
            }
            if !page.has_more {
                return Ok(());
            }
            if page.next_position <= position {
                return Err(StoreError::Backend(format!(
                    "the feed did not move past position {position}"
                )));
            }
            position = page.next_position;
        }
    }

    /// Reads the same file on connections of its own, which share one runtime and the store's
    /// resting rows: one opened with the handle, more as reads that run at once need them, up to
    /// as many as the machine runs threads at once (at least 2). `None` where no connection could
    /// be opened.
    fn reading(store: &LogStore<Self>) -> Option<Arc<dyn Reading>> {
        let path = store.path.clone()?;
        let runtime = sqlite_runtime().ok()?;
        let rows = store.rows.clone();
        let first = SqliteStore::open_existing(&path, Arc::clone(&runtime), rows.clone()).ok()?;
        let cap = std::thread::available_parallelism()
            .map_or(2, std::num::NonZero::get)
            .max(2);
        Some(Arc::new(SqliteReading {
            path,
            runtime,
            rows,
            cap,
            connections: Mutex::new(Connections {
                idle: vec![first],
                live: 1,
                waiting: 0,
            }),
            returned: Condvar::new(),
            retired: Arc::clone(&store.retired),
        }))
    }
}

#[cfg(feature = "sqlite")]
impl<L: Log> LogStore<L> {
    /// Hands every event of the tenant's feed past `position` to `each`, in commit order, a page
    /// of [`FEED_PAGE`] at a time, to the feed's end, counting each on `rows`; answers the
    /// position past the last event read. Every page moves the position forward, or the read
    /// stops as a backend error.
    fn feed(
        &self,
        rows: &RestingRows,
        mut position: u64,
        mut each: impl FnMut(RecordedEvent),
    ) -> Result<u64, StoreError> {
        loop {
            let page = self
                .runtime
                .block_on(self.log.read_feed(&self.tenant, position, FEED_PAGE))
                .map_err(backend)?;
            let read = u64::try_from(page.events.len()).unwrap_or(u64::MAX);
            rows.read.fetch_add(read, Ordering::SeqCst);
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

    /// The resting `rows`, under their lock, once every event appended past those they hold is
    /// folded in. The events are read and their rows decoded outside the lock, to the feed's
    /// end; under it, only those past the position another read may have folded to meanwhile are
    /// applied (SQLite's feed position is the event's `global_seq`). An append group commits
    /// whole and the read runs to the feed's end, so the rows hold each write group whole or not
    /// at all, and the lock is held only to apply what was read and to hand out the rows as they
    /// stand.
    fn caught_up<'a>(
        &self,
        rows: &'a RestingRows,
    ) -> Result<std::sync::MutexGuard<'a, Folded>, StoreError> {
        let start = rows.folded().position;
        let mut folds = Vec::new();
        let end = self.feed(rows, start, |event| {
            let position = event.global_seq;
            if let Some(fold) = Fold::of(event) {
                folds.push((position, fold));
            }
        })?;
        let mut held = rows.folded();
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

/// The rows of every entity of a SQLite store as they rest in its log, folded from its feed:
/// what the store and every connection of its reading handle scan and count. Memory holds each
/// entity's resting rows, never the events that led to them.
///
/// Only a SQLite file is folded so: SQLite has one writer at a time, so an event never commits
/// behind a feed position a read has already passed, and a read from the last position folded
/// misses nothing. A PostgreSQL store, whose transactions commit out of the order of their
/// positions, scans the entity's stream instead ([`LogStore::scan_named`]).
#[cfg(feature = "sqlite")]
#[derive(Debug, Default)]
pub(crate) struct RestingRows {
    folded: Mutex<Folded>,
    /// How many feed events were read for it ([`SqliteStore::events_read`]).
    read: AtomicU64,
}

#[cfg(feature = "sqlite")]
impl RestingRows {
    fn folded(&self) -> std::sync::MutexGuard<'_, Folded> {
        self.folded.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The feed folded up to a position: each entity's rows as one value a read takes as it stands,
/// so a fold changes an entity in place unless a read still holds it, and then a copy of that
/// entity alone.
#[cfg(feature = "sqlite")]
#[derive(Debug, Default)]
struct Folded {
    /// The feed position past the last event folded.
    position: u64,
    entities: BTreeMap<String, Arc<Entity>>,
}

/// One entity's rows in a [`Folded`] feed.
#[cfg(feature = "sqlite")]
#[derive(Debug, Clone, Default)]
struct Entity {
    /// Each instance's last `spindle.row` event, by its stream id, unless a `spindle.removed`
    /// event followed it.
    rows: BTreeMap<String, Arc<Stored>>,
    /// The streams whose last such event carried no row it could read: counted, never scanned.
    unreadable: BTreeSet<String>,
    /// Why a `spindle.row` event of the entity did not read, if one did not: a scan of the
    /// entity fails, as a read of the whole log did.
    malformed: Option<String>,
    /// The version of the entity's `spindle:entity` stream: how many writes of it landed.
    version: u64,
}

/// What one feed event changes in the resting rows, decoded from it.
#[cfg(feature = "sqlite")]
enum Fold {
    /// The entity's `spindle:entity` stream stands at this version.
    Version(String, u64),
    /// The instance of the entity rests holding this row, or why its row event did not read.
    Row(String, String, Result<Arc<Stored>, String>),
    /// The instance of the entity was removed.
    Removed(String, String),
}

#[cfg(feature = "sqlite")]
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

#[cfg(feature = "sqlite")]
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

#[cfg(feature = "sqlite")]
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

/// How many connections a [`SqliteReading`] keeps open beyond those its reads hold, when no read
/// waits for one.
#[cfg(feature = "sqlite")]
const IDLE: usize = 2;

/// The reading handle of a [`SqliteStore`]: the file, and its connections. Reads that run at
/// once each read on a connection of their own, up to `cap` of them; a read beyond that waits for
/// one handed back. Once no read waits, at most [`IDLE`] are kept for the next reads and the rest
/// closed.
#[cfg(feature = "sqlite")]
struct SqliteReading {
    path: PathBuf,
    /// What drives every connection's log calls.
    runtime: Arc<tokio::runtime::Runtime>,
    /// The store's resting rows, which every connection reads and folds the feed into.
    rows: Option<Arc<RestingRows>>,
    /// How many connections may be open at once.
    cap: usize,
    connections: Mutex<Connections>,
    /// Signalled whenever a connection is handed back or given up.
    returned: Condvar,
    /// The store this handle reads was closed ([`Drop`]): the file at its path may be another
    /// now, so reads take turns on the connections already open.
    retired: Arc<AtomicBool>,
}

/// The connections of a [`SqliteReading`].
#[cfg(feature = "sqlite")]
struct Connections {
    /// Those no read holds now.
    idle: Vec<SqliteStore>,
    /// How many are open, idle or held, or being opened.
    live: usize,
    /// How many reads wait for one.
    waiting: usize,
}

#[cfg(feature = "sqlite")]
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
                self.rows.clone(),
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

#[cfg(feature = "sqlite")]
impl Reading for SqliteReading {
    fn scan(&self, entity: &str) -> Result<Scan, StoreError> {
        self.with(|connection| connection.scan(entity))
    }

    fn counts(&self, entities: &[String]) -> Result<BTreeMap<String, usize>, StoreError> {
        self.with(|connection| connection.counts(entities))
    }
}

/// Each instance of an entity whose row rests, by stream id at the version of its stream there,
/// and the entity's version ([`LogStore::resting`]).
type Resting = (BTreeMap<String, u64>, u64);

/// What one write's `spindle.written` event names (schema [`WRITTEN_SCHEMA`]): each instance of
/// the entity whose row the write left, and each it removed, by stream id, at the version its
/// stream stands at after the write. An instance the write left nothing of, or only
/// conditioned the write on, is in neither.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Written {
    rows: BTreeMap<String, u64>,
    removed: BTreeMap<String, u64>,
}

impl Written {
    /// Notes that the write leaves the instance on stream `id` at `version`, removed or holding
    /// a row: a later note of the same stream in one write replaces an earlier one.
    pub(crate) fn note(&mut self, id: &str, version: u64, removed: bool) {
        let (to, from) = if removed {
            (&mut self.removed, &mut self.rows)
        } else {
            (&mut self.rows, &mut self.removed)
        };
        from.remove(id);
        to.insert(id.to_owned(), version);
    }

    /// The body of the `spindle.written` event: `{"rows": {id: version}, "removed": {id:
    /// version}}`.
    pub(crate) fn body(self) -> Value {
        let object = |entries: BTreeMap<String, u64>| {
            Value::Object(
                entries
                    .into_iter()
                    .map(|(id, version)| (id, Value::from(version)))
                    .collect(),
            )
        };
        let mut body = serde_json::Map::new();
        body.insert("rows".to_owned(), object(self.rows));
        body.insert("removed".to_owned(), object(self.removed));
        Value::Object(body)
    }

    /// What `event` of `entity`'s stream names, or `None` for one of the store of 0.10.0, which
    /// names no instances.
    fn read(event: &RecordedEvent, entity: &str) -> Result<Option<Self>, StoreError> {
        if event.schema_version < WRITTEN_SCHEMA {
            return Ok(None);
        }
        let malformed = || {
            StoreError::Backend(format!(
                "the event {} at version {} of the stream of {entity} does not name its \
                 instances as schema {} does",
                event.name, event.version, event.schema_version
            ))
        };
        if event.name != WRITTEN {
            return Err(malformed());
        }
        let entries = |key: &str| -> Result<BTreeMap<String, u64>, StoreError> {
            event
                .data
                .get(key)
                .and_then(Value::as_object)
                .ok_or_else(malformed)?
                .iter()
                .map(|(id, version)| {
                    version
                        .as_u64()
                        .filter(|version| *version > 0)
                        .map(|version| (id.clone(), version))
                        .ok_or_else(malformed)
                })
                .collect()
        };
        Ok(Some(Self {
            rows: entries("rows")?,
            removed: entries("removed")?,
        }))
    }

    /// Applies the write to `resting`, each instance whose row rests by stream id at its
    /// version.
    fn apply(self, resting: &mut BTreeMap<String, u64>) {
        for id in self.removed.into_keys() {
            resting.remove(&id);
        }
        resting.extend(self.rows);
    }
}

/// About how many bytes `value` takes as JSON: its strings and keys, and a few bytes for each
/// other value, without writing it.
fn weight(value: &Value) -> usize {
    match value {
        Value::String(text) => text.len() + 2,
        Value::Array(items) => items.iter().map(weight).sum::<usize>() + 2,
        Value::Object(entries) => {
            entries
                .iter()
                .map(|(key, value)| key.len() + 3 + weight(value))
                .sum::<usize>()
                + 2
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => 8,
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
pub(crate) fn meta(request_hash: String) -> CommandMeta {
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

pub(crate) fn backend(error: EventLogError) -> StoreError {
    StoreError::Backend(error.to_string())
}

#[cfg(all(test, feature = "sqlite"))]
mod tests {
    use eventlog_core::AtomicEventStore;
    use serde_json::json;

    use super::*;

    const TASK: &str = "a.Task";

    /// A store on a SQLite database in memory.
    fn store() -> SqliteStore {
        SqliteStore::open(Path::new(":memory:")).expect("the store opens")
    }

    fn change(identity: &str, expected: Option<u64>, state: &str) -> Change {
        Change {
            entity: TASK.to_owned(),
            identity: json!(identity),
            expected,
            row: Row {
                state: state.to_owned(),
                fields: BTreeMap::new(),
            },
            events: Vec::new(),
            scanned: Vec::new(),
        }
    }

    /// The last `spindle.written` event of `entity`'s stream: its schema and body.
    fn last_written(store: &SqliteStore, entity: &str) -> (u32, Value) {
        let stream = store.entity_stream(entity).expect("a stream id");
        let (_, event) = store.last(&stream).expect("reads").expect("written");
        assert_eq!(event.name, WRITTEN);
        (event.schema_version, event.data)
    }

    #[test]
    fn a_write_names_each_instance_it_leaves_at_its_streams_version() {
        let mut store = store();
        store
            .commit_all(&[
                Write::Commit(change("a", None, "Open")),
                Write::Commit(change("b", None, "Open")),
                Write::Commit(change("b", Some(1), "Done")),
            ])
            .expect("a group");
        assert_eq!(
            last_written(&store, TASK),
            (
                WRITTEN_SCHEMA,
                json!({"rows": {"\"a\"": 1, "\"b\"": 2}, "removed": {}})
            )
        );
        store
            .remove(&Removal {
                entity: TASK.to_owned(),
                identity: json!("a"),
                expected: 1,
                events: Vec::new(),
            })
            .expect("a is removed");
        assert_eq!(
            last_written(&store, TASK),
            (WRITTEN_SCHEMA, json!({"rows": {}, "removed": {"\"a\"": 2}}))
        );
        let mut conditioned = change("c", None, "Open");
        conditioned.entity = "b.Other".to_owned();
        conditioned.scanned = vec![Scanned {
            entity: TASK.to_owned(),
            version: 2,
        }];
        store.commit(&conditioned).expect("a conditioned write");
        assert_eq!(
            last_written(&store, TASK),
            (WRITTEN_SCHEMA, json!({"rows": {}, "removed": {}})),
            "an entity a write is conditioned on names no instance"
        );
        let scan = store.scan_named(TASK).expect("scans");
        assert_eq!(scan.version, 3);
        assert_eq!(scan, store.scan_log(TASK).expect("reads the log"));
        assert_eq!(scan.rows.len(), 1);
        assert_eq!(scan.rows[0].0, json!("b"));
        assert_eq!(scan.rows[0].1.version, 2);
    }

    /// A write as the store of 0.10.0 appended it: its `spindle.written` event names nothing.
    fn write_as_before(store: &SqliteStore, identity: &str) {
        let appends = vec![
            StreamAppend {
                stream: store.entity_stream(TASK).expect("a stream id"),
                expected: Expected::Any,
                events: vec![
                    NewEvent::new(WRITTEN, SCHEMA, Value::Object(serde_json::Map::new()))
                        .expect("an event"),
                ],
            },
            StreamAppend {
                stream: store.stream(TASK, &json!(identity)).expect("a stream id"),
                expected: Expected::NoStream,
                events: vec![row_event(&change(identity, None, "Old").row).expect("a row")],
            },
        ];
        let entries: Vec<&Vec<NewEvent>> = appends.iter().map(|append| &append.events).collect();
        let group = AppendGroup {
            tenant: store.tenant.clone(),
            meta: meta(request_hash(&entries).expect("a hash")),
            appends,
        };
        store
            .runtime
            .block_on(store.log.append_group(&group))
            .expect("appended");
    }

    #[test]
    fn an_entity_holding_a_write_that_names_no_instances_is_scanned_from_the_log() {
        let mut store = store();
        write_as_before(&store, "old");
        store
            .commit(&change("new", None, "Open"))
            .expect("a write after it");
        store
            .commit(&change("old", Some(1), "Moved"))
            .expect("the older instance moves");
        let scan = store.scan_named(TASK).expect("scans");
        assert_eq!(scan, store.scan_log(TASK).expect("reads the log"));
        assert_eq!(scan.version, 3);
        let rows: Vec<_> = scan
            .rows
            .iter()
            .map(|(identity, stored)| (identity.clone(), stored.row.state.clone(), stored.version))
            .collect();
        assert_eq!(
            rows,
            [
                (json!("new"), "Open".to_owned(), 1),
                (json!("old"), "Moved".to_owned(), 2)
            ]
        );
    }
}
