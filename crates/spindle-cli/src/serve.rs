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
use spindle_ingress::{Caller, Listener, Surface, Unserved};
use spindle_kernel::Decider;
use spindle_spec::EssIr;

use crate::decisions::Answers;
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
    /// With `--decisions` or `--answer-default`, what answers every request's external causes;
    /// without either, the host answers none and refuses a specification that declares one.
    pub decider: Option<Box<dyn Decider + Send>>,
    /// The run's own answers, beneath the decision table a reset's world carries.
    pub answers: Answers,
    /// `--fixed-actor` with `--fixed-caller`, in the order given: each served operation is sent
    /// or read as the first its grant names ([`Surface::fixing_callers`]), a test-backend switch,
    /// not authentication; with none, as no caller.
    pub callers: Vec<Caller>,
}

/// The fixed callers `run` was given: the n-th `--fixed-caller` is the n-th `--fixed-actor`'s.
/// One actor may leave its caller out (the empty caller, which [`fixed`] then binds); with several
/// each needs its own, and no actor is given twice, since the second would never be sent as.
pub(crate) fn callers(
    actors: Vec<String>,
    attributes: Vec<serde_json::Map<String, serde_json::Value>>,
) -> Result<Vec<Caller>> {
    let paired = actors.len() == attributes.len() || (actors.len() == 1 && attributes.is_empty());
    if !paired {
        bail!(
            "{} --fixed-actor and {} --fixed-caller: give each --fixed-actor its own \
             --fixed-caller, in the same order (`{{}}` for an actor with no attribute)",
            actors.len(),
            attributes.len()
        );
    }
    let mut attributes = attributes.into_iter();
    let mut callers: Vec<Caller> = Vec::with_capacity(actors.len());
    for actor in actors {
        if callers.iter().any(|caller| caller.actor == actor) {
            bail!("--fixed-actor {actor} is given twice; the first would always be sent as");
        }
        callers.push(Caller {
            actor,
            attributes: attributes.next().unwrap_or_default(),
        });
    }
    Ok(callers)
}

/// Refuses a fixed caller `ir` would refuse on the first command sent as it: an actor it does not
/// declare, or a caller that is not one of that actor's, by the kernel's own binding
/// ([`spindle_kernel::caller_binds`]).
fn fixed(ir: &EssIr, caller: &Caller) -> Result<()> {
    if !ir
        .actors()
        .keys()
        .any(|name| name.to_string() == caller.actor)
    {
        bail!(
            "--fixed-actor {}: the specification declares no such actor",
            caller.actor
        );
    }
    spindle_kernel::caller_binds(ir, Some(&caller.actor), Some(&caller.attributes))
        .with_context(|| format!("--fixed-caller for --fixed-actor {}", caller.actor))
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
///
/// The `external:` causes of every request are answered by `serving.decider`; without one, a
/// specification that declares an external outcome is refused before listening.
///
/// A `POST /api/reset` may carry a world ([`crate::world`]): rows written once every store is
/// empty, a decision table answering external causes before `serving.decider` does, and a seed
/// identities are minted from, each until the next reset.
///
/// With `serving.callers`, each command is sent as the first of them its grant names, each bound
/// against the model and printed on stderr, in order, before listening; a caller the model refuses
/// stops the run first.
pub(crate) fn serve(ir: EssIr, serving: Serving<'_>) -> Result<ExitCode> {
    for caller in &serving.callers {
        fixed(&ir, caller)?;
    }
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
    match serving.callers.as_slice() {
        [] => {}
        [caller] => eprintln!(
            "b10x-spindle: every command is sent as {} with the caller {} (--fixed-actor, \
             --fixed-caller): a test backend, not authentication",
            caller.actor,
            serde_json::Value::Object(caller.attributes.clone())
        ),
        callers => {
            for (position, caller) in callers.iter().enumerate() {
                eprintln!(
                    "b10x-spindle: fixed actor {} of {}: {} with the caller {}; an operation is \
                     sent as the first fixed actor its grant names (--fixed-actor, \
                     --fixed-caller): a test backend, not authentication",
                    position + 1,
                    callers.len(),
                    caller.actor,
                    serde_json::Value::Object(caller.attributes.clone())
                );
            }
        }
    }
    let surface = Surface::fixing_callers(
        &ir,
        &serving.components,
        serving.binding.as_ref(),
        &serving.callers,
    )?
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
    let opened = match serving.decider {
        Some(decider) => Host::deciding_with_retries(
            ir,
            environment,
            Arc::new(SystemClock),
            directory,
            decider,
            retries,
        ),
        None => Host::with_retries(ir, environment, Arc::new(SystemClock), directory, retries),
    };
    let mut host = match opened {
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
    let answers = serving.answers;
    let worlds = |ir: &EssIr, body: &serde_json::Value| {
        crate::world::read(ir, body, &answers).map_err(|error| format!("{error:#}"))
    };
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
            for failed in exchange.serve_with(&mut host, &worlds) {
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
