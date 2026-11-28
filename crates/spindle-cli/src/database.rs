//! `store.durable` bound to `postgres`: the settings that reach the database, the lease of each
//! component's tables a served run leads under, one store per component bound to it, and `store
//! migrate`, which creates their tables.
//!
//! Settings are read as every setting is, `SPINDLE_SETTING_<NAME>`: `database_url` (required),
//! `database_ca`, `database_schema` (`public`), `database_pool` (4), `database_connections`
//! (required with `database_ca`: the database's connection budget available to this deployment,
//! not the serving role's `CONNECTION LIMIT`) and `database_lease` (not given by default: the
//! seconds a lease is held from its last renewal). A URL is never printed: it holds a password.
//!
//! Several `run` processes of one specification given `database_lease` may share one database
//! (`story:several-writers-one-postgres`): each takes the lease of every component's tables in
//! name order ([`Leases`]), and the one that holds them all leads, the one writer of those tables;
//! the others stand by until the leader is gone. Each writes only under its lease: a store it
//! opens refuses a write once another process took the lease
//! ([`spindle_kernel::PostgresStore::open`]). Without `database_lease` a run takes no lease and
//! opens no connection for one, so two such processes on one store both write.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use spindle_capability::{Capability, Environment};
use spindle_host::Stores;
use spindle_kernel::{Database, Fit, Lease, Misfit, PostgresStore, Shapes, postgres_prefix};
use spindle_spec::EssIr;
use spindle_target::{ENV_SETTING_PREFIX, env_name};

/// The implementation of `store.durable` this module opens.
pub(crate) const POSTGRES: &str = "postgres";

/// A durable store holds rows the specification `run` serves cannot read
/// (`story:spec-change-over-stored-rows`): `run` prints it on stdout and exits 3 before
/// listening. `store` says which store, as [`spindle_host::HostError::Misfit`] does.
#[derive(Debug)]
pub(crate) struct Misfitted {
    store: String,
    misfits: Vec<Misfit>,
}

