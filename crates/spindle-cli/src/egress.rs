//! `[egress]`: a served `run` posts each published event the environment binds to an
//! `[http.<name>]` call to that call, at least once, in commit order per component store, and
//! resumes after a restart (`story:event-egress-webhook`).
//!
//! Each POST carries one event as a `spindle-egress/1` JSON object ([`FORMAT`]): `format`,
//! `event` (the qualified name), `component`, `position` (the event's store position, its own
//! place in the component's log, rising strictly from one event to the next, the events of one
//! write included: [`spindle_kernel::Reading::events_placed_after`]) and `fields` (the event's
//! fields as published). The call is made as an external cause's is ([`Call`]): its URL, timeout,
//! secret and `ca_file`, over verified TLS or `http://` on loopback only, a post decided on the
//! status alone ([`Call::post`]).
//!
//! One lane per component and call reads the component's committed events through a reading
//! handle of its own ([`spindle_kernel::Reading::events_placed_after`]), on a thread of its
//! own, and posts those bound to its call in order. A post that fails, or that is answered
//! anything but `2xx`, is sent again after a pause that doubles from [`FIRST_PAUSE`] up to
//! [`BACKOFF_CAP`], without end, and the lane's later events wait for it. A stopped lane
//! leaves after the post under way. Once every bound event of a write was answered `2xx`, the
//! write's position is the lane's cursor: the lane hands it to the run's
//! thread, which saves it in the component's own log ([`spindle_kernel::Store::egress_posted`],
//! a reserved `spindle:log` stream that is no published event) between two exchanges, and once
//! more as the run ends. A restart reads it back and posts the events after it, so an event
//! posted but whose cursor was not saved yet (a SIGKILL) is posted again.
//!
//! The cursor is kept in the store the component's state is kept in, so an egress from a
//! component kept in `memory` is refused at start. A reset (`POST /api/reset`) stops the lanes
//! before it empties the stores, and starts them again on the emptied stores after it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use spindle_capability::{Capability, Environment};
use spindle_kernel::{Committed, Database, PostgresStore, Reading, SqliteStore, Store};
use spindle_spec::EssIr;

use crate::calls::Call;
use crate::log;

/// The format of the body each event is posted in.
pub(crate) const FORMAT: &str = "spindle-egress/1";

/// The pause before a failed post is sent again the first time.
const FIRST_PAUSE: Duration = Duration::from_millis(100);

/// The longest pause between two posts of the same event.
const BACKOFF_CAP: Duration = Duration::from_secs(5);

/// How long a lane that read no new event waits before it reads again.
const IDLE: Duration = Duration::from_millis(100);

/// How many events a lane reads at once, write by write.
const PAGE: usize = 64;

/// How often a waiting lane looks whether it is stopped.
const STEP: Duration = Duration::from_millis(20);

/// Where a component's events, and its cursor, are kept.
enum Kept {
    /// The SQLite file `<component>.sqlite` in the run's state directory.
    Sqlite,
    /// The component's tables of the database the `database_*` settings name, one of `sharing`
    /// stores of the run on it.
    Postgres(Database, usize),
}

/// The events of one component posted to one call.
struct Lane {
    component: String,
    call: String,
    events: BTreeSet<String>,
    sent: Arc<Call>,
}

/// What a run's `[egress]` posts, each call resolved: read before the run opens anything.
pub(crate) struct Plan {
    lanes: Vec<Lane>,
    kept: BTreeMap<String, Kept>,
}

