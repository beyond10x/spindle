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
//! submitted again) or refused; a refusal is logged and the next entry is submitted.
//!
//! A request id is remembered in memory, by this start alone: Spindle keeps no record of request
//! ids across starts or replicas (`story:request-id-idempotency`). An entry whose id only a
//! refused entry carried is submitted again, and counts as what it becomes: the refusal left
//! nothing there, and what refused it may since have changed.
//!
//! A `POST /api/reset` drops the entries of the seed run under way that are not yet sent
//! ([`Seeding::cut`]). One with no world then submits the seed again from its first entry, once
//! every store is empty and before the reset answers, as a start does: no request id remembered,
//! and an end line counting that run alone ([`Seeding::restart`], `story:reset-reapplies-seed`).
//! The counts in the metrics go on across resets and never fall. A reset that gives a world loads
//! that world alone.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use ess_compiler::ir::{ResolvedCommand, ResolvedCondition};
use serde_json::{Map, Value};
use spindle_host::{Host, HostError, Report};
use spindle_ingress::{Recorder, SeedCounts};
use spindle_spec::EssIr;

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
        entries,
    })
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
    ended: bool,
}

/// What became of one entry.
enum Became {
    Applied,
    AlreadyThere,
    Refused(String),
}

impl Seeding {
    /// `seed` about to be submitted, its counts noted on `recorder` from now on.
    pub(crate) fn new(seed: Seed, recorder: Option<Recorder>) -> Self {
        let seeding = Self {
            seed,
            next: 0,
            submitted: BTreeSet::new(),
            counts: SeedCounts::default(),
            earlier: SeedCounts::default(),
            recorder,
            ended: false,
        };
        seeding.note();
        seeding
    }

    /// Whether an entry is left to submit.
    pub(crate) fn pending(&self) -> bool {
        self.next < self.seed.entries.len()
    }

    /// Ends the seed run under way where a reset emptied the stores it wrote: its entries not yet
    /// sent are dropped, named on stderr, and never submitted, and its end line is printed with
    /// what it counted so far. A run that already ended is left as it is
    /// (`story:reset-reapplies-seed`).
    pub(crate) fn cut(&mut self) {
        let left = self.seed.entries.len().saturating_sub(self.next);
        if left > 0 {
            eprintln!(
                "b10x-spindle: seed {}: a reset dropped the {left} entries not yet sent",
                self.seed.path.display()
            );
            self.next = self.seed.entries.len();
        }
        self.end();
    }

    /// Starts a new seed run from the first entry, as a start does: no request id remembered,
    /// the end line counting this run alone and printed again once it ends. The counts noted on
    /// the recorder go on from those of the earlier runs and never fall. A reset with no world
    /// calls it once every store is empty and the run under way is cut ([`Seeding::cut`]).
    pub(crate) fn restart(&mut self) {
        eprintln!(
            "b10x-spindle: seed {}: {} entries, submitted again after a reset, before it answers",
            self.seed.path.display(),
            self.seed.len()
        );
        self.earlier = summed(self.earlier, self.counts);
        self.counts = SeedCounts::default();
        self.next = 0;
        self.submitted.clear();
        self.ended = false;
    }

    /// Submits every entry left to `host`, one after another, then prints the end line; `true`
    /// once it ended. Before each entry it asks `stopped`, and where that answers `true` it
    /// submits nothing more and answers `false`, so a signal stops the run between two entries.
    pub(crate) fn finish(&mut self, host: &mut Host, stopped: impl Fn() -> bool) -> bool {
        while self.pending() {
            if stopped() {
                return false;
            }
            self.step(host);
        }
        self.end();
        true
    }

    /// Prints the end line of the run under way once: the counts of this run alone.
    fn end(&mut self) {
        if self.ended {
            return;
        }
        self.ended = true;
        let SeedCounts {
            sent,
            applied,
            already_there,
            refused,
        } = self.counts;
        eprintln!(
            "b10x-spindle: seed {} ended: {sent} sent, {applied} applied, \
             {already_there} already there, {refused} refused",
            self.seed.path.display()
        );
    }

    /// Submits the next entry to `host`, or, once none is left, prints the end line once. A
    /// refusal is printed with the entry's place, command, request id and error name; a command an
    /// entry's events led to that did not run is printed as `run` prints one a request led to.
    pub(crate) fn step(&mut self, host: &mut Host) {
        let Some(entry) = self.seed.entries.get(self.next) else {
            self.end();
            return;
        };
        let place = self.next + 1;
        self.next += 1;
        self.counts.sent += 1;
        let became = if self.submitted.contains(&entry.request_id) {
            Became::AlreadyThere
        } else {
            let became = submitted(host, entry);
            // Only an entry that left its effect in place answers for a later one with its id: a
            // refused entry left nothing, so a repeat of it is submitted again.
            if !matches!(became, Became::Refused(_)) {
                self.submitted.insert(entry.request_id.clone());
            }
            became
        };
        match became {
            Became::Applied => self.counts.applied += 1,
            Became::AlreadyThere => self.counts.already_there += 1,
            Became::Refused(error) => {
                self.counts.refused += 1;
                eprintln!(
                    "b10x-spindle: seed {} entry {place}: {} (request id {}) refused: {error}",
                    self.seed.path.display(),
                    entry.command,
                    entry.request_id
                );
            }
        }
        self.note();
    }

    /// Notes the counts of every seed run of this start on the recorder, where the run observes:
    /// a counter, so a reset never makes one fall.
    fn note(&self) {
        if let Some(recorder) = &self.recorder {
            recorder.seeded(summed(self.earlier, self.counts));
        }
    }
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

/// Submits `entry` to `host` as its actor and caller: what became of it.
fn submitted(host: &mut Host, entry: &Entry) -> Became {
    let submitted = host.submit_as(
        &entry.command,
        entry.input.clone(),
        entry.actor.as_deref(),
        entry.caller.as_ref(),
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
        Err(HostError::Execute { error, .. }) => Became::Refused(error.kind().to_owned()),
        Err(other) => Became::Refused(other.to_string()),
    }
}

/// Prints each command an entry's events led to that did not run, or that the model refused.
fn downstream(report: &Report) {
    for failed in &report.failed {
        eprintln!(
            "b10x-spindle: {} did not run {}, which a seed entry led to: {}",
            failed.component, failed.command, failed.error
        );
    }
    for refused in &report.refused {
        eprintln!(
            "b10x-spindle: {} refused {} ({}), which a seed entry led to, with {}",
            refused.component, refused.command, refused.outcome, refused.error.name
        );
    }
}
