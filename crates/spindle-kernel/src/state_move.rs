//! A state directory's streams moved into the PostgreSQL store: `b10x-spindle state move`.
//!
//! A component's SQLite file (`<component>.sqlite`, as `run --state` keeps it) is read through
//! its feed, in commit order, and appended into the component's PostgreSQL tables group by group:
//! the events one append group of the file recorded (one `request_id`) are one append group
//! again, each stream at the version its first event there follows. So every stream keeps its
//! versions, and with them the version a later commit of an instance names and the entity's
//! version a scan answers. Event ids, `recorded_at` and the feed's positions are new; Spindle
//! reads none of them. The command's actor, subject, request, trace, cause and `occurred_at` are
//! kept.

use std::collections::BTreeSet;
use std::path::Path;

use eventlog_core::{
    AppendGroup, AtomicEventStore, CommandMeta, EventStore, Expected, MAX_READ_LIMIT, NewEvent,
    RecordedEvent, StreamAppend, StreamId, TenantId, new_event_id, request_hash,
};
use eventlog_postgres::PostgresEventStore;
use eventlog_sqlite::SqliteEventStore;

use crate::postgres::{Database, PostgresStore};
use crate::sqlite::{PREFIX, TENANT, backend};
use crate::store::StoreError;

/// One component's SQLite file, opened to be read: nothing is created.
pub struct Source {
    log: SqliteEventStore,
}

impl std::fmt::Debug for Source {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Source").finish_non_exhaustive()
    }
}

impl Source {
    /// Opens the SQLite file at `path` that a store of `run --state` keeps, creating nothing.
    ///
    /// # Errors
    /// [`StoreError::Backend`] for a missing file, or one that holds no Spindle store.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let location = path.to_str().ok_or_else(|| {
            StoreError::Backend(format!("{} is not a UTF-8 path", path.display()))
        })?;
        let log = runtime()?
            .block_on(SqliteEventStore::open_existing(location, PREFIX))
            .map_err(|error| StoreError::Backend(format!("{}: {error}", path.display())))?;
        Ok(Self { log })
    }
}

/// One component's PostgreSQL tables, opened to be moved into.
pub struct Target {
    log: PostgresEventStore,
    tenant: TenantId,
    runtime: tokio::runtime::Runtime,
}

impl std::fmt::Debug for Target {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Target").finish_non_exhaustive()
    }
}

/// What one component's move appended.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Moved {
    /// The streams written: one per instance, one per entity written.
    pub streams: usize,
    /// The events appended.
    pub events: usize,
}

impl Target {
    /// Opens `component`'s tables in `database` as its store opens them ([`PostgresStore::open`]):
    /// the role and settings `run` takes.
    ///
    /// # Errors
    /// As [`PostgresStore::open`].
    pub fn open(database: &Database, component: &str, sharing: usize) -> Result<Self, StoreError> {
        let (log, runtime) = PostgresStore::open(database, component, sharing)?.into_parts();
        Ok(Self {
            log,
            tenant: TenantId::new(TENANT).map_err(backend)?,
            runtime,
        })
    }

    /// Whether the tables hold no event of Spindle's tenant: no stream is there to move onto.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the feed cannot be read.
    pub fn is_empty(&self) -> Result<bool, StoreError> {
        let page = self
            .runtime
            .block_on(self.log.read_feed(&self.tenant, 0, 1))
            .map_err(backend)?;
        Ok(page.events.is_empty())
    }

    /// Appends every event of `source`'s feed, in its order, one append group per group the file
    /// recorded, each stream at the version it held there.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the file cannot be read, or the database refuses an append:
    /// a stream already there refuses its group as a conflict. The groups appended before it stay.
    pub fn copy(&self, source: &Source) -> Result<Moved, StoreError> {
        let mut streams = BTreeSet::new();
        let mut events = 0;
        let mut group: Vec<RecordedEvent> = Vec::new();
        let mut position = 0;
        loop {
            let page = self
                .runtime
                .block_on(source.log.read_feed(&self.tenant, position, MAX_READ_LIMIT))
                .map_err(backend)?;
            for event in page.events {
                if group
                    .last()
                    .is_some_and(|last| last.request_id != event.request_id)
                {
                    self.append(&std::mem::take(&mut group))?;
                }
                streams.insert((event.stream_type.clone(), event.stream_id.clone()));
                events += 1;
                group.push(event);
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
        if !group.is_empty() {
            self.append(&group)?;
        }
        Ok(Moved {
            streams: streams.len(),
            events,
        })
    }

    /// Appends `events`, one group of the file, as one append group: each stream expected at the
    /// version before its first event.
    fn append(&self, events: &[RecordedEvent]) -> Result<(), StoreError> {
        let Some(first) = events.first() else {
            return Ok(());
        };
        let mut appends: Vec<StreamAppend> = Vec::new();
        for event in events {
            let stream = StreamId::new(self.tenant.clone(), &event.stream_type, &event.stream_id)
                .map_err(backend)?;
            let new = NewEvent::new(event.name.clone(), event.schema_version, event.data.clone())
                .map_err(backend)?;
            match appends.iter_mut().find(|append| append.stream == stream) {
                Some(append) => append.events.push(new),
                None => appends.push(StreamAppend {
                    stream,
                    expected: match event.version {
                        0 | 1 => Expected::NoStream,
                        version => Expected::Exact(version - 1),
                    },
                    events: vec![new],
                }),
            }
        }
        let entries: Vec<&Vec<NewEvent>> = appends.iter().map(|append| &append.events).collect();
        let group = AppendGroup {
            tenant: self.tenant.clone(),
            meta: CommandMeta {
                idempotency_key: new_event_id(),
                request_hash: request_hash(&entries).map_err(backend)?,
                subject: first.subject.clone(),
                actor: first.actor.clone(),
                request_id: first.request_id.clone(),
                trace_id: first.trace_id.clone(),
                causation_id: first.causation_id.clone(),
                causation_depth: first.causation_depth,
                occurred_at: first.occurred_at,
                claim: None,
            },
            appends,
        };
        self.runtime
            .block_on(self.log.append_group(&group))
            .map(|_| ())
            .map_err(backend)
    }
}

/// The runtime a file is opened on: one thread.
fn runtime() -> Result<tokio::runtime::Runtime, StoreError> {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(|error| StoreError::Backend(error.to_string()))
}
