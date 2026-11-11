//! The events published, read as a range after a cursor ([`Cursor`], [`Page`]): each
//! component's store is read only after the position the cursor marks for it, so a read with the
//! cursor at the end reads no event, however long the log is.

use std::collections::BTreeMap;
use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::{Value, json};
use spindle_kernel::Committed;

use super::{HostError, Published};

/// The version of a cursor's encoding.
const VERSION: u64 = 1;

/// Where a range of the events published ends ([`super::Host::published_after`]): opaque text,
/// written with [`fmt::Display`] and read back with [`Cursor::parse`], of the characters
/// `A-Z a-z 0-9 - _` only, so a query string carries it unescaped.
///
/// It holds the last event answered, by the sequence its command was attributed with, its
/// component and its place among that command's events in that component's store: what the
/// whole list orders the events by ([`super::Host::published`]). An event after it in that
/// order is after the cursor, whichever process answered it; so a cursor taken before a restart
/// or a reset still resumes after its last event, as long as the civil time does not go back
/// between two processes (the sequences are civil time).
///
/// It also marks, for each component, the position of its store up to which every event is
/// answered, and the run of the host and the store it was taken from. A read from the same store
/// of the same run reads only after that position. Where the store was replaced since (a reset
/// opens an empty memory or SQLite store, whose positions start over) or the host was opened
/// again, the store is read from its start once and its events up to the last one answered are
/// skipped; the cursor answered then marks the store as it is now.
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
/// [`Key`] counts them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Mark {
    generation: u64,
    position: u64,
    sequence: u64,
    ordinal: u64,
}

impl Mark {
    /// A store's start, as a store of `generation`.
    fn start(generation: u64) -> Self {
        Self {
            generation,
            position: 0,
            sequence: 0,
            ordinal: 0,
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
                json!([
                    component,
                    mark.generation,
                    mark.position,
                    mark.sequence,
                    mark.ordinal
                ])
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
        let [component, generation, position, sequence, ordinal] = mark.as_array()?.as_slice()
        else {
            return None;
        };
        read.insert(
            named(component)?,
            Mark {
                generation: generation.as_u64()?,
                position: position.as_u64()?,
                sequence: sequence.as_u64()?,
                ordinal: ordinal.as_u64()?,
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

/// The cursor after the last event published, of the stores `stores` names, each by its
/// component, the generation of the store held for it and its head ([`Store::events_head`]),
/// read once `sequence` ([`super::head_sequence`]) was the sequence of the last command started,
/// and in `run`: every event of an earlier command is at or before its store's head, and is
/// before the cursor; every event after a store's head is after it, whatever its command. The
/// cursor marks each store at its head, so a read with it reads only what was committed since.
/// Where a store is read from its start (another run, a store replaced), the events of the
/// commands before `sequence` are skipped, and those of later commands answered.
///
/// [`Store::events_head`]: spindle_kernel::Store::events_head
pub(super) fn head(run: u64, stores: &[(String, u64, u64)], sequence: u64) -> Cursor {
    let marks = stores
        .iter()
        .map(|(component, generation, position)| {
            (
                component.clone(),
                Mark {
                    generation: *generation,
                    position: *position,
                    sequence,
                    ordinal: 0,
                },
            )
        })
        .collect();
    // After every event of a sequence below `sequence`, before every event of it and later.
    let last = stores
        .iter()
        .map(|(component, _, _)| component)
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
/// in part of a write or before a restart, is read on past them.
///
/// # Errors
/// [`HostError::Cursor`] where `after` names a component `stores` does not, before anything is
/// read; any error of `read`.
pub(super) fn range(
    run: u64,
    stores: &[(String, u64)],
    after: &Cursor,
    limit: usize,
    mut read: impl FnMut(&str, u64, usize) -> Result<Vec<Committed>, HostError>,
) -> Result<Page, HostError> {
    let runs = |name: &str| stores.iter().any(|(component, _)| component == name);
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
    let mut read_from = Vec::with_capacity(stores.len());
    for (component, generation) in stores {
        let start = after
            .marks
            .get(component)
            .filter(|mark| after.run == run && mark.generation == *generation)
            .copied()
            .unwrap_or_else(|| Mark::start(*generation));
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
                let command = match committed.origin {
                    Some(origin) => {
                        if origin.sequence > sequence {
                            sequence = origin.sequence;
                            ordinal = 0;
                        }
                        origin.command
                    }
                    None => String::new(),
                };
                ordinal = ordinal.saturating_add(1);
                position = committed.position;
                let next = Keyed {
                    key: Key {
                        sequence,
                        component: component.clone(),
                        ordinal,
                    },
                    position,
                    published: Published {
                        component: component.clone(),
                        command,
                        event: committed.event,
                    },
                };
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
        .map(|((component, _), (start, keyed))| {
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
            let mark = whole.checked_sub(1).map_or(*start, |index| {
                let keyed = &keyed[index];
                Mark {
                    generation: start.generation,
                    position: keyed.position,
                    sequence: keyed.key.sequence,
                    ordinal: keyed.key.ordinal,
                }
            });
            (component.clone(), mark)
        })
        .collect();
    Ok(Page {
        events,
        next: Cursor { run, last, marks },
    })
}
