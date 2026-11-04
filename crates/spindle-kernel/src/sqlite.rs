//! Instances on SQLite, through `eventlog`: `store.durable` bound to `sqlite`.
//!
//! Each instance is one event stream, keyed by its entity and its identity. A commit appends the
//! events the command emitted followed by one `spindle.row` event carrying the row as it now
//! rests, in one append, so the row and the events that changed it are recorded together or not
//! at all. Loading reads the stream's last event. The stream's version is the version a later
//! commit names, so two writers that read the same row cannot both commit.

use std::path::Path;

use eventlog_core::{
    CommandMeta, EventLogError, EventStore, Expected, NewEvent, StreamId, TenantId, new_event_id,
    request_hash,
};
use eventlog_sqlite::SqliteEventStore;
use serde_json::Value;
use time::OffsetDateTime;

use crate::store::{Change, Row, Store, StoreError, Stored};

/// The table prefix Spindle's events are kept under.
const PREFIX: &str = "spindle";
/// The one tenant a single Spindle process holds.
const TENANT: &str = "local";
/// The event that carries an instance's row after a change.
const ROW: &str = "spindle.row";
/// Who records the change: the runtime itself.
const ACTOR: &str = "spindle-kernel";
/// The schema version of every event Spindle appends.
const SCHEMA: u32 = 1;

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
}

impl Store for SqliteStore {
    fn load(&mut self, entity: &str, identity: &Value) -> Result<Option<Stored>, StoreError> {
        let stream = self.stream(entity, identity)?;
        let Some(version) = self
            .runtime
            .block_on(self.log.stream_version(&stream))
            .map_err(backend)?
        else {
            return Ok(None);
        };
        let slice = self
            .runtime
            .block_on(self.log.read_stream(&stream, version.saturating_sub(1), 1))
            .map_err(backend)?;
        let last = slice
            .events
            .last()
            .filter(|event| event.name == ROW)
            .ok_or_else(|| {
                StoreError::Backend(format!(
                    "the stream of {entity} {identity} does not end with its row"
                ))
            })?;
        let row = serde_json::from_value::<Row>(last.data.clone())
            .map_err(|error| StoreError::Backend(format!("a stored row: {error}")))?;
        Ok(Some(Stored { row, version }))
    }

    fn commit(&mut self, change: &Change) -> Result<(), StoreError> {
        let stream = self.stream(&change.entity, &change.identity)?;
        let mut events = change
            .events
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
        let row = serde_json::to_value(&change.row)
            .map_err(|error| StoreError::Backend(error.to_string()))?;
        events.push(NewEvent::new(ROW, SCHEMA, row).map_err(backend)?);
        let meta = CommandMeta {
            idempotency_key: new_event_id(),
            request_hash: request_hash(&events).map_err(backend)?,
            subject: ACTOR.to_owned(),
            actor: ACTOR.to_owned(),
            request_id: new_event_id(),
            trace_id: new_event_id(),
            causation_id: None,
            causation_depth: 0,
            occurred_at: OffsetDateTime::now_utc(),
            claim: None,
        };
        let expected = change.expected.map_or(Expected::NoStream, Expected::Exact);
        self.runtime
            .block_on(self.log.append(&stream, expected, &events, &meta))
            .map(|_| ())
            .map_err(|error| match error {
                EventLogError::Conflict { expected, actual } => StoreError::Conflict {
                    entity: change.entity.clone(),
                    identity: change.identity.to_string(),
                    expected: Some(expected),
                    actual: Some(actual),
                },
                other => backend(other),
            })
    }
}

fn backend(error: EventLogError) -> StoreError {
    StoreError::Backend(error.to_string())
}
