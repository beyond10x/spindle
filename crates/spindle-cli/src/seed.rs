//! `run --seed <file>`, or the environment document's `[seed] file`: commands a served run submits
//! at every start, after its stores are open and beside the listener (`story:seed-at-start`).
//!
//! The file is one JSON array of entries `{"command", "input", "actor", "caller", "request_id"}`,
//! read and checked against the model before listening ([`load`]): a file that does not parse, or
//! an entry naming a command, input field or actor shape the specification does not declare, stops
//! the run, naming the file and the entry (counted from 1). The entries are then submitted one at
//! a time in the file's order, each as its own actor and caller, on the thread that runs every
//! command, between the requests the listener hands over ([`Seeding::step`]). Each counts once as
//! applied, already there (an outcome the specification declares `existing_instance:`, or a
//! request id an earlier entry of the file carried that was applied or already there, which is not
//! submitted again), refused (a declared refusing outcome) or rejected (a host or execute error:
//! it did not run); either is logged and the next entry is submitted. A seed run that ends having
//! rejected an entry marks the seed [failed](Seeding::failed), and the served run exits 1
//! (`story:run-fails-on-rejected-line`).
//!
//! A request id is remembered in memory, by this start: Spindle keeps no record of request ids
//! across starts or replicas (`story:request-id-idempotency`), and a start that resumes a seed on a
//! `--state` takes those of the entries sent before it from the file itself. An entry whose id only a
//! refused entry carried is submitted again, and counts as what it becomes: the refusal left
//! nothing there, and what refused it may since have changed.
//!
//! On a `--state` directory the run records the seed in [`MARKER`] beside the state's files, `0600`
//! ([`spindle_kernel::private`]): the digest of the seed file, how many of its entries were sent
//! and which of those were rejected, written after each entry through a file renamed over the
//! marker (`story:seed-once-on-durable-state`). A start with the same file sends the rejected
//! entries again, then the entries after those sent, and none once every one is done
//! ([`begin`]); it remembers the request ids of the entries sent before it that were not rejected.
//! One with another file stops before listening, naming both digests, unless the state holds no
//! row, when the file runs from its first entry. A state with no marker gets the whole seed, then
//! the marker. A reset by a run without a seed removes the marker ([`forget`]).
//!
//! A run on PostgreSQL given `database_lease` keeps the same record as the row `seed` of the
//! lease table of its first component in name order, which the lease thread reads and writes on
//! the lease connection, each write landing only while the lease is this run's
//! ([`crate::database::SeedRow`], `story:seed-under-a-lease`). The record is looked for in every
//! current component's lease table and, where none holds one, in the lease tables of components a
//! later release removed, found by table name (`story:seed-record-after-a-component-is-removed`).
//! It decides from that record
//! whatever lease token it took, standing by first or not, as a `--state` run decides from
//! [`MARKER`], with one difference: a store that holds rows and no record (seeded before the
//! record existed, or filled by `state move`) is taken as seeded by this file: the record is
//! written with every entry done and no entry is sent (D5 B). A reset by a leased run without a
//! seed removes the row ([`forget_row`]). Any other store (PostgreSQL without a lease, memory)
//! records nothing, and every start sends the whole seed.
//!
//! A `POST /api/reset` drops the entries of the seed run under way that are not yet sent
//! ([`Seeding::cut`]). One with no world then submits the seed again from its first entry, once
//! every store is empty and before the reset answers, as a start does: no request id remembered,
//! and an end line counting that run alone ([`Seeding::restart`], `story:reset-reapplies-seed`);
//! the marker reads 0 entries done before the first is sent. The counts in the metrics go on
//! across resets and never fall. A reset that gives a world loads that world alone, and the marker
//! reads every entry done ([`Seeding::loaded`]).
//!
//! The state of the seed run under way is published on a [`SeedState`] the served surface reads
//! beside this thread (`story:seed-readiness`): `GET /__spindle/seed` answers it, `running` with
//! the entries left or `ended`, with that run's counts as its end line prints them, and
//! `GET /__spindle/ready` answers `200` only once it reads `ended`. It reads `ended` once the last
//! entry's submission has returned, before the end line is printed, and from the first request
//! where the `--state` marker records every entry done; a reset with no world reads `running` from
//! before it empties the stores until its seed run ends.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Seek, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use ess_compiler::ir::{ResolvedCommand, ResolvedCondition};
use serde_json::{Map, Value};
use spindle_host::{Host, HostError, Report};
use spindle_ingress::{Recorder, SeedCounts, SeedPhase, SeedProgress, SeedState};
use spindle_spec::EssIr;

use crate::log;

/// One entry of a seed file.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    /// The command, by qualified name.
    command: String,
    /// Its input, every required field of it.
    #[serde(default)]
    input: Map<String, Value>,
    /// The actor it is sent as, by qualified name; none for no caller.
    #[serde(default)]
    actor: Option<String>,
    /// The caller's attributes, every required one of the actor's.
    #[serde(default)]
    caller: Option<Map<String, Value>>,
    /// The entry's request id: an entry whose id an earlier applied or already-there entry
    /// carried is not submitted again.
    request_id: String,
}

