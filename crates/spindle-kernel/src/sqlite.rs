//! Instances on SQLite, through `eventlog`: `store.durable` bound to `sqlite`.
//!
//! Each instance is one event stream, keyed by its entity and its identity. A commit appends the
//! events the command emitted followed by one `spindle.row` event carrying the row as it now
//! rests, in one append, so the row and the events that changed it are recorded together or not
//! at all. Loading reads the stream's last event. The stream's version is the version a later
//! commit names, so two writers that read the same row cannot both commit. A removal appends the
//! events and one `spindle.removed` event, which loading and scanning read as no row. Several
//! writes together ([`Store::commit_all`]) are one append group: every stream's append in one
//! transaction.

use std::collections::BTreeMap;
use std::path::Path;

use eventlog_core::{
    AppendGroup, AtomicEventStore, CommandMeta, EventLogError, EventStore, Expected, NewEvent,
    RecordedEvent, StreamAppend, StreamId, TenantId, new_event_id, request_hash,
};
use eventlog_sqlite::SqliteEventStore;
use serde_json::Value;
use time::OffsetDateTime;

use crate::store::{
    Change, Removal, Row, Store, StoreError, Stored, Write, first_writes, identity_from_key,
};

/// The table prefix Spindle's events are kept under.
const PREFIX: &str = "spindle";
/// The one tenant a single Spindle process holds.
const TENANT: &str = "local";
/// The event that carries an instance's row after a change.
const ROW: &str = "spindle.row";
/// The event that ends the stream of a removed instance (`deletes:`).
const REMOVED: &str = "spindle.removed";
/// Who records the change: the runtime itself.
const ACTOR: &str = "spindle-kernel";
/// The schema version of every event Spindle appends.
const SCHEMA: u32 = 1;
/// How many feed events one read of a scan asks for.
const FEED_PAGE: usize = 500;

/// Instances in one SQLite database file.
pub struct SqliteStore {
    log: SqliteEventStore,
    tenant: TenantId,
    runtime: tokio::runtime::Runtime,
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

    /// Refuses a write that names `expected` where the stream holds no row: it ends with the
    /// removal, so the instance is absent, as [`crate::MemoryStore`] answers it. A stream at
    /// another version, or none, is refused by the append itself.
    fn held_at(
        &self,
        stream: &StreamId,
        (entity, identity): (&str, &Value),
        expected: u64,
    ) -> Result<(), StoreError> {
        match self.last(stream)? {
            Some((_, last)) if last.name == REMOVED => Err(StoreError::Conflict {
                entity: entity.to_owned(),
                identity: identity.to_string(),
                expected: Some(expected),
                actual: None,
            }),
            _ => Ok(()),
        }
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

    /// Appends `events` followed by `end` to `stream` in one append, at `expected`. A conflict
    /// names `named`, the version the change or removal named (`None` for a creation), not the
    /// stream position the append was made at, and no held version for a stream never written
    /// (eventlog's 0), as [`crate::MemoryStore`] names them.
    fn append(
        &self,
        stream: &StreamId,
        (entity, identity): (&str, &Value),
        (expected, named): (Expected, Option<u64>),
        events: &[crate::Event],
        end: NewEvent,
    ) -> Result<(), StoreError> {
        let events = recorded(events, end)?;
        let meta = meta(request_hash(&events).map_err(backend)?);
        self.runtime
            .block_on(self.log.append(stream, expected, &events, &meta))
            .map(|_| ())
            .map_err(|error| match error {
                EventLogError::Conflict { actual, .. } => StoreError::Conflict {
                    entity: entity.to_owned(),
                    identity: identity.to_string(),
                    expected: named,
                    actual: (actual > 0).then_some(actual),
                },
                other => backend(other),
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

    /// Appends the change's events and its row. A creation (`expected: None`) on the stream of a
    /// removed instance goes on from the removal's version; a change naming a version of a removed
    /// instance is a conflict, as on [`crate::MemoryStore`].
    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        let stream = self.stream(&change.entity, &change.identity)?;
        let expected = match change.expected {
            Some(version) => {
                self.held_at(&stream, (&change.entity, &change.identity), version)?;
                Expected::Exact(version)
            }
            None => match self.last(&stream)? {
                Some((version, last)) if last.name == REMOVED => Expected::Exact(version),
                _ => Expected::NoStream,
            },
        };
        self.append(
            &stream,
            (&change.entity, &change.identity),
            (expected, change.expected),
            &change.events,
            row_event(&change.row)?,
        )
    }

    /// Appends the removal's events and a `spindle.removed` event: the stream's end, which
    /// [`Store::load`] and [`Store::scan`] read as no row.
    fn remove(&mut self, removal: &Removal) -> Result<(), StoreError> {
        let stream = self.stream(&removal.entity, &removal.identity)?;
        self.held_at(
            &stream,
            (&removal.entity, &removal.identity),
            removal.expected,
        )?;
        self.append(
            &stream,
            (&removal.entity, &removal.identity),
            (Expected::Exact(removal.expected), Some(removal.expected)),
            &removal.events,
            removed_event()?,
        )
    }

    /// Appends every write's events and its `spindle.row` or `spindle.removed` event as one
    /// append group: one transaction over every stream, each at the version its write names. A
    /// write naming a version its instance does not stand at is refused before anything is
    /// appended, naming that instance; a stream moved in between refuses the whole group.
    fn commit_all(&mut self, writes: &[Write]) -> Result<(), StoreError> {
        if writes.is_empty() {
            return Ok(());
        }
        let firsts = first_writes(writes);
        for write in &firsts {
            self.stands_at(write)?;
        }
        let mut appends = Vec::with_capacity(writes.len());
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
                // the first instance no longer where its write found it.
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
    /// Every page moves the position forward, or the read stops as a backend error.
    fn scan(&mut self, entity: &str) -> Result<Vec<(Value, Stored)>, StoreError> {
        let mut latest = BTreeMap::<String, Stored>::new();
        let mut position = 0;
        loop {
            let page = self
                .runtime
                .block_on(self.log.read_feed(&self.tenant, position, FEED_PAGE))
                .map_err(backend)?;
            for event in page.events {
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
        latest
            .into_iter()
            .map(|(identity, stored)| Ok((identity_from_key(&identity)?, stored)))
            .collect()
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
