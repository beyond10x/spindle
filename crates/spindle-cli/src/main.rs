//! `b10x-spindle`: load an ESS specification, derive what it needs, admit it, and run or play it.

mod calls;
mod component;
mod conform;
mod database;
mod decisions;
mod deploy;
mod egress;
mod lines;
mod listener_tls;
mod log;
mod replay;
mod scaled;
mod seed;
mod serve;
mod state;
mod state_lock;
mod token;
mod world;

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::Parser;
use signal_hook::consts::{SIGINT, SIGTERM};
use spindle_capability::{
    Admission, Binding, Capability, DECISIONS_FILE, Environment, Requirements,
};
use spindle_cli::cli::{Cli, Command, LogFormat, StateCommand, StoreCommand};
use spindle_host::{Host, HostError, rally};
use spindle_kernel::{MemoryStore, SqliteStore, Store};

/// Exit status of an `admit` the environment refused.
const REFUSED: u8 = 3;

/// The exit status of a command line `run` refuses before anything else, as clap exits on one it
/// cannot parse.
const USAGE: u8 = 2;

/// The implementation an environment binds `identity.mint` to for a mint drawn from a seed
/// (`--mint-seed`, 0 without it), which `run` reading stdin mints with.
const SEEDED_MINT: &str = "seeded";