/// A seed file read and checked against the model.
#[derive(Debug)]
pub(crate) struct Seed {
    path: PathBuf,
    /// `sha256:` and the hexadecimal SHA-256 of the file's bytes.
    digest: String,
    entries: Vec<Entry>,
}

impl Seed {
    /// How many entries it holds.
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// The file it was read from.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

/// Reads the seed file at `path` and checks each entry against `ir` as the kernel would bind it
/// before a command reads anything: a declared command some component accepts, every input field
/// declared and of its type with every required one given, a caller only with an actor, the actor
/// declared and the caller one of its callers. Two entries with one request id must be equal.
///
/// # Errors
/// The first thing wrong, naming the file and, where it is an entry's, the entry by its place in
/// the array, counted from 1, and its request id.
pub(crate) fn load(path: &Path, ir: &EssIr) -> Result<Seed> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading the seed file {}", path.display()))?;
    let document: Value = serde_json::from_str(&text)
        .with_context(|| format!("the seed file {} is not JSON", path.display()))?;
    let Value::Array(values) = document else {
        bail!(
            "the seed file {} is not a JSON array of entries {{\"command\", \"input\", \"actor\", \
             \"caller\", \"request_id\"}}",
            path.display()
        );
    };
    let mut entries: Vec<Entry> = Vec::with_capacity(values.len());
    let mut requests: BTreeMap<String, usize> = BTreeMap::new();
    for (index, value) in values.into_iter().enumerate() {
        let place = index + 1;
        let entry: Entry = serde_json::from_value(value)
            .with_context(|| format!("the seed file {}, entry {place}", path.display()))?;
        checked(ir, &entry).with_context(|| {
            format!(
                "the seed file {}, entry {place} (request id {:?})",
                path.display(),
                entry.request_id
            )
        })?;
        if let Some(first) = requests.get(&entry.request_id) {
            if entries[*first] != entry {
                bail!(
                    "the seed file {}, entry {place}: its request id {:?} is entry {}'s, with \
                     another command, input, actor or caller",
                    path.display(),
                    entry.request_id,
                    first + 1
                );
            }
        } else {
            requests.insert(entry.request_id.clone(), index);
        }
        entries.push(entry);
    }
    Ok(Seed {
        path: path.to_owned(),
        digest: ess_conformance::known_failures::sha256(text.as_bytes()),
        entries,
    })
}

/// The file in a `--state` directory that records the seed applied to that state.
pub(crate) const MARKER: &str = "seed.json";

/// What [`MARKER`] holds: the digest of the seed file applied, how many of its entries, from the
/// first, were sent, and which of those were rejected when last sent, each by its place in the
/// file counted from 1. A rejected entry did not run, so it is not done: a restart sends it again.
#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Recorded {
    digest: String,
    done: usize,
    #[serde(default)]
    rejected: BTreeSet<usize>,
}

/// What the marker at `path` records; `None` where there is none.
///
/// # Errors
/// A marker that cannot be read, or does not hold `{"digest", "done", "rejected"}`.
fn recorded(path: &Path) -> Result<Option<Recorded>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("reading the seed marker {}", path.display()));
        }
    };
    serde_json::from_str(&text).map(Some).with_context(|| {
        format!(
            "the seed marker {} does not hold {{\"digest\", \"done\", \"rejected\"}}",
            path.display()
        )
    })
}

/// Writes `recorded` to the marker at `path`: into a file beside it, created `0600` where absent,
/// then renamed over it, so a write cut short leaves the marker as it was.
fn record(path: &Path, recorded: &Recorded) -> std::io::Result<()> {
    let mut beside = path.as_os_str().to_owned();
    beside.push(".tmp");
    let beside = PathBuf::from(beside);
    let mut file = spindle_kernel::private::open_or_create(&beside)?;
    // A file left by a write cut short holds old text.
    file.set_len(0)?;
    file.rewind()?;
    serde_json::to_writer(&mut file, recorded)?;
    file.write_all(b"\n")?;
    file.flush()?;
    drop(file);
    std::fs::rename(&beside, path)
}

/// Where a run records the seed it applied.
pub(crate) enum Record<'a> {
    /// Nowhere: every start sends the whole seed.
    Nowhere,
    /// [`MARKER`] in the `--state` directory.
    State(&'a Path),
    /// The row `seed` of the store's lease table, on PostgreSQL with `database_lease`.
    Store(crate::database::SeedRow),
}

/// Where the seed's record is kept: a file beside the state, or a row of the store's lease table.
enum Marker {
    File(PathBuf),
    Row(crate::database::SeedRow),
}

impl Marker {
    /// What the record holds; `None` where there is none.
    ///
    /// # Errors
    /// A record that cannot be read, or does not hold `{"digest", "done", "rejected"}`.
    fn recorded(&self) -> Result<Option<Recorded>> {
        match self {
            Self::File(path) => recorded(path),
            Self::Row(row) => row
                .read()
                .with_context(|| format!("reading the seed record in {}", row.table()))?
                .map(|body| {
                    serde_json::from_value(body).with_context(|| {
                        format!(
                            "the row `seed` of {} does not hold {{\"digest\", \"done\", \
                             \"rejected\"}}",
                            row.table()
                        )
                    })
                })
                .transpose(),
        }
    }

