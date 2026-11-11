//! A state directory's streams moved into the PostgreSQL store: `b10x-spindle state move`.
//!
//! A component's SQLite file (`<component>.sqlite`, as `run --state` keeps it) is read in commit
//! order and appended into the component's PostgreSQL tables group by group: the events one
//! append group of the file recorded (one `request_id`) are one append group again, each stream
//! at the version its first event there follows. So every stream keeps its versions, and with
//! them the version a later commit of an instance names and the entity's version a scan answers.
//! Event ids, `recorded_at` and the feed's positions are new; Spindle reads none of them. The
//! command's actor, subject, request, trace, cause and `occurred_at` are kept.
//!
//! A write of 0.7.0 to 0.10.0, whose `spindle.written` event names no instances (schema 1), is
//! copied with that event naming the instances the write left (schema [`WRITTEN_SCHEMA`]), as
//! this release writes it, so a scan reads the entity's stream rather than the whole log. A file
//! holding a write of a release before 0.7.0, which recorded no entity's version, has its log
//! marked ([`MARKS`]) before the first group is copied, so every scan of it reads the whole log.
//!
//! Every group is built and checked from the file ([`Source::check`]) before anything is written:
//! eventlog's own check of an append group, and the one value PostgreSQL refuses that the SQLite
//! store keeps, U+0000 in a JSON string ([`nul`]). One move holds its schema ([`Lock`]) from
//! before it finds the tables empty ([`Target::is_empty`]) to after its last append, on a
//! session that also reads which tables the schema holds ([`Lock::has_tables`]); it holds one
//! [`Target`] open beside it at a time, [`CONNECTIONS`] connections in all.

use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Duration;

use eventlog_core::{
    AppendGroup, AtomicEventStore, CommandMeta, EventLogError, EventStore, Expected, NewEvent,
    RecordedEvent, StreamAppend, StreamId, TenantId, new_event_id, request_hash,
};
use eventlog_postgres::PostgresEventStore;
use eventlog_sqlite::SqliteEventStore;
use serde_json::Value;
use tokio_postgres::error::SqlState;

use crate::postgres::{Database, PostgresStore, prefix, session};
use crate::sqlite::{
    ENTITY, Log, MARKS, PREFIX, REMOVED, TENANT, WRITTEN, WRITTEN_SCHEMA, Written, backend, meta,
};
use crate::store::StoreError;

/// The event a move appends to the stream [`MARKS`] before it copies a write that recorded no
/// entity's version.
const UNNAMED: &str = "spindle.unnamed";

/// One component's SQLite file, opened to be read: nothing is created.
pub struct Source {
    log: SqliteEventStore,
    tenant: TenantId,
    runtime: tokio::runtime::Runtime,
    read: OnceCell<Read>,
}

impl std::fmt::Debug for Source {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Source").finish_non_exhaustive()
    }
}

/// What one component's move appended, or would append.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Moved {
    /// The streams written: one per instance, one per entity written.
    pub streams: usize,
    /// The events appended, the file's: the mark of a file holding writes that recorded no
    /// entity's version is not counted.
    pub events: usize,
}

