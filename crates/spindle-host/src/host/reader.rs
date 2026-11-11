//! What reads a host's committed state beside the host's thread ([`Reader`]).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use ess_compiler::EssIr;
use ess_compiler::ir::ResolvedView;
use serde_json::{Map, Value};
use spindle_kernel::StoreError;
use spindle_telemetry::Counters;
use spindle_timer::Clock;

use super::rows::Shared;
use super::{Cursor, HostError, Page, Published, listing, owner_of, readable};
use crate::admission::Placements;

/// What reads a host's committed state from any thread while the host's own thread runs
/// commands, bindings and ticks ([`super::Host::reader`]): the views of every component whose
/// store gives a reading handle ([`spindle_kernel::Store::reading`]), the events published, read
/// from those stores alike, the rows each entity holds, the counters and the events each command
/// appended.
///
/// A view is read through its store's reading handle, so it answers the store as the last write
/// that landed left it, never part of a write; a read never waits for the host's thread, and the
/// host's thread never waits for a read. A reset hands its readers the emptied stores only once
/// every store is emptied and its world loaded: until then they read the stores as they were.
/// A store the host was given (PostgreSQL) is emptied in place, so it cannot be read as it was:
/// a reset waits for the reads of it already running, and reads of it wait for the reset.
/// Cloning it is cheap, and every clone reads the same host.
#[derive(Clone)]
pub struct Reader {
    pub(super) ir: Arc<EssIr>,
    pub(super) clock: Arc<dyn Clock>,
    pub(super) placements: Arc<Placements>,
    pub(super) components: Arc<BTreeMap<String, Read>>,
    pub(super) emitted: Arc<Mutex<BTreeMap<(String, String), u64>>>,
    /// The host's run ([`super::Cursor`]).
    pub(super) run: u64,
}

/// What a [`Reader`] reads of one component.
pub(super) struct Read {
    pub(super) counters: Arc<Counters>,
    pub(super) shared: Arc<Shared>,
}

