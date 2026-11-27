//! The events published, read as a range after a cursor ([`Cursor`], [`Page`]): each
//! component's store is read only after the position the cursor marks for it, so a read with the
//! cursor at the end reads no event, however long the log is.

use std::collections::BTreeMap;
use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::{Value, json};
use spindle_kernel::{Committed, Event, Provenance};

use super::{HostError, Published};

/// The version of a cursor's encoding.
const VERSION: u64 = 1;

/// Where a range of the events published ends ([`super::Host::published_after`]): opaque text,
/// written with [`fmt::Display`] and read back with [`Cursor::parse`], of the characters
/// `A-Z a-z 0-9 - _` only, so a query string carries it unescaped.
///
/// It holds the last event answered, by the sequence its command was attributed with, its
/// component and its place among that command's events in that component's store: what the
/// whole list orders the events by ([`super::Host::published`]). Within one run, an event after
/// it in that order is after the cursor.
///
/// It also marks, for each component, the position of its store up to which every event is
/// answered, and the run of the host and the store it was taken from. A read from the same store
/// of the same run reads only after that position. Where this run replaced the store since (a
/// reset opens an empty memory or SQLite store), the store is read from its start once and its
/// events up to the last one answered are skipped, by the sequences, which grow within a run.
/// Where the host was opened again (a restart), each store is read after the position marked for
/// it, whatever sequences the new run's commands take, so on a fixed clock too, where the store
/// keeps the log of events the mark names ([`spindle_kernel::Store::events_id`]): a store that
/// kept its writes (SQLite, PostgreSQL) goes on from them, and a store of another log (a new
/// memory store, a SQLite file a reset created again) is read from its start. The cursor
/// answered then marks the stores as they are now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    /// The host run the marks were taken in; `0` for none.
    run: u64,
    /// The last event answered; `None` before the first.
    last: Option<Key>,
    /// Each component's mark, by component name.
    marks: BTreeMap<String, Mark>,
}

/// Where an event stands in the order published: its command's sequence, carried over an event
/// its store holds no command for from the event before it; its component; and its place among
/// its component's events of that sequence, from 1.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    sequence: u64,
    component: String,
    ordinal: u64,
}

/// How far one component's store is answered: every event of a write up to `position`, in the
/// store the host held as its `generation`, the last of them at `sequence` and `ordinal` as its
/// [`Key`] counts them; and the id of the store's log of events
/// ([`spindle_kernel::Store::events_id`]), `0` for none (a store that names none, or a cursor of
/// an earlier release, which encodes none).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Mark {
    generation: u64,
    position: u64,
    sequence: u64,
    ordinal: u64,
    log: u64,
}

impl Mark {
    /// A store's start, as a store of `generation` whose log is `log`.
    fn start(generation: u64, log: u64) -> Self {
        Self {
            generation,
            position: 0,
            sequence: 0,
            ordinal: 0,
            log,
        }
    }
}

/// One range of the events published ([`super::Host::published_after`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// The events after the cursor, at most the limit, in the order published.
    pub events: Vec<Published>,
    /// The cursor after the last of them: the cursor given, where none was answered and every
    /// store it marks is the one it was taken from.
    pub next: Cursor,
}

impl Cursor {
    /// The cursor before every event.
    pub fn start() -> Self {
        Self {
            run: 0,
            last: None,
            marks: BTreeMap::new(),
        }
    }

    /// The cursor `text` is, as [`fmt::Display`] writes one.
    ///
    /// # Errors
    /// Why `text` is not a cursor; it does not repeat `text`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let malformed = || "expected a cursor a range of the events answered as `next`".to_owned();
        let bytes = URL_SAFE_NO_PAD.decode(text).map_err(|_| malformed())?;
        let value = serde_json::from_slice::<Value>(&bytes).map_err(|_| malformed())?;
        let cursor = read(&value).ok_or_else(malformed)?;
        if cursor.to_string() != text {
            return Err(malformed());
        }
        Ok(cursor)
    }

    fn encoded(&self) -> Value {
        let last = self.last.as_ref().map_or(Value::Null, |key| {
            json!([key.sequence, key.component, key.ordinal])
        });
        let marks = self
            .marks
            .iter()
            .map(|(component, mark)| {
                let mut encoded = json!([
                    component,
                    mark.generation,
                    mark.position,
                    mark.sequence,
                    mark.ordinal
                ]);
                // A mark with no log id is written as an earlier release wrote it.
                if let (Value::Array(items), true) = (&mut encoded, mark.log != 0) {
                    items.push(json!(mark.log));
                }
                encoded
            })
            .collect::<Vec<_>>();
        json!([VERSION, self.run, last, marks])
    }
}