/// What a whole read of a file found.
#[derive(Debug)]
struct Read {
    moved: Moved,
    /// Whether a group writes an instance of an entity and no `spindle.written` event of it: a
    /// write of a release before 0.7.0.
    unnamed: bool,
    /// Every entity the file holds a stream of.
    entities: BTreeSet<String>,
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
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .map_err(|error| StoreError::Backend(error.to_string()))?;
        let log = runtime
            .block_on(SqliteEventStore::open_existing(location, PREFIX))
            .map_err(|error| StoreError::Backend(format!("{}: {error}", path.display())))?;
        Ok(Self {
            log,
            tenant: TenantId::new(TENANT).map_err(backend)?,
            runtime,
            read: OnceCell::new(),
        })
    }

    /// Reads the whole file and builds every append group a move of it appends, writing
    /// nothing: what a move would append. The file is read once; a move reads it again only to
    /// copy it.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the file cannot be read, or a group would be refused by
    /// PostgreSQL or by eventlog: a JSON string holding U+0000, naming the stream and where.
    pub fn check(&self) -> Result<Moved, StoreError> {
        self.read().map(|read| read.moved)
    }

    /// The file read whole, once.
    fn read(&self) -> Result<&Read, StoreError> {
        if let Some(read) = self.read.get() {
            return Ok(read);
        }
        let mut any_unnamed = false;
        let mut entities = BTreeSet::new();
        let moved = self.each_group(&mut |events| {
            group(&self.tenant, events)?;
            any_unnamed |= unnamed(events);
            for event in events {
                if event.stream_type == ENTITY {
                    entities.insert(event.stream_id.clone());
                } else if event.stream_type != MARKS.0 {
                    entities.insert(event.stream_type.clone());
                }
            }
            Ok(())
        })?;
        Ok(self.read.get_or_init(|| Read {
            moved,
            unnamed: any_unnamed,
            entities,
        }))
    }

    /// Every group the file recorded, in commit order, to `visit`: the events of one
    /// `request_id`, in their order.
    fn each_group(
        &self,
        visit: &mut dyn FnMut(&[RecordedEvent]) -> Result<(), StoreError>,
    ) -> Result<Moved, StoreError> {
        let mut streams = BTreeSet::new();
        let mut events = 0;
        let mut pending: Vec<RecordedEvent> = Vec::new();
        self.log
            .each_event(&self.runtime, &self.tenant, &mut |event: RecordedEvent| {
                if pending
                    .last()
                    .is_some_and(|last| last.request_id != event.request_id)
                {
                    visit(&pending)?;
                    pending.clear();
                }
                streams.insert((event.stream_type.clone(), event.stream_id.clone()));
                events += 1;
                pending.push(event);
                Ok(())
            })?;
        if !pending.is_empty() {
            visit(&pending)?;
        }
        Ok(Moved {
            streams: streams.len(),
            events,
        })
    }
}

/// Whether the group `events` writes an instance of an entity without a `spindle.written` event
/// of that entity, as every write before 0.7.0 did.
fn unnamed(events: &[RecordedEvent]) -> bool {
    let written: BTreeSet<&str> = events
        .iter()
        .filter(|event| event.stream_type == ENTITY && event.name == WRITTEN)
        .map(|event| event.stream_id.as_str())
        .collect();
    events.iter().any(|event| {
        event.stream_type != ENTITY
            && event.stream_type != MARKS.0
            && !written.contains(event.stream_type.as_str())
    })
}

/// One component's PostgreSQL tables, opened to be moved into.
pub struct Target {
    log: PostgresEventStore,
    tenant: TenantId,
    runtime: std::sync::Arc<tokio::runtime::Runtime>,
}

impl std::fmt::Debug for Target {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Target").finish_non_exhaustive()
    }
}

