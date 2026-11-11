//! `state move`: a `--state` directory's streams moved into the PostgreSQL store of every
//! component bound to `postgres`, versions kept ([`spindle_kernel::StateTarget::copy`]).
//!
//! A `run --state` on the directory is kept off by the directory's lock (`spindle.lock`,
//! [`crate::state_lock`]), which the move takes before it reads a file and holds to its end; a
//! running Spindle on PostgreSQL is not kept off the tables, so it is stopped first. Another
//! `state move` into the same schema is kept off by the lock this one holds
//! ([`spindle_kernel::StateLock`]).

use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use spindle_capability::Environment;
use spindle_kernel::{StateLock, StateSource, StateTarget, postgres_prefix};

/// `state move --from <directory> <spec> --environment <env>`: refuses, writing nothing, a
/// directory without the file of every component bound to `postgres`, a directory another
/// process holds the lock of (a `run --state` on it, naming that process, before any file is read
/// or any connection made), a file holding a value
/// PostgreSQL refuses ([`StateSource::check`], before any connection), a schema another move
/// holds or a role limited to fewer connections than the move holds at once, two
/// ([`StateLock::take`]), and a component whose tables already hold a stream, creating no table;
/// then moves each component's file into its tables, one component's open at a time, printing
/// one JSON line per component, and releases the schema.
pub(crate) fn move_from(from: &Path, spec: &Path, environment: &Path) -> Result<ExitCode> {
    let loaded = spindle_spec::load(spec)?;
    let text = std::fs::read_to_string(environment)
        .with_context(|| format!("reading {}", environment.display()))?;
    let parsed =
        Environment::parse(&text).with_context(|| format!("in {}", environment.display()))?;
    let components = match crate::database::components(&loaded.ir, &parsed) {
        Ok(components) => components,
        Err(refusal) => {
            println!("{refusal}");
            return Ok(ExitCode::from(crate::REFUSED));
        }
    };
    if components.is_empty() {
        bail!(
            "no component of {} has store.durable bound to postgres on {}: nothing to move into",
            spec.display(),
            parsed.name
        );
    }
    let missing: Vec<String> = components
        .iter()
        .map(|component| format!("{component}.sqlite"))
        .filter(|file| !from.join(file).is_file())
        .collect();
    if !missing.is_empty() {
        bail!(
            "{} holds no {}: the state of {} moves from the directory `run --state` kept, one \
             <component>.sqlite per component; nothing was moved",
            from.display(),
            missing.join(", "),
            spec.display()
        );
    }
    // The lock a `run --state` holds on the directory, held to the end of the move: no run
    // writes the files while they are read and copied.
    let _directory = crate::state_lock::directory(from).context("nothing was moved")?;
    let mut sources = Vec::with_capacity(components.len());
    for component in &components {
        let file = from.join(format!("{component}.sqlite"));
        let source =
            StateSource::open(&file).with_context(|| format!("opening {}", file.display()))?;
        source.check().with_context(|| {
            format!(
                "{} of {component} cannot move into PostgreSQL; nothing was moved",
                file.display()
            )
        })?;
        sources.push(source);
    }
    let declared: Vec<String> = loaded
        .ir
        .entities()
        .keys()
        .map(ToString::to_string)
        .collect();
    let database = crate::database::database()?;
    let lock = StateLock::take(&database).context("nothing was moved")?;
    let open = |component: &str| {
        StateTarget::open(&database, component, components.len())
            .with_context(|| format!("opening the PostgreSQL store of {component}"))
    };
    // One target open beside the lock at a time. Opening a loopback database's store creates
    // its tables, so a component without them is opened only once nothing refused the move.
    let mut held = Vec::new();
    let mut absent = Vec::new();
    for (component, source) in components.iter().zip(&sources) {
        if database.ca.is_none() && !lock.has_tables(component)? {
            absent.push(component);
            continue;
        }
        let target = open(component).context("nothing was moved")?;
        if !target.is_empty(&declared, source)? {
            held.push(format!("{component} ({}_*)", postgres_prefix(component)));
        }
        target.close()?;
    }
    if !held.is_empty() {
        bail!(
            "the PostgreSQL tables of {} already hold streams in the schema {}: nothing was \
             moved. Move into tables `store migrate` created empty",
            held.join(", "),
            database.schema
        );
    }
    for component in absent {
        open(component).context("nothing was moved")?.close()?;
    }
    for (component, source) in components.iter().zip(&sources) {
        let target = open(component)?;
        let moved = target.copy(source).with_context(|| {
            format!(
                "moving {} into the PostgreSQL store of {component}",
                from.join(format!("{component}.sqlite")).display()
            )
        })?;
        target.close()?;
        println!(
            "{}",
            serde_json::json!({
                "component": component,
                "streams": moved.streams,
                "events": moved.events,
            })
        );
    }
    lock.release()?;
    Ok(ExitCode::SUCCESS)
}
