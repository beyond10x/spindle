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
//! answers and a write decided on a scan names ([`crate::Write::scanned`]), so a row of the
//! entity written after the scan refuses the write. The event names the instances the write
//! left, each at its stream's version after it (schema 2: `{"rows": {stream: version},
//! "removed": {stream: version}}`), so a scan can read the entity's stream and one event per
//! instance rather than the whole log. The store of 0.10.0 wrote it empty (schema 1), and before
//! 0.7.0 wrote none. A SQLite file, which any release may have written, is read whole once, when
//! the store opens, into each entity's rows as they rest ([`RestingRows`]), which its scans and
//! counts answer from, folding in only the events appended since; a scan of a PostgreSQL store
//! reads the entity's stream, unless the log is marked as holding such writes ([`MARKS`]) or the
//! entity's stream holds one.
//!
//! A write attributed to a command ([`Store::attribute`]) names it in each of its `spindle.written`
//! events too (`"origin": {"command", "sequence", "cause"}`), the cause being what caused the
//! command ([`crate::Provenance`]: a request's or seed entry's caller with the request's id, a
//! tick, or a binding), and the append's actor is that caller's actor; an origin a release before
//! 0.23.0 wrote names no cause. Each append keeps a request id of its own. The events the
//! commands committed are read back from the tenant's feed past a position
//! ([`Store::events_after`]), each at the feed position of its append group's last event and
//! attributed as its group's `spindle.written` events name.
//!
//! The encoding is the store's, not SQLite's: [`LogStore`] keeps it on any eventlog event store
//! that appends groups and answers the tenant's whole log ([`Log`]), and the PostgreSQL store
//! (`PostgresStore`, the `postgres` feature) is the same store on PostgreSQL.

use std::collections::{BTreeMap, BTreeSet, btree_map};
#[cfg(feature = "sqlite")]
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
#[cfg(feature = "sqlite")]
use std::sync::{Condvar, Mutex, PoisonError};

use eventlog_core::EventStore;
use eventlog_core::{
    AppendGroup, AppendGroupResult, CommandMeta, EventLogError, Expected, Guard, MAX_READ_LIMIT,
    NewEvent, Read, ReadResult, RecordedEvent, StreamAppend, StreamId, TenantId, new_event_id,
    request_hash,
};
#[cfg(feature = "sqlite")]
use eventlog_sqlite::SqliteEventStore;
use serde_json::Value;
use time::OffsetDateTime;

use crate::store::{
    Change, Committed, DeadLetter, Origin, Position, Provenance, Reading, Removal, Row, Scan,
    Scanned, Store, StoreError, Stored, Write, first_writes, identity_from_key, moved,
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
/// The field of a `spindle.written` event's body naming the command its write was attributed to
/// ([`Store::attribute`]): `{"command": <qualified name>, "sequence": <n>}`. A write attributed
/// to none, and every write of a release before it, carries none.
const ORIGIN: &str = "origin";
/// The field of an origin naming what caused its command ([`Origin::cause`], [`Provenance`]); an
/// origin a release before 0.23.0 wrote carries none.
const CAUSE: &str = "cause";
/// The field of a `spindle.written` event's body, and of a `spindle.published` end's, naming the
/// outcome named for its write ([`Store::outcome`]); a write of a release before 0.23.0 names
/// none.
const OUTCOME: &str = "outcome";
/// The field of a `spindle.written` event's body listing the identity of each instance of its
/// entity the write wrote as its key text (the canonical JSON text the stream id and the `rows`
/// keys hold, in which U+0000 is escaped), each once in text order ([`Committed::instances`]); a
/// write of a release before 0.23.0, and one that wrote no instance of the entity, carries none.
const INSTANCES: &str = "instances";
/// The stream of the events a command committed with no row ([`Store::commit_events`]): each
/// such write appends them, then one [`PUBLISHED_END`] event naming the command under
/// [`ORIGIN`]. Of the stream type [`MARKS`] is of, which a scan and a state move count as no
/// entity's.
pub(crate) const PUBLISHED: (&str, &str) = ("spindle:log", "published");
/// The event closing each write of [`PUBLISHED`].
const PUBLISHED_END: &str = "spindle.published";
/// The stream a reset of a store emptied in place appends one [`RESET_MARK`] event to
/// ([`Store::forget_events`]): the events read back are those committed after the last.
pub(crate) const RESET: (&str, &str) = ("spindle:log", "reset");
/// The event of [`RESET`].
const RESET_MARK: &str = "spindle.reset";
/// The stream type of the streams carrying an entity's version, one per entity: no qualified
/// entity name holds a colon.
pub(crate) const ENTITY: &str = "spindle:entity";
/// The stream type and id of the stream whose presence marks a log holding writes that name no
/// instances (written before 0.7.0, or by 0.10.0 and earlier): `state move` appends an event to
/// it before it copies such a write into a PostgreSQL store. A scan of a marked log reads the
/// whole log.
pub(crate) const MARKS: (&str, &str) = ("spindle:log", "unnamed-writes");
/// The stream recording the specification the store was last written under
/// ([`LogStore::fit`]): each event one [`crate::Shapes`], the last the one in force. Of the
/// stream type [`MARKS`] is of, which a scan and a state move count as no entity's, and left out
/// of the events read back ([`LogStore::groups_after`]).
pub(crate) const SPECIFICATION: (&str, &str) = ("spindle:log", "specification");
/// The event of [`SPECIFICATION`].
const SPECIFIED: &str = "spindle.specified";
/// The stream id prefix of an egress cursor, a stream of the type [`MARKS`] is of, one per
/// `[http.<name>]` call: `egress:<name>` ([`Store::egress_posted`]). Each of its events is one
/// [`EGRESS_MARK`]; the last names the cursor. No published event: a read of the events
/// committed skips it.
const EGRESS: &str = "egress:";
/// The event of an egress cursor's stream: `{"position": <n>}`, the position of the last write
/// whose events were posted to the call.
const EGRESS_MARK: &str = "spindle.egress";
/// The stream of the store's dead letters ([`Store::keep_dead_letter`]): one [`DEAD_LETTER`]
/// event for each, its id the event's version plus the greatest `after` of the [`CONTINUED`]
/// events before it, and one [`REPLAYED`] event for each taken ([`Store::take_dead_letter`]).
/// Of the stream type [`MARKS`] is of; no read of the events answers it
/// ([`LogStore::groups_after`]), and only those after the last [`RESET`] mark are kept.
pub(crate) const DEAD_LETTERS: (&str, &str) = ("spindle:log", "dead-letters");
/// The event of [`DEAD_LETTERS`] that keeps one, its body the [`DeadLetter`].
const DEAD_LETTER: &str = "spindle.dead-letter";
/// The event of [`DEAD_LETTERS`] that takes one, `{"id": <id>}`.
const REPLAYED: &str = "spindle.replayed";
/// The event of [`DEAD_LETTERS`] that continues the ids of the store a reset replaced,
/// `{"after": <id>}` ([`Store::continue_dead_letter_ids`]): every dead letter kept after it has
/// an id greater than `after`.
const CONTINUED: &str = "spindle.dead-letter-ids-continued";
/// Who records the change: the runtime itself, the actor of an append whose cause names no actor
/// ([`LogStore::attributed`]).
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

        /// Whether the tenant's feed may withhold an event after its append committed
        /// ([`super::LogStore::published`]): PostgreSQL's does while any transaction older than
        /// the append is open on the server.
        const WITHHOLDS: bool;

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
    /// The command the writes are attributed to ([`Store::attribute`]): each write's
    /// `spindle.written` events carry it.
    origin: Option<Origin>,
    /// The outcome named for the writes ([`Store::outcome`]): each write's `spindle.written`
    /// events, and the end of a write of events no row carries, carry it.
    outcome: Option<String>,
    /// How far the feed holds this store's own appends, shared with its reading handle where
    /// that reads through the same event store ([`LogStore::published`]).
    own: Arc<Own>,
    /// How far this store read the stream [`DEAD_LETTERS`] for the ids it continues, and the
    /// greatest `after` of the [`CONTINUED`] events up to there ([`LogStore::offset_before`]).
    continued: (u64, u64),
    /// The check every write runs inside its own append transaction, where one is set
    /// ([`LogStore::fence`]): a PostgreSQL store opened under its component's lease refuses each
    /// write once another process took that lease (`story:several-writers-one-postgres`). A
    /// SQLite store has none: its state directory is locked to one process.
    fence: Option<Arc<dyn Guard>>,
    /// Whether the lease the fence names is known still held, checked before a read of the feed
    /// by position ([`Reading::events_placed_after`]), which an egress posts from: a process
    /// that lost its lease, or was paused past it, posts no event another process committed.
    #[cfg(feature = "postgres")]
    current: Option<Current>,
}

/// The check of [`LogStore::current`].
#[cfg(feature = "postgres")]
type Current = Arc<dyn Fn() -> Result<(), StoreError> + Send + Sync>;

