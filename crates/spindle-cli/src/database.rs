//! `store.durable` bound to `postgres`: the settings that reach the database, one store per
//! component bound to it, and `store migrate`, which creates their tables.
//!
//! Settings are read as every setting is, `SPINDLE_SETTING_<NAME>`: `database_url` (required),
//! `database_ca`, `database_schema` (`public`), `database_pool` (4) and `database_connections`
//! (required with `database_ca`: the database's connection budget available to this deployment,
//! not the serving role's `CONNECTION LIMIT`). A URL is never printed: it holds a password.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use spindle_capability::{Capability, Environment};
use spindle_host::Stores;
use spindle_kernel::{Database, Fit, Misfit, PostgresStore, Shapes, postgres_prefix};
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