    /// Writes `recorded`.
    ///
    /// # Errors
    /// The record cannot be written.
    fn record(&self, recorded: &Recorded) -> Result<()> {
        match self {
            Self::File(path) => Ok(record(path, recorded)?),
            Self::Row(row) => row.write(serde_json::to_value(recorded)?),
        }
    }

    /// What holds the record, as a line names it: the marker's path, or the row of the store.
    fn named(&self) -> String {
        match self {
            Self::File(path) => path.display().to_string(),
            Self::Row(row) => format!("the row `seed` of {}", row.table()),
        }
    }
}

/// Removes the row of every current component's lease table that records a seed, where a leased
/// run without a seed emptied the store or loaded a world (`story:seed-under-a-lease`, D6 A): a
/// later start with a seed treats the store as one with no record. A failure is printed on
/// stderr, naming the table whose row was left.
pub(crate) fn forget_row(row: &crate::database::SeedRow) {
    if let Err(error) = row.forget() {
        log::warn(format!(
            "a seed record could not be removed after the reset, so a start with that seed may \
             skip the entries it records done: {error:#}"
        ))
        .field("error", format!("{error:#}"))
        .write();
    }
}

/// The refusal [`spindle_ingress::console::reset`] gives `body` before it empties any store, for
/// the world it carries or its clock, where it gives one: `400` for a body that is not JSON or not
/// a world of the model, or a clock before the host's; `409` for a clock on a host without a
/// stepping clock. A leased run without a seed checks these before [`claimed`] touches any seed
/// record (`story:seed-record-after-a-component-is-removed`, adversary round 3); the checks of a
/// world's rows against the stores come after it, in the reset.
pub(crate) fn refused_world(
    host: &Host,
    body: &[u8],
    worlds: spindle_ingress::console::Worlds<'_>,
) -> Option<spindle_ingress::Answer> {
    if !spindle_ingress::console::carries_world(body) {
        return None;
    }
    let world = serde_json::from_slice::<Value>(body)
        .map_err(|error| format!("the reset body is not JSON: {error}"))
        .and_then(|value| worlds(host.ir(), &value));
    let at = match world {
        Ok(world) => world.clock?,
        Err(why) => return Some(spindle_ingress::Answer::refusal(400, why)),
    };
    match host.clock_at() {
        Err(error @ HostError::Clock(_)) => {
            Some(spindle_ingress::Answer::refusal(409, error.to_string()))
        }
        Err(error) => Some(spindle_ingress::Answer::refusal(500, error.to_string())),
        Ok(stands) if at < stands => Some(spindle_ingress::Answer::refusal(
            400,
            HostError::World(format!(
                "clock: the clock stands at {}; a world moves it forward only",
                spindle_host::rfc3339(stands).unwrap_or_default()
            ))
            .to_string(),
        )),
        Ok(_) => None,
    }
}

/// Before a reset by a leased run without a seed empties anything, takes the leases of the lease
/// tables of components the specification no longer has that hold a seed record, then removes
/// the records under them in one transaction (`story:seed-under-a-lease`, D6 A;
/// `story:seed-record-after-a-component-is-removed`, adversary rounds 1 to 4): [`forget_row`]
/// removes those of the current tables once the reset emptied the store. Answers how many leases
/// it holds, for [`unclaimed`], or the refusal of the reset where that cannot be done: `409`
/// naming the table whose lease another process holds, as a process of an earlier release still
/// running may, with no record touched; `500` naming the table that could not be read, whose
/// lease could not be taken or whose record could not be removed, with none removed, and also
/// where the lease thread did not answer in time. No store is emptied then, no lease is kept and
/// no record is lost: a claim the lease thread ends after the answer is undone behind it.
pub(crate) fn claimed(row: &crate::database::SeedRow) -> Result<usize, spindle_ingress::Answer> {
    match row.claim() {
        Ok(crate::database::Claim::Taken(tables)) => Ok(tables),
        Ok(crate::database::Claim::Held(table)) => Err(spindle_ingress::Answer::refusal(
            409,
            format!(
                "the reset is refused before any store is emptied: the seed record in {table}, \
                 the lease table of a component this specification no longer has, cannot be \
                 removed, since another process holds that table's lease, as one of an earlier \
                 release still running may; once that process has ended, or the row `seed` of \
                 {table} is removed, reset again"
            ),
        )),
        Err(error) => Err(spindle_ingress::Answer::refusal(
            500,
            format!(
                "the reset is refused before any store is emptied: a seed record in the lease \
                 table of a component this specification no longer has could not be removed: \
                 {error:#}"
            ),
        )),
    }
}