/// The feed positions of a store's own appends: the last one it committed, and the last one the
/// feed was seen to answer. A read of the feed waits until the feed answers the first
/// ([`LogStore::published`]).
#[derive(Default)]
struct Own {
    committed: AtomicU64,
    published: AtomicU64,
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

#[cfg(test)]
thread_local! {
    /// How many calls of its event store a [`LogStore`] blocked on, on this thread: what a test
    /// reads to tell how many log reads a write made.
    static LOG_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

impl<L> LogStore<L> {
    /// Blocks on `call`, one call of the event store, on the store's runtime.
    pub(crate) fn call<F: std::future::Future>(&self, call: F) -> F::Output {
        #[cfg(test)]
        LOG_CALLS.with(|calls| calls.set(calls.get() + 1));
        self.runtime.block_on(call)
    }
}

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
        if create {
            provisioned(path, location)?;
        }
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

/// Creates the database file at `path` before SQLite opens it, and its missing directories, kept
/// private ([`crate::private`]): the directories `0700`, the file `0600`, whatever the umask.
/// SQLite gives each journal, WAL and shared-memory file it creates beside a database the mode of
/// the database, so they are `0600` too. A file or directory that exists keeps its mode, and an
/// in-memory or URI location (`:memory:`, `file:…`) is SQLite's to open.
#[cfg(feature = "sqlite")]
fn provisioned(path: &Path, location: &str) -> Result<(), StoreError> {
    if location.is_empty() || location == ":memory:" || location.starts_with("file:") {
        return Ok(());
    }
    let failed = |what: &str, at: &Path, error: std::io::Error| {
        StoreError::Backend(format!("{what} {}: {error}", at.display()))
    };
    if let Some(directory) = path.parent() {
        crate::private::create_dir_all(directory)
            .map_err(|error| failed("creating the state directory", directory, error))?;
    }
    crate::private::open_or_create(path)
        .map(drop)
        .map_err(|error| failed("creating the state file", path, error))
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
            origin: None,
            outcome: None,
            own: Arc::default(),
            continued: (0, 0),
            fence: None,
            #[cfg(feature = "postgres")]
            current: None,
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
            origin: None,
            outcome: None,
            own: Arc::clone(&self.own),
            continued: (0, 0),
            fence: self.fence.clone(),
            #[cfg(feature = "postgres")]
            current: self.current.clone(),
        }
    }

    /// The event store.
    #[cfg(feature = "postgres")]
    pub(crate) fn log(&self) -> &L {
        &self.log
    }

    /// The tenant every stream of the store is of.
    #[cfg(feature = "postgres")]
    pub(crate) fn tenant(&self) -> &TenantId {
        &self.tenant
    }

    /// Every write from now on runs `fence` inside its own append transaction and lands only where
    /// it admits the write; its refusal ([`EventLogError::GuardRefused`]) is the write's error,
    /// its code the whole message ([`backend`]).
    #[cfg(feature = "postgres")]
    pub(crate) fn fence(&mut self, fence: Arc<dyn Guard>, current: Current) {
        self.fence = Some(fence);
        self.current = Some(current);
    }

    /// Refuses where the store is fenced and its lease is not known still held.
    #[cfg(feature = "postgres")]
    fn current(&self) -> Result<(), StoreError> {
        self.current.as_ref().map_or(Ok(()), |current| current())
    }

