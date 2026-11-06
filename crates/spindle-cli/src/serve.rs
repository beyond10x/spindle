//! `run` where a component is reached over the network: every component in one host, the
//! components that require `ingress.network` served over HTTP on 127.0.0.1.

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use anyhow::{Context, Result, bail};
use ess_gen::http::Binding;
use spindle_capability::{Capability, Environment};
use spindle_host::{Cause, Host, HostError, Retries, SystemClock};
use spindle_ingress::{Listener, Surface, Unserved};
use spindle_spec::EssIr;

use crate::{REFUSED, SIGNAL_POLL, SIGNALLED, signals};

/// The components of `ir` that require `ingress.network` when each runs on its own.
pub(crate) fn networked(ir: &EssIr) -> Vec<String> {
    ir.components()
        .values()
        .map(|component| component.name.as_str().to_owned())
        .filter(|name| {
            spindle_capability::derive(ir, Some(name)).is_some_and(|requirements| {
                requirements
                    .keys()
                    .any(|capability| capability.as_str() == Capability::INGRESS_NETWORK)
            })
        })
        .collect()
}

/// The `ess-http/1` binding at `path`, read and compiled against `ir` by ESS as `ess specify http
/// validate` compiles it, and held to a path on every route; `Ok(None)` when ESS refused it, the
/// refusals printed on stderr.
pub(crate) fn binding(path: &Path, ir: &EssIr) -> Result<Option<Binding>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let spec = if path
        .extension()
        .is_some_and(|extension| extension == "json")
    {
        ess_http::HttpSpec::from_json(&text)
            .with_context(|| format!("reading {} as HTTP binding JSON", path.display()))?
    } else {
        ess_http::HttpSpec::from_yaml(&text)
            .with_context(|| format!("reading {} as HTTP binding YAML", path.display()))?
    };
    let compiled = match ess_http::compile(&spec, ir, &ess_gen::http::derived(ir)) {
        Ok(compiled) => compiled,
        Err(diagnostics) => {
            eprintln!("{} was refused:\n{diagnostics}", path.display());
            return Ok(None);
        }
    };
    match Binding::new(compiled) {
        Ok(binding) => Ok(Some(binding)),
        Err(open) => {
            eprintln!("{} cannot be served:\n{open}", path.display());
            Ok(None)
        }
    }
}

/// Everything [`serve`] runs on.
pub(crate) struct Serving<'a> {
    /// The environment the host is admitted on.
    pub environment: &'a Environment,
    /// What `store.durable` is bound to, if anything requires it.
    pub durable: Option<&'a str>,
    /// `--state`: the directory of the state files.
    pub state: Option<&'a Path>,
    /// The components to serve.
    pub components: Vec<String>,
    /// The compiled `ess-http/1` binding, if one was given.
    pub binding: Option<Binding>,
    /// The port, `0` for one the system chooses.
    pub port: u16,
    /// What `ingress.network` is bound to; `http-loopback` adds the console routes.
    pub ingress: &'a str,
    /// `--drop-retries`: run each `on_failure: retry` binding as a `drop` binding.
    pub drop_retries: bool,
}