/// The egress of `environment` for `ir`: `Ok(None)` where it binds no event, `Ok(Some(Err))`
/// naming why it is refused (an event no component publishes, or one a component kept in
/// `memory` publishes), and each call resolved as an external cause's is.
///
/// # Errors
/// A call's URL or secret, or a `database_*` setting, that the run cannot use.
pub(crate) fn plan(
    ir: &EssIr,
    environment: &Environment,
) -> Result<Option<std::result::Result<Plan, String>>> {
    if environment.egress.is_empty() {
        return Ok(None);
    }
    let bound = match environment.egress_of(ir) {
        Ok(bound) => bound,
        Err(refused) => return Ok(Some(Err(refused.to_string()))),
    };
    let placements = match spindle_host::admit(ir, environment) {
        Ok(placements) => placements,
        Err(refusal) => return Ok(Some(Err(refusal.to_string()))),
    };
    let store = |component: &str| {
        placements.get(component).and_then(|bindings| {
            let bound = |capability: &str| {
                bindings
                    .iter()
                    .find(|binding| binding.capability.as_str() == capability)
                    .map(|binding| binding.implementation.as_str())
            };
            bound(Capability::STORE_DURABLE).or_else(|| bound("store.ephemeral"))
        })
    };
    let sharing = placements
        .keys()
        .filter(|component| store(component) == Some(crate::database::POSTGRES))
        .count();
    let mut kept = BTreeMap::new();
    let mut lanes: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    for egress in bound {
        let component = egress.component.as_str();
        if !kept.contains_key(component) {
            let place = match store(component) {
                Some("sqlite") => Kept::Sqlite,
                Some(crate::database::POSTGRES) => {
                    Kept::Postgres(crate::database::database()?, sharing)
                }
                other => {
                    return Ok(Some(Err(format!(
                        "[egress] posts {}, which {component} publishes, but {component} keeps \
                         its state in {}: an egress keeps its cursor in the component's store, \
                         so the component's state must be kept in sqlite or postgres",
                        egress.event,
                        other.unwrap_or("memory")
                    ))));
                }
            };
            kept.insert(component.to_owned(), place);
        }
        lanes
            .entry((egress.component, egress.call))
            .or_default()
            .insert(egress.event);
    }
    let mut platform = None;
    let mut calls: BTreeMap<String, Arc<Call>> = BTreeMap::new();
    let mut planned = Vec::with_capacity(lanes.len());
    for ((component, call), events) in lanes {
        let sent = match calls.get(&call) {
            Some(sent) => Arc::clone(sent),
            None => {
                let declared = environment
                    .http
                    .get(&call)
                    .ok_or_else(|| anyhow!("[egress] names the call `{call}`, which is absent"))?;
                let sent = Arc::new(Call::new(&call, declared, &mut platform)?);
                calls.insert(call.clone(), Arc::clone(&sent));
                sent
            }
        };
        planned.push(Lane {
            component,
            call,
            events,
            sent,
        });
    }
    Ok(Some(Ok(Plan {
        lanes: planned,
        kept,
    })))
}

/// A write whose bound events a lane posted: its position, the lane's cursor from now on.
struct Posted {
    component: String,
    call: String,
    position: u64,
}

/// The lanes under way, and the cursors they hand back.
struct Running {
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
    posted: mpsc::Receiver<Posted>,
}

/// The egress of a served run: a handle on each component's store, which reads its events for
/// the lanes and writes its cursors on the run's thread, and the lanes. Dropping it stops the
/// lanes and saves the cursors they handed back.
pub(crate) struct Egress {
    plan: Plan,
    /// The run's state directory, where each SQLite file is.
    state: PathBuf,
    handles: BTreeMap<String, Box<dyn Store>>,
    running: Option<Running>,
    /// Stopped for a reset: started again by the next [`Egress::tend`].
    paused: bool,
}

impl Egress {
    /// Opens a handle on each component's store, in `state` where it is a SQLite file, reads each
    /// lane's cursor and starts the lanes. The host has opened the stores before.
    ///
    /// # Errors
    /// A store that cannot be opened, read or read from another thread.
    pub(crate) fn start(plan: Plan, state: &Path) -> Result<Self> {
        let mut egress = Self {
            plan,
            state: state.to_owned(),
            handles: BTreeMap::new(),
            running: None,
            paused: false,
        };
        egress.launch()?;
        for lane in &egress.plan.lanes {
            let events: Vec<&str> = lane.events.iter().map(String::as_str).collect();
            log::info(format!(
                "{} of {} are posted to the HTTP call `{}` ([egress])",
                events.join(", "),
                lane.component,
                lane.call
            ))
            .field("component", lane.component.as_str())
            .field("call", lane.call.as_str())
            .write();
        }
        Ok(egress)
    }