impl Default for Cursor {
    fn default() -> Self {
        Self::start()
    }
}

impl fmt::Display for Cursor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&URL_SAFE_NO_PAD.encode(self.encoded().to_string()))
    }
}

/// The cursor `value` encodes ([`Cursor::encoded`]); `None` for anything else.
fn read(value: &Value) -> Option<Cursor> {
    let [version, run, last, marks] = value.as_array()?.as_slice() else {
        return None;
    };
    if version.as_u64()? != VERSION {
        return None;
    }
    let last = match last {
        Value::Null => None,
        Value::Array(key) => match key.as_slice() {
            [sequence, component, ordinal] => Some(Key {
                sequence: sequence.as_u64()?,
                component: named(component)?,
                ordinal: ordinal.as_u64()?,
            }),
            _ => return None,
        },
        _ => return None,
    };
    let mut read = BTreeMap::new();
    for mark in marks.as_array()? {
        let (component, generation, position, sequence, ordinal, log) =
            match mark.as_array()?.as_slice() {
                [component, generation, position, sequence, ordinal] => {
                    (component, generation, position, sequence, ordinal, 0)
                }
                [component, generation, position, sequence, ordinal, log] => (
                    component,
                    generation,
                    position,
                    sequence,
                    ordinal,
                    log.as_u64()?,
                ),
                _ => return None,
            };
        read.insert(
            named(component)?,
            Mark {
                generation: generation.as_u64()?,
                position: position.as_u64()?,
                sequence: sequence.as_u64()?,
                ordinal: ordinal.as_u64()?,
                log,
            },
        );
    }
    Some(Cursor {
        run: run.as_u64()?,
        last,
        marks: read,
    })
}

