//! `b10x-spindle`: load an ESS specification, derive what it needs, and admit it.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use spindle_capability::{Admission, Environment, Requirements};
use spindle_kernel::{External, MemoryStore, SqliteStore, Store};

/// Exit status of an `admit` the environment refused.
const REFUSED: u8 = 3;

#[derive(Debug, Parser)]
#[command(
    name = "b10x-spindle",
    version,
    about = "Run ESS specifications on the capabilities an environment provides"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Compile a specification and print what it declares.
    Check {
        /// The specification directory (holding `system.yaml`).
        spec: PathBuf,
        /// Print JSON instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Print the capabilities a specification, or one component of it, requires, and why.
    Capabilities {
        /// The specification directory.
        spec: PathBuf,
        /// Only what this component needs when it runs on its own.
        #[arg(long)]
        component: Option<String>,
        /// Print JSON instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Admit a specification against an environment: bind every capability or name what is missing.
    ///
    /// Exits 0 when admitted and 3 when refused.
    Admit {
        /// The specification directory.
        spec: PathBuf,
        /// The `spindle-environment/1` document.
        #[arg(long)]
        environment: PathBuf,
        /// Admit only this component.
        #[arg(long)]
        component: Option<String>,
        /// Print JSON instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Admit a specification, then execute commands read as JSON lines on stdin.
    ///
    /// Each line is `{"command": "<qualified name>", "input": {...}}`, with an optional
    /// `"external": {"<outcome>": true | false}` answering the command's external causes. Each
    /// line prints one JSON result: the outcome with its events and declared error, or a failure.
    /// Exits 3, running nothing, when admission is refused.
    Run {
        /// The specification directory.
        spec: PathBuf,
        /// The `spindle-environment/1` document.
        #[arg(long)]
        environment: PathBuf,
        /// The state file: required where `store.durable` is bound to `sqlite`, refused otherwise.
        #[arg(long)]
        state: Option<PathBuf>,
    },
}

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
            state,
        } => execute(&spec, &environment, state.as_deref()),
    }
}

fn execute(spec: &Path, environment: &Path, state: Option<&Path>) -> Result<ExitCode> {
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
    let durable = bindings
        .iter()
        .find(|binding| binding.capability.as_str() == "store.durable")
        .map(|binding| binding.implementation.as_str());
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
            Ok(request) => answer(&loaded.ir, store.as_mut(), &request),
        };
        writeln!(out, "{result}")?;
        out.flush()?;
    }
    Ok(ExitCode::SUCCESS)
}

/// The result line for one request.
fn answer(ir: &spindle_spec::EssIr, store: &mut dyn Store, request: &Request) -> serde_json::Value {
    let mut decider = |question: &External<'_>| {
        request
            .external
            .get(question.outcome)
            .copied()
            .ok_or_else(|| {
                format!(
                    "the request answers no `external.{}` for `{}`: {}",
                    question.outcome, question.capability, question.cause
                )
            })
    };
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