/// Releases the `tables` leases [`claimed`] took once the reset answered `answer`, and answers
/// what the reset answers then. With `restore`, as after a reset that changed no store's rows,
/// whatever it answered ([`spindle_host::Host::reset_changed_rows`]), the records [`claimed`]
/// removed are put back first, in one transaction, so a reset that changed nothing leaves them
/// too, and `answer` stands. Where that transaction failed, the answer keeps its refusal, its
/// status at least `500`, and says the records are lost, naming their tables; where the lease
/// thread did not answer, it says they were not put back as far as the run knows. Without
/// `restore` a failure is printed on stderr, and the leases run out by their time. A claim that
/// took no lease leaves `answer` as it is (adversary round 5).
pub(crate) fn unclaimed(
    row: &crate::database::SeedRow,
    answer: spindle_ingress::Answer,
    restore: bool,
    tables: usize,
) -> spindle_ingress::Answer {
    // A claim that took no lease removed no record: nothing to put back or release, and the
    // answer stands, whether or not the lease thread still runs.
    if tables == 0 {
        return answer;
    }
    let unclaimed = row.unclaim(restore, tables);
    let why = match unclaimed {
        Ok(Ok(())) => return answer,
        Ok(Err(lost)) => format!("is lost: {lost}"),
        Err(error) if restore => format!("was not put back, as far as this run knows: {error:#}"),
        Err(error) => {
            log::warn(format!(
                "the leases taken for the reset were not released; they run out by their time: \
                 {error:#}"
            ))
            .field("error", format!("{error:#}"))
            .write();
            return answer;
        }
    };
    let refused = answer
        .body
        .as_ref()
        .and_then(|body| body.get("refused"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    spindle_ingress::Answer::refusal(
        answer.status.max(500),
        format!(
            "{refused}; the reset changed no store's rows, yet the seed record removed for it from \
             the lease table of a component this specification no longer has {why}; a start with \
             that seed may take the store as seeded and skip its entries, so reset the store \
             before such a start"
        ),
    )
}

/// Removes the marker of the `--state` directory `state`, where a run without a seed emptied the
/// state or loaded a world: a later start with a seed treats the state as one with no marker.
/// A failure is printed on stderr.
pub(crate) fn forget(state: &Path) {
    let marker = state.join(MARKER);
    match std::fs::remove_file(&marker) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => log::warn(format!(
            "the seed marker {} could not be removed after the reset, so a start with that seed \
             may skip the entries it records done: {error}",
            marker.display()
        ))
        .field("error", error.to_string())
        .write(),
    }
}

/// The seeding of a start: `seed`, its counts noted on `recorder` and its state published on
/// `published` ([`Seeding::step`]). Where `record` keeps a record of the seed, a `--state`
/// directory's [`MARKER`] or the row of the store's lease table, that record decides where it
/// starts: with none, or one of another seed file on a state or store that holds no row
/// ([`Host::rows`]), from its first entry; with one of this file, after the entries it records
/// done, and with every one done it prints one line and sends nothing. A store that holds rows and
/// no record is taken as seeded by this file: one line, nothing sent, and the record written with
/// every entry done (`story:seed-under-a-lease`, D5 B). Prints on stderr what it will send, and
/// records where it starts.
///
/// # Errors
/// A record that cannot be read or written, or one of another seed file on a state or store that
/// holds a row: the run stops before it serves a request, naming both digests.
pub(crate) fn begin(
    seed: Seed,
    record: Record<'_>,
    host: &mut Host,
    recorder: Option<Recorder>,
    published: SeedState,
) -> Result<Seeding> {
    // Where the record is, what a line calls the state or store it records, and what kind of
    // place that is.
    let (marker, on, kind) = match record {
        Record::Nowhere => {
            log::info(format!(
                "seed {}: {} entries, submitted one at a time once listening",
                seed.path.display(),
                seed.len()
            ))
            .write();
            return Ok(Seeding::new(
                seed,
                (recorder, published),
                None,
                0,
                BTreeSet::new(),
                false,
            ));
        }
        Record::State(state) => (
            Marker::File(state.join(MARKER)),
            format!("the state {}", state.display()),
            "state",
        ),
        Record::Store(row) => (Marker::Row(row), "the PostgreSQL store".to_owned(), "store"),
    };
    let named = marker.named();
    let recorded = marker.recorded()?;
    // Rows a store holds without any record of a seed (D5 B); a state is not asked.
    let unrecorded = if recorded.is_none() && matches!(marker, Marker::Row(_)) {
        held(host)?
    } else {
        0
    };
    let (from, rejected, ran) = match recorded {
        None if unrecorded > 0 => {
            log::info(format!(
                "seed {} ({}): {on} holds {} rows and {named} records no seed, as a store seeded \
                 before that record was kept, or filled by `state move`, leaves it; the seed is \
                 taken as applied there, so no entry is sent, and the record now holds every \
                 entry done",
                seed.path.display(),
                seed.digest,
                unrecorded
            ))
            .write();
            (seed.len(), BTreeSet::new(), true)
        }
        None => {
            log::info(format!(
                "seed {} ({}): {} entries, submitted one at a time once listening; {named} \
                 records each entry done",
                seed.path.display(),
                seed.digest,
                seed.len(),
            ))
            .write();
            (0, BTreeSet::new(), false)
        }
        Some(recorded) if recorded.digest == seed.digest => {
            let done = recorded.done.min(seed.len());
            // Places counted from 1, as recorded, to indices into the entries.
            let rejected: BTreeSet<usize> = recorded
                .rejected
                .iter()
                .filter(|place| (1..=done).contains(*place))
                .map(|place| place - 1)
                .collect();
            if done == seed.len() && rejected.is_empty() {
                log::info(format!(
                    "seed {} ({}) already ran on {on}, as {named} records: no entry is sent",
                    seed.path.display(),
                    seed.digest,
                ))
                .write();
            } else {
                let places: Vec<String> = rejected
                    .iter()
                    .map(|index| (index + 1).to_string())
                    .collect();
                let again = if places.is_empty() {
                    String::new()
                } else {
                    format!(
                        ", but entries {} were rejected there and are sent again first",
                        places.join(", ")
                    )
                };
                log::info(format!(
                    "seed {} ({}): {} entries, the first {done} sent on {on}, as {named} \
                     records{again}; the {} after them are submitted one at a time once listening",
                    seed.path.display(),
                    seed.digest,
                    seed.len(),
                    seed.len() - done
                ))
                .write();
            }
            let ran = done == seed.len() && rejected.is_empty();
            (done, rejected, ran)
        }
        Some(recorded) => {
            let rows = held(host)?;
            if rows > 0 {
                bail!(
                    "the seed {} is {}, and {on} was seeded from {} ({named}) and holds {rows} \
                     rows: give the seed file it was seeded from, or run on a {kind} with no row",
                    seed.path.display(),
                    seed.digest,
                    recorded.digest,
                );
            }
            log::info(format!(
                "seed {} ({}): {} entries, submitted one at a time once listening; {on} was \
                 seeded from {} and holds no row, so this seed runs from its first entry",
                seed.path.display(),
                seed.digest,
                seed.len(),
                recorded.digest
            ))
            .write();
            (0, BTreeSet::new(), false)
        }
    };
    let seeding = Seeding::new(
        seed,
        (recorder, published),
        Some(marker),
        from,
        rejected,
        ran,
    );
    seeding
        .mark()
        .with_context(|| format!("writing the seed record of {on}"))?;
    Ok(seeding)
}

/// How many rows the stores of `host` hold, every entity's summed.
///
/// # Errors
/// A store that cannot be counted.
fn held(host: &mut Host) -> Result<usize> {
    Ok(host
        .rows()
        .context("counting the rows the state holds")?
        .iter()
        .map(|(_, held)| held)
        .sum())
}

/// Refuses an entry the kernel would refuse before reading anything, or the host would not route.
fn checked(ir: &EssIr, entry: &Entry) -> Result<()> {
    if entry.request_id.is_empty() {
        bail!("request_id is empty");
    }
    let command = declared(ir, &entry.command)?;
    for name in entry.input.keys() {
        if command.input_field(name).is_none() {
            bail!("{} declares no input field `{name}`", entry.command);
        }
    }
    for field in &command.input {
        match entry.input.get(&field.name) {
            Some(value) => spindle_kernel::admits(ir, &field.type_ref, value).map_err(|error| {
                anyhow::anyhow!(
                    "the input field `{}` of {}: {error}",
                    field.name,
                    entry.command
                )
            })?,
            None if field.type_ref.is_optional() => {}
            None => bail!(
                "the input field `{}` of {} is required",
                field.name,
                entry.command
            ),
        }
    }
    match (&entry.actor, &entry.caller) {
        (None, Some(_)) => bail!("a caller is given without an actor"),
        (None, None) => {}
        (Some(actor), caller) => {
            if !ir.actors().keys().any(|name| name.to_string() == *actor) {
                bail!("the specification declares no actor {actor}");
            }
            let caller = caller.clone().unwrap_or_default();
            spindle_kernel::caller_binds(ir, Some(actor), Some(&caller))
                .with_context(|| format!("the caller of the actor {actor}"))?;
        }
    }
    Ok(())
}

/// The command `name` of `ir`, where a component accepts it.
fn declared<'ir>(ir: &'ir EssIr, name: &str) -> Result<&'ir ResolvedCommand> {
    let command = ir
        .commands()
        .iter()
        .find_map(|(declared, command)| (declared.to_string() == name).then_some(command))
        .with_context(|| format!("the specification declares no command {name}"))?;
    let accepted = ir.components().values().any(|component| {
        component
            .accepts
            .iter()
            .any(|accepted| accepted.to_string() == name)
    });
    if !accepted {
        bail!("no component of the specification accepts {name}");
    }
    Ok(command)
}