fn named(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

/// One event read, with where it stands.
struct Keyed {
    key: Key,
    position: u64,
    published: Published,
}

/// Whether `keyed` was answered by a range whose last event is `last`.
fn answered(keyed: &Keyed, last: Option<&Key>) -> bool {
    last.is_some_and(|last| keyed.key <= *last)
}

/// `committed`, an event of `component`'s store read after the event whose key carried
/// `sequence` and `ordinal`, with where it stands; `sequence` and `ordinal` become its key's.
fn placed(component: &str, sequence: &mut u64, ordinal: &mut u64, committed: Committed) -> Keyed {
    let command = match committed.origin {
        Some(origin) => {
            if origin.sequence > *sequence {
                *sequence = origin.sequence;
                *ordinal = 0;
            }
            origin.command
        }
        None => String::new(),
    };
    *ordinal = ordinal.saturating_add(1);
    Keyed {
        key: Key {
            sequence: *sequence,
            component: component.to_owned(),
            ordinal: *ordinal,
        },
        position: committed.position,
        published: Published {
            component: component.to_owned(),
            command,
            event: committed.event,
        },
    }
}

/// How a read resumes a cursor of another run ([`resumed`]).
enum Resumed {
    /// The rest of the write the cursor answered in part, if any, and the cursor of this run
    /// after it, which reads on.
    Within(Vec<Published>, Cursor),
    /// The page answered: more of the write the cursor answered in part is left than the limit.
    Part(Page),
}

/// Where a read in `run` resumes `after`, a cursor of another run (taken before a restart, or
/// [`Cursor::start`]), from the stores `stores` names, each by its component, generation and log
/// id: `read` answers a component's events as [`range`]'s does, and `head` a component's head
/// ([`spindle_kernel::Store::events_head`]).
///
/// Each store whose log is the one the cursor marks ([`spindle_kernel::Store::events_id`]) is
/// read on after the position the cursor marks for it, whatever sequences this run's commands
/// take: a store that keeps its writes across a restart (SQLite, PostgreSQL) goes on from the
/// positions it kept. A store of another log (a new memory store, a SQLite file a reset created
/// again) started its positions over and is read from its start. Where the mark or the store
/// names no log (a cursor of an earlier release), a store whose head is before the position is
/// read from its start, and any other after the position. Every event after
/// those positions is after the cursor, but for the write the cursor answered in part: it is read
/// again, its events keyed as the cursor's run keyed them, and those up to the cursor's last are
/// skipped. The cursor of this run starts after that write, with no event answered in this run.
fn resumed(
    (run, limit): (u64, usize),
    stores: &[(String, u64, u64)],
    after: &Cursor,
    read: &mut impl FnMut(&str, u64, usize) -> Result<Vec<Committed>, HostError>,
    head: &mut impl FnMut(&str) -> Result<u64, HostError>,
) -> Result<Resumed, HostError> {
    let mut rest = Vec::new();
    let mut marks = BTreeMap::new();
    for (component, generation, log) in stores {
        let kept = match after.marks.get(component) {
            Some(mark) if mark.position == 0 => Some(*mark),
            // The same log goes on from the positions it kept; another started them over.
            Some(mark) if mark.log != 0 && *log != 0 => (mark.log == *log).then_some(*mark),
            // A mark of an earlier release, or a store that names no log, or none yet.
            Some(mark) if head(component)? >= mark.position => Some(*mark),
            _ => None,
        };
        let mut position = kept.map_or(0, |mark| mark.position);
        if let (Some(mark), Some(last)) = (kept, after.last.as_ref())
            && last.component == *component
            && (mark.sequence, mark.ordinal) < (last.sequence, last.ordinal)
        {
            let (mut sequence, mut ordinal) = (mark.sequence, mark.ordinal);
            let write = read(component, mark.position, 1)?
                .into_iter()
                .map(|committed| placed(component, &mut sequence, &mut ordinal, committed))
                .collect::<Vec<_>>();
            // The write that holds the cursor's last event, as the cursor's run keyed it.
            if write.iter().any(|keyed| keyed.key == *last) {
                let written_at = write.first().map_or(position, |keyed| keyed.position);
                let mut left = write
                    .into_iter()
                    .filter(|keyed| keyed.key > *last)
                    .collect::<Vec<_>>();
                if left.len() > limit {
                    left.truncate(limit);
                    let next = Cursor {
                        run: after.run,
                        last: left
                            .last()
                            .map(|keyed| keyed.key.clone())
                            .or_else(|| after.last.clone()),
                        marks: after.marks.clone(),
                    };
                    return Ok(Resumed::Part(Page {
                        events: left.into_iter().map(|keyed| keyed.published).collect(),
                        next,
                    }));
                }
                position = written_at;
                rest.extend(left.into_iter().map(|keyed| keyed.published));
            }
        }
        marks.insert(
            component.clone(),
            Mark {
                position,
                ..Mark::start(*generation, *log)
            },
        );
    }
    Ok(Resumed::Within(
        rest,
        Cursor {
            run,
            last: None,
            marks,
        },
    ))
}

/// The cursor after the last event published, of the stores `stores` names, each by its
/// component, the generation of the store held for it and its head ([`Store::events_head`]),
/// read once `sequence` ([`super::head_sequence`]) was the sequence of the last command started,
/// and in `run`: every event of an earlier command is at or before its store's head, and is
/// before the cursor; every event after a store's head is after it, whatever its command. The
/// cursor marks each store at its head, so a read with it reads only what was committed since.
/// Where this run replaced a store since, it is read from its start, the events of the commands
/// before `sequence` are skipped, and those of later commands answered; another run reads each
/// store after its head ([`range`]).
///
/// [`Store::events_head`]: spindle_kernel::Store::events_head
pub(super) fn head(run: u64, stores: &[(String, u64, u64, u64)], sequence: u64) -> Cursor {
    let marks = stores
        .iter()
        .map(|(component, generation, log, position)| {
            (
                component.clone(),
                Mark {
                    generation: *generation,
                    position: *position,
                    sequence,
                    ordinal: 0,
                    log: *log,
                },
            )
        })
        .collect();
    // After every event of a sequence below `sequence`, before every event of it and later.
    let last = stores
        .iter()
        .map(|(component, _, _, _)| component)
        .max()
        .map(|component| Key {
            sequence: sequence.saturating_sub(1),
            component: component.clone(),
            ordinal: u64::MAX,
        });
    Cursor { run, last, marks }
}

/// The events after `after`, at most `limit`, of the stores `stores` names, each by its
/// component and the generation of the store held for it, in the order published, read by
/// `read`, which answers a component's events after a position, up to a limit, as
/// [`spindle_kernel::Store::events_after`] does; and the cursor after them. `run` is the host's
/// run.
///
/// Each store is read only until it answered `limit` events after the cursor, or every one it
/// holds: the events a page answers are the first `limit` of those, in the order published, and
/// an event a store was not read as far as follows `limit` of that store's own, so no page of
/// the whole list skips it. A store whose first events after its mark were answered already,
/// in part of a write or before a reset, is read on past them.
///
/// A cursor of another run (taken before a restart) is resumed by store position, whatever
/// sequences this run's commands take ([`resumed`]): `head` answers a component's head
/// ([`spindle_kernel::Store::events_head`]), read only for such a cursor.
///
/// # Errors
/// [`HostError::Cursor`] where `after` names a component `stores` does not, before anything is
/// read; any error of `read` or `head`.
pub(super) fn range(
    run: u64,
    stores: &[(String, u64, u64)],
    after: &Cursor,
    limit: usize,
    mut read: impl FnMut(&str, u64, usize) -> Result<Vec<Committed>, HostError>,
    mut head: impl FnMut(&str) -> Result<u64, HostError>,
) -> Result<Page, HostError> {
    let runs = |name: &str| stores.iter().any(|(component, _, _)| component == name);
    if after
        .marks
        .keys()
        .chain(after.last.iter().map(|key| &key.component))
        .any(|name| !runs(name))
    {
        return Err(HostError::Cursor(
            "the cursor names a component this run does not have".to_owned(),
        ));
    }
    if after.run == run {
        return within(run, stores, after, limit, &mut read);
    }
    match resumed((run, limit), stores, after, &mut read, &mut head)? {
        Resumed::Part(page) => Ok(page),
        Resumed::Within(mut rest, cursor) => {
            let mut page = within(run, stores, &cursor, limit - rest.len(), &mut read)?;
            rest.append(&mut page.events);
            page.events = rest;
            Ok(page)
        }
    }
}

/// [`range`] for `after`, a cursor of this run, `run`.
fn within(
    run: u64,
    stores: &[(String, u64, u64)],
    after: &Cursor,
    limit: usize,
    read: &mut impl FnMut(&str, u64, usize) -> Result<Vec<Committed>, HostError>,
) -> Result<Page, HostError> {
    let mut read_from = Vec::with_capacity(stores.len());
    for (component, generation, log) in stores {
        let start = after
            .marks
            .get(component)
            .filter(|mark| mark.generation == *generation)
            .copied()
            .unwrap_or_else(|| Mark::start(*generation, *log));
        let mut sequence = start.sequence;
        let mut ordinal = start.ordinal;
        let mut keyed: Vec<Keyed> = Vec::new();
        let mut position = start.position;
        let mut fresh = 0;
        while fresh < limit {
            let wanted = limit - fresh;
            let committed = read(component, position, wanted)?;
            let answered_all = committed.len() < wanted;
            for committed in committed {
                let next = placed(component, &mut sequence, &mut ordinal, committed);
                position = next.position;
                if !answered(&next, after.last.as_ref()) {
                    fresh += 1;
                }
                keyed.push(next);
            }
            if answered_all {
                break;
            }
        }
        read_from.push((start, keyed));
    }
    let mut after_cursor = read_from
        .iter()
        .flat_map(|(_, keyed)| keyed.iter())
        .filter(|keyed| !answered(keyed, after.last.as_ref()))
        .collect::<Vec<_>>();
    after_cursor.sort_by(|left, right| left.key.cmp(&right.key));
    after_cursor.truncate(limit);
    let last = after_cursor
        .last()
        .map(|keyed| keyed.key.clone())
        .or_else(|| after.last.clone());
    let events = after_cursor
        .into_iter()
        .map(|keyed| keyed.published.clone())
        .collect();
    let marks = stores
        .iter()
        .zip(&read_from)
        .map(|((component, _, log), (start, keyed))| {
            let done = keyed
                .iter()
                .take_while(|keyed| answered(keyed, last.as_ref()))
                .count();
            // Every event of a write up to the mark is answered: one whose write is answered in
            // part is read again, and skipped as far as it is answered.
            let whole = match keyed.get(done) {
                None => done,
                Some(open) => keyed[..done]
                    .iter()
                    .rposition(|keyed| keyed.position < open.position)
                    .map_or(0, |index| index + 1),
            };
            let mark = whole.checked_sub(1).map_or(
                Mark {
                    log: *log,
                    ..*start
                },
                |index| {
                    let keyed = &keyed[index];
                    Mark {
                        generation: start.generation,
                        position: keyed.position,
                        sequence: keyed.key.sequence,
                        ordinal: keyed.key.ordinal,
                        log: *log,
                    }
                },
            );
            (component.clone(), mark)
        })
        .collect();
    Ok(Page {
        events,
        next: Cursor { run, last, marks },
    })
}

/// The version of a history cursor's encoding.
const HISTORY_VERSION: u64 = 1;

/// How many events one read of a store asks for while a history read filters them.
const HISTORY_READ: usize = 256;

/// Where a page of an instance's history ends ([`super::Host::history`]): opaque text, written
/// with [`fmt::Display`] and read back with [`HistoryCursor::parse`], of the characters
/// `A-Z a-z 0-9 - _` only, as a [`Cursor`] is.
///
/// It holds the position of the last write of the instance's store the page read past, the
/// store it was taken from (the host's run and the store's generation) and the sequence of that
/// write's command. A read from the same store, or from the store of a later run (a restart),
/// reads only after that position, whatever sequences the later run's commands take: a store
/// that keeps its writes across a restart (SQLite, PostgreSQL) goes on from the positions it
/// kept. Where this run replaced the store since (a reset), the store is read from its start and
/// every event of a command whose sequence is not after the cursor's is skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryCursor {
    run: u64,
    generation: u64,
    position: u64,
    sequence: u64,
}

