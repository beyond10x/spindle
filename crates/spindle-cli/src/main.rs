//! `b10x-spindle`: load an ESS specification, derive what it needs, admit it, and run or play it.

mod calls;
mod component;
mod conform;
mod database;
mod decisions;
mod deploy;
mod replay;
mod scaled;
mod serve;
mod state;
mod state_lock;
mod world;

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::Parser;
use decisions::Answers;
use signal_hook::consts::{SIGINT, SIGTERM};
use spindle_capability::{Admission, Capability, Environment, Requirements};
use spindle_cli::cli::{Cli, Command, StateCommand, StoreCommand};
use spindle_host::{Host, HostError, rally};
use spindle_kernel::{External, MemoryStore, SqliteStore, Store};

/// Exit status of an `admit` the environment refused.
const REFUSED: u8 = 3;

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
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("b10x-spindle: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Command::Check { spec, json } => {
            let loaded = spindle_spec::load(&spec)?;
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
            let loaded = spindle_spec::load(&spec)?;
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
            ..
        } => replay::replay(&spec, &environment, &scenario),
        Command::Run {
            spec,
            environment,
            state,
            decisions,
            answer_default,
            http,
            port,
            drop_retries,
            fixed_actor,
            fixed_caller,
            trusted_caller_header,
            trusted_caller_secret,
            observe,
            observe_requests,
            scenario: None,
        } => execute(&Execute {
            spec: &spec,
            environment: &environment,
            state: state.as_deref(),
            decisions: decisions.as_deref(),
            answer_default,
            http: http.as_deref(),
            port,
            drop_retries,
            callers: serve::callers(fixed_actor, fixed_caller)?,
            observe: observe.then_some(usize::from(observe_requests)),
            caller_header: trusted_caller_header.as_deref(),
            caller_secret: trusted_caller_secret.as_deref(),
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

/// The arguments of `run` without `--scenario`.
struct Execute<'a> {
    spec: &'a Path,
    environment: &'a Path,
    state: Option<&'a Path>,
    decisions: Option<&'a Path>,
    answer_default: bool,
    http: Option<&'a Path>,
    port: Option<u16>,
    drop_retries: bool,
    callers: Vec<spindle_ingress::Caller>,
    /// With `--observe`, how many request records to keep.
    observe: Option<usize>,
    /// `--trusted-caller-header`: the header each request proves its caller in.
    caller_header: Option<&'a str>,
    /// `--trusted-caller-secret`: the file holding the secret the proxy presents.
    caller_secret: Option<&'a Path>,
}

fn execute(arguments: &Execute<'_>) -> Result<ExitCode> {
    let Execute {
        spec,
        environment,
        state,
        decisions,
        answer_default,
        http,
        port,
        drop_retries,
        ref callers,
        observe,
        caller_header,
        caller_secret,
    } = *arguments;
    if let (Some(header), Some(caller)) = (caller_header, callers.first()) {
        bail!(
            "--trusted-caller-header {header} and --fixed-actor {} are given together: each \
             request proves its own caller, or one is fixed for the run, not both",
            caller.actor
        );
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
    let answers = decisions::load(&loaded.ir, decisions, answer_default)?;
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
    if let Some(mint) = bindings.iter().find(|binding| {
        binding.capability.as_str() == spindle_capability::Capability::IDENTITY_MINT
            && binding.implementation != spindle_kernel::UuidV4::IMPLEMENTATION
    }) {
        bail!(
            "identity.mint is bound to {} on {}; this build mints identities as {} only",
            mint.implementation,
            parsed.name,
            spindle_kernel::UuidV4::IMPLEMENTATION
        );
    }
    let calls = calls::Calls::bind(&parsed, bindings)
        .with_context(|| format!("in {}", environment.display()))?;
    let durable = bindings
        .iter()
        .find(|binding| binding.capability.as_str() == "store.durable")
        .map(|binding| binding.implementation.as_str());
    if let Some(ingress) = bindings
        .iter()
        .find(|binding| binding.capability.as_str() == Capability::INGRESS_NETWORK)
    {
        if ingress.implementation != spindle_ingress::HTTP_LOOPBACK {
            bail!(
                "ingress.network is bound to `{}` on {}; this build serves `{}` only",
                ingress.implementation,
                parsed.name,
                spindle_ingress::HTTP_LOOPBACK
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
        return serve::serve(
            loaded.ir,
            serve::Serving {
                environment: &parsed,
                durable,
                state,
                components,
                binding,
                port: port.unwrap_or(0),
                ingress: ingress.implementation.as_str(),
                drop_retries,
                decides,
                answers: answers.calling(calls),
                callers: callers.clone(),
                observe,
                caller_header,
                proxy_secret,
            },
        );
    }
    if observe.is_some() {
        bail!(
            "nothing in {} requires ingress.network, so --observe would be ignored",
            spec.display()
        );
    }
    if let Some(caller) = callers.first() {
        bail!(
            "nothing in {} requires ingress.network, so --fixed-actor {} would be ignored",
            spec.display(),
            caller.actor
        );
    }
    if let Some(header) = caller_header {
        bail!(
            "nothing in {} requires ingress.network, so --trusted-caller-header {header} would be \
             ignored",
            spec.display()
        );
    }
    if let Some(path) = http {
        bail!(
            "nothing in {} requires ingress.network, so --http {} would be ignored",
            spec.display(),
            path.display()
        );
    }
    if let Some(port) = port {
        bail!(
            "nothing in {} requires ingress.network, so --port {port} would be ignored",
            spec.display()
        );
    }
    if drop_retries {
        bail!(
            "nothing in {} requires ingress.network, and run reading stdin carries no binding, so \
             --drop-retries would be ignored",
            spec.display()
        );
    }
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
            Box::new(SqliteStore::open(path)?)
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
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for line in std::io::stdin().lock().lines() {
        let line = line.context("reading stdin")?;
        if line.trim().is_empty() {
            continue;
        }
        let result = match serde_json::from_str::<Request>(&line) {
            Err(error) => serde_json::json!({
                "failure": {"kind": "invalid_request", "message": error.to_string()},
            }),
            Ok(request) => answer(
                &loaded.ir,
                store.as_mut(),
                answers.answers(),
                &calls,
                &request,
            ),
        };
        writeln!(out, "{result}")?;
        out.flush()?;
    }
    Ok(ExitCode::SUCCESS)
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

/// The result line for one request: a cause bound to an HTTP call is answered by it, any other by
/// the line's own `external` object, then as `answers` answer it.
fn answer(
    ir: &spindle_spec::EssIr,
    store: &mut dyn Store,
    answers: &Answers,
    calls: &calls::Calls,
    request: &Request,
) -> serde_json::Value {
    let mut decider = |question: &External<'_>| {
        calls
            .answer(question)
            .unwrap_or_else(|| answers.decide(&request.external, question))
    };
    let invocation = spindle_kernel::Invocation {
        command: &request.command,
        input: &request.input,
        actor: request.actor.as_deref(),
        caller: request.caller.as_ref(),
    };
    match spindle_kernel::execute_as(
        ir,
        store,
        &mut decider,
        &mut spindle_kernel::UuidV4,
        &std::time::SystemTime::now,
        &invocation,
    ) {
        Ok(executed) => {
            let mut result = serde_json::json!({"command": request.command});
            if let (Some(object), Ok(serde_json::Value::Object(fields))) =
                (result.as_object_mut(), serde_json::to_value(&executed))
            {
                object.extend(fields);
            }
            result
        }
        Err(error) => serde_json::json!({
            "command": request.command,
            "failure": {"kind": error.kind(), "message": error.to_string()},
        }),
    }
}

/// One line per command the kernel refuses whole and per other outcome it does not take, of each
/// command `component` accepts or, without one, of every command, by command name. A command the
/// kernel cannot select among the outcomes of ([`spindle_kernel::selects`]), or none of whose
/// outcomes it takes ([`spindle_kernel::takes`]), is named once as not served, with the first
/// refusal: every request of it is refused, as `run` refuses it. Each outcome the kernel does not
/// take of any other command is named with what it lacks: a request that selects it is refused,
/// and every other outcome of its command runs.
fn unsupported_outcomes(ir: &spindle_spec::EssIr, component: Option<&str>) -> Vec<String> {
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
                spindle_kernel::takes(ir, command, outcome)
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

fn requirements(spec: &std::path::Path, component: Option<&str>) -> Result<Requirements> {
    let loaded = spindle_spec::load(spec)?;
    spindle_capability::derive(&loaded.ir, component)
        .with_context(|| format!("{} declares no component {component:?}", spec.display()))
}