/// Whether `outcome` of `command` is the branch the specification declares `existing_instance:`.
fn existing(ir: &EssIr, command: &str, outcome: &str) -> bool {
    declared(ir, command).is_ok_and(|command| {
        command.outcomes.iter().any(|declared| {
            declared.name.as_str() == outcome
                && declared.condition == ResolvedCondition::ExistingInstance
        })
    })
}

/// A seed being submitted: the next entry, the request ids of the entries applied or already
/// there so far in this seed run, its counts, and those of the runs before it in this start.
pub(crate) struct Seeding {
    seed: Seed,
    next: usize,
    submitted: BTreeSet<String>,
    counts: SeedCounts,
    /// What the seed runs a reset cut or saw end counted, before the one under way.
    earlier: SeedCounts,
    recorder: Option<Recorder>,
    /// Where the state of the seed run under way is published, which `GET /__spindle/seed` and
    /// `GET /__spindle/ready` read beside this thread (`story:seed-readiness`).
    published: SeedState,
    /// Whether a reset with no world is under way ([`Seeding::resetting`]): nothing is published
    /// until [`Seeding::restart`] or [`Seeding::unreset`], so the run reads running from the reset
    /// until its seed ends again.
    holding: bool,
    ended: bool,
    /// How many entries of the seed run under way did not run: a host or execute error, never a
    /// declared outcome (`story:run-fails-on-rejected-line`).
    rejected: u64,
    /// Whether a seed run of this start ended having rejected an entry: the run then exits 1.
    failed: bool,
    /// Where the run records the seed, where it does: the `--state` directory's [`MARKER`], or
    /// the row of the store's lease table.
    marker: Option<Marker>,
    /// The entries before `next` that a start sends again before `next`, in the file's order:
    /// those the marker recorded rejected.
    retry: Vec<usize>,
    /// The entries before `next` whose last sending was rejected, which the marker records.
    rejected_at: BTreeSet<usize>,
}