    /// Opens the handles and starts a lane per component and call after its saved cursor.
    fn launch(&mut self) -> Result<()> {
        for (component, kept) in &self.plan.kept {
            let store: Box<dyn Store> =
                match kept {
                    Kept::Sqlite => {
                        let file = self.state.join(format!("{component}.sqlite"));
                        Box::new(SqliteStore::open(&file).with_context(|| {
                            format!("opening {} for its egress", file.display())
                        })?)
                    }
                    Kept::Postgres(database, sharing) => Box::new(
                        PostgresStore::open(database, component, *sharing).with_context(|| {
                            format!("opening the PostgreSQL store of {component} for its egress")
                        })?,
                    ),
                };
            self.handles.insert(component.clone(), store);
        }
        let stop = Arc::new(AtomicBool::new(false));
        let (sender, posted) = mpsc::channel();
        let mut threads = Vec::with_capacity(self.plan.lanes.len());
        for lane in &self.plan.lanes {
            let handle = self
                .handles
                .get_mut(&lane.component)
                .ok_or_else(|| anyhow!("no store of {} was opened", lane.component))?;
            let cursor = handle.egress_position(&lane.call).with_context(|| {
                format!(
                    "reading the egress cursor of {} for `{}`",
                    lane.component, lane.call
                )
            })?;
            let reading = handle.reading().ok_or_else(|| {
                anyhow!(
                    "the store of {} cannot be read beside the run's thread",
                    lane.component
                )
            })?;
            let running = Delivery {
                component: lane.component.clone(),
                call: lane.call.clone(),
                events: lane.events.clone(),
                sent: Arc::clone(&lane.sent),
                reading,
                stop: Arc::clone(&stop),
                posted: sender.clone(),
            };
            let thread = std::thread::Builder::new()
                .name(format!("egress {} {}", lane.component, lane.call))
                .spawn(move || running.deliver(cursor))
                .context("starting an egress thread")?;
            threads.push(thread);
        }
        self.running = Some(Running {
            stop,
            threads,
            posted,
        });
        Ok(())
    }

    /// Saves the cursors the lanes handed back since the last call, on the run's thread, and
    /// starts the lanes again where a reset stopped them.
    pub(crate) fn tend(&mut self) {
        self.save();
        if self.paused {
            self.paused = false;
            if let Err(error) = self.launch() {
                log::error(format!(
                    "the egress did not start again after the reset, so no event is posted until \
                     the run starts again: {error:#}"
                ))
                .write();
                self.halt();
                self.handles.clear();
            }
        }
    }

    /// Stops the lanes and closes the handles before a reset empties the stores; the next
    /// [`Egress::tend`] opens them again.
    pub(crate) fn pause(&mut self) {
        self.halt();
        self.handles.clear();
        self.paused = true;
    }

    /// Stops the lanes, waiting for each to end its post under way, and saves every cursor they
    /// handed back.
    fn halt(&mut self) {
        let Some(running) = self.running.as_mut() else {
            return;
        };
        running.stop.store(true, Ordering::SeqCst);
        for thread in running.threads.drain(..) {
            let _ = thread.join();
        }
        self.save();
        self.running = None;
    }

    /// Writes the last cursor each lane handed back into its component's store.
    fn save(&mut self) {
        let Some(running) = &self.running else {
            return;
        };
        let mut last: BTreeMap<(String, String), u64> = BTreeMap::new();
        for posted in running.posted.try_iter() {
            last.insert((posted.component, posted.call), posted.position);
        }
        for ((component, call), position) in last {
            let Some(handle) = self.handles.get_mut(&component) else {
                continue;
            };
            if let Err(error) = handle.egress_posted(&call, position) {
                log::warn(format!(
                    "the egress cursor of {component} for `{call}` was not saved at {position}, \
                     so its events up to there are posted again after a restart: {error}"
                ))
                .field("component", component.as_str())
                .field("call", call.as_str())
                .write();
            }
        }
    }
}