/// One page of an instance's history ([`super::Host::history`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct History {
    /// The events after the cursor, oldest first, at most the limit, read write by write: a
    /// write's events are answered together, so the last write may bring more.
    pub events: Vec<Revision>,
    /// The cursor after the last write read: the cursor given, with nothing left to read.
    pub next: HistoryCursor,
}

/// One event of an instance's history: an event a write of the instance committed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revision {
    /// The position of its write in its store's commit order ([`Committed::position`]).
    pub position: u64,
    /// The civil time of its command, as RFC 3339 in UTC to the millisecond, read from the
    /// sequence the host attributed the command with ([`spindle_kernel::Origin::sequence`]);
    /// `None` for a write attributed to no command.
    pub at: Option<String>,
    /// The command, by qualified name; `None` for a write attributed to none.
    pub command: Option<String>,
    /// The outcome the command took ([`Committed::outcome`]).
    pub outcome: Option<String>,
    /// What caused the command ([`spindle_kernel::Origin::cause`]): its caller.
    pub cause: Option<Provenance>,
    /// The event and its payload.
    pub event: Event,
}

impl HistoryCursor {
    /// The cursor before every event.
    pub fn start() -> Self {
        Self {
            run: 0,
            generation: 0,
            position: 0,
            sequence: 0,
        }
    }

