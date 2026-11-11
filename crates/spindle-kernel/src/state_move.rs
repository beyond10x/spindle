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
//! Every group is built and checked from the file ([`Source::check`]) before anything is written:
//! eventlog's own check of an append group, and the one value PostgreSQL refuses that the SQLite
//! store keeps, U+0000 in a JSON string ([`nul`]).

use std::collections::BTreeSet;
use std::path::Path;

use eventlog_core::{
    AppendGroup, AtomicEventStore, CaptureError, CaptureLimits, CaptureResource, CommandMeta,
    Expected, NewEvent, RecordedEvent, StreamAppend, StreamId, TenantId, new_event_id,
    request_hash,
};
use eventlog_postgres::PostgresEventStore;
use eventlog_sqlite::SqliteEventStore;
use serde_json::Value;

use crate::postgres::{Database, PostgresStore, capture};
use crate::sqlite::{Log, PREFIX, TENANT, backend};
use crate::store::StoreError;

/// One component's SQLite file, opened to be read: nothing is created.
pub struct Source {
    log: SqliteEventStore,
    tenant: TenantId,
    runtime: tokio::runtime::Runtime,
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
    /// The events appended.
    pub events: usize,
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
        })
    }

    /// Reads the whole file and builds every append group a move of it appends, writing
    /// nothing: what a move would append.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the file cannot be read, or a group would be refused by
    /// PostgreSQL or by eventlog: a JSON string holding U+0000, naming the stream and where.
    pub fn check(&self) -> Result<Moved, StoreError> {
        self.each_group(&mut |events| group(&self.tenant, events).map(|_| ()))
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
    /// Read at one snapshot of every committed event, as a scan reads ([`capture`]), not
    /// through the feed, which hides an event while an older write transaction anywhere on the
    /// server is open. A log no scan has read gets its tenant's stream identity recorded, as the
    /// first scan of `run` records it; no event is written.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the log cannot be read.
    pub fn is_empty(&self) -> Result<bool, StoreError> {
        let limits = CaptureLimits {
            max_events: 0,
            max_blobs: u64::MAX,
            max_projection_rows: u64::MAX,
            max_payload_bytes: u64::MAX,
        };
        match capture(&self.log, &self.runtime, &self.tenant, limits) {
            Ok(captured) => Ok(captured.events.is_empty()),
            Err(
                CaptureError::LimitExceeded {
                    resource: CaptureResource::Events,
                    ..
                }
                | CaptureError::RedactedHistory,
            ) => Ok(false),
            Err(error) => Err(StoreError::Backend(format!("reading the log: {error}"))),
        }
    }

    /// Appends every event of `source`, in its order, one append group per group the file
    /// recorded, each stream at the version it held there.
    ///
    /// # Errors
    /// [`StoreError::Backend`] when the file cannot be read, a group cannot be built
    /// ([`Source::check`]), or the database refuses an append: a stream already there refuses
    /// its group as a conflict. The groups appended before it stay.
    pub fn copy(&self, source: &Source) -> Result<Moved, StoreError> {
        source.each_group(&mut |events| {
            let group = group(&self.tenant, events)?;
            self.runtime
                .block_on(self.log.append_group(&group))
                .map(|_| ())
                .map_err(backend)
        })
    }
}

/// `events`, one group of the file, as one append group, checked as eventlog checks it before
/// appending: each stream expected at the version before its first event.
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
    let mut appends: Vec<StreamAppend> = Vec::new();
    for event in events {
        let named = || {
            format!(
                "the event {} at version {} of the stream {} {}",
                event.name, event.version, event.stream_type, event.stream_id
            )
        };
        if let Some(path) = nul(&event.data, "body") {
            return Err(StoreError::Backend(format!(
                "{} holds U+0000 in {path}, which the SQLite store keeps and PostgreSQL refuses \
                 in a JSON string",
                named()
            )));
        }
        let invalid = |error: eventlog_core::EventLogError| {
            StoreError::Backend(format!("{}: {error}", named()))
        };
        let stream =
            StreamId::new(tenant.clone(), &event.stream_type, &event.stream_id).map_err(invalid)?;
        let new = NewEvent::new(event.name.clone(), event.schema_version, event.data.clone())
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

    use super::nul;

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
