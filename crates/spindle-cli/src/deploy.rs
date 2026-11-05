//! `deploy`: the control process, `spindle_control::Control`, as the binary runs it.

use std::path::Path;
use std::process::ExitCode;
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::json;
use signal_hook::consts::SIGHUP;
use spindle_capability::Environment;
use spindle_control::{Control, Options, PlacementDocument};
use spindle_target::Artifact;

use crate::{SIGNALLED, signals};

/// How often `deploy` looks for a signal.
const POLL: Duration = Duration::from_millis(50);

/// The arguments of `deploy`.
pub(crate) struct Deploying<'a> {
    pub spec: &'a Path,
    pub environment: &'a Path,
    pub placement: &'a Path,
    pub port: u16,
}

/// Runs the control process until SIGINT, SIGTERM or SIGHUP; see the `deploy` command's help.
pub(crate) fn deploy(deploying: &Deploying<'_>) -> Result<ExitCode> {
    let signalled = signals()?;
    let hangup = usize::try_from(SIGHUP).context("a signal number")?;
    signal_hook::flag::register_usize(SIGHUP, std::sync::Arc::clone(&signalled), hangup)
        .context("handling SIGHUP")?;
    let spec = std::path::absolute(deploying.spec)
        .with_context(|| format!("resolving {}", deploying.spec.display()))?;
    let environment_file = std::path::absolute(deploying.environment)
        .with_context(|| format!("resolving {}", deploying.environment.display()))?;
    let loaded = spindle_spec::load(&spec)?;
    let text = std::fs::read_to_string(&environment_file)
        .with_context(|| format!("reading {}", environment_file.display()))?;
    let environment =
        Environment::parse(&text).with_context(|| format!("in {}", environment_file.display()))?;
    let text = std::fs::read_to_string(deploying.placement)
        .with_context(|| format!("reading {}", deploying.placement.display()))?;
    let placement = PlacementDocument::parse(&text)
        .with_context(|| format!("in {}", deploying.placement.display()))?;
    if !spindle_control::ui_embedded() {
        eprintln!(
            "b10x-spindle: this build carries no observe UI (ui/dist was not built before it); \
             GET / answers 503"
        );
    }

    let binary = std::env::current_exe().context("finding this binary")?;
    let arguments = vec![
        "play".to_owned(),
        spec.display().to_string(),
        "--environment".to_owned(),
        environment_file.display().to_string(),
    ];
    let work = tempfile::Builder::new()
        .prefix("spindle-deploy-")
        .tempdir()
        .context("creating the work directory")?;
    let control = Control::start(
        loaded.ir,
        environment,
        placement,
        Box::new(move |_component: &str| Artifact::HostBinary {
            path: binary.clone(),
            args: arguments.clone(),
        }),
        Options::new(work.path()).with_port(deploying.port),
    )?;
    for not_run in control.not_run() {
        eprintln!(
            "b10x-spindle: {} is not run: {}",
            not_run.instance, not_run.why
        );
    }
    println!(
        "{}",
        json!({"listening": control.local_addr().to_string(), "url": control.url()})
    );

    let signal = loop {
        let signal = signalled.load(Ordering::SeqCst);
        if signal != 0 {
            break signal;
        }
        std::thread::sleep(POLL);
    };
    control.stop();
    drop(work);
    Ok(ExitCode::from(SIGNALLED.saturating_add(
        u8::try_from(signal).unwrap_or(u8::MAX - SIGNALLED),
    )))
}