impl std::fmt::Display for Misfitted {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "refused: {} holds rows this specification cannot read",
            self.store
        )?;
        for misfit in &self.misfits {
            write!(formatter, "\n  {misfit}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Misfitted {}

/// What `run` exits with for `error`: 3 with the misfit printed on stdout where it is
/// [`Misfitted`].
///
/// # Errors
/// `error` itself where it is not [`Misfitted`].
pub(crate) fn misfit_refused(error: anyhow::Error) -> Result<ExitCode> {
    let misfitted = error.downcast::<Misfitted>()?;
    println!("{misfitted}");
    Ok(ExitCode::from(crate::REFUSED))
}

/// The line `run` logs for a store, named by `store`, that held rows and recorded no
/// specification: it now records the one served, unchecked (D2).
pub(crate) fn unrecorded(store: &str) -> String {
    format!(
        "the store of {store} recorded no specification it was written under; it now records \
         this one, its rows unchecked"
    )
}

/// What `run` does with `fit`, the check of the store `store` names against the specification it
/// serves ([`spindle_kernel::LogStore::check`]): nothing where it fits, a line where it recorded no
/// specification before, and [`Misfitted`] where the specification cannot read a row.
///
/// # Errors
/// [`Misfitted`].
pub(crate) fn fitting(fit: Fit, store: &str, named: &str) -> Result<()> {
    match fit {
        Fit::Same | Fit::Fits { unrecorded: false } => Ok(()),
        Fit::Fits { unrecorded: true } => {
            crate::log::info(unrecorded(named)).write();
            Ok(())
        }
        Fit::Misfits(misfits) => Err(Misfitted {
            store: store.to_owned(),
            misfits,
        }
        .into()),
    }
}

/// The environment variable carrying the setting `name`.
fn variable(name: &str) -> String {
    format!("{ENV_SETTING_PREFIX}{}", env_name(name))
}

/// The setting `name`, or `None` where it is unset or empty.
fn setting(name: &str) -> Result<Option<String>> {
    match std::env::var(variable(name)) {
        Ok(value) if value.is_empty() => Ok(None),
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => bail!("{} is not UTF-8", variable(name)),
    }
}

/// The setting `name` as a whole number above 0.
fn count(name: &str) -> Result<Option<usize>> {
    setting(name)?
        .map(|text| {
            text.parse::<usize>()
                .ok()
                .filter(|count| *count > 0)
                .with_context(|| {
                    format!(
                        "{name} ({}) is {text:?}, not a whole number above 0",
                        variable(name)
                    )
                })
        })
        .transpose()
}

/// The database the settings name.
pub(crate) fn database() -> Result<Database> {
    let url = setting("database_url")?.with_context(|| {
        format!(
            "store.durable is bound to postgres, which needs the setting database_url ({}): \
             postgres://user:password@host:port/database",
            variable("database_url")
        )
    })?;
    let mut database = Database::new(url);
    database.ca = setting("database_ca")?.map(PathBuf::from);
    if let Some(schema) = setting("database_schema")? {
        database.schema = schema;
    }
    if let Some(pool) = count("database_pool")? {
        database.pool = pool;
    }
    database.connections = count("database_connections")?;
    Ok(database)
}

/// Every component of `ir` whose `store.durable` admission binds to `postgres` on `environment`.
pub(crate) fn components(
    ir: &EssIr,
    environment: &Environment,
) -> Result<Vec<String>, spindle_host::Refusal> {
    Ok(spindle_host::admit(ir, environment)?
        .into_iter()
        .filter(|(_, bindings)| {
            bindings.iter().any(|binding| {
                binding.capability.as_str() == Capability::STORE_DURABLE
                    && binding.implementation == POSTGRES
            })
        })
        .map(|(component, _)| component)
        .collect())
}

/// One store per component of `ir` bound to `postgres` on `environment`, by component, for
/// [`spindle_host::Host::with_stores`]. The settings and the connection budget are checked before
/// anything connects: `database_connections`, the database's connection budget available to this
/// deployment, where given, must hold every component's `database_pool` and one reserved
/// connection. The serving role's `CONNECTION LIMIT` is another number, from 1 to components x
/// `database_pool`, which the database checks when a store opens.
///
/// # Errors
/// A setting missing or malformed, a budget too small, or a store the database refused, naming it;
/// [`Misfitted`] where a store holds rows the specification cannot read, every store opened
/// before it closed again.
pub(crate) fn stores(ir: &EssIr, environment: &Environment) -> Result<Stores> {
    let database = database()?;
    let components = components(ir, environment).map_err(|refusal| anyhow::anyhow!("{refusal}"))?;
    budgeted(&database, &components)?;
    // Every store is checked before any records the specification, so a misfit in one leaves
    // every record as it was.
    let mut checked = Vec::new();
    for component in &components {
        let mut store = PostgresStore::open(&database, component, components.len())
            .with_context(|| format!("opening the PostgreSQL store of {component}"))?;
        let shapes = Shapes::owned(ir, component);
        let fit = store
            .check(&shapes)
            .with_context(|| format!("checking the PostgreSQL store of {component}"))?;
        let records = matches!(fit, Fit::Fits { .. });
        fitting(fit, &format!("the store of {component}"), component)?;
        checked.push((component, store, records.then_some(shapes)));
    }
    let mut stores = Stores::new();
    for (component, mut store, shapes) in checked {
        if let Some(shapes) = shapes {
            store
                .record(&shapes)
                .with_context(|| format!("recording the specification of {component}"))?;
        }
        stores.insert(component.clone(), Box::new(store));
    }
    Ok(stores)
}

/// The connection budget checked before anything connects: `database_connections`, where given,
/// must hold every component's `database_pool` and one reserved connection, the one each process
/// holds for its leases ([`Lease`]); with `database_ca` it is required.
///
/// # Errors
/// A budget too small or missing, naming both numbers.
fn budgeted(database: &Database, components: &[String]) -> Result<()> {
    let share = components.len().saturating_mul(database.pool);
    let needed = share.saturating_add(spindle_kernel::POSTGRES_RESERVED);
    match database.connections {
        Some(connections) if needed > connections => bail!(
            "database_connections is {connections}, below the {needed} connections run needs: {} \
             components x database_pool {} + {} reserved. database_connections is the \
             database's connection budget available to this deployment; the serving role's \
             CONNECTION LIMIT is separate, from 1 to {share}",
            components.len(),
            database.pool,
            spindle_kernel::POSTGRES_RESERVED
        ),
        None if database.ca.is_some() => bail!(
            "a hosted PostgreSQL (database_ca given) needs database_connections ({}), the \
             database's connection budget available to this deployment: at least {needed}, {} \
             components x database_pool {} + {} reserved. The serving role's CONNECTION LIMIT \
             is separate, from 1 to {share}",
            variable("database_connections"),
            components.len(),
            database.pool,
            spindle_kernel::POSTGRES_RESERVED
        ),
        _ => {}
    }
    Ok(())
}

/// `database_lease`: how long a lease is held from its last renewal, in whole seconds above 0;
/// `None` where it is not given, and the run takes no lease (D9 C).
fn lease_term() -> Result<Option<Duration>> {
    let Some(seconds) = count("database_lease")? else {
        return Ok(None);
    };
    match u64::try_from(seconds) {
        Ok(seconds) if seconds <= spindle_kernel::LEASE_MAX.as_secs() => {
            Ok(Some(Duration::from_secs(seconds)))
        }
        _ => bail!(
            "database_lease ({}) is {seconds}, above the longest lease, {} seconds (a day)",
            variable("database_lease"),
            spindle_kernel::LEASE_MAX.as_secs()
        ),
    }
}

/// How soon a renewal that did not land is first tried again; each further try waits twice as
/// long, at most [`Leases::renewal`], so a database that refuses at once is not asked in a loop.
const RETRY: Duration = Duration::from_millis(50);

/// The leases of every component of a served run bound to `postgres`, taken in name order through
/// one connection of their own, which reads and takes only the lease rows and the seed's record
/// beside them, and opens no store.
pub(crate) struct Leases {
    lease: Lease,
    /// The schema the components' tables are in.
    schema: String,
    /// The components, in name order.
    components: Vec<String>,
    /// The token of each lease taken, by component.
    held: BTreeMap<String, u64>,
    /// The trouble of the lease connection last told on stderr ([`Leases::told`]).
    told: Option<String>,
    /// The components other than the first whose lease tables the seed's record was read from,
    /// which the next write removes it from as it writes the first one's ([`Leases::answer`]).
    elsewhere: Vec<String>,
    /// The lease tables of no current component the seed's record was read from, by table name,
    /// which the next write moves it out of under each one's own lease ([`Leases::moved`]).
    orphans: Vec<String>,
    /// The lease tables of no current component whose leases a reset took before emptying
    /// anything, with their tokens ([`Leases::claim`]), renewed with the run's own until the
    /// removal after the reset releases them.
    claimed: Vec<(String, u64)>,
}

impl Leases {
    /// Checks the settings and the connection budget, then connects for the leases of every
    /// component of `ir` bound to `postgres` on `environment`; `None`, connecting to nothing,
    /// where `database_lease` is not given.
    ///
    /// # Errors
    /// A setting missing or malformed, a budget too small, or a database that cannot be reached.
    pub(crate) fn connect(ir: &EssIr, environment: &Environment) -> Result<Option<Self>> {
        let database = database()?;
        let Some(term) = lease_term()? else {
            return Ok(None);
        };
        let mut components =
            components(ir, environment).map_err(|refusal| anyhow::anyhow!("{refusal}"))?;
        components.sort();
        budgeted(&database, &components)?;
        let lease = Lease::connect(&database, term).context("connecting for the leases")?;
        Ok(Some(Self {
            lease,
            schema: database.schema,
            components,
            held: BTreeMap::new(),
            told: None,
            elsewhere: Vec::new(),
            orphans: Vec::new(),
            claimed: Vec::new(),
        }))
    }

    /// How often a standby asks for the leases: a third of the lease.
    pub(crate) fn poll(&self) -> Duration {
        self.lease.term() / 3
    }

    /// How often the leader renews the leases: a fifth of the lease, so a renewal queued behind a
    /// write that holds a lease row for up to four fifths of the lease lands before it runs out.
    fn renewal(&self) -> Duration {
        self.lease.term() / 5
    }

    /// Renews each lease held and takes, in name order, each one not held yet, stopping at the
    /// first another process holds: `None` once every one is held, else the component whose
    /// lease another process holds.
    ///
    /// # Errors
    /// A lease that cannot be read or written, or one held that another process took.
    pub(crate) fn take(&mut self) -> Result<Option<String>> {
        let taken = self.asked();
        self.told();
        taken
    }

    /// Prints a warning naming the leases and the connection error when the lease connection
    /// cannot be made or was closed, once per change of that error, and a line once it is made
    /// again: a process standing by reads the leases as held meanwhile, and cannot take over.
    fn told(&mut self) {
        let trouble = self.lease.trouble().map(str::to_owned);
        if trouble == self.told {
            return;
        }
        let leases = self.components.join(", ");
        match &trouble {
            Some(why) => crate::log::warn(format!(
                "the connection for the leases of {leases} cannot be made, so this run cannot \
                 take them over while it stands by; it tries again every {} ms: {why}",
                self.poll().as_millis()
            ))
            .field("error", why.as_str())
            .write(),
            None => {
                crate::log::info(format!(
                    "the connection for the leases of {leases} is made again"
                ))
                .write();
            }
        }
        self.told = trouble;
    }

    /// [`Leases::take`], without telling the connection's trouble.
    fn asked(&mut self) -> Result<Option<String>> {
        for component in &self.components {
            match self.held.get(component) {
                Some(&token) => match self.lease.renew(component, token) {
                    Ok(true) => {}
                    // Another process took it while this one stood by: this one stands by on.
                    Ok(false) => {
                        self.held.remove(component);
                        return Ok(Some(component.clone()));
                    }
                    // A renewal that did not land, as one waiting out a lock, leaves the lease
                    // held until its end: the run stands by and asks again.
                    Err(_) => return Ok(Some(component.clone())),
                },
                None => match self.lease.take(component)? {
                    Some(token) => {
                        self.held.insert(component.clone(), token);
                    }
                    None => return Ok(Some(component.clone())),
                },
            }
        }
        Ok(None)
    }

    /// The components, in name order.
    pub(crate) fn components(&self) -> &[String] {
        &self.components
    }

    /// Keeps every lease held on a thread of its own, renewing each every [`Leases::renewal`], until
    /// the [`Kept`] is dropped, which releases them. The same thread reads and writes the seed's
    /// record on the lease connection when asked ([`SeedRow`]).
    ///
    /// # Errors
    /// The thread cannot start.
    pub(crate) fn keep(self) -> Result<Kept> {
        let term = self.lease.term();
        let shared = Arc::new(Mutex::new(Renewal {
            lost: None,
            renewed: Instant::now(),
        }));
        let (asks, asked) = mpsc::channel::<Ask>();
        let first = self.components.first().cloned().unwrap_or_default();
        let row = SeedRow {
            asks: asks.clone(),
            // The thread answers between two renewals: at most one statement per lease before the
            // one asked, each bounded by the connection's statement timeout, and one more to
            // connect again.
            wait: self
                .lease
                .statement_wait()
                .saturating_mul(u32::try_from(self.components.len() + 2).unwrap_or(u32::MAX)),
            table: self.table_of(&first),
        };
        let noted = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("spindle-lease".to_owned())
            .spawn(move || self.renewing(&asked, &noted))
            .context("starting the lease thread")?;
        Ok(Kept {
            shared,
            term,
            row,
            stop: Some(asks),
            thread: Some(thread),
        })
    }

    /// Answers `ask` about the seed's record (`story:seed-under-a-lease`, D3 A, D4 A). It is kept
    /// in the lease table of the first component in name order, under the token this run took
    /// that lease at. A read looks in every component's lease table, a table not created yet
    /// holding none, so a record a run of an earlier specification kept under another first
    /// component is found. Records found alike in several tables, as a release rolled back and
    /// forward across a change of components leaves them, are one record; records that differ
    /// are refused, naming the tables. Where no current component's table holds a record, the
    /// lease tables of no current component, as those of a component a later release removed,
    /// are looked for once by their table name and read the same way
    /// (`story:seed-record-after-a-component-is-removed`, D2 A, D3 A). The next write gathers a
    /// record found elsewhere into the first component's table, in one statement under every
    /// lease involved, an old table's taken for it ([`Leases::moved`]); a removal removes it from
    /// every table, the old ones included.
    fn answer(&mut self, ask: Ask) {
        let held: Vec<(String, u64)> = self
            .components
            .iter()
            .filter_map(|component| {
                self.held
                    .get(component)
                    .map(|token| (component.clone(), *token))
            })
            .collect();
        let Some((first, token)) = held.first().cloned() else {
            let none = "this run holds no lease".to_owned();
            match ask {
                Ask::Stop => {}
                Ask::Read(reply) => drop(reply.send(Err(none))),
                Ask::Write(_, reply) | Ask::Forget(reply) | Ask::Unclaim(reply) => {
                    drop(reply.send(Err(none)));
                }
                Ask::Claim(reply) => drop(reply.send(Err(none))),
            }
            return;
        };
        match ask {
            Ask::Stop => {}
            Ask::Read(reply) => {
                let _ = reply.send(self.read(&held));
            }
            Ask::Write(body, reply) => {
                let written = if !self.orphans.is_empty() {
                    self.moved(&first, token, &body)
                } else if self.elsewhere.is_empty() {
                    self.lease.record_seed(&first, token, &body)
                } else {
                    let from: Vec<(&str, u64)> = self
                        .elsewhere
                        .iter()
                        .map(|old| {
                            (
                                old.as_str(),
                                self.held.get(old).copied().unwrap_or_default(),
                            )
                        })
                        .collect();
                    let gathered = self.lease.gather_seed(&from, (&first, token), &body);
                    if gathered.is_ok() {
                        self.elsewhere.clear();
                    }
                    gathered
                };
                let _ = reply.send(written.map_err(|error| error.to_string()));
            }
            Ask::Forget(reply) => {
                let mut removed = Ok(());
                for (component, token) in &held {
                    if let Err(error) = self.lease.forget_seed(component, *token) {
                        removed = Err(error.to_string());
                        break;
                    }
                }
                if removed.is_ok() {
                    removed = self.forget_claimed().map_err(|error| error.to_string());
                }
                if removed.is_ok() {
                    self.elsewhere.clear();
                    self.orphans.clear();
                }
                let _ = reply.send(removed);
            }
            Ask::Claim(reply) => {
                let _ = reply.send(self.claim().map_err(|error| error.to_string()));
            }
            Ask::Unclaim(reply) => {
                self.unclaim();
                let _ = reply.send(Ok(()));
            }
        }
    }

    /// The seed's record, looked for in the lease table of each component of `held`, in name
    /// order; notes where it was found other than in the first component's table.
    fn read(&mut self, held: &[(String, u64)]) -> Result<Option<serde_json::Value>, String> {
        let mut found = Vec::new();
        for (component, _) in held {
            if let Some(body) = self
                .lease
                .seed_record(component)
                .map_err(|error| error.to_string())?
            {
                found.push((component.clone(), body));
            }
        }
        self.orphans.clear();
        let Some((_, body)) = found.first().cloned() else {
            return self.orphaned();
        };
        if found.iter().any(|(_, other)| *other != body) {
            let tables: Vec<String> = found
                .iter()
                .map(|(component, _)| self.table_of(component))
                .collect();
            return Err(differently(&tables));
        }
        let first = held.first().map(|(component, _)| component.as_str());
        self.elsewhere = found
            .into_iter()
            .map(|(component, _)| component)
            .filter(|component| Some(component.as_str()) != first)
            .collect();
        Ok(Some(body))
    }

    /// The seed's record looked for, where no current component's lease table holds one, in the
    /// lease tables of no current component: listed by table name with one query, and read only
    /// where a table holds a lease row (`story:seed-record-after-a-component-is-removed`, D2 A).
    /// Records alike are one; records that differ are refused naming the tables (D3 A). Notes
    /// the tables it was found in.
    fn orphaned(&mut self) -> Result<Option<serde_json::Value>, String> {
        let mut found = Vec::new();
        for table in self.orphan_tables().map_err(|error| error.to_string())? {
            if let Some(body) = self
                .lease
                .orphan_seed_record(&table)
                .map_err(|error| error.to_string())?
            {
                found.push((table, body));
            }
        }
        let Some((_, body)) = found.first().cloned() else {
            return Ok(None);
        };
        if found.iter().any(|(_, other)| *other != body) {
            let tables: Vec<String> = found.into_iter().map(|(table, _)| table).collect();
            return Err(differently(&tables));
        }
        self.orphans = found.into_iter().map(|(table, _)| table).collect();
        Ok(Some(body))
    }

    /// The lease tables of the schema of no component of this run's specification.
    fn orphan_tables(&mut self) -> Result<Vec<String>, spindle_kernel::StoreError> {
        let current: Vec<&str> = self.components.iter().map(String::as_str).collect();
        self.lease.orphan_tables(&current)
    }

    /// Writes `body` as the record in the first component's lease table, `first` held at
    /// `token`, and moves it out of the lease tables of no current component it was read from:
    /// in one statement under the first component's lease and each such table's own, which this
    /// run takes for the move and releases after (`story:seed-record-after-a-component-is-removed`,
    /// D1 A). While another process holds one of those, as one of the release that still has the
    /// component may, the record is written into the first component's table alone and the old
    /// rows are left: a start reads the current table's from then on.
    fn moved(
        &mut self,
        first: &str,
        token: u64,
        body: &serde_json::Value,
    ) -> Result<(), spindle_kernel::StoreError> {
        let mut taken = Vec::new();
        let mut kept_by = None;
        let mut failed = None;
        for table in &self.orphans {
            match self.lease.take_orphan(table) {
                Ok(Some(held)) => taken.push((table.clone(), held)),
                Ok(None) => {
                    kept_by = Some(table.clone());
                    break;
                }
                Err(error) => {
                    failed = Some(error);
                    break;
                }
            }
        }
        let written = match (failed, kept_by) {
            (Some(error), _) => Err(error),
            (None, Some(table)) => {
                crate::log::warn(format!(
                    "the seed record in {table}, the lease table of a component this \
                     specification no longer has, is left there: another process holds that \
                     table's lease, as one of an earlier release still running may; the record \
                     is now kept in {}",
                    self.table_of(first)
                ))
                .write();
                self.lease.record_seed(first, token, body)
            }
            (None, None) => {
                let from: Vec<(&str, u64)> = taken
                    .iter()
                    .map(|(table, held)| (table.as_str(), *held))
                    .collect();
                self.lease.gather_orphan_seed(&from, (first, token), body)
            }
        };
        for (table, held) in &taken {
            if let Err(error) = self.lease.release_orphan(table, *held) {
                crate::log::warn(format!(
                    "the lease of {table} was not released after the seed record was moved out \
                     of it; it runs out by its time: {error}"
                ))
                .write();
            }
        }
        if written.is_ok() {
            self.orphans.clear();
        }
        written
    }

    /// Takes, before a reset by a run without a seed empties anything, the lease of every lease
    /// table of no current component that holds a seed record, so the removal after the reset
    /// ([`Leases::forget_claimed`]) cannot be kept from landing by a process of an earlier release
    /// (`story:seed-under-a-lease`, D6 A; `story:seed-record-after-a-component-is-removed`,
    /// adversary round 1). Answers the first such table whose lease another process holds, every
    /// lease taken released again: the reset is then refused before anything is emptied.
    fn claim(&mut self) -> Result<Option<String>, spindle_kernel::StoreError> {
        self.unclaim();
        for table in self.orphan_tables()? {
            if self.lease.orphan_seed_record(&table)?.is_none() {
                continue;
            }
            match self.lease.take_orphan(&table) {
                Ok(Some(held)) => self.claimed.push((table, held)),
                Ok(None) => {
                    self.unclaim();
                    return Ok(Some(table));
                }
                Err(error) => {
                    self.unclaim();
                    return Err(error);
                }
            }
        }
        Ok(None)
    }

    /// Releases every lease [`Leases::claim`] took, as after a reset that did not empty the
    /// store.
    fn unclaim(&mut self) {
        for (table, held) in std::mem::take(&mut self.claimed) {
            if let Err(error) = self.lease.release_orphan(&table, held) {
                crate::log::warn(format!(
                    "the lease of {table} was not released; it runs out by its time: {error}"
                ))
                .write();
            }
        }
    }

    /// Removes the seed's record from the lease tables [`Leases::claim`] took the leases of, as a
    /// reset by a run without a seed removes it from the current ones (`story:seed-under-a-lease`,
    /// D6 A), each under the lease taken for it, then releases them.
    fn forget_claimed(&mut self) -> Result<(), spindle_kernel::StoreError> {
        let mut forgotten = Ok(());
        for (table, held) in &self.claimed {
            if let Err(error) = self.lease.forget_orphan_seed(table, *held) {
                forgotten = Err(error);
                break;
            }
        }
        self.unclaim();
        forgotten
    }

    /// The lease table of `component`, `<schema>.<prefix>_p_lease`.
    fn table_of(&self, component: &str) -> String {
        format!(
            "{}.{}_p_lease",
            self.schema,
            spindle_kernel::postgres_prefix(component)
        )
    }

    /// Renews every lease held until `asked` reads [`Ask::Stop`], then releases them; notes on
    /// `shared` when each renewal began that landed, and why a lease was lost: another process
    /// took it, or no renewal landed for a whole lease. Between two renewals it answers each ask
    /// about the seed's record.
    fn renewing(mut self, asked: &mpsc::Receiver<Ask>, shared: &Mutex<Renewal>) {
        let term = self.lease.term();
        let mut renewed = Instant::now();
        let lose = |why: String| {
            shared.lock().unwrap_or_else(PoisonError::into_inner).lost = Some(why);
        };
        // Renewals start every fifth of the lease from the start of the last that landed; one that
        // did not land, as one that waited out a lock on the row, is tried again after a pause
        // from [`RETRY`] doubling up to a fifth of the lease, so a renewal queued behind a write
        // holding the row lands as the write ends.
        let mut next = Instant::now() + self.renewal();
        let mut retry = RETRY;
        // A stop or a dropped sender stops the renewals; an ask about the seed's record is
        // answered at once.
        loop {
            match asked.recv_timeout(next.saturating_duration_since(Instant::now())) {
                Ok(Ask::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Ok(ask) => {
                    self.answer(ask);
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            let began = Instant::now();
            let mut failed = None;
            for (component, &token) in &self.held {
                match self.lease.renew(component, token) {
                    Ok(true) => {}
                    Ok(false) => {
                        lose(format!(
                            "the lease of {component} was taken by another process, or its session \
                             ended, so this one writes nothing more and exits"
                        ));
                        return;
                    }
                    Err(error) => failed = Some(format!("{component}: {error}")),
                }
            }
            // A lease a reset took for the removal after it is held as long as the reset takes;
            // one lost meanwhile makes that removal fail by name, as a deposed write does.
            for (table, held) in &self.claimed {
                let _ = self.lease.renew_orphan(table, *held);
            }
            match failed {
                None => {
                    renewed = began;
                    next = began + self.renewal();
                    retry = RETRY;
                    shared
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .renewed = began;
                }
                Some(why) if renewed.elapsed() >= term => {
                    lose(format!(
                        "no renewal of the lease of {} landed for {term:?}, so another process may \
                         hold it, and this one exits: {why}",
                        self.components.join(", ")
                    ));
                    return;
                }
                Some(_) => {
                    next = Instant::now() + retry;
                    retry = retry.saturating_mul(2).min(self.renewal());
                }
            }
        }
        for (component, &token) in &self.held {
            if let Err(error) = self.lease.release(component, token) {
                crate::log::warn(format!(
                    "the lease of {component} was not released, so a process standing by takes it \
                     once it runs out: {error}"
                ))
                .write();
            }
        }
        self.unclaim();
    }
}

/// The refusal of records of the seed that differ, found in the lease tables `tables`.
fn differently(tables: &[String]) -> String {
    format!(
        "the seed is recorded differently in more than one lease table, {}, as a release rolled \
         back across a change of its components may leave it: keep the row `seed` of one of them \
         and remove the others, so a start knows which entries are done",
        tables.join(" and ")
    )
}

/// What the run's thread asks the lease thread ([`Leases::renewing`]).
enum Ask {
    /// Release the leases and end.
    Stop,
    /// Read the seed's record.
    Read(mpsc::Sender<Result<Option<serde_json::Value>, String>>),
    /// Write the seed's record.
    Write(serde_json::Value, mpsc::Sender<Result<(), String>>),
    /// Remove the seed's record, from the lease tables of no current component a [`Ask::Claim`]
    /// took too.
    Forget(mpsc::Sender<Result<(), String>>),
    /// Take the leases of the lease tables of no current component that hold a record, before a
    /// reset empties anything: the table whose lease another process holds, if one does.
    Claim(mpsc::Sender<Result<Option<String>, String>>),
    /// Release the leases a [`Ask::Claim`] took, as after a reset that emptied nothing.
    Unclaim(mpsc::Sender<Result<(), String>>),
}

/// Where a leading run records its seed (`story:seed-under-a-lease`): the row `seed` of the lease
/// table of its first component in name order, read and written by the lease thread on the lease
/// connection, each write landing only while the lease row names the token this run took (D3 A).
/// The run's thread waits for each answer, at most the statements the lease thread may run
/// before it ([`Leases::keep`]); no connection is opened for it.
#[derive(Clone)]
pub(crate) struct SeedRow {
    asks: mpsc::Sender<Ask>,
    wait: Duration,
    /// The lease table, `<schema>.<prefix>_p_lease`.
    table: String,
}

impl SeedRow {
    /// The lease table the record is a row of.
    pub(crate) fn table(&self) -> &str {
        &self.table
    }

    /// What the row holds, `None` where there is none.
    ///
    /// # Errors
    /// A row that cannot be read, or a lease thread that has ended or did not answer in time.
    pub(crate) fn read(&self) -> Result<Option<serde_json::Value>> {
        self.asked(Ask::Read)
    }

    /// Writes `body` as the row.
    ///
    /// # Errors
    /// A row that cannot be written, a lease another process took, or a lease thread that has
    /// ended or did not answer in time.
    pub(crate) fn write(&self, body: serde_json::Value) -> Result<()> {
        self.asked(|reply| Ask::Write(body, reply))
    }

    /// Removes the row.
    ///
    /// # Errors
    /// As [`SeedRow::write`].
    pub(crate) fn forget(&self) -> Result<()> {
        self.asked(Ask::Forget)
    }

    /// Takes, before a reset by a run without a seed empties anything, the lease of each lease
    /// table of no current component that holds a seed record, which [`SeedRow::forget`] then
    /// removes it from (`story:seed-record-after-a-component-is-removed`, adversary round 1):
    /// `Some` with the first such table whose lease another process holds, every lease taken
    /// released again, so the reset is refused.
    ///
    /// # Errors
    /// The tables cannot be read or a lease taken, or a lease thread that has ended or did not
    /// answer in time; every lease taken is released again.
    pub(crate) fn claim(&self) -> Result<Option<String>> {
        self.asked(Ask::Claim)
    }

    /// Releases the leases [`SeedRow::claim`] took, after a reset that emptied nothing.
    ///
    /// # Errors
    /// A lease thread that has ended or did not answer in time.
    pub(crate) fn unclaim(&self) -> Result<()> {
        self.asked(Ask::Unclaim)
    }

    /// Sends the ask `ask` makes of a reply channel and waits for its answer.
    fn asked<T>(&self, ask: impl FnOnce(mpsc::Sender<Result<T, String>>) -> Ask) -> Result<T> {
        let (reply, answer) = mpsc::channel();
        self.asks
            .send(ask(reply))
            .ok()
            .context("the lease thread has ended, so the seed's record is not read or written")?;
        match answer.recv_timeout(self.wait) {
            Ok(answered) => answered.map_err(|why| anyhow::anyhow!(why)),
            Err(mpsc::RecvTimeoutError::Timeout) => bail!(
                "the lease thread did not answer about the seed's record within {:?}",
                self.wait
            ),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                bail!("the lease thread has ended, so the seed's record is not read or written")
            }
        }
    }
}

/// What the lease thread notes for the run's thread ([`Kept`]).
struct Renewal {
    /// Why a lease was lost, once one was.
    lost: Option<String>,
    /// When the last renewal that landed began.
    renewed: Instant,
}

/// The leases a leading run holds, renewed beside it; dropping it releases them.
pub(crate) struct Kept {
    shared: Arc<Mutex<Renewal>>,
    term: Duration,
    /// Where the run records its seed.
    row: SeedRow,
    stop: Option<mpsc::Sender<Ask>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Kept {
    /// Why a lease was lost, once one was, waiting first where the last renewal that landed
    /// began two thirds of the lease ago or longer, as after the process was paused: until the
    /// lease thread renews them all or finds one lost, at most two leases. A write is fenced by
    /// its lease's token either way; this lets the run answer a request `503` rather than with
    /// the store's refusal of the write. A write the fence refused ([`Kept::fenced`]) is a lease
    /// lost at once.
    pub(crate) fn confirmed(&self) -> Option<String> {
        let stale = self.term.saturating_mul(2) / 3;
        let deadline = Instant::now() + self.term.saturating_mul(2);
        loop {
            if let Some(fenced) = Self::fenced() {
                return Some(fenced);
            }
            {
                let renewal = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
                if renewal.lost.is_some() {
                    return renewal.lost.clone();
                }
                if renewal.renewed.elapsed() < stale || Instant::now() >= deadline {
                    return None;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Why a write of this run was refused by its lease's fence, once one was, as when the
    /// database session holding the lease ended and another process took it before the next
    /// renewal: the lease is lost, so the run writes nothing more and exits.
    pub(crate) fn fenced() -> Option<String> {
        spindle_kernel::fenced().map(|why| {
            format!("a write was refused, so this run writes nothing more and exits: {why}")
        })
    }

    /// Where the run records its seed, whatever token it took each lease at
    /// (`story:seed-under-a-lease`).
    pub(crate) fn seed_row(&self) -> SeedRow {
        self.row.clone()
    }
}

impl Drop for Kept {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(Ask::Stop);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// `store migrate`: creates the tables of every component of `spec` bound to `postgres` on
/// `environment`, or checks the ones there, printing one JSON line per component.
pub(crate) fn migrate(spec: &Path, environment: &Path) -> Result<ExitCode> {
    let loaded = spindle_spec::load(spec)?;
    let text = std::fs::read_to_string(environment)
        .with_context(|| format!("reading {}", environment.display()))?;
    let parsed =
        Environment::parse(&text).with_context(|| format!("in {}", environment.display()))?;
    let components = match components(&loaded.ir, &parsed) {
        Ok(components) => components,
        Err(refusal) => {
            println!("{refusal}");
            return Ok(ExitCode::from(crate::REFUSED));
        }
    };
    if components.is_empty() {
        bail!(
            "no component of {} has store.durable bound to postgres on {}: no table to create",
            spec.display(),
            parsed.name
        );
    }
    let database = database()?;
    for component in &components {
        PostgresStore::migrate(&database, component)
            .with_context(|| format!("creating the PostgreSQL tables of {component}"))?;
        println!(
            "{}",
            serde_json::json!({
                "migrated": component,
                "tables": format!("{}_*", postgres_prefix(component)),
                "schema": database.schema,
            })
        );
    }
    Ok(ExitCode::SUCCESS)
}