/// What became of one entry.
enum Became {
    Applied,
    AlreadyThere,
    /// A declared refusing outcome answered it.
    Refused(String),
    /// It did not run: the host or the kernel refused it before an outcome.
    Rejected(String),
}

impl Seeding {
    /// `seed` about to be submitted from its entry `from` (counted from 0), the entries `rejected`
    /// before it sent again first, its counts noted on the recorder from now on, its state
    /// published on `published`, and the entries done written to `marker`. The request ids of the
    /// entries before `from` that were not rejected are remembered, as if this start had sent
    /// them. With `ran`, every entry was done before this start: the run has ended, reads ended
    /// from the first request and prints no end line.
    fn new(
        seed: Seed,
        (recorder, published): (Option<Recorder>, SeedState),
        marker: Option<Marker>,
        from: usize,
        rejected: BTreeSet<usize>,
        ran: bool,
    ) -> Self {
        let next = from.min(seed.len());
        let submitted = seed.entries[..next]
            .iter()
            .enumerate()
            .filter(|(index, _)| !rejected.contains(index))
            .map(|(_, entry)| entry.request_id.clone())
            .collect();
        let seeding = Self {
            ended: ran,
            seed,
            next,
            submitted,
            counts: SeedCounts::default(),
            earlier: SeedCounts::default(),
            recorder,
            published,
            holding: false,
            rejected: 0,
            failed: false,
            marker,
            retry: rejected.iter().copied().collect(),
            rejected_at: rejected,
        };
        seeding.note();
        seeding
    }

    /// Writes to the marker, where the run has one, the entries done: every entry before the
    /// next, but those whose last sending was rejected.
    fn mark(&self) -> Result<()> {
        let Some(marker) = &self.marker else {
            return Ok(());
        };
        let recorded = Recorded {
            digest: self.seed.digest.clone(),
            done: self.next,
            rejected: self.rejected_at.iter().map(|index| index + 1).collect(),
        };
        marker.record(&recorded)
    }

    /// [`Seeding::mark`], a failure printed on stderr: the run goes on, and a restart may send an
    /// entry again.
    fn marked(&self) {
        if let Err(error) = self.mark() {
            log::warn(format!(
                "seed {}: the marker {} could not be written, so a restart may send the entries \
                 done since the last one written again: {error:#}",
                self.seed.path.display(),
                self.marker.as_ref().map(Marker::named).unwrap_or_default()
            ))
            .field("error", format!("{error:#}"))
            .write();
        }
    }

    /// Where a reset loaded a world in place of the seed: the run under way is cut
    /// ([`Seeding::cut`]) and the marker reads every entry done, none rejected, so a restart sends
    /// none.
    pub(crate) fn loaded(&mut self) {
        self.cut();
        self.rejected_at.clear();
        self.marked();
    }

    /// Whether an entry is left to submit.
    pub(crate) fn pending(&self) -> bool {
        !self.retry.is_empty() || self.next < self.seed.entries.len()
    }

    /// Ends the seed run under way where a reset emptied the stores it wrote: its entries not yet
    /// sent are dropped, named on stderr, and never submitted, and its end line is printed with
    /// what it counted so far. A run that already ended is left as it is
    /// (`story:reset-reapplies-seed`).
    pub(crate) fn cut(&mut self) {
        let left = self.retry.len() + self.seed.entries.len().saturating_sub(self.next);
        if left > 0 {
            log::info(format!(
                "seed {}: a reset dropped the {left} entries not yet sent",
                self.seed.path.display()
            ))
            .write();
            self.retry.clear();
            self.next = self.seed.entries.len();
        }
        self.end();
        self.publish();
    }

