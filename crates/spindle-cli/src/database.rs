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
use spindle_kernel::{Database, PostgresStore, postgres_prefix};
use spindle_spec::EssIr;
use spindle_target::{ENV_SETTING_PREFIX, env_name};

/// The implementation of `store.durable` this module opens.
pub(crate) const POSTGRES: &str = "postgres";

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
fn database() -> Result<Database> {
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
fn components(ir: &EssIr, environment: &Environment) -> Result<Vec<String>, spindle_host::Refusal> {
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
/// A setting missing or malformed, a budget too small, or a store the database refused, naming it.
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
    let mut stores = Stores::new();
    for component in &components {
        let store = PostgresStore::open(&database, component, components.len())
            .with_context(|| format!("opening the PostgreSQL store of {component}"))?;
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