    /// Appends `group` in one transaction, under the store's fence where it has one.
    fn grouped(&self, group: &AppendGroup) -> Result<AppendGroupResult, EventLogError> {
        match &self.fence {
            Some(fence) => self.call(self.log.append_group_guarded(group, Arc::clone(fence))),
            None => self.call(self.log.append_group(group)),
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
            .call(self.log.stream_version(stream))
            .map_err(backend)?
        else {
            return Ok(None);
        };
        let slice = self
            .call(self.log.read_stream(stream, version.saturating_sub(1), 1))
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
        stands(write, actual, removed)
    }

    /// [`LogStore::stands_at`] for each of `firsts`, by instance, read together: a store that
    /// folds the rows of a SQLite file answers each instance whose row rests there from those
    /// rows, caught up to the feed's end once ([`LogStore::caught_up`]); every other instance (one
    /// never written or removed, a row that did not read, any instance of a PostgreSQL store) is
    /// read from its stream. A stream another writer moves after this read refuses the append
    /// group, which names the version each stream stands at.
    fn standing<'w>(
        &self,
        firsts: &[&'w Write],
    ) -> Result<BTreeMap<(&'w str, String), Option<u64>>, StoreError> {
        let resting = self.resting_versions(firsts)?;
        let mut standing = BTreeMap::new();
        for (write, resting) in firsts.iter().zip(resting) {
            let removed = match resting {
                Some(version) => stands(write, Some(version), None)?,
                None => self.stands_at(write)?,
            };
            let (entity, identity) = write.instance();
            standing.insert((entity, identity.to_string()), removed);
        }
        Ok(standing)
    }

    /// The version of each of `firsts`' instances whose row rests in the folded rows of a SQLite
    /// file, caught up once; `None` for every other.
    #[cfg(feature = "sqlite")]
    fn resting_versions(&self, firsts: &[&Write]) -> Result<Vec<Option<u64>>, StoreError> {
        let Some(rows) = &self.rows else {
            return Ok(vec![None; firsts.len()]);
        };
        let folded = self.caught_up(rows)?;
        Ok(firsts
            .iter()
            .map(|write| {
                let (entity, identity) = write.instance();
                folded
                    .entities
                    .get(entity)
                    .and_then(|rows| rows.rows.get(&identity.to_string()))
                    .map(|stored| stored.version)
            })
            .collect())
    }

    /// Without SQLite no store folds rows: `None` for every instance.
    #[cfg(not(feature = "sqlite"))]
    #[allow(clippy::unused_self, clippy::unnecessary_wraps)]
    fn resting_versions(&self, firsts: &[&Write]) -> Result<Vec<Option<u64>>, StoreError> {
        Ok(vec![None; firsts.len()])
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
            .call(self.log.stream_version(&stream))
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

impl<L: Log> LogStore<L> {
    /// The instance's row as its stream's last event holds it ([`Store::load`]).
    fn loaded(&self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
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
}

impl<L: Log> Store for LogStore<L> {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.loaded(entity, identity)
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
    /// appended, naming that instance, the versions of all of them read together
    /// ([`LogStore::standing`]); a stream moved in between refuses the whole group. The
    /// group appends one `spindle.written` event to the stream of each entity it writes, and of
    /// each entity a write was conditioned on ([`Write::scanned`]), that one at the version the
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
        // Where each instance's stream ends removed before the group, which a write naming no
        // version appends after: every instance whose row rests read together, not one read each.
        let ends_removed = self.standing(&firsts)?;
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
        // The identities each entity's writes wrote, by their text: each once, in text order.
        let mut named = BTreeMap::<&str, BTreeSet<String>>::new();
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
            named
                .entry(entity)
                .or_default()
                .insert(identity.to_string());
            instances.push(StreamAppend {
                stream,
                expected,
                events,
            });
        }
        let mut appends = Vec::with_capacity(instances.len() + marks.len());
        for (entity, expected) in marks {
            let mut body = written.remove(entity).unwrap_or_default().body();
            let identities = named.remove(entity).unwrap_or_default();
            self.attributing(&mut body, identities);
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
            meta: self.attributed(request_hash(&entries).map_err(backend)?),
            appends,
        };
        match self.grouped(&group) {
            Ok(landed) => {
                self.landed(&landed);
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

    /// Names no outcome until one is named again.
    fn attribute(&mut self, origin: Option<Origin>) {
        self.origin = origin;
        self.outcome = None;
    }

    fn outcome(&mut self, outcome: &str) {
        self.outcome = Some(outcome.to_owned());
    }

    /// Appends `events` to the stream [`PUBLISHED`], closed by a `spindle.published` event
    /// naming the command they are attributed to, as one append group. The group is conditioned
    /// on `scanned` as [`Store::commit_all`] conditions one: one `spindle.written` event naming
    /// no instance appended to the stream of each entity, at the version its scan answered.
    fn commit_events(
        &mut self,
        events: &[crate::Event],
        scanned: &[Scanned],
    ) -> Result<(), StoreError> {
        if events.is_empty() {
            return Ok(());
        }
        for scanned in scanned {
            self.unmoved(scanned)?;
        }
        let mut appends = Vec::with_capacity(scanned.len() + 1);
        for scanned in scanned {
            let mut body = Written::default().body();
            self.attributing(&mut body, BTreeSet::new());
            appends.push(StreamAppend {
                stream: self.entity_stream(&scanned.entity)?,
                expected: match scanned.version {
                    0 => Expected::NoStream,
                    version => Expected::Exact(version),
                },
                events: vec![NewEvent::new(WRITTEN, WRITTEN_SCHEMA, body).map_err(backend)?],
            });
        }
        let mut body = Value::Object(serde_json::Map::new());
        self.attributing(&mut body, BTreeSet::new());
        let end = NewEvent::new(PUBLISHED_END, SCHEMA, body).map_err(backend)?;
        let (kind, id) = PUBLISHED;
        appends.push(StreamAppend {
            stream: StreamId::new(self.tenant.clone(), kind, id).map_err(backend)?,
            expected: Expected::Any,
            events: recorded(events, end)?,
        });
        match self.append(appends) {
            Ok(()) => {
                // As after a write group: the entity versions it moved are folded in now.
                #[cfg(feature = "sqlite")]
                if let Some(rows) = &self.rows {
                    drop(self.caught_up(rows));
                }
                Ok(())
            }
            Err(EventLogError::Conflict { .. }) => {
                for scanned in scanned {
                    self.unmoved(scanned)?;
                }
                Err(StoreError::Backend(
                    "another writer moved an entity while the events were appended".to_owned(),
                ))
            }
            Err(other) => Err(backend(other)),
        }
    }

    /// Appends one `spindle.reset` event to the stream [`RESET`]: the events read back are
    /// those committed after the last such event ([`LogStore::committed_after`]).
    fn forget_events(&mut self) -> Result<(), StoreError> {
        let mark = NewEvent::new(RESET_MARK, SCHEMA, Value::Object(serde_json::Map::new()))
            .map_err(backend)?;
        let (kind, id) = RESET;
        self.append(vec![StreamAppend {
            stream: StreamId::new(self.tenant.clone(), kind, id).map_err(backend)?,
            expected: Expected::Any,
            events: vec![mark],
        }])
        .map_err(backend)
    }

    fn events_after(&mut self, position: u64, limit: usize) -> Result<Vec<Committed>, StoreError> {
        self.committed_after(position, limit)
    }

    fn events_head(&mut self) -> Result<u64, StoreError> {
        self.feed_end()
    }

    fn events_id(&mut self) -> Result<u64, StoreError> {
        self.log_id()
    }

    /// The `position` of the last event of the call's cursor stream ([`EGRESS`]), `0` for none.
    fn egress_position(&mut self, call: &str) -> Result<u64, StoreError> {
        let stream = self.egress_stream(call)?;
        let Some((_, last)) = self.last(&stream)? else {
            return Ok(0);
        };
        last.data
            .get("position")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                StoreError::Backend(format!(
                    "the egress cursor of {call} at version {} names no position",
                    last.version
                ))
            })
    }

    /// Appends one `spindle.egress` event to the call's cursor stream ([`EGRESS`]), as an append
    /// group of its own.
    fn egress_posted(&mut self, call: &str, position: u64) -> Result<(), StoreError> {
        let mark = NewEvent::new(
            EGRESS_MARK,
            SCHEMA,
            serde_json::json!({"position": position}),
        )
        .map_err(backend)?;
        self.append(vec![StreamAppend {
            stream: self.egress_stream(call)?,
            expected: Expected::Any,
            events: vec![mark],
        }])
        .map_err(backend)
    }

    /// Appends one [`DEAD_LETTER`] event to the stream [`DEAD_LETTERS`], as an append group of
    /// its own; its id is the event's version in that stream past the ids the stream was told to
    /// continue before it ([`CONTINUED`]).
    fn keep_dead_letter(&mut self, letter: &DeadLetter) -> Result<u64, StoreError> {
        let body = serde_json::to_value(letter).map_err(|error| {
            StoreError::Backend(format!("a dead letter does not encode: {error}"))
        })?;
        let event = NewEvent::new(DEAD_LETTER, SCHEMA, body).map_err(backend)?;
        let landed = self
            .appended(vec![StreamAppend {
                stream: self.dead_letter_stream()?,
                expected: Expected::Any,
                events: vec![event],
            }])
            .map_err(backend)?;
        let version = landed
            .appends
            .first()
            .map(|append| append.last_version)
            .ok_or_else(|| StoreError::Backend("the dead letter was not appended".to_owned()))?;
        Ok(self.offset_before(version)? + version)
    }

    /// Read from the stream [`DEAD_LETTERS`], from the first event after the last reset mark
    /// ([`RESET`]) on.
    fn dead_letters(&mut self) -> Result<Vec<(u64, DeadLetter)>, StoreError> {
        Ok(self.ledger()?.letters.into_iter().collect())
    }

    /// Appends one [`REPLAYED`] event naming `id` to the stream [`DEAD_LETTERS`], at the version
    /// the stream stood at when it was read: a dead letter taken in between refuses the take.
    fn take_dead_letter(&mut self, id: u64) -> Result<Option<DeadLetter>, StoreError> {
        let stream = self.dead_letter_stream()?;
        let ledger = self.ledger()?;
        let Some(letter) = ledger.letters.get(&id).cloned() else {
            return Ok(None);
        };
        let event =
            NewEvent::new(REPLAYED, SCHEMA, serde_json::json!({"id": id})).map_err(backend)?;
        self.appended(vec![StreamAppend {
            stream,
            expected: Expected::Exact(ledger.version),
            events: vec![event],
        }])
        .map_err(|error| match error {
            EventLogError::Conflict { .. } => StoreError::Backend(format!(
                "another writer kept or took a dead letter while dead letter {id} was taken"
            )),
            other => backend(other),
        })?;
        Ok(Some(letter))
    }

    /// The stream's version past the ids it was told to continue: no id it answered is greater.
    fn last_dead_letter_id(&mut self) -> Result<u64, StoreError> {
        let ledger = self.ledger()?;
        Ok(ledger.offset + ledger.version)
    }

    /// Appends one [`CONTINUED`] event naming `after` to the stream [`DEAD_LETTERS`], unless the
    /// next id is past `after` already.
    fn continue_dead_letter_ids(&mut self, after: u64) -> Result<(), StoreError> {
        if self.last_dead_letter_id()? >= after {
            return Ok(());
        }
        let event = NewEvent::new(CONTINUED, SCHEMA, serde_json::json!({"after": after}))
            .map_err(backend)?;
        self.appended(vec![StreamAppend {
            stream: self.dead_letter_stream()?,
            expected: Expected::Any,
            events: vec![event],
        }])
        .map_err(backend)?;
        Ok(())
    }
}

impl<L: Log> LogStore<L> {
    /// The egress cursor stream of the call `call` ([`EGRESS`]).
    fn egress_stream(&self, call: &str) -> Result<StreamId, StoreError> {
        StreamId::new(self.tenant.clone(), MARKS.0, format!("{EGRESS}{call}")).map_err(backend)
    }
}

/// What the stream [`DEAD_LETTERS`] holds ([`LogStore::ledger`]).
struct Ledger {
    /// The dead letters it keeps and no [`REPLAYED`] event took, by id.
    letters: BTreeMap<u64, DeadLetter>,
    /// What the id of a dead letter appended next adds to its version: the greatest `after` a
    /// [`CONTINUED`] event named, `0` for none.
    offset: u64,
    /// The version of the last event read, `0` for none.
    version: u64,
}

impl<L: Log> LogStore<L> {
    /// The stream [`DEAD_LETTERS`] of the store's tenant.
    fn dead_letter_stream(&self) -> Result<StreamId, StoreError> {
        let (kind, id) = DEAD_LETTERS;
        StreamId::new(self.tenant.clone(), kind, id).map_err(backend)
    }

    /// The greatest `after` of the [`CONTINUED`] events of the stream [`DEAD_LETTERS`] before
    /// `version`, `0` for none: what the id of the dead letter at `version` adds to it. Reads on
    /// from where the last call stopped, so keeping one dead letter after another does not read
    /// the stream again from its start.
    fn offset_before(&mut self, version: u64) -> Result<u64, StoreError> {
        let stream = self.dead_letter_stream()?;
        let (mut read, mut offset) = self.continued;
        if read >= version {
            (read, offset) = (0, 0);
        }
        'pages: while read + 1 < version {
            let page = self
                .call(self.log.read_stream(&stream, read, MAX_READ_LIMIT))
                .map_err(backend)?;
            if page.events.is_empty() {
                break;
            }
            for event in page.events {
                if event.version >= version {
                    break 'pages;
                }
                read = event.version;
                if event.name == CONTINUED {
                    let after = event.data.get("after").and_then(Value::as_u64);
                    offset = offset.max(after.unwrap_or(0));
                }
            }
            if page.end_of_stream {
                break;
            }
        }
        self.continued = (read, offset);
        Ok(offset)
    }

    /// The stream [`DEAD_LETTERS`] read whole: each dead letter's id is its
    /// version plus the greatest `after` of the [`CONTINUED`] events before it, and only those
    /// appended after the last reset mark ([`RESET`]), compared by feed position, are kept.
    fn ledger(&self) -> Result<Ledger, StoreError> {
        let reset = StreamId::new(self.tenant.clone(), RESET.0, RESET.1).map_err(backend)?;
        let forgotten = self.last(&reset)?.map_or(0, |(_, mark)| mark.global_seq);
        let stream = self.dead_letter_stream()?;
        let mut ledger = Ledger {
            letters: BTreeMap::new(),
            offset: 0,
            version: 0,
        };
        loop {
            let page = self
                .call(
                    self.log
                        .read_stream(&stream, ledger.version, MAX_READ_LIMIT),
                )
                .map_err(backend)?;
            let read = page.events.len();
            for event in page.events {
                ledger.version = event.version;
                if event.name == CONTINUED {
                    let after = event.data.get("after").and_then(Value::as_u64);
                    ledger.offset = ledger.offset.max(after.unwrap_or(0));
                    continue;
                }
                if event.global_seq <= forgotten {
                    continue;
                }
                match event.name.as_str() {
                    DEAD_LETTER => {
                        let letter =
                            serde_json::from_value::<DeadLetter>(event.data).map_err(|error| {
                                StoreError::Backend(format!(
                                    "the dead letter at version {} does not read: {error}",
                                    event.version
                                ))
                            })?;
                        ledger.letters.insert(ledger.offset + event.version, letter);
                    }
                    REPLAYED => {
                        if let Some(id) = event.data.get("id").and_then(Value::as_u64) {
                            ledger.letters.remove(&id);
                        }
                    }
                    _ => {}
                }
            }
            if page.end_of_stream || read == 0 {
                return Ok(ledger);
            }
        }
    }

    /// What an append records about the command it is for ([`meta`]), its actor the actor of the
    /// caller its writes are attributed to ([`Provenance::sender`]) where the cause names one.
    /// The request id stays fresh for each append: a read of the feed tells one write from the
    /// next by it ([`LogStore::groups_after`]).
    fn attributed(&self, request_hash: String) -> CommandMeta {
        let mut meta = meta(request_hash);
        if let Some(actor) = self
            .origin
            .as_ref()
            .and_then(|origin| origin.cause.as_ref())
            .and_then(Provenance::sender)
            .and_then(|sender| sender.actor.clone())
        {
            meta.actor = actor;
        }
        meta
    }

    /// Adds to `body`, a `spindle.written` or `spindle.published` event's, the command the writes
    /// are attributed to ([`ORIGIN`]), the outcome named for them ([`OUTCOME`]) and, where any,
    /// `identities`, the key texts of the entity's instances the write wrote ([`INSTANCES`]).
    fn attributing(&self, body: &mut Value, identities: BTreeSet<String>) {
        let Value::Object(fields) = body else {
            return;
        };
        if let Some(origin) = &self.origin {
            fields.insert(ORIGIN.to_owned(), origin_body(origin));
        }
        if let Some(outcome) = &self.outcome {
            fields.insert(OUTCOME.to_owned(), Value::String(outcome.clone()));
        }
        if !identities.is_empty() {
            fields.insert(
                INSTANCES.to_owned(),
                identities.into_iter().map(Value::String).collect(),
            );
        }
    }

    /// Appends `appends` as one append group.
    fn append(&self, appends: Vec<StreamAppend>) -> Result<(), EventLogError> {
        self.appended(appends).map(drop)
    }

    /// Appends `appends` as one append group, and answers where each landed.
    fn appended(&self, appends: Vec<StreamAppend>) -> Result<AppendGroupResult, EventLogError> {
        let entries: Vec<&Vec<NewEvent>> = appends.iter().map(|append| &append.events).collect();
        let hash =
            request_hash(&entries).map_err(|error| EventLogError::Backend(error.to_string()))?;
        let group = AppendGroup {
            tenant: self.tenant.clone(),
            meta: self.attributed(hash),
            appends,
        };
        let landed = self.grouped(&group)?;
        self.landed(&landed);
        Ok(landed)
    }

    /// Records the feed position of the last event of an append group this store committed: a
    /// read of the feed answers it, and every event before it ([`LogStore::published`]).
    fn landed(&self, group: &AppendGroupResult) {
        let last = group
            .appends
            .iter()
            .flat_map(|append| &append.events)
            .map(|event| event.global_seq)
            .max();
        if let Some(last) = last {
            self.own.committed.fetch_max(last, Ordering::SeqCst);
        }
    }

    /// Returns once the tenant's feed answers every append this store, or the store its reading
    /// handle reads ([`LogStore::shared`]), committed before the call: what a read of the feed
    /// waits for, so that a writer reads back every event it committed. A feed that may withhold
    /// a committed event ([`Log::WITHHOLDS`]: PostgreSQL's, while any transaction older than the
    /// append is open anywhere on the server, and every event after one it withholds) is asked,
    /// one event at a time, until it answers the last such append or [`PUBLISH_WAIT`] passed.
    /// Where `stop` is given it is looked at before each ask, and once it is set the wait is given
    /// up: `false`, where the feed answering the appends is `true`. A reader stopped at exit so
    /// leaves within one pause between two asks, at most 100 ms, and one ask
    /// (`story:leader-exits-promptly-on-sigterm`).
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the feed cannot be read, or still withholds the append after
    /// [`PUBLISH_WAIT`].
    fn published(&self, stop: Option<&AtomicBool>) -> Result<bool, StoreError> {
        if !L::WITHHOLDS {
            return Ok(true);
        }
        let committed = self.own.committed.load(Ordering::SeqCst);
        if self.own.published.load(Ordering::SeqCst) >= committed {
            return Ok(true);
        }
        let started = std::time::Instant::now();
        let mut pause = std::time::Duration::from_millis(5);
        loop {
            if stop.is_some_and(|stop| stop.load(Ordering::SeqCst)) {
                return Ok(false);
            }
            // The feed answers a prefix of the committed events, so it answers the one at
            // `committed` where it answers any at or past it.
            let page = self
                .call(self.log.read_feed(&self.tenant, committed - 1, 1))
                .map_err(backend)?;
            if !page.events.is_empty() {
                self.own.published.fetch_max(committed, Ordering::SeqCst);
                return Ok(true);
            }
            if started.elapsed() >= PUBLISH_WAIT {
                return Err(StoreError::Backend(format!(
                    "the feed still withholds this store's write at position {committed} after \
                     {PUBLISH_WAIT:?}: PostgreSQL publishes an event only once every \
                     transaction older than its write has ended, and one is still open on the \
                     server (pg_stat_activity names it)"
                )));
            }
            std::thread::sleep(pause);
            pause = (pause * 2).min(std::time::Duration::from_millis(100));
        }
    }
}

/// How long a read of the feed waits for it to answer the store's own writes
/// ([`LogStore::published`]): as long as a read of a reading handle waits for its turn
/// (`TURN_WAIT`), three times eventlog's longest transaction of a store
/// (`PoolOptions::transaction_timeout`, 10 s), so a transaction of any Spindle store on the same
/// server delays the read and does not fail it, and a bulk write of another owner fails it only
/// once it held the server that long.
const PUBLISH_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// The reading handle of a store whose event store reads from any thread through a pool of its
/// own: another [`LogStore`] on the same event store and runtime ([`LogStore::shared`]), and the
/// turns its reads take ([`Turns`]), so the reads in flight never hold every connection of the
/// pool the writer takes its own from.
#[cfg(feature = "postgres")]
pub(crate) struct SharedReading<L> {
    store: LogStore<L>,
    turns: Turns,
}

#[cfg(feature = "postgres")]
impl<L> SharedReading<L> {
    /// Reads `store`, at most `cap` reads at once (at least 1), each waiting its turn for at most
    /// [`TURN_WAIT`].
    pub(crate) fn new(store: LogStore<L>, cap: usize) -> Self {
        Self {
            store,
            turns: Turns::new(cap, TURN_WAIT),
        }
    }
}

#[cfg(feature = "postgres")]
impl<L: Log> SharedReading<L> {
    /// The events placed after `position` on a turn, the lease checked before the read and after
    /// it where the store is fenced; no event once `stop`, where given, is set while the read
    /// waits for its turn or for the feed.
    fn placed_unless(
        &self,
        position: u64,
        limit: usize,
        stop: Option<&AtomicBool>,
    ) -> Result<Vec<(u64, Committed)>, StoreError> {
        self.store.current()?;
        let Some(_turn) = self.turns.take_unless(stop)? else {
            return Ok(Vec::new());
        };
        let placed = self.store.placed_after(position, limit, stop)?;
        self.store.current()?;
        Ok(placed)
    }
}

#[cfg(feature = "postgres")]
impl<L: Log> Reading for SharedReading<L> {
    fn scan(&self, entity: &str) -> Result<Scan, StoreError> {
        let _turn = self.turns.take()?;
        self.store.scanned(entity)
    }

    fn counts(&self, entities: &[String]) -> Result<BTreeMap<String, usize>, StoreError> {
        let _turn = self.turns.take()?;
        self.store.counted(entities)
    }

    /// Reads through the store's own pool, one statement on a connection of it at a time, as
    /// its scans do ([`Checked`]); each read of the position takes a turn as a scan does.
    fn at(&self, entities: &[String]) -> Result<Box<dyn Position + '_>, StoreError> {
        Ok(Box::new(Checked::new(&self.store, &self.turns, entities)))
    }

    /// Reads the feed through the store's own pool on a turn, as a scan does.
    fn events_after(&self, position: u64, limit: usize) -> Result<Vec<Committed>, StoreError> {
        let _turn = self.turns.take()?;
        self.store.committed_after(position, limit)
    }

    /// Reads the feed through the store's own pool on a turn, as a scan does. Where the store is
    /// fenced, its lease is checked before the read and again after it, so a read that a pause
    /// past the lease interrupted, and that ran on once the process resumed, answers nothing.
    fn events_placed_after(
        &self,
        position: u64,
        limit: usize,
    ) -> Result<Vec<(u64, Committed)>, StoreError> {
        self.placed_unless(position, limit, None)
    }

    /// [`SharedReading::events_placed_after`], whose wait for its turn and for the feed to answer
    /// the store's own writes looks at `stop`, and answers no event once it is set.
    fn events_placed_after_unless_stopped(
        &self,
        position: u64,
        limit: usize,
        stop: &AtomicBool,
    ) -> Result<Vec<(u64, Committed)>, StoreError> {
        self.placed_unless(position, limit, Some(stop))
    }

    /// Reads the feed through the store's own pool on a turn, as a scan does.
    fn events_head(&self) -> Result<u64, StoreError> {
        let _turn = self.turns.take()?;
        self.store.feed_end()
    }

    fn events_id(&self) -> Result<u64, StoreError> {
        let _turn = self.turns.take()?;
        self.store.log_id()
    }
}

/// How long a read of a [`SharedReading`] waits for its turn before it is refused.
#[cfg(feature = "postgres")]
const TURN_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// How often a read waiting for its turn with a stop flag looks at it ([`Turns::take_unless`]).
#[cfg(feature = "postgres")]
const STOP_LOOK: std::time::Duration = std::time::Duration::from_millis(20);

/// The reads in flight on a [`SharedReading`]: at most `cap` at once. A read beyond that waits,
/// in the order the reads arrived, until one ends, and is refused only once it waited `wait`.
#[cfg(feature = "postgres")]
pub(crate) struct Turns {
    cap: usize,
    wait: std::time::Duration,
    state: std::sync::Mutex<Waiting>,
    /// Signalled whenever a read ends or a waiting one leaves the line.
    ended: std::sync::Condvar,
}

/// The reads of [`Turns`] in flight, and those waiting, first in line first.
#[cfg(feature = "postgres")]
#[derive(Default)]
struct Waiting {
    running: usize,
    line: std::collections::VecDeque<u64>,
    next: u64,
}

/// A read's turn: it ends when dropped.
#[cfg(feature = "postgres")]
pub(crate) struct Turn<'t>(&'t Turns);