    /// Starts a new seed run from the first entry, as a start does: no request id remembered,
    /// the end line counting this run alone and printed again once it ends. The counts noted on
    /// the recorder go on from those of the earlier runs and never fall. A reset with no world
    /// calls it once every store is empty and the run under way is cut ([`Seeding::cut`]). The
    /// marker reads 0 entries done before the first is sent, so a run stopped before that entry is
    /// done starts the seed from its first entry again.
    pub(crate) fn restart(&mut self) {
        log::info(format!(
            "seed {}: {} entries, submitted again after a reset, before it answers",
            self.seed.path.display(),
            self.seed.len()
        ))
        .write();
        self.earlier = summed(self.earlier, self.noted());
        self.counts = SeedCounts::default();
        self.rejected = 0;
        self.next = 0;
        self.retry.clear();
        self.rejected_at.clear();
        self.marked();
        self.submitted.clear();
        self.ended = false;
        self.holding = false;
        self.publish();
    }

    /// Publishes the seed as running from its first entry, none sent, before a reset with no world
    /// empties the stores, so no request reads the run ready while its stores are empty
    /// (`story:seed-readiness`). Until [`Seeding::restart`] counts the new run, nothing else is
    /// published: the run cut inside the reset ([`Seeding::cut`]) never reads ended. A reset that
    /// fails publishes the run as it stood again ([`Seeding::unreset`]).
    pub(crate) fn resetting(&mut self) {
        self.published
            .publish(SeedProgress::starting(count(self.seed.entries.len())));
        self.holding = true;
    }

    /// Publishes the seed run as it stands, after a reset that [`Seeding::resetting`] announced
    /// failed and emptied nothing.
    pub(crate) fn unreset(&mut self) {
        self.holding = false;
        self.publish();
    }

    /// Submits every entry left to `host`, one after another, then prints the end line. Before
    /// each entry it asks `stopped`, and where that answers `true` it submits nothing more, so a
    /// signal stops the run between two entries, the entries left [pending](Seeding::pending).
    pub(crate) fn finish(&mut self, host: &mut Host, stopped: impl Fn() -> bool) {
        while self.pending() {
            if stopped() {
                return;
            }
            self.step(host);
        }
        self.end();
    }

    /// Prints the end line of the run under way once: the counts of this run alone. The state is
    /// published ended first, so a reader that saw the end line reads `ended` and `200` on
    /// `GET /__spindle/seed` and `GET /__spindle/ready` (`story:readiness-test-steady-under-load`).
    fn end(&mut self) {
        if self.ended {
            return;
        }
        self.ended = true;
        if self.rejected > 0 {
            self.failed = true;
        }
        self.publish();
        let SeedCounts {
            sent,
            applied,
            already_there,
            refused,
        } = self.counts;
        let rejected = self.rejected;
        let level = if rejected > 0 {
            log::Level::Warn
        } else {
            log::Level::Info
        };
        log::line(
            level,
            format!(
                "seed {} ended: {sent} sent, {applied} applied, {already_there} already there, \
                 {refused} refused, {rejected} rejected",
                self.seed.path.display()
            ),
        )
        .write();
    }

    /// Publishes the state of the seed run under way, read beside this thread by
    /// `GET /__spindle/seed` and `GET /__spindle/ready`: ended only once its last entry's
    /// submission has returned, so whatever that entry wrote is in place. Inside a reset with no
    /// world it publishes nothing ([`Seeding::resetting`]).
    fn publish(&self) {
        if self.holding {
            return;
        }
        let left = self.retry.len() + self.seed.entries.len().saturating_sub(self.next);
        self.published.publish(SeedProgress {
            phase: if self.ended {
                SeedPhase::Ended
            } else {
                SeedPhase::Running
            },
            entries: count(self.seed.entries.len()),
            left: count(left),
            sent: self.counts.sent,
            applied: self.counts.applied,
            already_there: self.counts.already_there,
            refused: self.counts.refused,
            rejected: self.rejected,
        });
    }

    /// Whether a seed run of this start ended having rejected an entry, so the run exits 1
    /// (`story:run-fails-on-rejected-line`).
    pub(crate) fn failed(&self) -> bool {
        self.failed
    }

    /// The seed's file.
    pub(crate) fn path(&self) -> &Path {
        self.seed.path()
    }