impl std::fmt::Debug for Reader {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Reader")
            .field("components", &self.components.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl Reader {
    /// The compiled specification the host runs.
    pub fn ir(&self) -> &EssIr {
        &self.ir
    }

    /// What admission bound for each component.
    pub fn placements(&self) -> &Placements {
        &self.placements
    }

    /// The counters of `component`, as [`super::Host::counters`] reads them.
    pub fn counters(&self, component: &str) -> Option<&Counters> {
        self.components
            .get(component)
            .map(|read| read.counters.as_ref())
    }

    /// The rows each entity holds now, as [`super::Host::rows`] answers them: kept as each write
    /// lands, and counted through the store's reading handle only where that count failed.
    ///
    /// # Errors
    /// [`HostError::Store`] when the rows had to be counted and the store could not be read, or
    /// gives no reading handle.
    pub fn rows(&self) -> Result<Vec<(String, usize)>, HostError> {
        let mut counted = BTreeMap::new();
        let mut rows = Vec::new();
        for entity in self.ir.entities().values() {
            let Some((owner, read)) = self
                .ir
                .components()
                .values()
                .find(|component| component.owns.contains(&entity.domain))
                .and_then(|owner| {
                    let name = owner.name.as_str();
                    self.components.get(name).map(|read| (name, read))
                })
            else {
                continue;
            };
            if !counted.contains_key(owner) {
                let held = match read.shared.kept() {
                    Some(held) => held,
                    None => read
                        .shared
                        .reading()
                        .ok_or_else(|| {
                            StoreError::Backend(format!(
                                "the rows of {owner} could not be counted, and its store is read \
                                 on the host's thread only"
                            ))
                        })?
                        .counts(&read.shared.entities)?,
                };
                counted.insert(owner, held);
            }
            let name = entity.name.to_string();
            let held = counted
                .get(owner)
                .and_then(|held| held.get(&name))
                .copied()
                .unwrap_or_default();
            rows.push((name, held));
        }
        Ok(rows)
    }

    /// How many events each command appended, as [`super::Host::emitted`] answers them.
    pub fn emitted(&self) -> BTreeMap<(String, String), u64> {
        self.emitted
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Every event published, as [`super::Host::published`] answers them: read from each
    /// component's store through its reading handle, beside the host's thread.
    ///
    /// # Errors
    /// [`HostError::Unsupported`] where a component's store gives no reading handle
    /// ([`Reader::reads_events`]), and [`HostError::Store`] where a store cannot be read.
    pub fn published(&self) -> Result<Vec<Published>, HostError> {
        let mut committed = Vec::with_capacity(self.components.len());
        for (name, read) in self.components.iter() {
            let reading = read.shared.reading().ok_or_else(|| {
                HostError::Unsupported(format!("{name} is read on the host's thread only"))
            })?;
            committed.push((name.clone(), reading.events_after(0, usize::MAX)?));
        }
        Ok(super::merged(committed))
    }

    /// The events published after `after`, at most `limit`, as
    /// [`super::Host::published_after`] answers them: read from each component's store through
    /// its reading handle, beside the host's thread, only after the position the cursor marks.
    ///
    /// # Errors
    /// [`HostError::Unsupported`] where a component's store gives no reading handle
    /// ([`Reader::reads_events`]), [`HostError::Cursor`] where `after` names a component the host
    /// does not run, and [`HostError::Store`] where a store cannot be read.
    pub fn published_after(&self, after: &Cursor, limit: usize) -> Result<Page, HostError> {
        let mut stores = Vec::with_capacity(self.components.len());
        let mut handles = BTreeMap::new();
        for (name, read) in self.components.iter() {
            // The generation and the handle are taken together: a reset hands its readers both
            // at once.
            let (generation, reading) = read.shared.handed();
            let reading = reading.ok_or_else(|| {
                HostError::Unsupported(format!("{name} is read on the host's thread only"))
            })?;
            stores.push((name.clone(), generation));
            handles.insert(name.as_str(), reading);
        }
        super::range::range(self.run, &stores, after, limit, |name, position, wanted| {
            let reading = handles
                .get(name)
                .ok_or_else(|| HostError::Unsupported(format!("{name} is not running")))?;
            Ok(reading.events_after(position, wanted)?)
        })
    }

    /// Whether this reader answers the events published ([`Reader::published`]): whether every
    /// component's store gives a reading handle now. Where one does not, they are read on the
    /// host's thread.
    pub fn reads_events(&self) -> bool {
        self.components
            .values()
            .all(|read| read.shared.reading().is_some())
    }

    /// Whether this reader answers `view`: one the host reads ([`crate::reads`]) whose owning
    /// component's store gives a reading handle now, and, where the view declares fields read
    /// through a reference, so does the store of each component owning an entity a reference
    /// names. A view it does not answer is read on the host's thread.
    pub fn reads(&self, view: &str) -> bool {
        readable(&self.ir, view).is_ok_and(|(declared, owner)| {
            read_from(&self.ir, declared, owner).is_ok_and(|read| {
                read.keys().all(|component| {
                    self.components
                        .get(component)
                        .is_some_and(|read| read.shared.reading().is_some())
                })
            })
        })
    }

    /// Every row `view` holds: [`Reader::list_with`] with no parameter values.
    ///
    /// # Errors
    /// As [`Reader::list_with`].
    pub fn list(&self, view: &str) -> Result<Vec<Map<String, Value>>, HostError> {
        self.list_with(view, &Map::new())
    }

    /// Every row `view` holds for `parameters`, as [`super::Host::list_with`] answers them, read
    /// through the reading handle of the store of the component that owns the view's source.
    ///
    /// A view declaring fields read through a reference reads one position of each store it
    /// reads ([`spindle_kernel::Reading::at`]): the source's instances are scanned, and each row
    /// a reference names loaded by its identity, from the same position of its store, so a read
    /// sees each write group whole or not at all. Each component's writes land in its own store,
    /// so the rows of two components are each as some write left them. Where a store cannot take
    /// its entities at once (PostgreSQL) and one of them moved while it was read, the view is
    /// read again from a new position, up to [`POSITIONS`] times.
    ///
    /// # Errors
    /// As [`super::Host::list_with`]; [`HostError::Unsupported`] where a store it reads gives no
    /// reading handle ([`Reader::reads`]), before anything is read; [`HostError::Store`] where a
    /// store moved under each of [`POSITIONS`] reads.
    pub fn list_with(
        &self,
        view: &str,
        parameters: &Map<String, Value>,
    ) -> Result<Vec<Map<String, Value>>, HostError> {
        let listing = listing(&self.ir, self.clock.as_ref(), view, parameters)?;
        let read = read_from(&self.ir, listing.declared, listing.owner.clone())?;
        let readings = read
            .into_iter()
            .map(|(component, entities)| {
                let reading = self
                    .components
                    .get(&component)
                    .and_then(|read| read.shared.reading())
                    .ok_or_else(|| {
                        HostError::Unsupported(format!(
                            "{component} is read on the host's thread only"
                        ))
                    })?;
                Ok((component, (reading, entities)))
            })
            .collect::<Result<BTreeMap<_, _>, HostError>>()?;
        if !crate::view::reads_related(listing.declared) {
            let (reading, _) = &readings[&listing.owner];
            let scan = reading.scan(&listing.entity)?;
            return listing.rows(&self.ir, view, scan, &mut |_, entity, _| {
                Err(HostError::Unsupported(format!(
                    "view {view} reads no field through a reference, yet {entity} was loaded"
                )))
            });
        }
        for _ in 0..POSITIONS {
            // Taken in the order of the components' names, the order a reset closes the gates of
            // stores it empties in place: a read holding one position never waits at a gate a
            // reset closed while that reset waits for the read.
            let positions = readings
                .iter()
                .map(|(component, (reading, entities))| {
                    Ok((component.as_str(), reading.at(entities)?))
                })
                .collect::<Result<BTreeMap<_, _>, StoreError>>()?;
            let scan = positions[listing.owner.as_str()].scan(&listing.entity)?;
            let rows = listing.rows(&self.ir, view, scan, &mut |component, entity, identity| {
                let position = positions.get(component).ok_or_else(|| {
                    HostError::Unsupported(format!("{component} is not read for view {view}"))
                })?;
                Ok(position.load(entity, identity)?)
            })?;
            let mut held = true;
            for position in positions.values() {
                held = held && position.holds()?;
            }
            if held {
                return Ok(rows);
            }
        }
        Err(StoreError::Backend(format!(
            "view {view}: a store it reads moved while each of {POSITIONS} reads of it ran"
        ))
        .into())
    }
}

/// How many positions [`Reader::list_with`] reads a view with related fields from at most, where
/// a store moved while a read of it ran.
const POSITIONS: usize = 8;

/// The entities a read of `declared`, whose source `owner` owns, reads, by the component that
/// owns each: the source, and every entity a field of it reads through a reference names.
///
/// # Errors
/// [`HostError::Unsupported`] where no component owns a referenced entity.
fn read_from(
    ir: &EssIr,
    declared: &ResolvedView,
    owner: String,
) -> Result<BTreeMap<String, Vec<String>>, HostError> {
    let mut read = BTreeMap::from([(owner, vec![ir.entity(&declared.source).name.to_string()])]);
    for reference in declared
        .references
        .iter()
        .filter(|reference| !reference.fields.is_empty())
    {
        let entity = ir.entity(&reference.entity);
        let entities = read.entry(owner_of(ir, entity)?).or_default();
        let name = entity.name.to_string();
        if !entities.contains(&name) {
            entities.push(name);
        }
    }
    Ok(read)
}
