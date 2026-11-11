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
use spindle_spec::EssIr;

use crate::decisions::{self, Run};
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
    /// `--address`: the IP address the listener binds, 127.0.0.1 by default; off loopback only
    /// with `caller_header`, every request without the proxy's secret refused before it is read
    /// ([`Listener::bind_reading_on`]).
    pub address: std::net::IpAddr,
    /// What `ingress.network` is bound to; `http-loopback` adds the console routes.
    pub ingress: &'a str,
    /// `--drop-retries`: run each `on_failure: retry` binding as a `drop` binding.
    pub drop_retries: bool,
    /// With `--decisions` or `--answer-default`, or where every external cause is bound to an
    /// HTTP call: `answers` and their calls answer every request's external causes
    /// ([`decisions::deciding`]); otherwise the host answers none and refuses a specification
    /// that declares one.
    pub decides: bool,
    /// The run's own answers, beneath the decision table a reset's world carries, and the HTTP
    /// calls the environment binds causes to above both ([`Run::calling`]).
    pub answers: Run,
    /// `--fixed-actor` with `--fixed-caller`, in the order given: each served operation is sent
    /// or read as the first its grant names ([`Surface::fixing_callers`]), a test-backend switch,
    /// not authentication; with none, as no caller.
    pub callers: Vec<Caller>,
    /// `--observe`, with how many request records to keep: the surface records every request
    /// outside `/__spindle/` and answers `GET /__spindle/requests` ([`Surface::observing`]) and
    /// `GET /__spindle/metrics`, beside this thread as every read is, and
    /// `GET /__spindle/ui` with the panel this build embeds ([`PANEL`]).
    pub observe: Option<usize>,
    /// `--trusted-caller-header`: each request proves its caller in this header, which a proxy in
    /// front of the listener sets ([`Surface::proving_callers`]); never with `callers`.
    pub caller_header: Option<&'a str>,
    /// `--trusted-caller-secret`: the file, and the secret read from it ([`proxy_secret`]), that
    /// the proxy presents in [`Surface::SECRET_HEADER`]; given exactly when `caller_header` is.
    pub proxy_secret: Option<(&'a Path, String)>,
}

/// The secret in the file `path` (`--trusted-caller-secret`), one trailing newline (`\n` or
/// `\r\n`) dropped. No message names the secret or any of the file's text.
///
/// # Errors
/// A file that cannot be read as UTF-8, is empty once the newline is dropped, or holds a byte
/// that is no visible ASCII character, which no header could carry.
pub(crate) fn proxy_secret(path: &Path) -> Result<String> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading --trusted-caller-secret {}", path.display()))?;
    let secret = text.strip_suffix('\n').map_or(text.as_str(), |line| {
        line.strip_suffix('\r').unwrap_or(line)
    });
    if secret.is_empty() {
        bail!(
            "--trusted-caller-secret {} is empty: the proxy must present a secret",
            path.display()
        );
    }
    if !secret.bytes().all(|byte| byte.is_ascii_graphic()) {
        bail!(
            "--trusted-caller-secret {} holds a byte that is no visible ASCII character, which no \
             header carries",
            path.display()
        );
    }
    Ok(secret.to_owned())
}

