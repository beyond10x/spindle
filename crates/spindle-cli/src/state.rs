//! `state move`: a `--state` directory's streams moved into the PostgreSQL store of every
//! component bound to `postgres`, versions kept ([`spindle_kernel::StateTarget::copy`]).
//!
//! Spindle is stopped first: nothing here keeps another process off the directory or the tables.

use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use spindle_capability::Environment;
use spindle_kernel::{StateSource, StateTarget, postgres_prefix};

/// `state move --from <directory> <spec> --environment <env>`: refuses, writing nothing, a
/// directory without the file of every component bound to `postgres`, a file holding a value
/// PostgreSQL refuses ([`StateSource::check`], before any connection) and a component whose
/// tables already hold a stream; then moves each component's file into its tables, printing one
/// JSON line per component.
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
    let database = crate::database::database()?;
    let mut targets = Vec::with_capacity(components.len());
    for component in &components {
        targets.push(
            StateTarget::open(&database, component, components.len())
                .with_context(|| format!("opening the PostgreSQL store of {component}"))?,
        );
    }
    let mut held = Vec::new();
    for (component, target) in components.iter().zip(&targets) {
        if !target.is_empty()? {
            held.push(format!("{component} ({}_*)", postgres_prefix(component)));
        }
    }
    if !held.is_empty() {
        bail!(
            "the PostgreSQL tables of {} already hold streams in the schema {}: nothing was \
             moved. Move into tables `store migrate` created empty",
            held.join(", "),
            database.schema
        );
    }
    for ((component, source), target) in components.iter().zip(&sources).zip(&targets) {
        let moved = target.copy(source).with_context(|| {
            format!(
                "moving {} into the PostgreSQL store of {component}",
                from.join(format!("{component}.sqlite")).display()
            )
        })?;
        println!(
            "{}",
            serde_json::json!({
                "component": component,
                "streams": moved.streams,
                "events": moved.events,
            })
        );
    }
    Ok(ExitCode::SUCCESS)
}
