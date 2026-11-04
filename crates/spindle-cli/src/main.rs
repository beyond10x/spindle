//! `b10x-spindle`: load an ESS specification, derive what it needs, and admit it.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use spindle_capability::{Admission, Environment, Requirements};

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
    }
}

fn requirements(spec: &std::path::Path, component: Option<&str>) -> Result<Requirements> {
    let loaded = spindle_spec::load(spec)?;
    spindle_capability::derive(&loaded.ir, component)
        .with_context(|| format!("{} declares no component {component:?}", spec.display()))
}