    /// The cursor `text` is, as [`fmt::Display`] writes one.
    ///
    /// # Errors
    /// Why `text` is not a cursor; it does not repeat `text`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let malformed = || "expected a cursor a history read answered as `next`".to_owned();
        let bytes = URL_SAFE_NO_PAD.decode(text).map_err(|_| malformed())?;
        let value = serde_json::from_slice::<Value>(&bytes).map_err(|_| malformed())?;
        let numbers = value
            .as_array()
            .map(|items| items.iter().map(Value::as_u64).collect::<Option<Vec<_>>>())
            .ok_or_else(malformed)?
            .ok_or_else(malformed)?;
        let [HISTORY_VERSION, run, generation, position, sequence] = numbers.as_slice() else {
            return Err(malformed());
        };
        let cursor = Self {
            run: *run,
            generation: *generation,
            position: *position,
            sequence: *sequence,
        };
        if cursor.to_string() != text {
            return Err(malformed());
        }
        Ok(cursor)
    }
}

impl Default for HistoryCursor {
    fn default() -> Self {
        Self::start()
    }
}

impl fmt::Display for HistoryCursor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let encoded = json!([
            HISTORY_VERSION,
            self.run,
            self.generation,
            self.position,
            self.sequence
        ]);
        formatter.write_str(&URL_SAFE_NO_PAD.encode(encoded.to_string()))
    }
}