// `PANEL`: the observe panel, `ui/dist-panel/panel.html`, where it was built before this crate
// (`build.rs`).
include!(concat!(env!("OUT_DIR"), "/panel.rs"));

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
/// `<serving.address>:<port>`, `127.0.0.1` by default, until SIGINT or SIGTERM, exiting 130 or
/// 143.
///
/// Prints one JSON line once listening, `{"listening": "<address>", "components": [...]}`, then
/// runs each command and reset on this thread in arrival order, and answers each read beside it
/// through the host's reader ([`spindle_ingress::Listener::bind_reading`]): a view,
/// `GET /api/model`, `GET /api/events` and every `GET /__spindle/` route, so none waits for a
/// command, a slow external call included. Beside the routes ESS publishes it serves the console
/// routes ([`spindle_ingress::Console`]) where `ingress.network` is bound to `http-loopback`. State
/// bound to `sqlite` is kept in the directory `state`, one file per component, or without one in
/// a temporary directory, printed on stderr and removed when the run ends; state bound to
/// `memory` takes none, and state bound to `postgres` is kept in the database the `database_*`
/// settings name, each component in its own tables, opened before anything else is printed
/// ([`crate::database::stores`]).
///
/// Before listening it prints on stderr one line per operation it does not serve
/// ([`Surface::unserved`]), one per outcome of a served command it refuses where a request
/// selects it ([`spindle_ingress::Route::refused`]) and, with `drop_retries`, one per binding
/// whose retries it drops. A run that would serve no operation, one of them one the ingress
/// cannot route, is refused. Each
/// command a binding invoked that did not execute is printed on stderr as it is dropped, after
/// its last attempt where the binding retries it ([`spindle_host::RETRY_ATTEMPTS`]).
///
/// The `external:` causes of every request are answered where `serving.decides`: a cause the
/// environment binds to an HTTP call by one request to it ([`Run::calling`]), any other by
/// `serving.answers`; otherwise a specification that declares an external outcome is refused
/// before listening.
///
/// A `POST /api/reset` may carry a world ([`crate::world`]): rows written once every store is
/// empty, a decision table answering external causes before `serving.answers` do, and a seed
/// identities are minted from, each until the next reset.
///
/// With `serving.observe`, and only then, the decider is wrapped so that each external cause it
/// answers is noted on the surface's recorder with what answered it ([`decisions::deciding`]).
///
/// With `serving.callers`, each command is sent as the first of them its grant names, each bound
/// against the model and printed on stderr, in order, before listening; a caller the model refuses
/// stops the run first. With `serving.caller_header` instead, each request proves its own caller
/// in that header beside the proxy's secret, `serving.proxy_secret` ([`Surface::proving_callers`]);
/// the header's name and the secret's file, never a value, are printed on stderr before
/// listening, and a name that is no HTTP field name stops the run.
pub(crate) fn serve(ir: EssIr, serving: Serving<'_>) -> Result<ExitCode> {
    for caller in &serving.callers {
        fixed(&ir, caller)?;
    }
    let signalled = signals()?;
    let environment = serving.environment;
    let scratch;
    let _lock;
    let directory = match (serving.durable, serving.state) {
        (Some("sqlite"), Some(directory)) => {
            std::fs::create_dir_all(directory)
                .with_context(|| format!("creating the state directory {}", directory.display()))?;
            _lock = crate::state_lock::directory(directory)?;
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
        (None | Some("memory" | "postgres"), Some(path)) => bail!(
            "nothing on {} keeps state in a file, so --state {} would be ignored",
            environment.name,
            path.display()
        ),
        (None | Some("memory" | "postgres"), None) => {
            scratch = tempfile::tempdir().context("creating an empty state directory")?;
            scratch.path()
        }
        (Some(other), _) => bail!("this build has no `{other}` store for store.durable"),
    };
    let stores = match serving.durable {
        Some(crate::database::POSTGRES) => crate::database::stores(&ir, environment)?,
        _ => spindle_host::Stores::new(),
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
    let surface = match (serving.caller_header, &serving.proxy_secret) {
        (Some(header), Some((_, secret))) => Surface::proving_callers(
            &ir,
            &serving.components,
            serving.binding.as_ref(),
            header,
            secret,
        )?,
        (Some(header), None) => {
            bail!("--trusted-caller-header {header} needs --trusted-caller-secret <FILE>")
        }
        (None, _) => Surface::fixing_callers(
            &ir,
            &serving.components,
            serving.binding.as_ref(),
            &serving.callers,
        )?,
    }
    .with_console(serving.ingress)?;
    if let (Some(header), Some((path, _))) = (serving.caller_header, &serving.proxy_secret) {
        eprintln!(
            "b10x-spindle: each request's caller is read from its {header} header \
             (--trusted-caller-header) where it carries the proxy's secret, read from {} \
             (--trusted-caller-secret), in its {} header; both are trusted only because a proxy \
             in front of this listener sets them: nothing may reach the listener but that proxy, \
             and the proxy must remove both headers from every client request",
            path.display(),
            Surface::SECRET_HEADER
        );
    }
    let surface = match (serving.observe, PANEL) {
        (Some(capacity), Some(page)) => surface.observing(capacity).with_panel(page),
        (Some(capacity), None) => surface.observing(capacity),
        (None, _) => surface,
    };
    let unserved = surface.unserved();
    for line in unserved.iter().filter_map(|route| route.not_served()) {
        eprintln!("b10x-spindle: {line}");
    }
    for route in surface.routes() {
        for (outcome, why) in route.refused() {
            eprintln!(
                "b10x-spindle: unsupported outcome `{}/{outcome}` at {} {}, answered 501 where a \
                 request selects it: {why}",
                route.operation.name(),
                route.method.as_str().to_uppercase(),
                route.path
            );
        }
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
    let answers = serving.answers;
    let recorder = surface.recorder().cloned();
    let decider = serving
        .decides
        .then(|| decisions::deciding(&answers, None, recorder.as_ref()).boxed());
    let opened = Host::with_stores(
        ir,
        environment,
        Arc::new(SystemClock),
        directory,
        decider,
        retries,
        stores,
    );
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
    let listener = Listener::bind_reading_on(serving.address, serving.port, surface, host.reader())
        .context("binding the HTTP listener")?;
    let worlds = |ir: &EssIr, body: &serde_json::Value| {
        crate::world::read(ir, body, |table| {
            decisions::deciding(&answers, Some(table), recorder.as_ref()).boxed()
        })
        .map_err(|error| format!("{error:#}"))
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
