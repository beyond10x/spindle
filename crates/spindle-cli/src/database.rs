//! `store.durable` bound to `postgres`: the settings that reach the database, the lease of each
//! component's tables a served run leads under, one store per component bound to it, and `store
//! migrate`, which creates their tables.
//!
//! Settings are read as every setting is, `SPINDLE_SETTING_<NAME>`: `database_url` (required),
//! `database_ca`, `database_schema` (`public`), `database_pool` (4), `database_connections`
//! (required with `database_ca`: the database's connection budget available to this deployment,
//! not the serving role's `CONNECTION LIMIT`) and `database_lease` (10: the seconds a lease is
//! held from its last renewal). A URL is never printed: it holds a password.
//!
//! Several `run` processes of one specification may share one database
//! (`story:several-writers-one-postgres`): each takes the lease of every component's tables in
//! name order ([`Leases`]), and the one that holds them all leads, the one writer of those tables;
//! the others stand by until the leader is gone. Each writes only under its lease: a store it
//! opens refuses a write once another process took the lease
//! ([`spindle_kernel::PostgresStore::open`]).

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

/// How long a lease is held from its last renewal when `database_lease` is not given.
const LEASE: Duration = Duration::from_secs(10);

/// `database_lease`: how long a lease is held from its last renewal, in whole seconds above 0.
fn lease_term() -> Result<Duration> {
    Ok(count("database_lease")?.map_or(LEASE, |seconds| {
        Duration::from_secs(u64::try_from(seconds).unwrap_or(u64::MAX))
    }))
}

/// The leases of every component of a served run bound to `postgres`, taken in name order through
/// one connection of their own, which reads and takes only the lease rows and opens no store.
pub(crate) struct Leases {
    lease: Lease,
    /// The components, in name order.
    components: Vec<String>,
    /// The token of each lease taken, by component.
    held: BTreeMap<String, u64>,
}

impl Leases {
    /// Checks the settings and the connection budget, then connects for the leases of every
    /// component of `ir` bound to `postgres` on `environment`.
    ///
    /// # Errors
    /// A setting missing or malformed, a budget too small, or a database that cannot be reached.
    pub(crate) fn connect(ir: &EssIr, environment: &Environment) -> Result<Self> {
        let database = database()?;
        let term = lease_term()?;
        let mut components =
            components(ir, environment).map_err(|refusal| anyhow::anyhow!("{refusal}"))?;
        components.sort();
        budgeted(&database, &components)?;
        let lease = Lease::connect(&database, term).context("connecting for the leases")?;
        Ok(Self {
            lease,
            components,
            held: BTreeMap::new(),
        })
    }

    /// How often a standby asks for the leases: a third of the lease.
    pub(crate) fn poll(&self) -> Duration {
        self.lease.term() / 3
    }

    /// Renews each lease held and takes, in name order, each one not held yet, stopping at the
    /// first another process holds: `None` once every one is held, else the component whose
    /// lease another process holds.
    ///
    /// # Errors
    /// A lease that cannot be read or written, or one held that another process took.
    pub(crate) fn take(&mut self) -> Result<Option<String>> {
        for component in &self.components {
            match self.held.get(component) {
                Some(&token) => {
                    if !self.lease.renew(component, token)? {
                        bail!("another process took the lease of {component} while standing by");
                    }
                }
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

    /// Keeps every lease held on a thread of its own, renewing each at [`Leases::poll`], until
    /// the [`Kept`] is dropped, which releases them.
    ///
    /// # Errors
    /// The thread cannot start.
    pub(crate) fn keep(self) -> Result<Kept> {
        let lost = Arc::new(Mutex::new(None));
        let (stop, stopped) = mpsc::channel::<()>();
        let noted = Arc::clone(&lost);
        let thread = std::thread::Builder::new()
            .name("spindle-lease".to_owned())
            .spawn(move || self.renewing(&stopped, &noted))
            .context("starting the lease thread")?;
        Ok(Kept {
            lost,
            stop: Some(stop),
            thread: Some(thread),
        })
    }

    /// Renews every lease held until `stopped` reads, then releases them; notes on `lost` why
    /// one was lost: another process took it, or no renewal landed for a whole lease.
    fn renewing(mut self, stopped: &mpsc::Receiver<()>, lost: &Mutex<Option<String>>) {
        let term = self.lease.term();
        let mut renewed = Instant::now();
        // A message or a dropped sender stops the renewals.
        while let Err(mpsc::RecvTimeoutError::Timeout) = stopped.recv_timeout(self.poll()) {
            let mut failed = None;
            for (component, &token) in &self.held {
                match self.lease.renew(component, token) {
                    Ok(true) => {}
                    Ok(false) => {
                        *lost.lock().unwrap_or_else(PoisonError::into_inner) = Some(format!(
                            "another process took the lease of {component}, so this one writes \
                             nothing more and exits"
                        ));
                        return;
                    }
                    Err(error) => failed = Some(format!("{component}: {error}")),
                }
            }
            match failed {
                None => renewed = Instant::now(),
                Some(why) if renewed.elapsed() >= term => {
                    *lost.lock().unwrap_or_else(PoisonError::into_inner) = Some(format!(
                        "no renewal of the lease of {} landed for {term:?}, so another process may \
                         hold it, and this one exits: {why}",
                        self.components.join(", ")
                    ));
                    return;
                }
                Some(_) => {}
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
    }
}

/// The leases a leading run holds, renewed beside it; dropping it releases them.
pub(crate) struct Kept {
    lost: Arc<Mutex<Option<String>>>,
    stop: Option<mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Kept {
    /// Why a lease was lost, once one was.
    pub(crate) fn lost(&self) -> Option<String> {
        self.lost
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Drop for Kept {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
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