/// The history of the instance of `entity` with `identity` after `after`, at most `limit` events
/// (write by write), read from the store that holds it, of `generation` in `run`, by `read`,
/// which answers the store's events after a position, up to a limit, as
/// [`spindle_kernel::Store::events_after`] does: every event of every write that names the
/// instance among those it wrote ([`Committed::instances`]), oldest first; and the cursor after
/// the last write read. `at` turns a command's sequence into its time.
///
/// # Errors
/// Any error of `read`.
pub(super) fn history(
    (run, generation): (u64, u64),
    after: &HistoryCursor,
    (entity, identity): (&str, &Value),
    limit: usize,
    at: impl Fn(u64) -> Option<String>,
    mut read: impl FnMut(u64, usize) -> Result<Vec<Committed>, HostError>,
) -> Result<History, HostError> {
    let same_run = after.run == run;
    // Read after the cursor's position from the store it marks, and from a store of another run
    // (a restart), whose positions go on from the ones it kept: its commands' sequences may repeat
    // the earlier run's on a fixed clock. A store this run replaced (a reset) is read from its start,
    // skipping what the cursor answered by its command's sequence, which grows within a run.
    let (mut position, skipped) = if !same_run || after.generation == generation {
        (after.position, 0)
    } else {
        (0, after.sequence)
    };
    let mut sequence = if same_run { after.sequence } else { 0 };
    let mut events = Vec::new();
    'read: while events.len() < limit {
        let committed = read(position, HISTORY_READ)?;
        let answered_all = committed.len() < HISTORY_READ;
        let mut writes = committed.into_iter().peekable();
        while let Some(first) = writes.next() {
            let mut write = vec![first];
            while let Some(next) = writes.next_if(|next| next.position == write[0].position) {
                write.push(next);
            }
            let written_at = write[0].position;
            let origin = write[0].origin.clone();
            let command_sequence = origin.as_ref().map(|origin| origin.sequence);
            let wrote = write[0]
                .instances
                .iter()
                .any(|(written, held)| written == entity && held == identity);
            if wrote && command_sequence.is_none_or(|sequence| sequence > skipped) {
                events.extend(write.into_iter().map(|committed| Revision {
                    position: committed.position,
                    at: command_sequence.and_then(&at),
                    command: origin.as_ref().map(|origin| origin.command.clone()),
                    outcome: committed.outcome,
                    cause: origin.as_ref().and_then(|origin| origin.cause.clone()),
                    event: committed.event,
                }));
            }
            if let Some(command_sequence) = command_sequence {
                sequence = sequence.max(command_sequence);
            }
            position = written_at;
            if events.len() >= limit {
                break 'read;
            }
        }
        if answered_all {
            break;
        }
    }
    Ok(History {
        events,
        next: HistoryCursor {
            run,
            generation,
            position,
            sequence,
        },
    })
}