/// One command line read by `run`.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    command: String,
    #[serde(default)]
    input: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    external: BTreeMap<String, bool>,
    #[serde(default)]
    actor: Option<String>,
    #[serde(default)]
    caller: Option<serde_json::Map<String, serde_json::Value>>,
    /// The instant the clock moves forward to before the command runs (`run --clock`).
    #[serde(default)]
    at: Option<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    // `--log-format json` is read only by a served run, as the flags `--stdin` conflicts with in
    // clap are; `text` is its default, so clap cannot refuse it there and it is refused here, as
    // clap refuses a conflict.
    if let Command::Run {
        stdin: true,
        log_format: LogFormat::Json,
        ..
    } = &cli.command
    {
        <Cli as clap::CommandFactory>::command()
            .error(
                clap::error::ErrorKind::ArgumentConflict,
                "the argument '--stdin' cannot be used with '--log-format json': only a run \
                 serving HTTP writes its log as JSON lines",
            )
            .exit();
    }
    if let Command::Run { log_format, .. } = &cli.command {
        log::set(*log_format);
    }
    match run(cli) {
        Ok(code) => code,
        Err(error) => {
            log::error(format!("{error:#}")).write();
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Command::Check { spec, json } => {
            let loaded = spindle_spec::load_model(&spec)?;
            let ir = &loaded.ir;
            let summary = serde_json::json!({
                "system": ir.system().to_string(),
                "version": ir.version().to_string(),
                "files": loaded.files.len(),
                "entities": ir.entities().len(),
                "commands": ir.commands().len(),
                "events": ir.events().len(),
                "views": ir.views().len(),
                "bindings": ir.bindings().len(),
                "components": ir.components().values().map(|component| component.name.as_str()).collect::<Vec<_>>(),
            });
            if json {
                println!("{summary:#}");
            } else {
                println!(
                    "{} {} compiled from {} file(s): {} entities, {} commands, {} events, {} views, {} bindings, {} components",
                    summary["system"].as_str().unwrap_or_default(),
                    summary["version"].as_str().unwrap_or_default(),
                    loaded.files.len(),
                    ir.entities().len(),
                    ir.commands().len(),
                    ir.events().len(),
                    ir.views().len(),
                    ir.bindings().len(),
                    ir.components().len(),
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Capabilities {
            spec,
            component,
            json,
        } => {
            let requirements = requirements(&spec, component.as_deref())?;
            if json {
                println!("{:#}", serde_json::to_value(&requirements)?);
            } else {
                for (capability, reasons) in &requirements {
                    println!("{capability}");
                    for reason in reasons {
                        println!("  {} {}: {}", reason.kind, reason.name, reason.because);
                    }
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Metrics {
            spec,
            component,
            json,
        } => {
            let loaded = spindle_spec::load(&spec)?;
            let catalogue =
                spindle_telemetry::catalogue(&loaded.ir, &component).with_context(|| {
                    format!("{} declares no component {component:?}", spec.display())
                })?;
            if json {
                println!("{:#}", serde_json::to_value(&catalogue)?);
            } else {
                for counter in &catalogue.counters {
                    println!("{} {} {}", counter.name, counter.kind, counter.source);
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Admit {
            spec,
            environment,
            component,
            json,
        } => {
            let loaded = spindle_spec::load_model(&spec)?;
            let requirements = spindle_capability::derive(&loaded.ir, component.as_deref())
                .with_context(|| {
                    format!("{} declares no component {component:?}", spec.display())
                })?;
            let text = std::fs::read_to_string(&environment)
                .with_context(|| format!("reading {}", environment.display()))?;
            let environment = Environment::parse(&text)
                .with_context(|| format!("in {}", environment.display()))?;
            let admission = spindle_capability::admit(&requirements, &environment);
            if json {
                println!("{:#}", serde_json::to_value(&admission)?);
            } else {
                match &admission {
                    Admission::Admitted { bindings } => {
                        println!("admitted on {}", environment.name);
                        for binding in bindings {
                            println!("  {} = {}", binding.capability, binding.implementation);
                        }
                    }
                    Admission::Refused { missing } => {
                        println!("refused on {}: no implementation for", environment.name);
                        for capability in missing {
                            println!("  {capability}");
                        }
                    }
                }
            }
            for line in unsupported_outcomes(&loaded.ir, component.as_deref()) {
                eprintln!("b10x-spindle: {line}");
            }
            for line in unserved_views(&loaded.ir, component.as_deref()) {
                eprintln!("b10x-spindle: {line}");
            }
            Ok(if admission.is_admitted() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(REFUSED)
            })
        }
        Command::Run {
            spec,
            environment,
            scenario: Some(scenario),
            decisions,
            clock,
            mint_seed,
            ..
        } => replay::replay(
            &spec,
            &environment,
            &scenario,
            decisions.as_deref(),
            clock,
            mint_seed,
        ),
        Command::Run {
            spec,
            environment,
            state,
            stdin,
            decisions,
            answer_default,
            http,
            port,
            address,
            tls,
            console,
            drop_retries,
            fixed_actor,
            fixed_caller,
            trusted_caller_header,
            trusted_caller_secret,
            caller_token,
            observe,
            observe_requests,
            observe_series,
            seed,
            drain,
            clock,
            mint_seed,
            log_format,
            scenario: None,
        } => execute(&Execute {
            spec: &spec,
            environment: &environment,
            state: state.as_deref(),
            stdin,
            decisions: decisions.as_deref(),
            answer_default,
            http: http.as_deref(),
            port,
            address,
            tls: tls
                .tls_cert
                .as_deref()
                .zip(tls.tls_key.as_deref())
                .map(|(cert, key)| listener_tls::Files {
                    cert,
                    key,
                    client_ca: tls.tls_client_ca.as_deref(),
                }),
            console,
            drop_retries,
            callers: serve::callers(fixed_actor, fixed_caller)?,
            observe: observe.then_some(usize::from(observe_requests)),
            observe_series,
            caller_header: trusted_caller_header.as_deref(),
            caller_secret: trusted_caller_secret.as_deref(),
            caller_token: caller_token.as_deref(),
            seed: seed.as_deref(),
            drain: Duration::from_secs(drain),
            clock,
            mint_seed,
            log_format,
        }),
        Command::Play {
            spec,
            environment,
            target_score,
            match_id,
            speed,
            component: Some(component),
            listen,
        } => component::play(&component::Playing {
            spec: &spec,
            environment: &environment,
            target_score,
            match_id: &match_id,
            speed,
            component: &component,
            listen,
        }),
        Command::Play {
            spec,
            environment,
            target_score,
            match_id,
            speed,
            component: None,
            ..
        } => play(&spec, &environment, target_score, &match_id, speed),
        Command::Deploy {
            spec,
            environment,
            placement,
            port,
        } => deploy::deploy(&deploy::Deploying {
            spec: &spec,
            environment: &environment,
            placement: &placement,
            port,
        }),
        Command::Conform {
            spec,
            environment,
            suite,
            report,
            results,
            counts,
            known_failing,
            accounting_out,
        } => conform::conform(&conform::Conform {
            spec: &spec,
            environment: &environment,
            suite: suite.as_deref(),
            report: report.as_deref(),
            results: results.as_deref(),
            counts: counts.as_deref(),
            known_failing: known_failing.as_deref(),
            accounting_out: accounting_out.as_deref(),
        }),
        Command::Store {
            command: StoreCommand::Migrate { spec, environment },
        } => database::migrate(&spec, &environment),
        Command::State {
            command:
                StateCommand::Move {
                    from,
                    spec,
                    environment,
                },
        } => state::move_from(&from, &spec, &environment),
    }
}

/// The command that opens a rally match.
const OPEN_MATCH: &str = "rally.table.OpenMatch";
/// The view a rally frame is drawn from.
const MATCH_STATE: &str = "rally.table.MatchState";
/// The component whose counters play prints under the board.
const TABLE: &str = "rally-table";
/// Clears the terminal and moves the cursor home.
const CLEAR: &str = "\x1b[2J\x1b[H";

/// Records SIGINT and SIGTERM, by number, in the flag returned.
fn signals() -> Result<Arc<AtomicUsize>> {
    let signalled = Arc::new(AtomicUsize::new(0));
    for signal in [SIGINT, SIGTERM] {
        let number = usize::try_from(signal).context("a signal number")?;
        signal_hook::flag::register_usize(signal, Arc::clone(&signalled), number)
            .context("handling SIGINT and SIGTERM")?;
    }
    Ok(signalled)
}

/// Sets `flag` on SIGINT and SIGTERM, beside [`signals`]: a served run's listener reads it on its
/// own threads, so readiness reads `503` and a request arriving is refused `503` as the signal
/// arrives, while a command still holds the run's thread (`story:health-and-graceful-drain`).
fn stopping_on(flag: &Arc<AtomicBool>) -> Result<()> {
    for signal in [SIGINT, SIGTERM] {
        signal_hook::flag::register(signal, Arc::clone(flag))
            .context("handling SIGINT and SIGTERM")?;
    }
    Ok(())
}

fn play(
    spec: &Path,
    environment: &Path,
    target_score: i64,
    match_id: &str,
    speed: u32,
) -> Result<ExitCode> {
    let signalled = signals()?;
    let loaded = spindle_spec::load(spec)?;
    let text = std::fs::read_to_string(environment)
        .with_context(|| format!("reading {}", environment.display()))?;
    let parsed =
        Environment::parse(&text).with_context(|| format!("in {}", environment.display()))?;
    refuse_seed(&parsed, environment, "play")?;
    refuse_decisions(&parsed, environment, "play")?;
    let state = tempfile::tempdir().context("creating the state directory")?;
    let clock = scaled::Scaled::new(speed);
    let mut host = match Host::new(loaded.ir, &parsed, Arc::new(clock), state.path()) {
        Ok(host) => host,
        Err(HostError::Refused(refusal)) => {
            println!("{refusal}");
            return Ok(ExitCode::from(REFUSED));
        }
        Err(error) => return Err(error.into()),
    };
    host.submit(
        OPEN_MATCH,
        serde_json::Map::from_iter([
            ("match_id".to_owned(), serde_json::json!(match_id)),
            ("target_score".to_owned(), serde_json::json!(target_score)),
        ]),
    )?;
    let context =
        serde_json::Map::from_iter([("match_id".to_owned(), serde_json::json!(match_id))]);
    for binding in host.periodic() {
        host.start(&binding, context.clone())?;
    }

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    loop {
        let view = host
            .view(MATCH_STATE, &serde_json::json!(match_id))?
            .with_context(|| format!("no match {match_id}"))?;
        write!(out, "{CLEAR}{}", rally::frame(&view)?)?;
        if let Some(counters) = host.counters(TABLE) {
            write!(out, "{}", counted(counters))?;
        }
        out.flush()?;
        if rally::winner(&view).is_some() {
            host.stop();
            return Ok(ExitCode::SUCCESS);
        }
        let deadline = host
            .next_deadline()
            .context("no periodic binding is running, so the match cannot go on")?;
        if let Some(signal) = wait_until(deadline, &clock, &signalled) {
            host.stop();
            return Ok(ExitCode::from(SIGNALLED.saturating_add(signal)));
        }
        let report = host.advance()?;
        for failed in &report.failed {
            eprintln!(
                "b10x-spindle: {} did not run {}: {}",
                failed.component, failed.command, failed.error
            );
        }
        for refused in &report.refused {
            eprintln!(
                "b10x-spindle: {} refused {} ({}) with {} {}",
                refused.component,
                refused.command,
                refused.outcome,
                refused.error.name,
                serde_json::Value::Object(refused.error.fields.clone())
            );
        }
    }
}

/// Refuses `environment`, read from `path`, where it names a seed file in a `[seed]` table:
/// `command` submits no seed, as only a run serving HTTP does ([`seed`]). `play` plays on a
/// temporary state, and `deploy` hands the same document to every component's
/// `play --component` process, where an entry for another component could not run.
///
/// # Errors
/// Naming the document, the table, its file and `command`.
pub(crate) fn refuse_seed(environment: &Environment, path: &Path, command: &str) -> Result<()> {
    if let Some(seeded) = &environment.seed {
        bail!(
            "{} names the seed file {} in its [seed] table, which {command} would never submit: \
             a seed is submitted by a run serving HTTP; remove the table for {command}",
            path.display(),
            seeded.file
        );
    }
    Ok(())
}

/// Refuses `environment`, read from `path`, where it names a decisions file in a `[decisions]`
/// table: `command` answers no external cause from a file, as only `run` does ([`execute`]).
///
/// # Errors
/// Naming the document, the table, its file and `command`.
pub(crate) fn refuse_decisions(
    environment: &Environment,
    path: &Path,
    command: &str,
) -> Result<()> {
    if let Some(decisions) = &environment.decisions {
        bail!(
            "{} names the decisions file {} in its [decisions] table, which {command} would \
             never read: external causes are answered from it by run; remove the table for \
             {command}",
            path.display(),
            decisions.file
        );
    }
    Ok(())
}

/// Every counter of `counters` that is not zero, one per line.
fn counted(counters: &spindle_telemetry::Counters) -> String {
    counters
        .snapshot()
        .counters
        .iter()
        .filter(|counter| counter.value > 0)
        .map(|counter| format!("{} {}\n", counter.name, counter.value))
        .collect()
}

/// The exit status of a run a signal stopped is this plus the signal's number.
const SIGNALLED: u8 = 128;
/// How often a wait looks for a signal.
const SIGNAL_POLL: Duration = Duration::from_millis(50);

/// Sleeps until `clock` reads `deadline`, or until a signal is recorded in `signalled`; returns
/// its number.
fn wait_until(deadline: Instant, clock: &scaled::Scaled, signalled: &AtomicUsize) -> Option<u8> {
    loop {
        let signal = signalled.load(Ordering::SeqCst);
        if signal != 0 {
            return Some(u8::try_from(signal).unwrap_or(u8::MAX - SIGNALLED));
        }
        let left = clock.until(deadline);
        if left.is_zero() {
            return None;
        }
        std::thread::sleep(left.min(SIGNAL_POLL));
    }
}

/// The implementation `run` answers an external cause as when the request line, `--decisions`
/// or `--answer-default` does.
const OPERATOR_PROMPT: &str = "operator-prompt";

/// The decisions file `run` answers external causes from: `flag` (`--decisions`), for every
/// cause, or else the environment's `[decisions]` file, for the causes `bindings` bind to
/// [`DECISIONS_FILE`] alone.
///
/// # Errors
/// An external cause bound to an implementation `run` does not provide (one other than
/// [`OPERATOR_PROMPT`], [`DECISIONS_FILE`] and an `[http.<name>]` call `environment` declares),
/// naming the capability and the name; or one bound to [`DECISIONS_FILE`] where neither the flag
/// nor the environment names a file.
fn decisions_file(
    environment: &Environment,
    bindings: &[Binding],
    flag: Option<&Path>,
) -> Result<Option<DecisionsFile>> {
    let external = bindings.iter().filter(|binding| {
        binding
            .capability
            .as_str()
            .starts_with(Capability::EXTERNAL_PREFIX)
    });
    if let Some(unprovided) = external.clone().find(|binding| {
        let name = binding.implementation.as_str();
        ![OPERATOR_PROMPT, DECISIONS_FILE].contains(&name) && !environment.http.contains_key(name)
    }) {
        bail!(
            "{} is bound to `{}` on {}; run answers an external cause as {OPERATOR_PROMPT}, \
             {DECISIONS_FILE} or an [http.<name>] call the environment declares",
            unprovided.capability,
            unprovided.implementation,
            environment.name
        );
    }
    if let Some(path) = flag {
        return Ok(Some(DecisionsFile {
            path: path.to_owned(),
            only: None,
        }));
    }
    let from_file: BTreeSet<String> = external
        .filter(|binding| {
            binding.implementation.as_str() == DECISIONS_FILE
                && !environment.http.contains_key(DECISIONS_FILE)
        })
        .map(|binding| binding.capability.as_str().to_owned())
        .collect();
    let Some(first) = from_file.first() else {
        return Ok(None);
    };
    match &environment.decisions {
        Some(named) => Ok(Some(DecisionsFile {
            path: PathBuf::from(&named.file),
            only: Some(from_file),
        })),
        None => bail!(
            "{first} is bound to {DECISIONS_FILE} on {}, but no decisions file is named: give \
             --decisions <FILE>, or name it in the environment's [decisions] table",
            environment.name
        ),
    }
}

/// The decisions file of a `run`, and the causes it answers.
struct DecisionsFile {
    /// `--decisions`, or the environment's `[decisions]` file.
    path: PathBuf,
    /// For the environment's file, the capabilities bound to [`DECISIONS_FILE`]: it answers no
    /// other cause. `None` for `--decisions`, which answers every cause.
    only: Option<BTreeSet<String>>,
}

/// The arguments of `run` without `--scenario`.
struct Execute<'a> {
    spec: &'a Path,
    environment: &'a Path,
    state: Option<&'a Path>,
    /// `--stdin`: read JSON lines on stdin even where a component requires `ingress.network`.
    stdin: bool,
    decisions: Option<&'a Path>,
    answer_default: bool,
    http: Option<&'a Path>,
    port: Option<u16>,
    /// `--address`: the IP address to serve on, 127.0.0.1 without it.
    address: Option<std::net::IpAddr>,
    /// `--tls-cert` with `--tls-key`, and `--tls-client-ca`: the files a served run listens
    /// with over TLS; plain HTTP without them.
    tls: Option<listener_tls::Files<'a>>,
    /// `--console` (`SPINDLE_SETTING_CONSOLE`): whether the console routes are served, decided
    /// with the address where not given ([`spindle_cli::cli::console_served`]).
    console: Option<spindle_cli::cli::Switch>,
    drop_retries: bool,
    callers: Vec<spindle_ingress::Caller>,
    /// With `--observe`, how many request records to keep.
    observe: Option<usize>,
    /// `--observe-series`: how a metrics scrape writes the labels taken from the specification.
    observe_series: spindle_ingress::SeriesMode,
    /// `--trusted-caller-header`: the header each request proves its caller in.
    caller_header: Option<&'a str>,
    /// `--trusted-caller-secret`: the file holding the secret the proxy presents.
    caller_secret: Option<&'a Path>,
    /// `--caller-token`: the mapping file each request's bearer token is verified and mapped by.
    caller_token: Option<&'a Path>,
    /// `--seed`: the seed file to submit at every start, in place of the environment's.
    seed: Option<&'a Path>,
    /// `--drain`: how long a served run answers the requests waiting when a signal arrives.
    drain: Duration,
    /// `--clock`: the instant the run's manual clock starts at, the system clock without it.
    clock: Option<std::time::SystemTime>,
    /// `--mint-seed`: the seed every identity and generated value is minted from.
    mint_seed: Option<u64>,
    /// `--log-format`: how the lines on stderr are written ([`log`]), set in `main`; JSON only
    /// where the run serves HTTP.
    log_format: LogFormat,
}

fn execute(arguments: &Execute<'_>) -> Result<ExitCode> {
    let Execute {
        spec,
        environment,
        state,
        stdin,
        decisions,
        answer_default,
        http,
        port,
        address,
        tls,
        console,
        drop_retries,
        ref callers,
        observe,
        observe_series,
        caller_header,
        caller_secret,
        caller_token,
        seed,
        drain,
        clock,
        mint_seed,
        log_format,
    } = *arguments;
    // Off loopback a verified token takes the place of the proxy's header and secret, and is
    // carried only over the listener's TLS (`story:verified-caller-token`, D3 B).
    if let (Some(address), Some(mapping)) = (
        address.filter(|address| spindle_ingress::exposed(*address)),
        caller_token,
    ) && tls.is_none()
    {
        log::error(format!(
            "--address {address} is not loopback, so --caller-token {} is served only over TLS, \
             so no bearer token crosses the network in the clear: give --tls-cert <FILE> with \
             --tls-key <FILE>",
            mapping.display()
        ))
        .write();
        return Ok(ExitCode::from(USAGE));
    }
    if let Some(mapping) = caller_token
        && console == Some(spindle_cli::cli::Switch::On)
    {
        log::error(format!(
            "--console on and --caller-token {} are given together: on a token run the console \
             routes are off, since a reset or a model read must not answer every holder of a \
             token",
            mapping.display()
        ))
        .write();
        return Ok(ExitCode::from(USAGE));
    }
    if let Some(address) = address.filter(|address| spindle_ingress::exposed(*address))
        && caller_token.is_none()
        && (caller_header.is_none() || caller_secret.is_none())
    {
        log::error(format!(
            "--address {address} is not loopback, so every request must come through an \
             authenticating proxy: give --trusted-caller-header <NAME> with \
             --trusted-caller-secret <FILE> (--fixed-actor proves nobody)"
        ))
        .write();
        return Ok(ExitCode::from(USAGE));
    }
    // A certificate, key or CA file that cannot be served is a mistake on the command line: it
    // exits as clap exits on one, before anything is loaded or listens.
    let listening_tls = match tls.map(listener_tls::server_config).transpose() {
        Ok(config) => config,
        Err(refused) => {
            log::error(refused).write();
            return Ok(ExitCode::from(USAGE));
        }
    };
    if let (Some(header), Some(caller)) = (caller_header, callers.first()) {
        bail!(
            "--trusted-caller-header {header} and --fixed-actor {} are given together: each \
             request proves its own caller, or one is fixed for the run, not both",
            caller.actor
        );
    }
    // One caller source per run (`story:verified-caller-token`, D2 A).
    if let Some(mapping) = caller_token {
        if let Some(header) = caller_header {
            bail!(
                "--caller-token {} and --trusted-caller-header {header} are given together: \
                 each request proves its caller by one source, a verified token or a proxy's \
                 header, not both",
                mapping.display()
            );
        }
        if let Some(caller) = callers.first() {
            bail!(
                "--caller-token {} and --fixed-actor {} are given together: each request proves \
                 its own caller, or one is fixed for the run, not both",
                mapping.display(),
                caller.actor
            );
        }
    }
    let proxy_secret = match (caller_header, caller_secret) {
        (Some(_), Some(path)) => Some((path, serve::proxy_secret(path)?)),
        (Some(header), None) => bail!(
            "--trusted-caller-header {header} needs --trusted-caller-secret <FILE>: the proxy is \
             trusted only by the secret it presents on every request"
        ),
        (None, Some(path)) => bail!(
            "--trusted-caller-secret {} is given without --trusted-caller-header, so it would be \
             ignored",
            path.display()
        ),
        (None, None) => None,
    };
    let loaded = spindle_spec::load(spec)?;
    let requirements = spindle_capability::derive(&loaded.ir, None)
        .with_context(|| format!("deriving the capabilities of {}", spec.display()))?;
    let text = std::fs::read_to_string(environment)
        .with_context(|| format!("reading {}", environment.display()))?;
    let parsed =
        Environment::parse(&text).with_context(|| format!("in {}", environment.display()))?;
    let admission = spindle_capability::admit(&requirements, &parsed);
    let Admission::Admitted { bindings } = &admission else {
        println!("{}", serde_json::to_value(&admission)?);
        return Ok(ExitCode::from(REFUSED));
    };
    let minting = bindings
        .iter()
        .find(|binding| binding.capability.as_str() == Capability::IDENTITY_MINT)
        .map(|binding| binding.implementation.as_str());
    if let Some(other) = minting
        .filter(|bound| ![spindle_kernel::UuidV4::IMPLEMENTATION, SEEDED_MINT].contains(bound))
    {
        bail!(
            "identity.mint is bound to {other} on {}; this build mints identities as {} or \
             {SEEDED_MINT} only",
            parsed.name,
            spindle_kernel::UuidV4::IMPLEMENTATION
        );
    }
    let decisions = decisions_file(&parsed, bindings, decisions)?;
    let mut answers = decisions::load(
        &loaded.ir,
        decisions.as_ref().map(|file| file.path.as_path()),
        answer_default,
    )?;
    if let Some(only) = decisions.as_ref().and_then(|file| file.only.as_ref()) {
        answers = answers.answering_only(only);
    }
    let calls = calls::Calls::bind(&parsed, bindings)
        .with_context(|| format!("in {}", environment.display()))?;
    let egress = match egress::plan(&loaded.ir, &parsed)
        .with_context(|| format!("in {}", environment.display()))?
    {
        None => None,
        Some(Ok(plan)) => Some(plan),
        Some(Err(refusal)) => {
            log::error(refusal).write();
            return Ok(ExitCode::from(REFUSED));
        }
    };
    let seed_path = seed.map(Path::to_owned).or_else(|| {
        parsed
            .seed
            .as_ref()
            .map(|seeded| std::path::PathBuf::from(&seeded.file))
    });
    let durable = bindings
        .iter()
        .find(|binding| binding.capability.as_str() == "store.durable")
        .map(|binding| binding.implementation.as_str());
    let networked = bindings
        .iter()
        .find(|binding| binding.capability.as_str() == Capability::INGRESS_NETWORK);
    if let Some(ingress) = networked.filter(|_| !stdin) {
        if ingress.implementation != spindle_ingress::HTTP_LOOPBACK {
            bail!(
                "ingress.network is bound to `{}` on {}; this build serves `{}` only",
                ingress.implementation,
                parsed.name,
                spindle_ingress::HTTP_LOOPBACK
            );
        }
        if minting == Some(SEEDED_MINT) && mint_seed.is_none() {
            bail!(
                "identity.mint is bound to {SEEDED_MINT} on {}; a run serving HTTP mints from \
                 the seed --mint-seed <N> names: give it",
                parsed.name
            );
        }
        let decides = decisions.is_some() || answer_default || calls.answer_every(bindings);
        let binding = match http {
            Some(path) => match serve::binding(path, &loaded.ir)? {
                Some(binding) => Some(binding),
                None => return Ok(ExitCode::FAILURE),
            },
            None => None,
        };
        let components = serve::networked(&loaded.ir);
        let seed = seed_path
            .map(|path| seed::load(&path, &loaded.ir))
            .transpose()?;
        // The key set is fetched now, before anything listens: a run that cannot fetch it
        // exits 1 (`story:verified-caller-token`, D5 A).
        let tokens = caller_token
            .map(|path| token::Verifier::start(path, &loaded.ir).map(|verifier| (path, verifier)))
            .transpose()?;
        let address = address.unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
        return serve::serve(
            loaded.ir,
            serve::Serving {
                environment: &parsed,
                durable,
                state,
                components,
                binding,
                port: port.unwrap_or(0),
                address,
                ingress: ingress.implementation.as_str(),
                console: tokens.is_none() && spindle_cli::cli::console_served(console, address),
                drop_retries,
                decides,
                answers: answers.calling(calls),
                callers: callers.clone(),
                observe,
                observe_series,
                caller_header,
                proxy_secret,
                tokens,
                tls: listening_tls,
                seed,
                drain,
                clock,
                mint_seed,
                egress,
            },
        );
    }
    // Why a flag only a served run reads would do nothing here.
    let unserved = if networked.is_some() {
        "--stdin reads JSON lines instead of serving HTTP".to_owned()
    } else {
        format!("nothing in {} requires ingress.network", spec.display())
    };
    if let Some(path) = seed_path {
        let given = if seed.is_some() {
            "--seed"
        } else {
            "the environment's [seed] file"
        };
        bail!(
            "{unserved}, so {given} {} would never be submitted: a seed is submitted by a run \
             serving HTTP",
            path.display()
        );
    }
    if egress.is_some() {
        bail!(
            "{unserved}, so the [egress] of {} would never post an event: an event is posted by a \
             run serving HTTP",
            environment.display()
        );
    }
    if observe.is_some() {
        bail!("{unserved}, so --observe would be ignored");
    }
    if log_format == LogFormat::Json {
        bail!(
            "{unserved}, so --log-format json would not be kept: only a run serving HTTP \
             writes its log as JSON lines"
        );
    }
    if let Some(caller) = callers.first() {
        bail!(
            "{unserved}, so --fixed-actor {} would be ignored",
            caller.actor
        );
    }
    if let Some(header) = caller_header {
        bail!("{unserved}, so --trusted-caller-header {header} would be ignored");
    }
    if let Some(path) = caller_token {
        bail!(
            "{unserved}, so --caller-token {} would be ignored",
            path.display()
        );
    }
    if let Some(path) = http {
        bail!("{unserved}, so --http {} would be ignored", path.display());
    }
    if let Some(port) = port {
        bail!("{unserved}, so --port {port} would be ignored");
    }
    if let Some(address) = address {
        bail!("{unserved}, so --address {address} would be ignored");
    }
    if let Some(files) = tls {
        bail!(
            "{unserved}, so --tls-cert {} would be ignored",
            files.cert.display()
        );
    }
    let bindings = lines::Bindings::new(spec, &loaded.ir, drop_retries)?;
    let scratch;
    let _lock;
    let mut store: Box<dyn Store> = match (durable, state) {
        (None | Some("memory"), None) => Box::new(MemoryStore::default()),
        (None, Some(path)) => bail!(
            "{} requires no store.durable, so nothing would read or write --state {}",
            spec.display(),
            path.display()
        ),
        (Some("memory"), Some(path)) => bail!(
            "store.durable is bound to memory on {}, which keeps no state file: --state {} would \
             be ignored",
            parsed.name,
            path.display()
        ),
        (Some("sqlite"), Some(path)) => {
            _lock = state_lock::file(path)?;
            let mut store = SqliteStore::open(path)?;
            let named = format!("the state file {}", path.display());
            let fit = store.fit(&spindle_kernel::Shapes::of(&loaded.ir))?;
            if let Err(error) = database::fitting(fit, &named, &path.display().to_string()) {
                return database::misfit_refused(error);
            }
            Box::new(store)
        }
        (Some("sqlite"), None) => {
            scratch = tempfile::tempdir().context("creating a temporary state directory")?;
            removed_on_signal(scratch.path())?;
            eprintln!(
                "b10x-spindle: state is kept in {}, removed when the run ends",
                scratch.path().display()
            );
            Box::new(SqliteStore::open(&scratch.path().join("state.sqlite"))?)
        }
        (Some(database::POSTGRES), _) => bail!(
            "store.durable is bound to postgres on {}, which keeps each component in tables of its \
             own; run reading stdin keeps one store for every component: serve the specification \
             over HTTP (ingress.network) instead",
            parsed.name
        ),
        (Some(other), _) => bail!("this build has no `{other}` store for store.durable"),
    };

    for line in unsupported_outcomes(&loaded.ir, None) {
        eprintln!("b10x-spindle: {line}");
    }
    let seed = match (mint_seed, minting) {
        (Some(seed), _) => Some(seed),
        (None, Some(SEEDED_MINT)) => Some(0),
        (None, _) => None,
    };
    let mut seeded = seed
        .map(|seed| {
            held_identities(&loaded.ir, store.as_mut())
                .map(|held| spindle_kernel::Seeded::new(seed).avoiding_identities(held.iter()))
        })
        .transpose()?;
    let mut random = spindle_kernel::UuidV4;
    let mut clock = clock;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut read = 0_usize;
    let mut invoked = 0_usize;
    let mut rejected: Vec<String> = Vec::new();
    let mut answered = lines::Answered::default();
    for (index, line) in std::io::stdin().lock().lines().enumerate() {
        let line = line.context("reading stdin")?;
        if line.trim().is_empty() {
            continue;
        }
        read += 1;
        let mut fired = Vec::new();
        let result = match lines::Line::parse(&line) {
            Err(message) => serde_json::json!({
                "failure": {"kind": "invalid_request", "message": message},
            }),
            Ok(lines::Line::Arrange(rows)) => {
                match lines::arrange(&loaded.ir, store.as_mut(), rows) {
                    Err(message) => serde_json::json!({
                        "failure": {"kind": "invalid_arrangement", "message": message},
                    }),
                    Ok(arranged) => {
                        // A seeded mint never mints an identity an arranged row holds.
                        if let Some(seeded) = seeded.as_mut() {
                            *seeded = seeded
                                .clone()
                                .avoiding_identities(arranged.iter().map(|row| &row.identity));
                        }
                        lines::Arranged::answer(&arranged)
                    }
                }
            }
            Ok(lines::Line::Command(mut request)) => {
                match answered
                    .resolve(index + 1, &mut request.input)
                    .map_err(|message| ("unresolved_reference", message))
                    .and_then(|()| {
                        moved(clock, request.at.as_deref())
                            .map_err(|message| ("invalid_request", message))
                    }) {
                    Err((kind, message)) => serde_json::json!({
                        "command": request.command,
                        "failure": {"kind": kind, "message": message},
                    }),
                    Ok(moved) => {
                        let now = move || moved.unwrap_or_else(std::time::SystemTime::now);
                        let mint: &mut dyn spindle_kernel::Mint = match seeded.as_mut() {
                            Some(seeded) => seeded,
                            None => &mut random,
                        };
                        let mut running = lines::Running {
                            ir: &loaded.ir,
                            store: store.as_mut(),
                            answers: answers.answers(),
                            calls: &calls,
                            mint,
                            now: &now,
                        };
                        let (result, events) = answer(&mut running, &request);
                        // A rejected line did not run, so the clock stays where it stood.
                        if result.get("failure").is_none() {
                            clock = moved;
                        }
                        fired = bindings.fire(&mut running, index + 1, events);
                        result
                    }
                }
            }
        };
        answered.keep(index + 1, result.clone());
        if let Some(failure) = result.get("failure") {
            let kind = failure["kind"].as_str().unwrap_or_default();
            rejected.push(format!(
                "line {} was rejected ({kind}): it did not run",
                index + 1
            ));
        }
        writeln!(out, "{result}")?;
        invoked += fired.len();
        for fired in fired {
            rejected.extend(fired.rejected);
            writeln!(out, "{}", fired.line)?;
        }
        out.flush()?;
    }
    if rejected.is_empty() {
        return Ok(ExitCode::SUCCESS);
    }
    for rejection in &rejected {
        eprintln!("b10x-spindle: {rejection}");
    }
    if bindings.is_empty() {
        eprintln!(
            "b10x-spindle: {} rejected of {read} commands read, so the run exits 1",
            rejected.len()
        );
    } else {
        eprintln!(
            "b10x-spindle: {} rejected of {read} lines read and {invoked} commands their \
             bindings invoked, so the run exits 1",
            rejected.len()
        );
    }
    Ok(ExitCode::FAILURE)
}

/// Every identity an instance in `store` holds, of every entity `ir` declares: what a seeded mint
/// on state an earlier run kept never mints again.
fn held_identities(
    ir: &spindle_spec::EssIr,
    store: &mut dyn Store,
) -> Result<Vec<serde_json::Value>> {
    let mut held = Vec::new();
    for entity in ir.entities().keys() {
        let entity = entity.to_string();
        let scan = store
            .scan(&entity)
            .with_context(|| format!("reading the identities {entity} holds"))?;
        held.extend(scan.rows.into_iter().map(|(identity, _)| identity));
    }
    Ok(held)
}

/// Removes `directory` and exits 130 or 143 on SIGINT or SIGTERM: `run` reading stdin blocks in a
/// read a signal does not interrupt, so a thread of its own waits for the signal instead.
fn removed_on_signal(directory: &Path) -> Result<()> {
    let mut signals = signal_hook::iterator::Signals::new([SIGINT, SIGTERM])
        .context("handling SIGINT and SIGTERM")?;
    let directory = directory.to_owned();
    std::thread::Builder::new()
        .name("spindle-signals".to_owned())
        .spawn(move || {
            if let Some(signal) = signals.forever().next() {
                let _ = std::fs::remove_dir_all(&directory);
                let signal = u8::try_from(signal).unwrap_or(u8::MAX - SIGNALLED);
                std::process::exit(i32::from(SIGNALLED.saturating_add(signal)));
            }
        })
        .context("starting the signal thread")?;
    Ok(())
}

/// The instant the run's manual clock stands at once a line's `at` moved it: `clock` where the line
/// gives none, `None` (the system clock) on a run without `--clock`.
///
/// # Errors
/// The message of a line whose `at` is no RFC 3339 instant, is earlier than the clock, or is given
/// on a run without `--clock`.
fn moved(
    clock: Option<std::time::SystemTime>,
    at: Option<&str>,
) -> Result<Option<std::time::SystemTime>, String> {
    let Some(at) = at else {
        return Ok(clock);
    };
    let Some(standing) = clock else {
        return Err(format!(
            "`at` {at} moves the clock --clock starts the run on; this run reads the system clock"
        ));
    };
    let instant = spindle_cli::cli::instant(at).map_err(|error| format!("`at`: {error}"))?;
    if instant < standing {
        return Err(format!(
            "`at` {at} is before the clock's current instant: the clock only moves forward"
        ));
    }
    Ok(Some(instant))
}

/// The result line for one request, and the events its command emitted where it took effect: a
/// cause bound to an HTTP call is answered by it, any other by the line's own `external` object,
/// then as the run's answers answer it. Every identity and generated value is minted by the
/// run's mint, and its clock is the instant every guard and generated `Timestamp` reads.
fn answer(
    running: &mut lines::Running<'_>,
    request: &Request,
) -> (serde_json::Value, Vec<spindle_kernel::Event>) {
    let invocation = spindle_kernel::Invocation {
        command: &request.command,
        input: &request.input,
        actor: request.actor.as_deref(),
        caller: request.caller.as_ref(),
    };
    let result = running.execute(&invocation, &request.external);
    let printed = lines::printed(&request.command, &result);
    let events = match result {
        Ok(executed) if executed.error.is_none() => executed.events,
        _ => Vec::new(),
    };
    (printed, events)
}

/// The name of every component of `ir`: what `run` reading stdin reaches, whose one store holds
/// every component's rows (`story:cross-component-related-reads`).
fn every_component(ir: &spindle_spec::EssIr) -> std::collections::BTreeSet<String> {
    ir.components()
        .values()
        .map(|declared| declared.name.as_str().to_owned())
        .collect()
}

/// One line per command the kernel refuses whole and per other outcome it does not take, of each
/// command `component` accepts or, without one, of every command, by command name. A command the
/// kernel cannot select among the outcomes of ([`spindle_kernel::selects`]), or none of whose
/// outcomes it takes ([`spindle_kernel::takes`]), is named once as not served, with the first
/// refusal: every request of it is refused, as `run` refuses it. Each outcome the kernel does not
/// take of any other command is named with what it lacks: a request that selects it is refused,
/// and every other outcome of its command runs. The commands run on a store reaching the stores
/// of every component, as `run` runs them in one process, or of `component` alone, as
/// `play --component` runs it ([`spindle_kernel::takes_reaching`]): a read of an entity a
/// reached component owns is read from its owner's store, and one whose owner is not reached is
/// named.
fn unsupported_outcomes(ir: &spindle_spec::EssIr, component: Option<&str>) -> Vec<String> {
    let reached: std::collections::BTreeSet<String> = every_component(ir)
        .into_iter()
        .filter(|name| component.is_none_or(|component| component == name))
        .collect();
    let accepted = |command: &str| {
        component.is_none_or(|component| {
            ir.components().values().any(|declared| {
                declared.name.as_str() == component
                    && declared
                        .accepts
                        .iter()
                        .any(|accepted| accepted.to_string() == command)
            })
        })
    };
    let mut lines = Vec::new();
    for (name, command) in ir.commands() {
        let name = name.to_string();
        if !accepted(&name) {
            continue;
        }
        if let Err(refused) = spindle_kernel::selects(command) {
            lines.push(format!(
                "not served: {name}, refused for every request: {refused}"
            ));
            continue;
        }
        let refused = command
            .outcomes
            .iter()
            .filter_map(|outcome| {
                spindle_kernel::takes_reaching(ir, command, outcome, &reached)
                    .err()
                    .map(|refused| (outcome.name.as_str(), refused))
            })
            .collect::<Vec<_>>();
        match refused.first() {
            Some((_, first)) if refused.len() == command.outcomes.len() => lines.push(format!(
                "not served: {name}, refused for every request: {first}"
            )),
            _ => lines.extend(refused.iter().map(|(outcome, refused)| {
                format!(
                    "unsupported outcome `{name}/{outcome}`, refused where a request selects it: \
                     {refused}"
                )
            })),
        }
    }
    lines
}

/// One line per view `run` would not serve over HTTP, of `component` or, without one, of every
/// component reached over the network, as `run` prints it at startup
/// ([`spindle_ingress::Route::not_served`]): the routes ESS derives, read without serving. A
/// command is named by [`unsupported_outcomes`] instead. Where the routes cannot be derived, the
/// one line saying why: `run` refuses to listen for the same reason.
fn unserved_views(ir: &spindle_spec::EssIr, component: Option<&str>) -> Vec<String> {
    let components: Vec<String> = serve::networked(ir)
        .into_iter()
        .filter(|name| component.is_none_or(|component| component == name))
        .collect();
    if components.is_empty() {
        return Vec::new();
    }
    match spindle_ingress::Surface::new(ir, &components, None) {
        Ok(surface) => surface
            .unserved()
            .into_iter()
            .filter(|route| matches!(route.operation, ess_http::Operation::View(_)))
            .filter_map(spindle_ingress::Route::not_served)
            .collect(),
        Err(refused) => vec![format!("the HTTP routes are not derived: {refused}")],
    }
}

fn requirements(spec: &std::path::Path, component: Option<&str>) -> Result<Requirements> {
    let loaded = spindle_spec::load_model(spec)?;
    spindle_capability::derive(&loaded.ir, component)
        .with_context(|| format!("{} declares no component {component:?}", spec.display()))
}