#[cfg(feature = "postgres")]
impl Turns {
    pub(crate) fn new(cap: usize, wait: std::time::Duration) -> Self {
        Self {
            cap: cap.max(1),
            wait,
            state: std::sync::Mutex::default(),
            ended: std::sync::Condvar::new(),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Waiting> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// A turn: at once while fewer than `cap` reads run and none waits, else once every read
    /// that arrived before this one has had its turn and one ends.
    ///
    /// # Errors
    /// [`StoreError::Backend`] where the read waited `wait` for its turn.
    pub(crate) fn take(&self) -> Result<Turn<'_>, StoreError> {
        self.take_unless(None)?
            .ok_or_else(|| StoreError::Backend("a read with no stop flag was stopped".to_owned()))
    }

    /// [`Turns::take`], looking at `stop`, where given, at least every [`STOP_LOOK`] while it
    /// waits: `None` once it is set, the read leaving the line.
    ///
    /// # Errors
    /// [`StoreError::Backend`] where the read waited `wait` for its turn.
    pub(crate) fn take_unless(
        &self,
        stop: Option<&AtomicBool>,
    ) -> Result<Option<Turn<'_>>, StoreError> {
        let stopped = || stop.is_some_and(|stop| stop.load(Ordering::SeqCst));
        let mut state = self.lock();
        if stopped() {
            return Ok(None);
        }
        if state.line.is_empty() && state.running < self.cap {
            state.running += 1;
            return Ok(Some(Turn(self)));
        }
        let ticket = state.next;
        state.next += 1;
        state.line.push_back(ticket);
        let deadline = std::time::Instant::now() + self.wait;
        loop {
            if state.line.front() == Some(&ticket) && state.running < self.cap {
                state.line.pop_front();
                state.running += 1;
                drop(state);
                // The next in line may take a turn left free as well.
                self.ended.notify_all();
                return Ok(Some(Turn(self)));
            }
            let now = std::time::Instant::now();
            let given_up = stopped();
            if given_up || now >= deadline {
                state.line.retain(|waiting| *waiting != ticket);
                drop(state);
                self.ended.notify_all();
                if given_up {
                    return Ok(None);
                }
                return Err(StoreError::Backend(format!(
                    "a read waited {:?} for one of the {} reads that may run beside the writer \
                     to end, and was given up",
                    self.wait, self.cap
                )));
            }
            let look = if stop.is_some() {
                (deadline - now).min(STOP_LOOK)
            } else {
                deadline - now
            };
            state = self
                .ended
                .wait_timeout(state, look)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
    }
}

#[cfg(feature = "postgres")]
impl Drop for Turn<'_> {
    fn drop(&mut self) {
        self.0.lock().running -= 1;
        self.0.ended.notify_all();
    }
}