/// Runs every component of `ir` in one host and serves `serving.components` on
/// `127.0.0.1:<port>` until SIGINT or SIGTERM, exiting 130 or 143.
///
/// Prints one JSON line once listening, `{"listening": "<address>", "components": [...]}`, then
/// answers each request in arrival order. Beside the routes ESS publishes it serves the console
/// routes ([`spindle_ingress::Console`]) where `ingress.network` is bound to `http-loopback`. State
/// bound to `sqlite` is kept in the directory `state`, one file per component, or without one in
/// a temporary directory, printed on stderr and removed when the run ends; state bound to
/// `memory` takes none.
///
/// Before listening it prints on stderr one line per operation it does not serve
/// ([`Surface::unserved`]) and, with `drop_retries`, one per binding whose retries it drops. A
/// run that would serve no operation, one of them one the ingress cannot route, is refused. Each
/// command a binding invoked that did not execute is printed on stderr as it is dropped, after
/// its last attempt where the binding retries it ([`spindle_host::RETRY_ATTEMPTS`]).
pub(crate) fn serve(ir: EssIr, serving: Serving<'_>) -> Result<ExitCode> {
    let signalled = signals()?;
    let environment = serving.environment;
    let scratch;
    let directory = match (serving.durable, serving.state) {
        (Some("sqlite"), Some(directory)) => {
            std::fs::create_dir_all(directory)
                .with_context(|| format!("creating the state directory {}", directory.display()))?;
            directory
        }
        (Some("sqlite"), None) => {
            scratch = tempfile::tempdir().context("creating a temporary state directory")?;
            eprintln!(
                "b10x-spindle: state is kept in {}, removed when the run ends",
                scratch.path().display()
            );
            scratch.path()
        }
        (None | Some("memory"), Some(path)) => bail!(
            "nothing on {} keeps state in a file, so --state {} would be ignored",
            environment.name,
            path.display()
        ),
        (None | Some("memory"), None) => {
            scratch = tempfile::tempdir().context("creating an empty state directory")?;
            scratch.path()
        }
        (Some(other), _) => bail!("this build has no `{other}` store for store.durable"),
    };
    let surface = Surface::new(&ir, &serving.components, serving.binding.as_ref())?
        .with_console(serving.ingress)?;
    let unserved = surface.unserved();
    for route in &unserved {
        eprintln!(
            "b10x-spindle: not served: {} at {} {}: {}",
            route.operation,
            route.method.as_str().to_uppercase(),
            route.path,
            route
                .unserved()
                .map(ToString::to_string)
                .unwrap_or_default()
        );
    }
    if !unserved.is_empty()
        && unserved.len() == surface.routes().len()
        && unserved
            .iter()
            .any(|route| matches!(route.unserved(), Some(Unserved::Ingress(_))))
    {
        bail!(
            "{} serves no operation: each one above is one this ingress cannot route or the \
             kernel does not run",
            serving.components.join(", ")
        );
    }
    let retried = spindle_host::retried(&ir);
    let retries = if serving.drop_retries {
        Retries::Dropped
    } else {
        Retries::Retried
    };
    let mut host =
        match Host::with_retries(ir, environment, Arc::new(SystemClock), directory, retries) {
            Ok(host) => host,
            Err(HostError::Refused(refusal)) => {
                println!("{refusal}");
                return Ok(ExitCode::from(REFUSED));
            }
            Err(error) => return Err(error.into()),
        };
    if serving.drop_retries {
        for binding in &retried {
            eprintln!(
                "b10x-spindle: binding {binding} retries on failure; --drop-retries: a failed \
                 invocation is dropped, not retried"
            );
        }
    }
    let listener = Listener::bind(serving.port, surface).context("binding the HTTP listener")?;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    writeln!(
        out,
        "{}",
        serde_json::json!({
            "listening": listener.local_addr().to_string(),
            "components": serving.components,
        })
    )?;
    out.flush()?;
    loop {
        let signal = signalled.load(Ordering::SeqCst);
        if signal != 0 {
            let signal = u8::try_from(signal).unwrap_or(u8::MAX - SIGNALLED);
            return Ok(ExitCode::from(SIGNALLED.saturating_add(signal)));
        }
        if let Some(exchange) = listener.next(SIGNAL_POLL) {
            for failed in exchange.serve(&mut host) {
                let binding = match &failed.cause {
                    Cause::Delivered { binding } | Cause::Tick { binding } => binding.as_str(),
                    Cause::Submitted => "the request",
                };
                let attempts = if failed.attempts > 1 {
                    format!(" after {} attempts", failed.attempts)
                } else {
                    String::new()
                };
                eprintln!(
                    "b10x-spindle: {} did not run {} ({binding}), dropped{attempts}: {}",
                    failed.component, failed.command, failed.error
                );
            }
        }
    }
}
