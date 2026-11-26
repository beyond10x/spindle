//! `run` where a component is reached over the network: every component in one host, the
//! components that require `ingress.network` served over HTTP on 127.0.0.1.

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use ess_gen::http::Binding;
use spindle_capability::{Capability, Environment};
use spindle_host::{Cause, Clock, Host, HostError, ManualClock, Retries, SystemClock};
use spindle_ingress::console::{self, Worlds};
use spindle_ingress::{Caller, Console, Exchange, Listener, Recorder, Surface, Unfixed};
use spindle_spec::EssIr;

use crate::decisions::{self, Run};
use crate::seed::Seeding;
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
    /// The seed file read and checked before listening ([`crate::seed::load`]): its entries are
    /// submitted once listening, one between each two exchanges ([`crate::seed::Seeding`]), and
    /// again, all before the answer, after each reset with no world ([`reset_seeded`]).
    pub seed: Option<crate::seed::Seed>,
    /// `--clock`: the host runs on a manual clock standing at this instant, never moved; the
    /// system clock without it.
    pub clock: Option<std::time::SystemTime>,
    /// `--mint-seed`: the host mints from this seed from its first command, and again after each
    /// reset whose world carries no mint ([`Host::seeded`]).
    pub mint_seed: Option<u64>,
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
    let callers: Vec<Caller> = actors
        .into_iter()
        .map(|actor| Caller {
            actor,
            attributes: attributes.next().unwrap_or_default(),
        })
        .collect();
    if let Err(twice) = Caller::repeated(&callers) {
        bail!("--fixed-actor {} {twice}", twice.actor());
    }
    Ok(callers)
}

/// Refuses a fixed caller `ir` would refuse on the first command sent as it ([`Caller::fixed`]):
/// an actor it does not declare, or a caller that is not one of that actor's, by the kernel's own
/// binding ([`spindle_kernel::caller_binds`]).
fn fixed(ir: &EssIr, caller: &Caller) -> Result<()> {
    match caller.fixed(ir) {
        Ok(()) => Ok(()),
        Err(unbound @ Unfixed::Unbound { .. }) => Err(anyhow::Error::new(unbound)
            .context(format!("--fixed-caller for --fixed-actor {}", caller.actor))),
        Err(refused) => bail!("--fixed-actor {}: {refused}", caller.actor),
    }
}

/// What the startup line says a fixed caller carries: the names of its attributes, in order, never
/// their values (`story:private-local-state`), so a log of the run holds no identity it was fixed
/// with: `the caller attributes owner_id, tenant`, or `no caller attributes`.
fn carried(caller: &Caller) -> String {
    if caller.attributes.is_empty() {
        return "no caller attributes".to_owned();
    }
    let names: Vec<&str> = caller.attributes.keys().map(String::as_str).collect();
    format!("the caller attributes {}", names.join(", "))
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
/// against the model and printed on stderr, in order, before listening: its actor and the names of
/// its attributes, never their values ([`carried`]); a caller the model refuses stops the run
/// first. With `serving.caller_header` instead, each request proves its own caller
/// in that header beside the proxy's secret, `serving.proxy_secret` ([`Surface::proving_callers`]);
/// the header's name and the secret's file, never a value, are printed on stderr before
/// listening, and a name that is no HTTP field name stops the run.
///
/// With `serving.seed`, its entries are submitted once listening, on this thread, one before each
/// wait for the next exchange, so a request never waits for the whole seed; while one is left the
/// wait does not block ([`Seeding`]). A reset drops the entries not yet sent; one with no world
/// then submits them again, all of them, before it answers, and one that gives a world does not
/// ([`reset_seeded`]). A signal stops a reset's seed between two entries. Once a seed run ends
/// having rejected an entry ([`Seeding::failed`]), the run stops listening, the answers already
/// handed back given a moment to be written, and exits 1 (`story:run-fails-on-rejected-line`).
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
            spindle_kernel::private::create_dir_all(directory)
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
            "b10x-spindle: every command is sent as {} with {} (--fixed-actor, \
             --fixed-caller): a test backend, not authentication",
            caller.actor,
            carried(caller)
        ),
        callers => {
            for (position, caller) in callers.iter().enumerate() {
                eprintln!(
                    "b10x-spindle: fixed actor {} of {}: {} with {}; an operation is \
                     sent as the first fixed actor its grant names (--fixed-actor, \
                     --fixed-caller): a test backend, not authentication",
                    position + 1,
                    callers.len(),
                    caller.actor,
                    carried(caller)
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
    for line in surface.report() {
        eprintln!("b10x-spindle: {line}");
    }
    if surface.serves_nothing() {
        bail!(Surface::serves_nothing_reason(&serving.components));
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
    let clock: Arc<dyn Clock> = match serving.clock {
        Some(at) => Arc::new(ManualClock::at(at)),
        None => Arc::new(SystemClock),
    };
    let opened = Host::with_stores(ir, environment, clock, directory, decider, retries, stores)
        .and_then(|host| match serving.mint_seed {
            Some(seed) => host.seeded(seed),
            None => Ok(host),
        });
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
    let mut seeding = serving.seed.map(|seed| {
        eprintln!(
            "b10x-spindle: seed {}: {} entries, submitted one at a time once listening",
            seed.path().display(),
            seed.len()
        );
        Seeding::new(seed, recorder.clone())
    });
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
        if let Some(seeding) = seeding.as_mut() {
            seeding.step(&mut host);
            if seeding.failed() {
                eprintln!(
                    "b10x-spindle: seed {} rejected an entry, so the run exits 1",
                    seeding.path().display()
                );
                drop(listener);
                std::thread::sleep(SIGNAL_POLL);
                return Ok(ExitCode::FAILURE);
            }
        }
        let wait = if seeding.as_ref().is_some_and(Seeding::pending) {
            Duration::ZERO
        } else {
            SIGNAL_POLL
        };
        if let Some(exchange) = listener.next(wait) {
            if exchange.console() == Some(Console::Reset)
                && let Some(seeding) = seeding.as_mut()
            {
                let stopped = || signalled.load(Ordering::SeqCst) != 0;
                reset_seeded(
                    exchange,
                    &mut host,
                    &worlds,
                    seeding,
                    recorder.as_ref(),
                    stopped,
                );
                continue;
            }
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

/// Answers `exchange`, a `POST /api/reset` on a run with a seed: every store emptied and the world
/// its body carries loaded, as [`console::reset`] answers it, and the reset counted on `recorder`
/// where it succeeded. A reset that succeeded drops the entries of the seed run under way not yet
/// sent ([`Seeding::cut`]), so a world is loaded alone. Where the body gives no world
/// ([`console::carries_world`]), `seeding` is then submitted again from its first entry to its
/// end ([`Seeding::restart`], [`Seeding::finish`]) before the answer is sent, so the reset answers
/// with the seed's effects in place (`story:reset-reapplies-seed`).
///
/// Where `stopped` answers `true` between two entries, a signal arrived: nothing more is
/// submitted and the reset is not answered, and the serve loop exits as the signal asks.
fn reset_seeded(
    exchange: Exchange,
    host: &mut Host,
    worlds: Worlds<'_>,
    seeding: &mut Seeding,
    recorder: Option<&Recorder>,
    stopped: impl Fn() -> bool,
) {
    let answer = console::reset(host, exchange.body(), Some(worlds));
    if answer.status == 200 {
        if let Some(recorder) = recorder {
            recorder.count_reset();
        }
        seeding.cut();
        if !console::carries_world(exchange.body()) {
            seeding.restart();
            if !seeding.finish(host, stopped) {
                return;
            }
        }
    }
    exchange.respond(answer);
}