/// A position of a store whose transactions commit out of the order of their positions
/// (PostgreSQL), which cannot take its entities at once: each entity is read as it stands when
/// first read, its version noted, and the position holds while every entity it read still stands
/// at that version. Every write of an instance lands with a `spindle.written` event on its
/// entity's stream, so an entity at the version noted holds every row read of it.
#[cfg(feature = "postgres")]
pub(crate) struct Checked<'s, L> {
    store: &'s LogStore<L>,
    /// The turns of the handle it was taken through: each of its reads takes one.
    turns: &'s Turns,
    entities: BTreeSet<String>,
    /// The version each entity read stood at when first read.
    noted: std::sync::Mutex<BTreeMap<String, u64>>,
    /// Set where a later scan of an entity answered another version than the one noted.
    moved: AtomicBool,
}

#[cfg(feature = "postgres")]
impl<'s, L: Log> Checked<'s, L> {
    pub(crate) fn new(store: &'s LogStore<L>, turns: &'s Turns, entities: &[String]) -> Self {
        Self {
            store,
            turns,
            entities: entities.iter().cloned().collect(),
            noted: std::sync::Mutex::default(),
            moved: AtomicBool::new(false),
        }
    }

    fn noted(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, u64>> {
        self.noted
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn taken(&self, entity: &str) -> Result<(), StoreError> {
        if self.entities.contains(entity) {
            return Ok(());
        }
        Err(crate::store::not_taken(entity))
    }
}

#[cfg(feature = "postgres")]
impl<L: Log> Position for Checked<'_, L> {
    fn scan(&self, entity: &str) -> Result<Scan, StoreError> {
        self.taken(entity)?;
        let scan = {
            let _turn = self.turns.take()?;
            self.store.scanned(entity)?
        };
        let noted = *self
            .noted()
            .entry(entity.to_owned())
            .or_insert(scan.version);
        if noted != scan.version {
            self.moved.store(true, Ordering::SeqCst);
        }
        Ok(scan)
    }

    /// Notes the entity's version before its first load, so a write landed between that and
    /// [`Position::holds`] is seen.
    fn load(&self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        self.taken(entity)?;
        let _turn = self.turns.take()?;
        if !self.noted().contains_key(entity) {
            let version = self.store.entity_version(entity)?;
            self.noted().entry(entity.to_owned()).or_insert(version);
        }
        self.store.loaded(entity, identity)
    }

    fn holds(&self) -> Result<bool, StoreError> {
        if self.moved.load(Ordering::SeqCst) {
            return Ok(false);
        }
        let noted = self.noted().clone();
        let _turn = self.turns.take()?;
        for (entity, version) in noted {
            if self.store.entity_version(&entity)? != version {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// The events of one append group read from the feed: what [`LogStore::committed_after`] answers
/// for it.
struct Group {
    /// The request id every event of the group carries.
    request: String,
    /// The feed position of its last event.
    position: u64,
    origin: Option<Origin>,
    /// The outcome its `spindle.written` events, or its `spindle.published` end, name.
    outcome: Option<String>,
    /// The instances its `spindle.written` events name, by entity and identity text.
    instances: BTreeMap<(String, String), Value>,
    /// Each event of a command, with its own feed position.
    events: Vec<(u64, crate::Event)>,
}

impl<L: Log> LogStore<L> {
    /// The events committed by the append groups after the one ending at feed position
    /// `position`, in commit order, group by group up to the one that brings them to `limit`
    /// ([`crate::whole_writes`]), each at its group's last feed position, attributed to the
    /// command its group's `spindle.written` events name. The rows, removals, entity versions and
    /// dead letters ([`DEAD_LETTERS`]) the groups appended are left out, so a group that committed
    /// no event of a command answers nothing. Read once the feed answers every write this store
    /// committed ([`LogStore::published`]).
    ///
    /// Where the read stops before the feed's end, a reset mark ([`RESET`]) past where it stopped
    /// forgets every event it read: the feed is then read again from the last mark.
    fn committed_after(&self, position: u64, limit: usize) -> Result<Vec<Committed>, StoreError> {
        Ok(self
            .placed_after(position, limit, None)?
            .into_iter()
            .map(|(_, committed)| committed)
            .collect())
    }

    /// [`LogStore::committed_after`], each event with its own feed position
    /// ([`Reading::events_placed_after`]); no event where `stop` is given and is set while the
    /// read waits for the feed to answer the store's own writes ([`LogStore::published`]).
    fn placed_after(
        &self,
        position: u64,
        limit: usize,
        stop: Option<&AtomicBool>,
    ) -> Result<Vec<(u64, Committed)>, StoreError> {
        if limit == 0 || !self.published(stop)? {
            return Ok(Vec::new());
        }
        let reset = StreamId::new(self.tenant.clone(), RESET.0, RESET.1).map_err(backend)?;
        let mut from = position;
        let groups = loop {
            let (groups, stopped) = self.groups_after(from, limit)?;
            let Some(read_to) = stopped else {
                break groups;
            };
            match self.last(&reset)? {
                Some((_, mark)) if mark.global_seq > read_to => from = mark.global_seq,
                _ => break groups,
            }
        };
        Ok(groups
            .into_iter()
            .flat_map(|group| {
                let (position, origin, outcome) = (group.position, group.origin, group.outcome);
                let instances: Vec<(String, Value)> = group
                    .instances
                    .into_iter()
                    .map(|((entity, _), identity)| (entity, identity))
                    .collect();
                group.events.into_iter().map(move |(placed, event)| {
                    (
                        placed,
                        Committed {
                            position,
                            origin: origin.clone(),
                            outcome: outcome.clone(),
                            instances: instances.clone(),
                            event,
                        },
                    )
                })
            })
            .collect())
    }

    /// The id of the log ([`Store::events_id`]), read from the first event of the tenant's feed:
    /// its event id, which the event log draws at random when it appends the event, folded into
    /// 64 bits. The first event stays the first while the log is kept, a reset that empties the
    /// log in place included; a file created again, or a log a state move copied, starts with
    /// another. `0` for an empty feed. Nothing is written for it, so a file of any release has one.
    fn log_id(&self) -> Result<u64, StoreError> {
        let page = self
            .call(self.log.read_feed(&self.tenant, 0, 1))
            .map_err(backend)?;
        Ok(page.events.first().map_or(0, |event| {
            // FNV-1a: the same id in every process and release.
            let folded = event
                .event_id
                .bytes()
                .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
                    (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
                });
            folded.max(1)
        }))
    }

    /// The feed position of the last event of the tenant's feed, `0` for an empty one
    /// ([`Store::events_head`]): found by reading one feed event at a time past positions that
    /// double, then halve, so about twice the binary logarithm of the feed's length of them,
    /// none of them answered; once the feed answers every write this store committed
    /// ([`LogStore::published`]).
    fn feed_end(&self) -> Result<u64, StoreError> {
        self.published(None)?;
        let next = |after: u64| -> Result<Option<u64>, StoreError> {
            let page = self
                .call(self.log.read_feed(&self.tenant, after, 1))
                .map_err(backend)?;
            Ok(page.events.first().map(|event| event.global_seq))
        };
        // The feed holds an event at `last` and none past `none`.
        let Some(mut last) = next(0)? else {
            return Ok(0);
        };
        let mut step: u64 = 1;
        let mut none = loop {
            let probe = last.saturating_add(step);
            match next(probe)? {
                Some(at) => {
                    last = at;
                    step = step.saturating_mul(2);
                }
                None => break probe,
            }
        };
        while last < none {
            let middle = last + (none - last) / 2;
            match next(middle)? {
                Some(at) => last = at,
                None => none = middle,
            }
        }
        Ok(last)
    }

    /// The append groups after feed position `position`, read a page of [`FEED_PAGE`] at a time,
    /// up to the group that brings their events to `limit`; an append group commits whole, so
    /// every group read is read whole. With them, where the read stopped before the feed's end,
    /// the feed position of the last event it read.
    fn groups_after(
        &self,
        position: u64,
        limit: usize,
    ) -> Result<(Vec<Group>, Option<u64>), StoreError> {
        let mut groups: Vec<Group> = Vec::new();
        let mut count = 0;
        let mut read_to = position;
        let mut at = position;
        loop {
            let page = self
                .call(self.log.read_feed(&self.tenant, at, FEED_PAGE))
                .map_err(backend)?;
            for event in page.events {
                if groups
                    .last()
                    .is_none_or(|group| group.request != event.request_id)
                {
                    if count >= limit {
                        return Ok((groups, Some(read_to)));
                    }
                    groups.push(Group {
                        request: event.request_id.clone(),
                        position: event.global_seq,
                        origin: None,
                        outcome: None,
                        instances: BTreeMap::new(),
                        events: Vec::new(),
                    });
                }
                read_to = event.global_seq;
                let Some(group) = groups.last_mut() else {
                    continue;
                };
                group.position = group.position.max(event.global_seq);
                if event.stream_type == ENTITY {
                    if event.name == WRITTEN {
                        if group.origin.is_none() {
                            group.origin = origin_of(&event.data);
                        }
                        if group.outcome.is_none() {
                            group.outcome = outcome_of(&event.data);
                        }
                        for text in event
                            .data
                            .get(INSTANCES)
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                        {
                            let Some(text) = text.as_str() else {
                                return Err(StoreError::Backend(format!(
                                    "the event {} at version {} of the stream {} {} names an \
                                     instance by no key text",
                                    event.name, event.version, event.stream_type, event.stream_id
                                )));
                            };
                            if let btree_map::Entry::Vacant(vacant) = group
                                .instances
                                .entry((event.stream_id.clone(), text.to_owned()))
                            {
                                vacant.insert(identity_from_key(text)?);
                            }
                        }
                    }
                    continue;
                }
                let stream = (event.stream_type.as_str(), event.stream_id.as_str());
                if stream == RESET {
                    groups.clear();
                    count = 0;
                    continue;
                }
                if stream == PUBLISHED && event.name == PUBLISHED_END {
                    if group.origin.is_none() {
                        group.origin = origin_of(&event.data);
                    }
                    if group.outcome.is_none() {
                        group.outcome = outcome_of(&event.data);
                    }
                    continue;
                }
                if stream == MARKS
                    || stream == SPECIFICATION
                    || stream == DEAD_LETTERS
                    || event.name == ROW
                    || event.name == REMOVED
                    || (event.stream_type == MARKS.0 && event.stream_id.starts_with(EGRESS))
                {
                    continue;
                }
                let Value::Object(payload) = event.data else {
                    return Err(StoreError::Backend(format!(
                        "the event {} at version {} of the stream {} {} carries no payload \
                         object",
                        event.name, event.version, event.stream_type, event.stream_id
                    )));
                };
                group.events.push((
                    event.global_seq,
                    crate::Event {
                        name: event.name,
                        payload,
                    },
                ));
                count += 1;
            }
            if !page.has_more {
                return Ok((groups, None));
            }
            if page.next_position <= at {
                return Err(StoreError::Backend(format!(
                    "the feed did not move past position {at}"
                )));
            }
            at = page.next_position;
        }
    }
}

/// What a body names `origin` as, under [`ORIGIN`]: `{"command", "sequence"}`, and `"cause"`
/// ([`Provenance`]) where the origin names one.
fn origin_body(origin: &Origin) -> Value {
    let mut body = serde_json::json!({"command": origin.command, "sequence": origin.sequence});
    if let (Some(cause), Value::Object(fields)) = (&origin.cause, &mut body)
        && let Ok(cause) = serde_json::to_value(cause)
    {
        fields.insert(CAUSE.to_owned(), cause);
    }
    body
}

/// The outcome a `spindle.written` event's body, or a `spindle.published` end's, names under
/// [`OUTCOME`], if it names one.
fn outcome_of(body: &Value) -> Option<String> {
    body.get(OUTCOME)?.as_str().map(str::to_owned)
}

/// The command a `spindle.written` event's body names under [`ORIGIN`], if it names one, with
/// its cause; an origin a release before 0.23.0 wrote, `{"command", "sequence"}` only, names no
/// cause.
fn origin_of(body: &Value) -> Option<Origin> {
    let origin = body.get(ORIGIN)?;
    Some(Origin {
        command: origin.get("command")?.as_str()?.to_owned(),
        sequence: origin.get("sequence")?.as_u64()?,
        cause: origin
            .get(CAUSE)
            .and_then(|cause| serde_json::from_value(cause.clone()).ok()),
    })
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
    /// The specification the store was last written under ([`SPECIFICATION`]), or `None` for a
    /// store that records none: an empty one, or one a release before this one wrote.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the log cannot be read or its record does not read.
    pub fn recorded(&self) -> Result<Option<crate::Shapes>, StoreError> {
        let (kind, id) = SPECIFICATION;
        let stream = StreamId::new(self.tenant.clone(), kind, id).map_err(backend)?;
        self.last(&stream)?
            .map(|(_, event)| {
                serde_json::from_value(event.data).map_err(|error| {
                    StoreError::Backend(format!("the recorded specification: {error}"))
                })
            })
            .transpose()
    }

    /// Checks the rows the store holds against `current`, the specification a run is about to
    /// serve them under (`story:spec-change-over-stored-rows`), writing nothing:
    /// [`crate::Fit::Same`] where the store was last written under `current` (the same digest),
    /// [`crate::Fit::Misfits`] naming every row the specification cannot read
    /// ([`crate::Shapes::misfits`]), and otherwise [`crate::Fit::Fits`]. A store that records no
    /// specification fits unchecked, saying whether it held rows.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the store cannot be read.
    pub fn check(&mut self, current: &crate::Shapes) -> Result<crate::Fit, StoreError> {
        Ok(match self.recorded()? {
            Some(recorded) if recorded.digest == current.digest => crate::Fit::Same,
            Some(recorded) => {
                let misfits = current.misfits(&recorded, self)?;
                if misfits.is_empty() {
                    crate::Fit::Fits { unrecorded: false }
                } else {
                    crate::Fit::Misfits(misfits)
                }
            }
            None => crate::Fit::Fits {
                unrecorded: current.held(self)?,
            },
        })
    }

    /// Appends one event recording `current` as the specification the store is written under
    /// from now on ([`SPECIFICATION`]).
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the store cannot be written.
    pub fn record(&mut self, current: &crate::Shapes) -> Result<(), StoreError> {
        let body = serde_json::to_value(current).map_err(|error| {
            StoreError::Backend(format!("recording the specification: {error}"))
        })?;
        let event = NewEvent::new(SPECIFIED, SCHEMA, body).map_err(backend)?;
        let (kind, id) = SPECIFICATION;
        self.append(vec![StreamAppend {
            stream: StreamId::new(self.tenant.clone(), kind, id).map_err(backend)?,
            expected: Expected::Any,
            events: vec![event],
        }])
        .map_err(backend)
    }

    /// [`LogStore::check`], then, where the rows fit a specification the store does not record
    /// yet ([`crate::Fit::Fits`]), [`LogStore::record`]: what a run that opens one store does
    /// before it serves (D3: the record is written at open, after the check passes, and only
    /// where the recorded digest differs).
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the store cannot be read or written.
    pub fn fit(&mut self, current: &crate::Shapes) -> Result<crate::Fit, StoreError> {
        let fit = self.check(current)?;
        if let crate::Fit::Fits { .. } = fit {
            self.record(current)?;
        }
        Ok(fit)
    }
}

impl<L: Log> LogStore<L> {
    /// Whether the log is marked as holding writes that name no instances ([`MARKS`]).
    fn marked(&self) -> Result<bool, StoreError> {
        let (kind, id) = MARKS;
        let mark = StreamId::new(self.tenant.clone(), kind, id).map_err(backend)?;
        Ok(self
            .call(self.log.stream_version(&mark))
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
            let answered = match self.call(self.log.read_many(&reads)) {
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
                .call(self.log.read_stream(&stream, version, MAX_READ_LIMIT))
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

    /// SQLite has one writer: an append is in the feed once it committed.
    const WITHHOLDS: bool = false;

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
                .call(self.log.read_feed(&self.tenant, position, FEED_PAGE))
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

    /// Each of `entities` as the resting rows hold it once caught up ([`LogStore::caught_up`]),
    /// all taken under one hold of their lock: one position of the feed, each write group whole
    /// or not at all, `None` for an entity holding nothing yet.
    fn taken(&self, entities: &[String]) -> Result<Taken, StoreError> {
        let rows = self.rows.as_ref().ok_or_else(|| {
            StoreError::Backend("this store holds no resting rows to take a position of".to_owned())
        })?;
        let folded = self.caught_up(rows)?;
        Ok(Taken(
            entities
                .iter()
                .map(|entity| (entity.clone(), folded.entities.get(entity).cloned()))
                .collect(),
        ))
    }
}

/// A position of a SQLite store ([`Reading::at`]): the entities it was taken for, each as the
/// resting rows held it at one position of the feed ([`LogStore::taken`]). A fold after it lands
/// on a copy of an entity it holds, never on what it reads.
#[cfg(feature = "sqlite")]
struct Taken(BTreeMap<String, Option<Arc<Entity>>>);

#[cfg(feature = "sqlite")]
impl Position for Taken {
    fn scan(&self, entity: &str) -> Result<Scan, StoreError> {
        match self.0.get(entity) {
            Some(Some(rows)) => rows.scan(),
            Some(None) => Ok(Scan::default()),
            None => Err(crate::store::not_taken(entity)),
        }
    }

    fn load(&self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        match self.0.get(entity) {
            Some(Some(rows)) => rows.load(identity),
            Some(None) => Ok(None),
            None => Err(crate::store::not_taken(entity)),
        }
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

    /// The instance whose identity is `identity`, as its last `spindle.row` event holds it, if
    /// one rests; an instance whose row did not read fails, as its scan does.
    fn load(&self, identity: &Value) -> Result<Option<Stored>, StoreError> {
        let key = identity.to_string();
        if self.unreadable.contains(&key) {
            return Err(StoreError::Backend(
                self.malformed
                    .clone()
                    .unwrap_or_else(|| format!("the row of {key} does not read")),
            ));
        }
        Ok(self.rows.get(&key).map(|stored| Stored::clone(stored)))
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

    fn events_after(&self, position: u64, limit: usize) -> Result<Vec<Committed>, StoreError> {
        self.with(|connection| connection.committed_after(position, limit))
    }

    fn events_placed_after(
        &self,
        position: u64,
        limit: usize,
    ) -> Result<Vec<(u64, Committed)>, StoreError> {
        self.with(|connection| connection.placed_after(position, limit, None))
    }

    fn events_head(&self) -> Result<u64, StoreError> {
        self.with(|connection| connection.feed_end())
    }

    fn events_id(&self) -> Result<u64, StoreError> {
        self.with(|connection| connection.log_id())
    }

    /// The entities taken from the resting rows a connection caught up ([`LogStore::taken`]);
    /// the connection is handed back before the position is read.
    fn at(&self, entities: &[String]) -> Result<Box<dyn Position + '_>, StoreError> {
        Ok(Box::new(
            self.with(|connection| connection.taken(entities))?,
        ))
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
/// Refuses `write` where its instance stands at `actual`, a version other than the one it names
/// (`None`: no row), as [`StoreError::Conflict`]; otherwise answers `removed`, the version its
/// stream ends at with a `spindle.removed` event, if it does.
fn stands(
    write: &Write,
    actual: Option<u64>,
    removed: Option<u64>,
) -> Result<Option<u64>, StoreError> {
    if actual == write.expected() {
        return Ok(removed);
    }
    let (entity, identity) = write.instance();
    Err(StoreError::Conflict {
        entity: entity.to_owned(),
        identity: identity.to_string(),
        expected: write.expected(),
        actual,
    })
}

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

/// `error` as the store's: a fence's refusal by its code alone, which names what refused the
/// write ([`LogStore::fence`]).
pub(crate) fn backend(error: EventLogError) -> StoreError {
    match error {
        EventLogError::GuardRefused { code } => StoreError::Backend(code),
        other => StoreError::Backend(other.to_string()),
    }
}

#[cfg(all(test, feature = "sqlite"))]
mod tests {
    use eventlog_core::AtomicEventStore;
    use serde_json::json;

    use super::*;
    use crate::store::Sender;

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
                json!({"rows": {"\"a\"": 1, "\"b\"": 2}, "removed": {}, "instances": ["\"a\"", "\"b\""]})
            )
        );
        store
            .remove(&Removal {
                entity: TASK.to_owned(),
                identity: json!("a"),
                expected: 1,
                events: Vec::new(),
                scanned: Vec::new(),
            })
            .expect("a is removed");
        assert_eq!(
            last_written(&store, TASK),
            (
                WRITTEN_SCHEMA,
                json!({"rows": {}, "removed": {"\"a\"": 2}, "instances": ["\"a\""]})
            )
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

    fn log_calls() -> u64 {
        LOG_CALLS.with(std::cell::Cell::get)
    }

    /// Writes 24 tasks, `t-0` to `t-23`, into `store` in one group: each at version 1.
    fn seed(store: &mut SqliteStore) {
        let created: Vec<Write> = (0..24)
            .map(|index| Write::Commit(change(&format!("t-{index}"), None, "Open")))
            .collect();
        store.commit_all(&created).expect("24 rows");
    }

    /// How many calls of the event store renewing the first `rows` of `store`'s tasks in one
    /// group made, each named at version `at`.
    fn renewing(store: &mut SqliteStore, rows: usize, at: u64) -> u64 {
        let writes: Vec<Write> = (0..rows)
            .map(|index| Write::Commit(change(&format!("t-{index}"), Some(at), "Renewed")))
            .collect();
        let before = log_calls();
        store.commit_all(&writes).expect("the renewal lands");
        log_calls() - before
    }

    /// 0.17.0: renewing 24 loans made 48 blocking log reads, two per row, before one append. A
    /// bulk write reads the version of every row it names together: renewing 24 rows makes as
    /// many calls of the log as renewing one, and a row another writer moved in between is still
    /// a conflict naming it, with nothing appended.
    #[test]
    fn a_bulk_write_reads_versions_in_one_batch() {
        // A unit test has no `CARGO_TARGET_TMPDIR`: the workspace's own `target/tmp`, which
        // cargo makes for the integration tests.
        let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp");
        std::fs::create_dir_all(&scratch).expect("the scratch directory");
        let directory = tempfile::tempdir_in(scratch).expect("a directory");
        let path = directory.path().join("state.sqlite");

        let mut single = store();
        seed(&mut single);
        let one = renewing(&mut single, 1, 1);
        let mut bulk = store();
        seed(&mut bulk);
        let all = renewing(&mut bulk, 24, 1);
        assert_eq!(
            all, one,
            "renewing 24 rows made {all} calls of the log, renewing one made {one}"
        );
        assert!(all <= 6, "renewing 24 rows made {all} calls of the log");

        let mut store = SqliteStore::open(&path).expect("the store opens");
        seed(&mut store);
        renewing(&mut store, 24, 1);

        let mut other = SqliteStore::open(&path).expect("a second writer opens");
        other
            .commit(&change("t-5", Some(2), "Moved"))
            .expect("the second writer moves one row");
        let writes: Vec<Write> = (0..24)
            .map(|index| Write::Commit(change(&format!("t-{index}"), Some(2), "Again")))
            .collect();
        match store.commit_all(&writes) {
            Err(StoreError::Conflict {
                identity,
                expected,
                actual,
                ..
            }) => {
                assert_eq!(identity, json!("t-5").to_string());
                assert_eq!((expected, actual), (Some(2), Some(3)));
            }
            other => panic!("a conflict naming t-5, not {other:?}"),
        }
        let scan = store.scan(TASK).expect("scans");
        assert!(
            scan.rows.iter().all(|(identity, stored)| {
                let moved = *identity == json!("t-5");
                stored.row.state == if moved { "Moved" } else { "Renewed" }
            }),
            "the refused group appended nothing: {scan:?}"
        );
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

    /// A write as the store of 0.22.0 appended it, attributed to `command`: its
    /// `spindle.written` event's origin names the command and its sequence only, and its
    /// instance's stream carries one event of the command before the row.
    fn write_as_released(store: &SqliteStore, identity: &str, command: &str, sequence: u64) {
        let appends = vec![
            StreamAppend {
                stream: store.entity_stream(TASK).expect("a stream id"),
                expected: Expected::Any,
                events: vec![
                    NewEvent::new(
                        WRITTEN,
                        WRITTEN_SCHEMA,
                        json!({
                            "rows": {json!(identity).to_string(): 2},
                            "removed": {},
                            "origin": {"command": command, "sequence": sequence},
                        }),
                    )
                    .expect("an event"),
                ],
            },
            StreamAppend {
                stream: store.stream(TASK, &json!(identity)).expect("a stream id"),
                expected: Expected::NoStream,
                events: vec![
                    NewEvent::new("a.Opened", SCHEMA, json!({"id": identity})).expect("an event"),
                    row_event(&change(identity, None, "Old").row).expect("a row"),
                ],
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

    /// `story:events-record-their-caller` (D3 A): an origin 0.22.0 wrote, `{"command",
    /// "sequence"}` only, reads back with its command and sequence and no cause, beside a write
    /// of this release that names its caller; nothing is migrated.
    #[test]
    fn a_store_written_before_reads_back_no_caller() {
        let mut store = store();
        write_as_released(&store, "old", "a.Open", 7);
        let cause = Provenance::Request(Sender {
            actor: Some("a.Owner".to_owned()),
            attributes: serde_json::Map::new(),
            request: Some("r-1".to_owned()),
        });
        store.attribute(Some(Origin {
            command: "a.Open".to_owned(),
            sequence: 8,
            cause: Some(cause.clone()),
        }));
        let mut opened = change("new", None, "Open");
        opened.events = vec![crate::Event {
            name: "a.Opened".to_owned(),
            payload: serde_json::Map::from_iter([("id".to_owned(), json!("new"))]),
        }];
        store.commit(&opened).expect("a write after the upgrade");
        let origins: Vec<Option<Origin>> = store
            .events_after(0, usize::MAX)
            .expect("the events read")
            .into_iter()
            .map(|committed| committed.origin)
            .collect();
        assert_eq!(
            origins,
            [
                Some(Origin {
                    command: "a.Open".to_owned(),
                    sequence: 7,
                    cause: None,
                }),
                Some(Origin {
                    command: "a.Open".to_owned(),
                    sequence: 8,
                    cause: Some(cause),
                }),
            ]
        );
    }

    /// `story:entity-event-history`: a write 0.22.0 stored names no instances, so the history
    /// of an instance, which a host reads as the events of the writes naming it
    /// ([`Committed::instances`]), holds only the writes after the upgrade: here the second
    /// write of `old`, not the one that created it.
    #[test]
    fn a_store_written_before_reads_history_only_from_new_writes() {
        let mut store = store();
        write_as_released(&store, "old", "a.Open", 7);
        store.attribute(Some(Origin {
            command: "a.Revise".to_owned(),
            sequence: 8,
            cause: None,
        }));
        store.outcome("revised");
        let mut revised = change("old", Some(2), "Revised");
        revised.events = vec![crate::Event {
            name: "a.Revised".to_owned(),
            payload: serde_json::Map::from_iter([("id".to_owned(), json!("old"))]),
        }];
        store.commit(&revised).expect("a write after the upgrade");
        let committed = store.events_after(0, usize::MAX).expect("the events read");
        type Read<'a> = (&'a str, Vec<(String, Value)>, Option<&'a str>);
        let read: Vec<Read<'_>> = committed
            .iter()
            .map(|committed| {
                (
                    committed.event.name.as_str(),
                    committed.instances.clone(),
                    committed.outcome.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            read,
            [
                ("a.Opened", Vec::new(), None),
                (
                    "a.Revised",
                    vec![(TASK.to_owned(), json!("old"))],
                    Some("revised")
                ),
            ]
        );
        let history: Vec<&str> = committed
            .iter()
            .filter(|committed| {
                committed
                    .instances
                    .contains(&(TASK.to_owned(), json!("old")))
            })
            .map(|committed| committed.event.name.as_str())
            .collect();
        assert_eq!(history, ["a.Revised"], "only the write after the upgrade");
    }

    /// `story:entity-event-history`: a write of an instance whose identity holds U+0000 names it
    /// in its `spindle.written` event as the identity's key text, in which U+0000 is escaped, so
    /// no event of the log holds U+0000 in a JSON string, which PostgreSQL refuses and a state
    /// move checks for ([`crate::state_move::nul`]); the write still reads back naming the
    /// identity itself.
    #[cfg(feature = "postgres")]
    #[test]
    fn an_identity_holding_u0000_reaches_no_event_body_raw() {
        let mut store = store();
        let identity = json!("a\u{0}b");
        let mut opened = change("a\u{0}b", None, "Open");
        opened.events = vec![crate::Event {
            name: "a.Opened".to_owned(),
            payload: serde_json::Map::new(),
        }];
        store.attribute(Some(Origin {
            command: "a.Open".to_owned(),
            sequence: 1,
            cause: None,
        }));
        store.outcome("opened");
        store.commit(&opened).expect("a write");
        let page = store
            .call(store.log.read_feed(&store.tenant, 0, 1_000))
            .expect("the feed reads");
        assert!(!page.events.is_empty());
        let raw: Vec<String> = page
            .events
            .iter()
            .filter_map(|event| {
                crate::state_move::nul(&event.data, "body").map(|at| {
                    format!(
                        "{} {} {}: {at}",
                        event.stream_type, event.stream_id, event.name
                    )
                })
            })
            .collect();
        assert_eq!(raw, Vec::<String>::new(), "no body holds U+0000");
        let committed = store.events_after(0, usize::MAX).expect("the events read");
        assert_eq!(committed.len(), 1, "{committed:?}");
        assert_eq!(committed[0].instances, [(TASK.to_owned(), identity)]);
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

    /// `story:metrics-read-without-scan`: the host counts every store's rows when it opens one,
    /// so a restart pays for `counts` once per component. The SQLite store answers it in one read
    /// of the feed; the trait's default scans each entity, one read of the feed per entity (42 s
    /// on a model of 125 entities). Held in calls of the event store, not in time
    /// (`story:ci-tests-deterministic`): the case timed `counts` against eight scans and failed
    /// on a loaded runner with nothing wrong.
    #[test]
    fn adversary_counts_sqlite_counts_every_entity_in_about_one_read_of_the_feed() {
        const ENTITIES: usize = 60;
        const ROWS: usize = 25;
        let mut store = store();
        let entities = (0..ENTITIES)
            .map(|index| format!("adversary.counts.E{index}"))
            .collect::<Vec<_>>();
        for entity in &entities {
            let writes = (0..ROWS)
                .map(|index| {
                    Write::Commit(Change {
                        entity: entity.clone(),
                        ..change(&format!("r{index}"), None, "Open")
                    })
                })
                .collect::<Vec<_>>();
            store.commit_all(&writes).expect("the rows land");
        }
        let counts = store.counts(&entities).expect("the store counts");
        assert_eq!(
            counts,
            entities
                .iter()
                .map(|entity| (entity.clone(), ROWS))
                .collect::<BTreeMap<_, _>>()
        );

        let before = log_calls();
        store.scan(&entities[0]).expect("a scan");
        let scan = log_calls() - before;
        let before = log_calls();
        store.counts(&entities).expect("the store counts");
        let counted = log_calls() - before;
        let before = log_calls();
        for entity in &entities {
            store.scan(entity).expect("a scan");
        }
        let by_default = log_calls() - before;
        assert_eq!(scan, 1, "one scan read the feed {scan} times");
        assert_eq!(
            by_default,
            scan * ENTITIES as u64,
            "a scan of each entity, the trait's default, reads the feed once per entity"
        );
        assert_eq!(
            counted, scan,
            "counting {ENTITIES} entities made {counted} calls of the event store, one scan \
             made {scan}: the feed is read once per entity"
        );
    }
}

#[cfg(all(test, feature = "postgres"))]
mod turns {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::Turns;

    /// A read beyond the cap is not refused: it waits until a read in flight ends.
    #[test]
    fn a_read_beyond_the_cap_waits_for_one_to_end() {
        let turns = Turns::new(2, Duration::from_secs(10));
        let first = turns.take().expect("a first turn");
        let _second = turns.take().expect("a second turn");
        let (sent, taken) = mpsc::channel();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let turn = turns.take();
                sent.send(turn.is_ok()).expect("the test listens");
            });
            assert!(
                taken.recv_timeout(Duration::from_millis(200)).is_err(),
                "a third read ran beside two with a cap of 2"
            );
            drop(first);
            assert_eq!(
                taken.recv_timeout(Duration::from_secs(5)),
                Ok(true),
                "the third read took its turn once one ended"
            );
        });
    }

    /// Reads waiting for a turn take it in the order they arrived.
    #[test]
    fn waiting_reads_take_their_turns_in_arrival_order() {
        let turns = Turns::new(1, Duration::from_secs(10));
        let held = turns.take().expect("the one turn");
        let (sent, order) = mpsc::channel();
        std::thread::scope(|scope| {
            for index in 0..4 {
                let (turns, sent) = (&turns, sent.clone());
                scope.spawn(move || {
                    let _turn = turns.take().expect("a turn");
                    sent.send(index).expect("the test listens");
                });
                // Each read is in line before the next arrives.
                while turns.lock().line.len() <= index {
                    std::thread::yield_now();
                }
            }
            drop(held);
        });
        drop(sent);
        assert_eq!(order.iter().collect::<Vec<_>>(), [0, 1, 2, 3]);
    }

    /// A read that waits past the deadline is refused with a store error naming the wait, and
    /// leaves the line, so the next read is not held behind it.
    #[test]
    fn a_read_waiting_past_the_deadline_is_refused() {
        let turns = Turns::new(1, Duration::from_millis(100));
        let held = turns.take().expect("the one turn");
        let Err(error) = turns.take() else {
            panic!("a second read took a turn beside the first with a cap of 1");
        };
        assert!(
            error
                .to_string()
                .contains("a read waited 100ms for one of the 1 reads"),
            "{error}"
        );
        assert!(
            turns.lock().line.is_empty(),
            "the refused read left the line"
        );
        drop(held);
        assert!(turns.take().is_ok(), "the next read takes the freed turn");
    }

    /// A cap of 0 (a pool of 1) still lets one read run.
    #[test]
    fn a_cap_of_zero_still_runs_one_read() {
        let turns = Turns::new(0, Duration::from_millis(100));
        let _held = turns.take().expect("one read runs");
        assert!(turns.take().is_err(), "a second waits and is refused");
    }
}
