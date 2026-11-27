//! `run` where a component is reached over the network: every component in one host, the
//! components that require `ingress.network` served over HTTP on 127.0.0.1.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use ess_gen::http::Binding;
use spindle_capability::{Capability, Environment};
use spindle_host::{
    Cause, Clock, Escalated, Escalation, Failed, Host, HostError, ManualClock, RefusedCommand,
    Retries, SystemClock,
};
use spindle_ingress::console::{self, Worlds};
use spindle_ingress::{
    Answer, Caller, Console, Exchange, Listener, Recorder, SeedPhase, SeedProgress, SeedState,
    Surface, Unfixed,
};
use spindle_spec::EssIr;

use crate::decisions::{self, Run};
use crate::log;
use crate::seed::Seeding;
use crate::{REFUSED, SIGNAL_POLL, SIGNALLED, signals, stopping_on};

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
            log::error(format!("{} was refused:\n{diagnostics}", path.display()))
                .bare()
                .write();
            return Ok(None);
        }
    };
    match Binding::new(compiled) {
        Ok(binding) => Ok(Some(binding)),
        Err(open) => {
            log::error(format!("{} cannot be served:\n{open}", path.display()))
                .bare()
                .write();
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
    /// What `ingress.network` is bound to; `http-loopback` adds the console routes where
    /// `console` holds.
    pub ingress: &'a str,
    /// `--console` (`SPINDLE_SETTING_CONSOLE`), decided with `address` where not given
    /// ([`spindle_cli::cli::console_served`]): without it no console route exists, and each
    /// answers `404` as an unrouted path does; a route at a console path is refused at start
    /// ([`Surface::with_console`]) either way.
    pub console: bool,
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
    /// `--observe-series`: how `GET /__spindle/metrics` writes the labels whose values come from
    /// the specification ([`Surface::observing_series`]).
    pub observe_series: spindle_ingress::SeriesMode,
    /// `--trusted-caller-header`: each request proves its caller in this header, which a proxy in
    /// front of the listener sets ([`Surface::proving_callers`]); never with `callers`.
    pub caller_header: Option<&'a str>,
    /// `--trusted-caller-secret`: the file, and the secret read from it ([`proxy_secret`]), that
    /// the proxy presents in [`Surface::SECRET_HEADER`]; given exactly when `caller_header` is.
    pub proxy_secret: Option<(&'a Path, String)>,
    /// `--tls-cert` with `--tls-key`, and `--tls-client-ca`: the TLS every connection is served
    /// over, a client certificate required where it verifies clients
    /// ([`Listener::bind_reading_on_tls`]); plain HTTP without it.
    pub tls: Option<std::sync::Arc<rustls::ServerConfig>>,
    /// The seed file read and checked before listening ([`crate::seed::load`]): its entries are
    /// submitted once listening, one between each two exchanges ([`crate::seed::Seeding`]), and
    /// again, all before the answer, after each reset with no world ([`reset_seeded`]).
    pub seed: Option<crate::seed::Seed>,
    /// `--drain`: how long, once a signal arrives, the run goes on answering the requests already
    /// waiting for it.
    pub drain: Duration,
    /// `--clock`: the host runs on a manual clock standing at this instant, never moved; the
    /// system clock without it.
    pub clock: Option<std::time::SystemTime>,
    /// `--mint-seed`: the host mints from this seed from its first command, and again after each
    /// reset whose world carries no mint ([`Host::seeded`]).
    pub mint_seed: Option<u64>,
    /// The environment's `[egress]`, checked and its calls resolved: each bound event is posted
    /// to its call beside the run's thread once the host is open ([`crate::egress`]).
    pub egress: Option<crate::egress::Plan>,
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
/// routes ([`spindle_ingress::Console`]) where `ingress.network` is bound to `http-loopback` and
/// [`Serving::console`] holds. State
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
/// ([`reset_seeded`]). A signal stops a reset's seed between two entries, and the reset still
/// answers. Once a seed run ends having rejected an entry ([`Seeding::failed`]), the run exits 1
/// (`story:run-fails-on-rejected-line`). The seed's state is answered beside this thread, with or
/// without `--observe`: `GET /__spindle/seed` and `GET /__spindle/ready`, `200` only once the seed
/// run under way has ended, and from the first request on a run without a seed
/// ([`Surface::with_seed`], `story:seed-readiness`).
///
/// Once a signal arrives the run drains (`story:health-and-graceful-drain`): the listener's
/// stopping flag is set by the signal itself ([`Listener::stopping`]), so `GET /__spindle/ready`
/// answers `503` and every request that arrives from then on is answered `503` at once, while this
/// thread finishes the exchange it is answering; it then answers the exchanges already queued, in
/// arrival order, until none is left or `serving.drain` has passed since it finished (a bound past
/// what the clock can name sets no deadline). A reset it takes in the drain is answered `503`, the
/// stores left as they are. Once that is done, or a seed that rejected an entry ends, the loop
/// takes no further exchange: every answer handed back is written, each request still queued is
/// answered `503`, and only then does the run exit ([`Listener::close`],
/// `story:serve-loop-signal-exit-answers`). `GET /__spindle/live` answers `200` while this thread
/// answers an exchange it took or asks the listener for one within [`spindle_ingress::LIVENESS`].
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
            log::info(format!(
                "state is kept in {}, removed when the run ends",
                scratch.path().display()
            ))
            .write();
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
    // On PostgreSQL with `database_lease` the run leads only under the lease of every component's
    // tables (`story:several-writers-one-postgres`): it opens the stores once it holds them all,
    // here where it took them at once, or later, standing by, where another process holds one.
    // Without `database_lease` it takes no lease and opens them as before (D9 C).
    let mut standby = None;
    let mut kept = None;
    let stores = match serving.durable {
        Some(crate::database::POSTGRES) => {
            let leases = crate::database::Leases::connect(&ir, environment)?;
            let held = match leases {
                Some(mut leases) => match leases.take()? {
                    None => {
                        kept = Some(leases.keep()?);
                        None
                    }
                    Some(held) => Some((leases, held)),
                },
                None => None,
            };
            if held.is_some() {
                standby = held;
                spindle_host::Stores::new()
            } else {
                match crate::database::stores(&ir, environment) {
                    Ok(stores) => stores,
                    Err(error) => return crate::database::misfit_refused(error),
                }
            }
        }
        _ => spindle_host::Stores::new(),
    };
    match serving.callers.as_slice() {
        [] => {}
        [caller] => log::warn(format!(
            "every command is sent as {} with {} (--fixed-actor, --fixed-caller): a test \
             backend, not authentication",
            caller.actor,
            carried(caller)
        ))
        .write(),
        callers => {
            for (position, caller) in callers.iter().enumerate() {
                log::warn(format!(
                    "fixed actor {} of {}: {} with {}; an operation is sent as the first fixed \
                     actor its grant names (--fixed-actor, --fixed-caller): a test backend, not \
                     authentication",
                    position + 1,
                    callers.len(),
                    caller.actor,
                    carried(caller)
                ))
                .write();
            }
        }
    }
    // Every component runs in the one host below, so a command reads each entity from its
    // owner's store (`story:cross-component-related-reads`).
    let reached: BTreeSet<String> = ir
        .components()
        .values()
        .map(|component| component.name.as_str().to_owned())
        .collect();
    let reaching = Surface::reaching(&reached);
    let surface = match (serving.caller_header, &serving.proxy_secret) {
        (Some(header), Some((_, secret))) => reaching.proving_callers(
            &ir,
            &serving.components,
            serving.binding.as_ref(),
            header,
            secret,
        )?,
        (Some(header), None) => {
            bail!("--trusted-caller-header {header} needs --trusted-caller-secret <FILE>")
        }
        (None, _) => reaching.fixing_callers(
            &ir,
            &serving.components,
            serving.binding.as_ref(),
            &serving.callers,
        )?,
    };
    // The shadow check runs with the console on or off: a route at a console path exits 1
    // before listening either way, so turning the console on later never changes which route a
    // path reaches.
    let consoled = surface.clone().with_console(serving.ingress)?;
    let surface = if serving.console { consoled } else { surface };
    if let (Some(header), Some((path, _))) = (serving.caller_header, &serving.proxy_secret) {
        log::info(format!(
            "each request's caller is read from its {header} header (--trusted-caller-header) \
             where it carries the proxy's secret, read from {} (--trusted-caller-secret), in its \
             {} header; both are trusted only because a proxy in front of this listener sets \
             them: nothing may reach the listener but that proxy, and the proxy must remove both \
             headers from every client request",
            path.display(),
            Surface::SECRET_HEADER
        ))
        .write();
    }
    let surface = match (serving.observe, PANEL) {
        (Some(capacity), Some(page)) => surface
            .observing_series(capacity, serving.observe_series)
            .with_panel(page),
        (Some(capacity), None) => surface.observing_series(capacity, serving.observe_series),
        (None, _) => surface,
    };
    // The seed's state, published by `Seeding` on this thread and answered by the listener at
    // `GET /__spindle/seed` and `GET /__spindle/ready`, with or without `--observe`.
    let published = serving
        .seed
        .as_ref()
        .map(|seed| SeedState::new(u64::try_from(seed.len()).unwrap_or(u64::MAX)));
    let surface = match &published {
        Some(published) => surface.with_seed(published.clone()),
        None => surface,
    };
    for (line, operation) in reported(&surface) {
        let line = log::warn(line);
        match operation {
            Some(operation) => line.field(operation.kind(), operation.name()),
            None => line,
        }
        .write();
    }
    if surface
        .unserved()
        .iter()
        .any(|route| !route.reads_caller().is_empty())
    {
        log::info(
            "a command that reads caller attributes is served where the run fixes its caller \
             (--fixed-actor with --fixed-caller), a test backend, or where each request proves \
             its caller through a trusted proxy (--trusted-caller-header with \
             --trusted-caller-secret)",
        )
        .write();
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
    // Standing by, the run listens before any host exists, and opens the stores once it holds
    // every lease (`story:several-writers-one-postgres`).
    let mut stores = stores;
    let mut surface = Some(surface);
    let mut standing = None;
    if let Some((leases, held)) = standby {
        let waiting = Listener::bind_standing_by_on(
            serving.address,
            serving.port,
            surface.take().context("the surface")?,
            serving.tls.clone(),
        )
        .context("binding the HTTP listener")?;
        stopping_on(&waiting.stopping())?;
        listening(&waiting, &serving.components)?;
        match stand_by(&waiting, leases, &held, &signalled)? {
            Ok(leading) => {
                kept = Some(leading);
                stores = match crate::database::stores(&ir, environment) {
                    Ok(stores) => stores,
                    Err(error) => {
                        waiting.close(CLOSE_GRACE);
                        return crate::database::misfit_refused(error);
                    }
                };
                standing = Some(waiting);
            }
            Err(code) => {
                waiting.close(CLOSE_GRACE);
                return Ok(code);
            }
        }
    }
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
        Err(misfit @ HostError::Misfit { .. }) => {
            println!("{misfit}");
            return Ok(ExitCode::from(REFUSED));
        }
        Err(error) => return Err(error.into()),
    };
    for component in host.unrecorded() {
        log::info(crate::database::unrecorded(component)).write();
    }
    // Each bound event is posted beside this thread; the cursors are saved on it, between two
    // exchanges and once more as the run ends (`story:event-egress-webhook`).
    let mut egress = serving
        .egress
        .map(|plan| crate::egress::Egress::start(plan, directory))
        .transpose()?;
    if serving.drop_retries {
        for binding in &retried {
            log::info(format!(
                "binding {binding} retries on failure; --drop-retries: a failed invocation is \
                 dropped, not retried"
            ))
            .field("binding", binding.to_string())
            .write();
        }
    }
    // Where a `--state` records the seed applied to it (`seed.json`), a seed it does not record
    // stops the run here, before listening.
    let mut seeding = match (serving.seed, published) {
        // A run that took over the leases from another process skips the seed, which ran under
        // the first leader (`story:several-writers-one-postgres`, D5).
        (Some(seed), Some(published)) if standing.is_some() => {
            log::info(format!(
                "seed {} is not submitted: this run took over the leases of its stores from \
                 another process, which submitted it",
                seed.path().display()
            ))
            .write();
            published.publish(SeedProgress {
                phase: SeedPhase::Ended,
                left: 0,
                ..SeedProgress::starting(published.read().entries)
            });
            None
        }
        (Some(seed), Some(published)) => Some(crate::seed::begin(
            seed,
            serving.state,
            &mut host,
            recorder.clone(),
            published,
        )?),
        _ => None,
    };
    let listener = match (standing, surface) {
        (Some(standing), _) => {
            // The listener standing by leads: it reads through the host from now on.
            standing.lead(host.reader());
            standing
        }
        (None, surface) => {
            let surface = surface.context("the surface")?;
            let listener = match serving.tls {
                Some(tls) => Listener::bind_reading_on_tls(
                    serving.address,
                    serving.port,
                    surface,
                    host.reader(),
                    tls,
                ),
                None => {
                    Listener::bind_reading_on(serving.address, serving.port, surface, host.reader())
                }
            }
            .context("binding the HTTP listener")?;
            // The signal itself stops the listener taking work, while a command may hold this
            // thread.
            stopping_on(&listener.stopping())?;
            listening(&listener, &serving.components)?;
            listener
        }
    };
    let stopping = listener.stopping();
    let worlds = |ir: &EssIr, body: &serde_json::Value| {
        crate::world::read(ir, body, |table| {
            decisions::deciding(&answers, Some(table), recorder.as_ref()).boxed()
        })
        .map_err(|error| format!("{error:#}"))
    };
    // The exit status a signal asks for, once one arrived.
    let signal_exit = || {
        let signal = signalled.load(Ordering::SeqCst);
        (signal != 0).then(|| {
            let signal = u8::try_from(signal).unwrap_or(u8::MAX - SIGNALLED);
            ExitCode::from(SIGNALLED.saturating_add(signal))
        })
    };
    // Once a signal arrived: the exit status it asks for, and until when the exchanges already
    // queued are taken (`story:health-and-graceful-drain`). A bound past what the clock can name
    // sets no deadline: the drain then ends once no exchange is queued.
    let mut draining: Option<(ExitCode, Option<Instant>)> = None;
    // The signal is checked wherever the loop would otherwise take more work: before a seed
    // entry, and before the wait for an exchange. From then on the loop takes only the exchanges
    // already queued, none once the drain bound has passed, and sends no seed entry.
    let code = loop {
        if let Some(egress) = egress.as_mut() {
            egress.tend();
        }
        // A leader that lost a lease writes nothing more: every request still queued is answered
        // `503` below and the run exits 1, for its supervisor to start it again
        // (`story:several-writers-one-postgres`, D4).
        if let Some(lost) = kept.as_ref().and_then(crate::database::Kept::lost) {
            log::error(lost).write();
            break ExitCode::FAILURE;
        }
        match draining {
            Some((code, Some(until))) if Instant::now() >= until => break code,
            Some(_) => {}
            None => {
                if let Some(code) = signal_exit() {
                    stopping.store(true, Ordering::SeqCst);
                    draining = Some((code, Instant::now().checked_add(serving.drain)));
                    continue;
                }
            }
        }
        if draining.is_none()
            && let Some(seeding) = seeding.as_mut()
        {
            seeding.step(&mut host);
            if seeding.failed() {
                log::error(format!(
                    "seed {} rejected an entry, so the run exits 1",
                    seeding.path().display()
                ))
                .write();
                break ExitCode::FAILURE;
            }
            if signal_exit().is_some() {
                continue;
            }
        }
        let wait = if draining.is_some() || seeding.as_ref().is_some_and(Seeding::pending) {
            Duration::ZERO
        } else {
            SIGNAL_POLL
        };
        let Some(exchange) = listener.next(wait) else {
            match draining {
                Some((code, _)) => break code,
                None => continue,
            }
        };
        if exchange.console() == Some(Console::Reset)
            && (draining.is_some() || stopping.load(Ordering::SeqCst))
        {
            // A reset is no command the drain finishes: it would empty the stores of a run that is
            // stopping, so it is refused as a request queued past the drain is, the stores left as
            // they are (`story:health-and-graceful-drain`).
            exchange.respond(Answer::refusal(503, "the run is stopping on a signal"));
            continue;
        }
        if exchange.console() == Some(Console::Reset)
            && let Some(egress) = egress.as_mut()
        {
            // The egress reads the stores the reset empties: it stops before, and starts again
            // on the emptied stores at the top of the next turn.
            egress.pause();
        }
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
        if exchange.console() == Some(Console::Reset)
            && let Some(state) = serving.state
        {
            // A run without a seed on a `--state`: a reset that empties the state or loads a world
            // leaves no seed's entries done, so the marker goes (story:seed-once-on-durable-state).
            let answer = console::reset(&mut host, exchange.body(), Some(&worlds));
            if answer.status == 200 {
                if let Some(recorder) = recorder.as_ref() {
                    recorder.count_reset();
                }
                crate::seed::forget(state);
            }
            exchange.respond(answer);
            continue;
        }
        let failures = exchange.serve_with(&mut host, &worlds);
        let mut escalations = host.take_escalated();
        for failed in failures {
            let bound = match &failed.cause {
                Cause::Delivered { binding } | Cause::Tick { binding } => Some(binding.as_str()),
                Cause::Submitted => None,
            };
            let binding = bound.unwrap_or("the request");
            // The escalation of this attempt, where its binding escalated it rather than drop it.
            let escalated = escalations
                .iter()
                .position(|escalated| {
                    escalated.outcome.is_none()
                        && Some(escalated.binding.as_str()) == bound
                        && escalated.component == failed.component
                        && escalated.command == failed.command
                })
                .map(|at| escalations.remove(at));
            let mut line = log::warn(did_not_run(&failed, binding, escalated.as_ref()))
                .field("component", failed.component.as_str())
                .field("command", failed.command.as_str());
            if let Some(binding) = bound {
                line = line.field("binding", binding);
            }
            if let Some(id) = failed.dead_letter {
                line = line.field("dead_letter", id);
            }
            if let Some(why) = &failed.not_kept {
                line = line.field("not_kept", why.as_str());
            }
            line.field("attempts", failed.attempts)
                .field("error", failed.error.as_str())
                .write();
        }
        // A refused command its store kept as a dead letter prints nothing, as before; one its
        // store could not keep prints why (story:dead-letters-kept-and-replayed).
        for refused in host.take_unkept() {
            let bound = match &refused.cause {
                Cause::Delivered { binding } | Cause::Tick { binding } => Some(binding.as_str()),
                Cause::Submitted => None,
            };
            let mut line = log::warn(refused_not_kept(&refused, bound.unwrap_or("the request")))
                .field("component", refused.component.as_str())
                .field("command", refused.command.as_str());
            if let Some(binding) = bound {
                line = line.field("binding", binding);
            }
            if let Some(why) = &refused.not_kept {
                line = line.field("not_kept", why.as_str());
            }
            line.field("outcome", refused.outcome.as_str())
                .field("attempts", refused.attempts)
                .field("error", refused.error.name.as_str())
                .write();
        }
        for escalated in escalations {
            let refusal = escalated.outcome.as_deref().unwrap_or("a failure");
            log::warn(format!(
                "{} refused {} ({}) with {refusal}, {}",
                escalated.component,
                escalated.command,
                escalated.binding,
                escalation(&escalated)
            ))
            .field("component", escalated.component.as_str())
            .field("command", escalated.command.as_str())
            .field("binding", escalated.binding.as_str())
            .write();
        }
    };
    // Every answer handed back is written, and each request still queued is answered 503, before
    // the run exits; readiness reads 503 meanwhile.
    stopping.store(true, Ordering::SeqCst);
    listener.close(CLOSE_GRACE);
    // The lanes stop, each after its post under way, and the cursors they handed back are saved.
    drop(egress);
    Ok(code)
}

/// Prints the line a served run prints once it listens:
/// `{"listening": "<address>", "components": [...]}`.
fn listening(listener: &Listener, components: &[String]) -> Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    writeln!(
        out,
        "{}",
        serde_json::json!({
            "listening": listener.local_addr().to_string(),
            "components": components,
        })
    )?;
    out.flush()?;
    Ok(())
}

/// Stands by on `listener` until this run holds every lease of `leases`
/// (`story:several-writers-one-postgres`): it asks for them every [`crate::database::Leases::poll`],
/// a third of the lease, answering each request handed to it meanwhile `503`, and prints one line
/// per component once it took them all. Answers the leases kept from then on, or the exit status a
/// signal asks for, which stops the standby.
///
/// # Errors
/// A lease that cannot be read or written, or the lease thread that cannot start.
fn stand_by(
    listener: &Listener,
    mut leases: crate::database::Leases,
    held: &str,
    signalled: &std::sync::atomic::AtomicUsize,
) -> Result<std::result::Result<crate::database::Kept, ExitCode>> {
    log::info(format!(
        "standing by: another process holds the lease of {held}, so this run serves nothing \
         until it takes the leases of {}",
        leases.components().join(", ")
    ))
    .field("component", held)
    .write();
    let mut asked = Instant::now();
    loop {
        let signal = signalled.load(Ordering::SeqCst);
        if signal != 0 {
            let signal = u8::try_from(signal).unwrap_or(u8::MAX - SIGNALLED);
            return Ok(Err(ExitCode::from(SIGNALLED.saturating_add(signal))));
        }
        let wait = leases
            .poll()
            .saturating_sub(asked.elapsed())
            .min(SIGNAL_POLL);
        if let Some(exchange) = listener.next(wait) {
            exchange.respond(Answer::refusal(
                503,
                "the run is on standby: another process holds the lease of its store",
            ));
            continue;
        }
        if asked.elapsed() < leases.poll() {
            continue;
        }
        asked = Instant::now();
        if leases.take()?.is_none() {
            for component in leases.components() {
                log::info(format!("took the lease of {component}"))
                    .field("component", component.as_str())
                    .write();
            }
            return Ok(Ok(leases.keep()?));
        }
    }
}

/// Each line of [`Surface::report`], as it is written, with the operation it names: the route of
/// each operation not served ([`Route::not_served`](spindle_ingress::Route::not_served)), in the
/// order of [`Surface::unserved`], then the route of each outcome refused
/// ([`Route::refused`](spindle_ingress::Route::refused)), route by route, the order `report`
/// writes them in. A line gets its operation only where it names it, in backticks, so a line
/// `report` adds or reorders is still written, without one.
fn reported(surface: &Surface) -> Vec<(String, Option<&ess_http::Operation>)> {
    let unserved = surface
        .unserved()
        .into_iter()
        .filter(|route| route.not_served().is_some())
        .map(|route| &route.operation);
    let refused = surface
        .routes()
        .iter()
        .flat_map(|route| route.refused().iter().map(move |_| &route.operation));
    let mut operations = unserved.chain(refused);
    surface
        .report()
        .into_iter()
        .map(|line| {
            let operation = operations
                .next()
                .filter(|operation| line.contains(&format!("`{}", operation.name())));
            (line, operation)
        })
        .collect()
}

/// What a served run says an escalation did: published its event, or, unmet, published nothing,
/// naming the binding, the event and what the command's input lacks.
/// The line a served run prints for `failed`, a command `binding` invoked that did not execute:
/// its escalation where its binding escalated it (`escalated`), else the dead letter its store
/// kept it as (story:dead-letters-kept-and-replayed), else dropped, naming why its store did not
/// keep it where it is one the store could not keep.
fn did_not_run(failed: &Failed, binding: &str, escalated: Option<&Escalated>) -> String {
    let attempts = if failed.attempts > 1 {
        format!(" after {} attempts", failed.attempts)
    } else {
        String::new()
    };
    let fate = match (escalated, failed.dead_letter) {
        (Some(escalated), _) => escalation(escalated),
        (None, Some(id)) => format!("kept as dead letter {id}"),
        (None, None) => "dropped".to_owned(),
    };
    let unkept = match (escalated, &failed.not_kept) {
        (None, Some(why)) => format!(", not kept as a dead letter: {why}"),
        _ => String::new(),
    };
    format!(
        "{} did not run {} ({binding}), {fate}{attempts}{unkept}: {}",
        failed.component, failed.command, failed.error
    )
}

/// The line a served run prints for `refused`, a command `binding` invoked that its model refused
/// and whose dead letter its store could not keep: dropped, naming the store's reason.
fn refused_not_kept(refused: &RefusedCommand, binding: &str) -> String {
    let attempts = if refused.attempts > 1 {
        format!(" after {} attempts", refused.attempts)
    } else {
        String::new()
    };
    let why = refused.not_kept.as_deref().unwrap_or("no reason given");
    format!(
        "{} refused {} ({binding}) with {}, dropped{attempts}, not kept as a dead letter: {why}: {}",
        refused.component, refused.command, refused.outcome, refused.error.name
    )
}

fn escalation(escalated: &Escalated) -> String {
    match &escalated.escalation {
        Escalation::Published(_) => format!("escalated into {}", escalated.event),
        Escalation::Unmet(why) => format!(
            "escalation into {} unmet, nothing published: {why}",
            escalated.event
        ),
    }
}

/// How long the run waits, once it stops, for the answers it handed back to be written.
const CLOSE_GRACE: Duration = Duration::from_secs(2);

/// Answers `exchange`, a `POST /api/reset` on a run with a seed: every store emptied and the world
/// its body carries loaded, as [`console::reset`] answers it, and the reset counted on `recorder`
/// where it succeeded. A reset that succeeded drops the entries of the seed run under way not yet
/// sent ([`Seeding::cut`]), so a world is loaded alone, and on a `--state` records every entry done
/// ([`Seeding::loaded`]), so a restart sends none. Where the body gives no world
/// ([`console::carries_world`]), `seeding` is then submitted again from its first entry to its
/// end ([`Seeding::restart`], [`Seeding::finish`]) before the answer is sent, so the reset answers
/// with the seed's effects in place (`story:reset-reapplies-seed`).
///
/// Where `stopped` answers `true` between two entries, a signal arrived: nothing more is
/// submitted, the reset is answered all the same, with the entries done in place and the marker
/// counting them, and the serve loop then exits as the signal asks
/// (`story:serve-loop-signal-exit-answers`).
fn reset_seeded(
    exchange: Exchange,
    host: &mut Host,
    worlds: Worlds<'_>,
    seeding: &mut Seeding,
    recorder: Option<&Recorder>,
    stopped: impl Fn() -> bool,
) {
    let plain = !console::carries_world(exchange.body());
    if plain {
        // No request reads the run ready while the reset empties its stores and seeds them again
        // (`story:seed-readiness`).
        seeding.resetting();
    }
    let answer = console::reset(host, exchange.body(), Some(worlds));
    if answer.status != 200 && plain {
        seeding.unreset();
    }
    if answer.status == 200 {
        if let Some(recorder) = recorder {
            recorder.count_reset();
        }
        if console::carries_world(exchange.body()) {
            seeding.loaded();
        } else {
            seeding.cut();
            seeding.restart();
            seeding.finish(host, stopped);
        }
    }
    exchange.respond(answer);
}

#[cfg(test)]
mod tests {
    use spindle_host::{Cause, Failed, RefusedCommand};
    use spindle_kernel::DeclaredError;

    use super::{did_not_run, refused_not_kept};

    fn failed(dead_letter: Option<u64>, not_kept: Option<&str>) -> Failed {
        Failed {
            component: "shipping".to_owned(),
            command: "depot.shipping.Ship".to_owned(),
            cause: Cause::Delivered {
                binding: "ship-on-order".to_owned(),
            },
            caused_by: Some("depot.orders.PlaceOrder".to_owned()),
            error: "the cause is undecided".to_owned(),
            kind: Some("undecided"),
            attempts: 5,
            dead_letter,
            not_kept: not_kept.map(str::to_owned),
        }
    }

    /// A dead letter its store could not keep is printed dropped, with the store's reason
    /// (story:dead-letters-kept-and-replayed).
    #[test]
    fn a_dead_letter_its_store_could_not_keep_is_printed_dropped_with_why() {
        assert_eq!(
            did_not_run(
                &failed(None, Some("store: the disk is full")),
                "ship-on-order",
                None
            ),
            "shipping did not run depot.shipping.Ship (ship-on-order), dropped after 5 attempts, \
             not kept as a dead letter: store: the disk is full: the cause is undecided"
        );
        assert_eq!(
            did_not_run(&failed(Some(3), None), "ship-on-order", None),
            "shipping did not run depot.shipping.Ship (ship-on-order), kept as dead letter 3 \
             after 5 attempts: the cause is undecided"
        );
        assert_eq!(
            did_not_run(&failed(None, None), "ship-on-order", None),
            "shipping did not run depot.shipping.Ship (ship-on-order), dropped after 5 attempts: \
             the cause is undecided"
        );
    }

    /// A refused command whose dead letter its store could not keep is printed dropped, with the
    /// store's reason (story:dead-letters-kept-and-replayed).
    #[test]
    fn a_refused_command_its_store_could_not_keep_is_printed_dropped_with_why() {
        let refused = RefusedCommand {
            component: "shipping".to_owned(),
            command: "depot.shipping.Ship".to_owned(),
            cause: Cause::Delivered {
                binding: "ship-on-order".to_owned(),
            },
            caused_by: Some("depot.orders.PlaceOrder".to_owned()),
            outcome: "busy".to_owned(),
            error: DeclaredError {
                name: "depot.shipping.Busy".to_owned(),
                fields: serde_json::Map::new(),
                unknown_instance: false,
            },
            attempts: 5,
            dead_letter: None,
            not_kept: Some("store: the disk is full".to_owned()),
        };
        assert_eq!(
            refused_not_kept(&refused, "ship-on-order"),
            "shipping refused depot.shipping.Ship (ship-on-order) with busy, dropped after 5 \
             attempts, not kept as a dead letter: store: the disk is full: depot.shipping.Busy"
        );
    }
}
