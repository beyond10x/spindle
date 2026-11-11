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
//! The encoding is the store's, not SQLite's: [`LogStore`] keeps it on any eventlog event store
//! that appends groups, and the PostgreSQL store (`PostgresStore`, the `postgres` feature) is the
//! same store on PostgreSQL.

use std::collections::BTreeMap;
#[cfg(feature = "sqlite")]
use std::path::Path;
use std::time::{Duration, Instant};

use eventlog_core::{
    AppendGroup, AtomicEventStore, CommandMeta, EventLogError, Expected, NewEvent, RecordedEvent,
    StreamAppend, StreamId, TenantId, new_event_id, request_hash,
};
#[cfg(feature = "sqlite")]
use eventlog_sqlite::SqliteEventStore;
use serde_json::Value;
use time::OffsetDateTime;

use crate::store::{
    Change, Removal, Row, Scan, Scanned, Store, StoreError, Stored, Write, first_writes,
    identity_from_key, moved,
};

/// The table prefix Spindle's events are kept under.
pub(crate) const PREFIX: &str = "spindle";
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
/// How long a scan waits for the feed to show every write the entity's version counts.
const SETTLE: Duration = Duration::from_secs(10);
/// How long a scan waits before it reads a lagging feed again.
const SETTLE_POLL: Duration = Duration::from_millis(20);

/// Instances in an eventlog event store: one SQLite database file ([`SqliteStore`]) or one
/// component's tables of a PostgreSQL database (`PostgresStore`).
pub struct LogStore<L> {
    log: L,
    tenant: TenantId,
    runtime: tokio::runtime::Runtime,
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
    /// Opens the database at `path`, creating it and its tables when absent.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the file cannot be opened or provisioned.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .map_err(|error| StoreError::Backend(error.to_string()))?;
        let location = path.to_str().ok_or_else(|| {
            StoreError::Backend(format!("{} is not a UTF-8 path", path.display()))
        })?;
        let log = runtime
            .block_on(SqliteEventStore::open(location, PREFIX))
            .map_err(backend)?;
        Self::on(log, runtime)
    }
}

impl<L: AtomicEventStore> LogStore<L> {
    /// The store on `log`, every call blocking on `runtime`.
    pub(crate) fn on(log: L, runtime: tokio::runtime::Runtime) -> Result<Self, StoreError> {
        let tenant = TenantId::new(TENANT).map_err(backend)?;
        Ok(Self {
            log,
            tenant,
            runtime,
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
}

impl<L: AtomicEventStore> Store for LogStore<L> {
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
            Ok(_) => Ok(()),
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

    /// Reads the tenant's feed in commit order and keeps, for each stream of `entity`, its last
    /// `spindle.row` event: the row as it rests and the stream version a later commit names. A
    /// `spindle.removed` event after it leaves the stream out, until a later row.
    /// Every page moves the position forward, or the read stops as a backend error. The entity's
    /// version is read first, so a write landing during the read leaves it behind the rows.
    ///
    /// A feed may lag a commit: PostgreSQL's shows an event only once every transaction older
    /// than it has ended. So the feed is read again, for up to [`SETTLE`], until it holds as many
    /// of the entity's `spindle.written` events as the version read first: each write appends one
    /// in the transaction of its rows, so then every row written up to that version is in it.
    fn scan(&mut self, entity: &str) -> Result<Scan, StoreError> {
        let version = self.entity_version(entity)?;
        let deadline = Instant::now() + SETTLE;
        loop {
            let (latest, written) = self.feed(entity)?;
            if written >= version {
                let rows = latest
                    .into_iter()
                    .map(|(identity, stored)| Ok((identity_from_key(&identity)?, stored)))
                    .collect::<Result<_, StoreError>>()?;
                return Ok(Scan { rows, version });
            }
            if Instant::now() >= deadline {
                return Err(StoreError::Backend(format!(
                    "the feed shows {written} of the {version} writes of {entity} after \
                     {SETTLE:?}: a transaction older than the last of them is still open"
                )));
            }
            std::thread::sleep(SETTLE_POLL);
        }
    }
}

impl<L: AtomicEventStore> LogStore<L> {
    /// The last row of every stream of `entity` the feed holds, by identity key, and how many of
    /// the entity's `spindle.written` events it holds.
    fn feed(&self, entity: &str) -> Result<(BTreeMap<String, Stored>, u64), StoreError> {
        let mut latest = BTreeMap::<String, Stored>::new();
        let mut written = 0;
        let mut position = 0;
        loop {
            let page = self
                .runtime
                .block_on(self.log.read_feed(&self.tenant, position, FEED_PAGE))
                .map_err(backend)?;
            for event in page.events {
                if event.stream_type == ENTITY && event.stream_id == entity {
                    written += 1;
                    continue;
                }
                if event.stream_type != entity {
                    continue;
                }
                if event.name == REMOVED {
                    latest.remove(&event.stream_id);
                    continue;
                }
                if event.name != ROW {
                    continue;
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
            }
            if !page.has_more {
                break;
            }
            if page.next_position <= position {
                return Err(StoreError::Backend(format!(
                    "the feed did not move past position {position}"
                )));
            }
            position = page.next_position;
        }
        Ok((latest, written))
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

pub(crate) fn backend(error: EventLogError) -> StoreError {
    StoreError::Backend(error.to_string())
}
