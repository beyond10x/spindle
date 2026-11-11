//! What reads a host's committed state beside the host's thread ([`Reader`]).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use ess_compiler::EssIr;
use serde_json::{Map, Value};
use spindle_kernel::StoreError;
use spindle_telemetry::Counters;
use spindle_timer::Clock;

use super::rows::Shared;
use super::{HostError, Published, listing, readable};
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
            committed.push((name.clone(), reading.events_after(0)?));
        }
        Ok(super::merged(committed))
    }

    /// Whether this reader answers the events published ([`Reader::published`]): whether every
    /// component's store gives a reading handle now. Where one does not, they are read on the
    /// host's thread.
    pub fn reads_events(&self) -> bool {
        self.components
            .values()
            .all(|read| read.shared.reading().is_some())
    }

    /// Whether this reader answers `view`: one the host reads ([`crate::reads`]), whose owning
    /// component's store gives a reading handle now, and that declares no field read through a
    /// reference: such a field reads the row its reference names by its identity
    /// ([`spindle_kernel::Store::load`]), which a reading handle does not answer. A view it does
    /// not answer is read on the host's thread.
    pub fn reads(&self, view: &str) -> bool {
        readable(&self.ir, view).is_ok_and(|(declared, owner)| {
            !crate::view::reads_related(declared)
                && self
                    .components
                    .get(&owner)
                    .is_some_and(|read| read.shared.reading().is_some())
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
    /// # Errors
    /// As [`super::Host::list_with`]; [`HostError::Unsupported`] where that store gives no
    /// reading handle, or the view declares a field read through a reference
    /// ([`Reader::reads`]), before anything is read.
    pub fn list_with(
        &self,
        view: &str,
        parameters: &Map<String, Value>,
    ) -> Result<Vec<Map<String, Value>>, HostError> {
        let listing = listing(&self.ir, self.clock.as_ref(), view, parameters)?;
        if crate::view::reads_related(listing.declared) {
            return Err(HostError::Unsupported(format!(
                "view {view} reads fields through a reference, read on the host's thread only"
            )));
        }
        let reading = self
            .components
            .get(&listing.owner)
            .and_then(|read| read.shared.reading())
            .ok_or_else(|| {
                HostError::Unsupported(format!(
                    "{} is read on the host's thread only",
                    listing.owner
                ))
            })?;
        let scan = reading.scan(&listing.entity)?;
        listing.rows(&self.ir, view, scan, None)
    }
}