impl Drop for Egress {
    fn drop(&mut self) {
        self.halt();
    }
}

/// One lane on its own thread.
struct Delivery {
    component: String,
    call: String,
    events: BTreeSet<String>,
    sent: Arc<Call>,
    reading: Arc<dyn Reading>,
    stop: Arc<AtomicBool>,
    posted: mpsc::Sender<Posted>,
}

impl Delivery {
    /// Reads the component's events after `cursor` and posts each bound to the call, in commit
    /// order, until stopped.
    fn deliver(self, mut cursor: u64) {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                log::error(format!(
                    "the egress of {} to `{}` did not start: {error}",
                    self.component, self.call
                ))
                .write();
                return;
            }
        };
        let mut unread = false;
        while !self.stop.load(Ordering::SeqCst) {
            let page = match self.reading.events_placed_after(cursor, PAGE) {
                Ok(page) => {
                    unread = false;
                    page
                }
                Err(error) => {
                    if !unread {
                        log::warn(format!(
                            "the egress of {} to `{}` cannot read the store, and reads it again: \
                             {error}",
                            self.component, self.call
                        ))
                        .write();
                    }
                    unread = true;
                    if !self.wait(IDLE) {
                        return;
                    }
                    continue;
                }
            };
            if page.is_empty() {
                if !self.wait(IDLE) {
                    return;
                }
                continue;
            }
            let mut posted = false;
            for (index, (placed, committed)) in page.iter().enumerate() {
                if self.events.contains(&committed.event.name) {
                    // A stopped lane leaves its page after the post under way: the rest is
                    // posted after the next start, from the cursor saved.
                    if self.stop.load(Ordering::SeqCst) || !self.post(&runtime, *placed, committed)
                    {
                        return;
                    }
                    posted = true;
                }
                let last_of_write = page
                    .get(index + 1)
                    .is_none_or(|(_, next)| next.position != committed.position);
                if last_of_write {
                    cursor = committed.position;
                    if posted {
                        posted = false;
                        let _ = self.posted.send(Posted {
                            component: self.component.clone(),
                            call: self.call.clone(),
                            position: cursor,
                        });
                    }
                }
            }
        }
    }

    /// Posts `committed`, at its own store position `placed`, until the call answers `2xx`;
    /// `false` where the lane was stopped first.
    fn post(&self, runtime: &tokio::runtime::Runtime, placed: u64, committed: &Committed) -> bool {
        let body = serde_json::json!({
            "format": FORMAT,
            "event": committed.event.name,
            "component": self.component,
            "position": placed,
            "fields": committed.event.payload,
        })
        .to_string();
        let mut pause = FIRST_PAUSE;
        loop {
            match self.sent.post(runtime, body.clone()) {
                Ok(()) => return true,
                Err(why) => log::warn(format!(
                    "{} of {} at position {} was not posted: {why}; posting it again in {} ms",
                    committed.event.name,
                    self.component,
                    placed,
                    pause.as_millis()
                ))
                .field("component", self.component.as_str())
                .field("call", self.call.as_str())
                .field("event", committed.event.name.as_str())
                .write(),
            }
            if !self.wait(pause) {
                return false;
            }
            pause = (pause * 2).min(BACKOFF_CAP);
        }
    }

    /// Waits `pause`; `false` where the lane was stopped meanwhile.
    fn wait(&self, pause: Duration) -> bool {
        let mut left = pause;
        while !left.is_zero() {
            if self.stop.load(Ordering::SeqCst) {
                return false;
            }
            let step = left.min(STEP);
            std::thread::sleep(step);
            left -= step;
        }
        !self.stop.load(Ordering::SeqCst)
    }
}