impl Target {
    /// Opens `component`'s tables in `database` as its store opens them ([`PostgresStore::open`]):
    /// the role and settings `run` takes. Where PostgreSQL refuses the connection as one too
    /// many (SQLSTATE 53300), as it may while the session of a target just closed is still
    /// ending, it tries again for up to [`OPEN_WAIT`].
    ///
    /// # Errors
    /// As [`PostgresStore::open`]; a connection still refused as one too many after
    /// [`OPEN_WAIT`] names the connections a move holds ([`CONNECTIONS`]).
    pub fn open(database: &Database, component: &str, sharing: usize) -> Result<Self, StoreError> {
        let until = std::time::Instant::now() + OPEN_WAIT;
        let opened = loop {
            match PostgresStore::open(database, component, sharing) {
                Err(StoreError::Backend(why))
                    if why == EventLogError::Overloaded.to_string()
                        && std::time::Instant::now() < until =>
                {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(StoreError::Backend(why)) if why == EventLogError::Overloaded.to_string() => {
                    return Err(StoreError::Backend(format!(
                        "PostgreSQL refused a connection to the tables of {component} as one too \
                         many, still after {} s: the role database_url names is at its \
                         CONNECTION LIMIT, or the server at max_connections. A state move holds \
                         {CONNECTIONS} connections at once, one holding the schema's lock and \
                         one to the tables of the component it reads or copies; stop every run \
                         on the database first",
                        OPEN_WAIT.as_secs()
                    )));
                }
                other => break other?,
            }
        };
        let (log, runtime) = opened.into_parts()?;
        Ok(Self {
            log,
            tenant: TenantId::new(TENANT).map_err(backend)?,
            runtime,
        })
    }

    /// Closes the tables' connections, so the next target opened fits the connections a move
    /// holds ([`CONNECTIONS`]).
    ///
    /// # Errors
    /// [`StoreError::Backend`] when a connection is still in use past eventlog's shutdown wait.
    pub fn close(self) -> Result<(), StoreError> {
        self.runtime
            .block_on(self.log.shutdown())
            .map_err(|error| StoreError::Backend(format!("closing the tables: {error}")))
    }

    /// Whether the tables hold no stream a move of `source` could land on: neither the stream
    /// of any entity in `declared` (those the specification declares) or in the file, nor the
    /// mark of a log holding writes that recorded no entity's version ([`MARKS`]).
    ///
    /// Every write of a PostgreSQL store appends to the stream of each entity it writes, and a
    /// move to the mark before anything else, so tables holding any of Spindle's writes hold
    /// one of these. One read of each stream's last version, a committed event whatever
    /// transaction is open elsewhere on the server: it takes no lock, writes nothing and does
    /// not grow with the log.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the file or the tables cannot be read.
    pub fn is_empty(&self, declared: &[String], source: &Source) -> Result<bool, StoreError> {
        let entities: BTreeSet<&str> = declared
            .iter()
            .chain(&source.read()?.entities)
            .map(String::as_str)
            .collect();
        let mut streams = Vec::with_capacity(entities.len() + 1);
        for entity in entities {
            streams.push(StreamId::new(self.tenant.clone(), ENTITY, entity).map_err(backend)?);
        }
        let (kind, id) = MARKS;
        streams.push(StreamId::new(self.tenant.clone(), kind, id).map_err(backend)?);
        for stream in &streams {
            let version = self
                .runtime
                .block_on(self.log.stream_version(stream))
                .map_err(|error| StoreError::Backend(format!("reading the tables: {error}")))?;
            if version.is_some() {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Appends every event of `source`, in its order, one append group per group the file
    /// recorded, each stream at the version it held there; first, where the file holds a write
    /// that recorded no entity's version, one event to [`MARKS`].
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the file cannot be read, a group cannot be built
    /// ([`Source::check`]), or the database refuses an append: a stream already there refuses
    /// its group as a conflict. The groups appended before it stay.
    pub fn copy(&self, source: &Source) -> Result<Moved, StoreError> {
        if source.read()?.unnamed {
            self.mark()?;
        }
        source.each_group(&mut |events| {
            let group = group(&self.tenant, events)?;
            self.runtime
                .block_on(self.log.append_group(&group))
                .map(|_| ())
                .map_err(backend)
        })
    }

    /// Appends the mark of a log holding writes that recorded no entity's version.
    fn mark(&self) -> Result<(), StoreError> {
        let (kind, id) = MARKS;
        let appends = vec![StreamAppend {
            stream: StreamId::new(self.tenant.clone(), kind, id).map_err(backend)?,
            expected: Expected::NoStream,
            events: vec![
                NewEvent::new(UNNAMED, 1, Value::Object(serde_json::Map::new()))
                    .map_err(backend)?,
            ],
        }];
        let entries: Vec<&Vec<NewEvent>> = appends.iter().map(|append| &append.events).collect();
        let group = AppendGroup {
            tenant: self.tenant.clone(),
            meta: meta(request_hash(&entries).map_err(backend)?),
            appends,
        };
        self.runtime
            .block_on(self.log.append_group(&group))
            .map(|_| ())
            .map_err(|error| StoreError::Backend(format!("marking the log: {error}")))
    }
}

/// The lock one `state move` holds on its target schema while it runs, a PostgreSQL
/// session-level advisory lock on a connection of its own: the key is [`LOCK_CLASS`] and the
/// schema's name hashed, a pair of keys eventlog's own locks, each one 64-bit key, never take.
pub struct Lock {
    client: tokio_postgres::Client,
    schema: String,
    runtime: tokio::runtime::Runtime,
}

impl std::fmt::Debug for Lock {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Lock")
            .field("schema", &self.schema)
            .finish_non_exhaustive()
    }
}

/// The first key of a move's lock: `SPMV`.
const LOCK_CLASS: i32 = 0x5350_4D56;

/// How long a move waits for another move's lock on its schema before it is refused.
const LOCK_WAIT: Duration = Duration::from_secs(5);

/// The connections of its role a move holds at once: its lock's session ([`Lock`]) and one
/// [`Target`]'s, each component's opened and closed in turn. Within what `run` is admitted with
/// for two components or more, which holds one connection per component; one component whose
/// serving role is limited to one connection is refused ([`Lock::take`]).
const CONNECTIONS: usize = 2;

/// How long [`Target::open`] tries again a connection PostgreSQL refuses as one too many.
const OPEN_WAIT: Duration = Duration::from_secs(5);

impl Lock {
    /// Takes the lock on `database`'s schema, waiting up to [`LOCK_WAIT`] for another move to
    /// end. The session has no statement or idle timeout, so it keeps the lock while the move's
    /// appends run on other connections.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when another move holds the schema past the wait, naming it; the
    /// role's `CONNECTION LIMIT` is below [`CONNECTIONS`], naming both; or the database cannot be
    /// reached as a store reaches it.
    pub fn take(database: &Database) -> Result<Self, StoreError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| StoreError::Backend(error.to_string()))?;
        let client = session(database, &runtime)?;
        let limit: i32 = runtime
            .block_on(client.query_one(
                "SELECT CASE WHEN rolsuper THEN -1 ELSE rolconnlimit END FROM pg_roles WHERE \
                 rolname = current_user",
                &[],
            ))
            .map(|row| row.get(0))
            .map_err(|error| {
                StoreError::Backend(format!("reading the role's CONNECTION LIMIT: {error}"))
            })?;
        if usize::try_from(limit).is_ok_and(|limit| limit < CONNECTIONS) {
            return Err(StoreError::Backend(format!(
                "the role database_url names has CONNECTION LIMIT {limit}, and a state move holds \
                 {CONNECTIONS} connections of it at once: one holding the schema's lock and one \
                 to the tables of the component it reads or copies, each in turn. Move with a \
                 role limited to {CONNECTIONS} or more (for one component, with database_pool \
                 {CONNECTIONS} or more, which run admits such a role with)"
            )));
        }
        let schema = database.schema.clone();
        let taken = runtime.block_on(async {
            client
                .batch_execute(&format!(
                    "SELECT set_config(name, '0', false) FROM pg_settings WHERE name IN \
                     ('statement_timeout', 'idle_session_timeout', \
                     'idle_in_transaction_session_timeout'); SET lock_timeout = {}",
                    LOCK_WAIT.as_millis()
                ))
                .await?;
            client
                .execute(
                    "SELECT pg_advisory_lock($1, hashtext($2))",
                    &[&LOCK_CLASS, &schema],
                )
                .await
        });
        match taken {
            Ok(_) => Ok(Self {
                client,
                schema,
                runtime,
            }),
            Err(error) if error.code() == Some(&SqlState::LOCK_NOT_AVAILABLE) => {
                Err(StoreError::Backend(format!(
                    "another state move into the schema {schema} holds it, still after {} s: \
                     one move at a time writes into a schema",
                    LOCK_WAIT.as_secs()
                )))
            }
            Err(error) => Err(StoreError::Backend(format!(
                "taking the lock of a state move on the schema {schema}: {error}"
            ))),
        }
    }

    /// Whether the schema holds `component`'s event table, read on the lock's session from the
    /// catalogue: it opens no store, so on a loopback database, where opening a store creates its
    /// tables, it creates nothing. Tables without it hold no stream.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the catalogue cannot be read.
    pub fn has_tables(&self, component: &str) -> Result<bool, StoreError> {
        let table = format!("{}_events", prefix(component));
        self.runtime
            .block_on(self.client.query_one(
                "SELECT EXISTS (SELECT FROM pg_catalog.pg_tables WHERE schemaname = $1 AND \
                 tablename = $2)",
                &[&self.schema, &table],
            ))
            .map(|row| row.get::<_, bool>(0))
            .map_err(|error| StoreError::Backend(format!("reading the schema's tables: {error}")))
    }

    /// Releases the lock, after the move's last append.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the session ended before: the lock was not held to the end.
    pub fn release(self) -> Result<(), StoreError> {
        let released = self
            .runtime
            .block_on(self.client.query_one(
                "SELECT pg_advisory_unlock($1, hashtext($2))",
                &[&LOCK_CLASS, &self.schema],
            ))
            .map(|row| row.get::<_, bool>(0));
        match released {
            Ok(true) => Ok(()),
            Ok(false) | Err(_) => Err(StoreError::Backend(format!(
                "the lock of this state move on the schema {} ended before the move did: another \
                 move may have written into it",
                self.schema
            ))),
        }
    }
}

/// `events`, one group of the file, as one append group, checked as eventlog checks it before
/// appending: each stream expected at the version before its first event. A `spindle.written`
/// event that names no instances (schema 1, of 0.7.0 to 0.10.0) names, as one of this release
/// does ([`Written`]), each instance of its entity the group leaves, at the group's last event
/// of the instance's stream.
///
/// # Errors
/// [`StoreError::Backend`] when an event's body holds U+0000 in a JSON string ([`nul`]), or
/// eventlog refuses the group, naming the stream.
fn group(tenant: &TenantId, events: &[RecordedEvent]) -> Result<AppendGroup, StoreError> {
    let Some(first) = events.first() else {
        return Err(StoreError::Backend(
            "a group of the file holds no event".to_owned(),
        ));
    };
    let mut named: BTreeMap<&str, Written> = events
        .iter()
        .filter(|event| {
            event.stream_type == ENTITY
                && event.name == WRITTEN
                && event.schema_version < WRITTEN_SCHEMA
        })
        .map(|event| (event.stream_id.as_str(), Written::default()))
        .collect();
    for event in events {
        if let Some(written) = named.get_mut(event.stream_type.as_str()) {
            written.note(&event.stream_id, event.version, event.name == REMOVED);
        }
    }
    let mut appends: Vec<StreamAppend> = Vec::new();
    for event in events {
        let described = || {
            format!(
                "the event {} at version {} of the stream {} {}",
                event.name, event.version, event.stream_type, event.stream_id
            )
        };
        if let Some(path) = nul(&event.data, "body") {
            return Err(StoreError::Backend(format!(
                "{} holds U+0000 in {path}, which the SQLite store keeps and PostgreSQL refuses \
                 in a JSON string",
                described()
            )));
        }
        let invalid = |error: eventlog_core::EventLogError| {
            StoreError::Backend(format!("{}: {error}", described()))
        };
        let stream =
            StreamId::new(tenant.clone(), &event.stream_type, &event.stream_id).map_err(invalid)?;
        let rewritten = (event.stream_type == ENTITY
            && event.name == WRITTEN
            && event.schema_version < WRITTEN_SCHEMA)
            .then(|| named.remove(event.stream_id.as_str()).unwrap_or_default());
        let new = match rewritten {
            Some(written) => NewEvent::new(WRITTEN, WRITTEN_SCHEMA, written.body()),
            None => NewEvent::new(event.name.clone(), event.schema_version, event.data.clone()),
        }
        .map_err(invalid)?;
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
        tenant: tenant.clone(),
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
    group.fingerprint().map_err(|error| {
        StoreError::Backend(format!(
            "the write {} to the stream {} {}: {error}",
            first.request_id, first.stream_type, first.stream_id
        ))
    })?;
    Ok(group)
}

/// Where `value` holds U+0000 in a string or an object key, the one JSON value the SQLite store
/// keeps that PostgreSQL's `jsonb` refuses (SQLSTATE 22P05), written from `at` as JSON keys and
/// indexes: `body["fields"]["note"]`. Every other value a [`Value`] holds `jsonb` takes: its
/// numbers are finite, its strings valid Unicode, and its depth within what the SQLite store
/// reads back.
fn nul(value: &Value, at: &str) -> Option<String> {
    match value {
        Value::String(text) => text.contains('\0').then(|| at.to_owned()),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .find_map(|(index, item)| nul(item, &format!("{at}[{index}]"))),
        Value::Object(fields) => fields.iter().find_map(|(key, item)| {
            let path = format!("{at}[{}]", Value::String(key.clone()));
            if key.contains('\0') {
                Some(format!("the key {path}"))
            } else {
                nul(item, &path)
            }
        }),
        Value::Null | Value::Bool(_) | Value::Number(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// An event of `stream_type` `stream_id` at `version`, of the group `request`.
    fn recorded(
        request: &str,
        stream: (&str, &str),
        version: u64,
        name: &str,
        schema_version: u32,
        data: Value,
    ) -> RecordedEvent {
        RecordedEvent {
            global_seq: 0,
            tenant: TenantId::new(TENANT).expect("a tenant"),
            stream_type: stream.0.to_owned(),
            stream_id: stream.1.to_owned(),
            version,
            event_id: new_event_id(),
            name: name.to_owned(),
            schema_version,
            occurred_at: time::OffsetDateTime::UNIX_EPOCH,
            recorded_at: time::OffsetDateTime::UNIX_EPOCH,
            subject: "s".to_owned(),
            actor: "a".to_owned(),
            request_id: request.to_owned(),
            trace_id: "t".to_owned(),
            causation_id: None,
            causation_depth: 0,
            redacted_at: None,
            data,
            digest: None,
            parents: Vec::new(),
        }
    }

    const TASK: &str = "a.Task";

    /// The events `group` copies to `stream`: name, schema and body.
    fn copied(group: &AppendGroup, stream: (&str, &str)) -> Vec<(String, u32, Value)> {
        group
            .appends
            .iter()
            .filter(|append| {
                append.stream.stream_type() == stream.0 && append.stream.stream_id() == stream.1
            })
            .flat_map(|append| &append.events)
            .map(|event| (event.name.clone(), event.schema_version, event.data.clone()))
            .collect()
    }

    #[test]
    fn a_written_event_naming_no_instances_is_copied_naming_each_the_group_left() {
        let tenant = TenantId::new(TENANT).expect("a tenant");
        let events = [
            recorded("r", (ENTITY, TASK), 4, WRITTEN, 1, json!({})),
            recorded("r", (ENTITY, "b.Other"), 2, WRITTEN, 1, json!({})),
            recorded("r", (TASK, "\"a\""), 3, "a.Touched", 1, json!({"n": 1})),
            recorded(
                "r",
                (TASK, "\"a\""),
                4,
                "spindle.row",
                1,
                json!({"state": "x"}),
            ),
            recorded(
                "r",
                (TASK, "\"b\""),
                1,
                "spindle.row",
                1,
                json!({"state": "y"}),
            ),
            recorded("r", (TASK, "\"b\""), 2, REMOVED, 1, json!({})),
            recorded("r", (TASK, "\"c\""), 6, REMOVED, 1, json!({})),
        ];
        let group = group(&tenant, &events).expect("the group is built");
        assert_eq!(
            copied(&group, (ENTITY, TASK)),
            [(
                WRITTEN.to_owned(),
                WRITTEN_SCHEMA,
                json!({"rows": {"\"a\"": 4}, "removed": {"\"b\"": 2, "\"c\"": 6}})
            )]
        );
        assert_eq!(
            copied(&group, (ENTITY, "b.Other")),
            [(
                WRITTEN.to_owned(),
                WRITTEN_SCHEMA,
                json!({"rows": {}, "removed": {}})
            )],
            "an entity the write was only conditioned on names no instance"
        );
        assert_eq!(
            copied(&group, (TASK, "\"a\"")),
            [
                ("a.Touched".to_owned(), 1, json!({"n": 1})),
                ("spindle.row".to_owned(), 1, json!({"state": "x"}))
            ],
            "every other event is copied as it is"
        );
        let current = json!({"rows": {"\"z\"": 9}, "removed": {}});
        let events = [
            recorded(
                "s",
                (ENTITY, TASK),
                5,
                WRITTEN,
                WRITTEN_SCHEMA,
                current.clone(),
            ),
            recorded(
                "s",
                (TASK, "\"z\""),
                9,
                "spindle.row",
                1,
                json!({"state": "z"}),
            ),
        ];
        assert_eq!(
            copied(&group_of(&tenant, &events), (ENTITY, TASK)),
            [(WRITTEN.to_owned(), WRITTEN_SCHEMA, current)],
            "a written event that names its instances is copied as it is"
        );
    }

    fn group_of(tenant: &TenantId, events: &[RecordedEvent]) -> AppendGroup {
        group(tenant, events).expect("the group is built")
    }

    #[test]
    fn a_group_writing_an_instance_with_no_written_event_of_its_entity_is_unnamed() {
        let written = recorded("r", (ENTITY, TASK), 1, WRITTEN, 1, json!({}));
        let row = recorded("r", (TASK, "\"a\""), 1, "spindle.row", 1, json!({}));
        let other = recorded("r", ("b.Other", "\"b\""), 1, "spindle.row", 1, json!({}));
        assert!(!unnamed(&[written.clone(), row.clone()]));
        assert!(!unnamed(std::slice::from_ref(&written)));
        assert!(unnamed(std::slice::from_ref(&row)), "a write before 0.7.0");
        assert!(
            unnamed(&[written, row, other]),
            "an entity of the group with no written event"
        );
    }

    #[test]
    fn nul_finds_u0000_in_any_string_or_key_and_nothing_else() {
        assert_eq!(
            nul(&json!({"a": "b\u{0}c"}), "body"),
            Some("body[\"a\"]".to_owned())
        );
        assert_eq!(
            nul(&json!({"a": [1, {"b": ["x", "\u{0}"]}]}), "body"),
            Some("body[\"a\"][1][\"b\"][1]".to_owned())
        );
        assert_eq!(
            nul(&json!({"a": {"k\u{0}": 1}}), "body"),
            Some("the key body[\"a\"][\"k\\u0000\"]".to_owned())
        );
        assert_eq!(nul(&json!("\u{0}"), "body"), Some("body".to_owned()));
        assert_eq!(
            nul(
                &json!({"a": "\\u0000", "b": [null, true, 1.5, -3, u64::MAX], "\u{1}": "\u{7f}"}),
                "body"
            ),
            None,
            "an escaped backslash-u and other control characters are kept"
        );
    }
}
