//! `b10x-spindle`: load an ESS specification, derive what it needs, admit it, and run or play it.

mod decisions;
mod replay;
mod serve;

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
use spindle_cli::cli::{Cli, Command};
use spindle_host::{Host, HostError, SystemClock, rally};
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
            let requirements = requirements(&spec, component.as_deref())?;
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
            scenario: None,
        } => execute(&Execute {
            spec: &spec,
            environment: &environment,
            state: state.as_deref(),
            decisions: decisions.as_deref(),
            answer_default,
            http: http.as_deref(),
            port,
        }),
        Command::Play {
            spec,
            environment,
            target_score,
            match_id,
        } => play(&spec, &environment, target_score, &match_id),
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

fn play(spec: &Path, environment: &Path, target_score: i64, match_id: &str) -> Result<ExitCode> {
    let signalled = signals()?;
    let loaded = spindle_spec::load(spec)?;
    let text = std::fs::read_to_string(environment)
        .with_context(|| format!("reading {}", environment.display()))?;
    let parsed =
        Environment::parse(&text).with_context(|| format!("in {}", environment.display()))?;
    let state = tempfile::tempdir().context("creating the state directory")?;
    let clock = std::sync::Arc::new(SystemClock);
    let mut host = match Host::new(loaded.ir, &parsed, clock, state.path()) {
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
        if let Some(signal) = wait_until(deadline, &signalled) {
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

/// Sleeps until `deadline`, or until a signal is recorded in `signalled`; returns its number.
fn wait_until(deadline: Instant, signalled: &AtomicUsize) -> Option<u8> {
    loop {
        let signal = signalled.load(Ordering::SeqCst);
        if signal != 0 {
            return Some(u8::try_from(signal).unwrap_or(u8::MAX - SIGNALLED));
        }
        let left = deadline.saturating_duration_since(Instant::now());
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
    } = *arguments;
    let loaded = spindle_spec::load(spec)?;
    let answers = Answers::load(&loaded.ir, decisions, answer_default)?;
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
        if decisions.is_some() || answer_default {
            bail!(
                "--decisions and --answer-default answer external causes of stdin lines, and run \
                 serving ingress.network reads no stdin"
            );
        }
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
            },
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
        (Some("sqlite"), Some(path)) => Box::new(SqliteStore::open(path)?),
        (Some("sqlite"), None) => {
            bail!(
                "store.durable is bound to sqlite on {}: name the state file with --state",
                parsed.name
            )
        }
        (Some(other), _) => bail!("this build has no `{other}` store for store.durable"),
    };

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
            Ok(request) => answer(&loaded.ir, store.as_mut(), &answers, &request),
        };
        writeln!(out, "{result}")?;
        out.flush()?;
    }
    Ok(ExitCode::SUCCESS)
}

/// The result line for one request.
fn answer(
    ir: &spindle_spec::EssIr,
    store: &mut dyn Store,
    answers: &Answers,
    request: &Request,
) -> serde_json::Value {
    let mut decider = |question: &External<'_>| answers.decide(&request.external, question);
    match spindle_kernel::execute(ir, store, &mut decider, &request.command, &request.input) {
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

fn requirements(spec: &std::path::Path, component: Option<&str>) -> Result<Requirements> {
    let loaded = spindle_spec::load(spec)?;
    spindle_capability::derive(&loaded.ir, component)
        .with_context(|| format!("{} declares no component {component:?}", spec.display()))
}