    /// Submits the next entry to `host`, and once none is left prints the end line once: right
    /// after the last entry, so a seed that rejected one stops the run before the loop takes
    /// another exchange ([`Seeding::failed`]). A
    /// refusal is printed with the entry's place, command, request id and error name; a command an
    /// entry's events led to that did not run is printed as `run` prints one a request led to.
    /// An entry the marker recorded rejected is sent again before the next one. Once the entry is
    /// sent the marker counts it ([`MARKER`]), and records it rejected where it was.
    pub(crate) fn step(&mut self, host: &mut Host) {
        let index = if self.retry.is_empty() {
            self.next
        } else {
            self.retry.remove(0)
        };
        let Some(entry) = self.seed.entries.get(index) else {
            self.end();
            return;
        };
        let place = index + 1;
        if index == self.next {
            self.next += 1;
        }
        self.counts.sent += 1;
        let became = if self.submitted.contains(&entry.request_id) {
            Became::AlreadyThere
        } else {
            let became = submitted(host, entry, &self.seed.digest);
            // Only an entry that left its effect in place answers for a later one with its id: a
            // refused or rejected entry left nothing, so a repeat of it is submitted again.
            if !matches!(became, Became::Refused(_) | Became::Rejected(_)) {
                self.submitted.insert(entry.request_id.clone());
            }
            became
        };
        if matches!(became, Became::Rejected(_)) {
            self.rejected_at.insert(index);
        } else {
            self.rejected_at.remove(&index);
        }
        match became {
            Became::Applied => self.counts.applied += 1,
            Became::AlreadyThere => self.counts.already_there += 1,
            Became::Refused(error) => {
                self.counts.refused += 1;
                log::warn(format!(
                    "seed {} entry {place}: {} (request id {}) refused: {error}",
                    self.seed.path.display(),
                    entry.command,
                    entry.request_id
                ))
                .field("command", entry.command.as_str())
                .field("request_id", entry.request_id.as_str())
                .field("error", error)
                .write();
            }
            Became::Rejected(error) => {
                self.rejected += 1;
                log::error(format!(
                    "seed {} entry {place}: {} (request id {}) rejected: {error}",
                    self.seed.path.display(),
                    entry.command,
                    entry.request_id
                ))
                .field("command", entry.command.as_str())
                .field("request_id", entry.request_id.as_str())
                .field("error", error)
                .write();
            }
        }
        self.marked();
        self.note();
        if !self.pending() {
            self.end();
        }
    }

    /// The counts of the seed run under way as the metrics count them: `refused` is every entry
    /// that did not apply, a rejected one included, so the metric families keep their series.
    fn noted(&self) -> SeedCounts {
        SeedCounts {
            refused: self.counts.refused + self.rejected,
            ..self.counts
        }
    }

    /// Notes the counts of every seed run of this start on the recorder, where the run observes:
    /// a counter, so a reset never makes one fall. Publishes the state of the run under way.
    fn note(&self) {
        if let Some(recorder) = &self.recorder {
            recorder.seeded(summed(self.earlier, self.noted()));
        }
        self.publish();
    }
}

/// `entries` as a count.
fn count(entries: usize) -> u64 {
    u64::try_from(entries).unwrap_or(u64::MAX)
}

/// The counts `one` and `other` make together.
fn summed(one: SeedCounts, other: SeedCounts) -> SeedCounts {
    SeedCounts {
        sent: one.sent + other.sent,
        applied: one.applied + other.applied,
        already_there: one.already_there + other.already_there,
        refused: one.refused + other.refused,
    }
}

/// Submits `entry` of the seed with `digest` to `host` as its actor and caller, each event it
/// commits recording the seed by that digest, that caller and the entry's `request_id` as its
/// cause: what became of it.
fn submitted(host: &mut Host, entry: &Entry, digest: &str) -> Became {
    let submitted = host.submit_caused(
        &entry.command,
        entry.input.clone(),
        entry.actor.as_deref(),
        entry.caller.as_ref(),
        &BTreeMap::new(),
        spindle_kernel::Provenance::Seed {
            digest: digest.to_owned(),
            sender: spindle_kernel::Sender {
                actor: entry.actor.clone(),
                attributes: entry.caller.clone().unwrap_or_default(),
                request: Some(entry.request_id.clone()),
            },
        },
    );
    match submitted {
        Ok(report) => {
            downstream(&report);
            let outcome = report
                .ran
                .first()
                .map(|ran| ran.executed.outcome.as_str())
                .unwrap_or_default();
            if existing(host.ir(), &entry.command, outcome) {
                Became::AlreadyThere
            } else {
                Became::Applied
            }
        }
        Err(HostError::CommandRefused { outcome, error, .. }) => {
            if existing(host.ir(), &entry.command, &outcome) {
                Became::AlreadyThere
            } else {
                Became::Refused(error.name.clone())
            }
        }
        Err(HostError::Execute { error, .. }) => Became::Rejected(error.kind().to_owned()),
        Err(other) => Became::Rejected(other.to_string()),
    }
}

/// Prints each command an entry's events led to that did not run, or that the model refused.
fn downstream(report: &Report) {
    for failed in &report.failed {
        let mut line = log::warn(format!(
            "{} did not run {}, which a seed entry led to: {}",
            failed.component, failed.command, failed.error
        ))
        .field("component", failed.component.as_str())
        .field("command", failed.command.as_str());
        if let spindle_host::Cause::Delivered { binding } | spindle_host::Cause::Tick { binding } =
            &failed.cause
        {
            line = line.field("binding", binding.as_str());
        }
        line.field("attempts", failed.attempts)
            .field("error", failed.error.as_str())
            .write();
    }
    for refused in &report.refused {
        log::warn(format!(
            "{} refused {} ({}), which a seed entry led to, with {}",
            refused.component, refused.command, refused.outcome, refused.error.name
        ))
        .field("component", refused.component.to_string())
        .field("command", refused.command.to_string())
        .field("error", refused.error.name.clone())
        .write();
    }
}
