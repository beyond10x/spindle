//! `conform`: a specification's conformance suite, run against every component of the system in
//! one process.
//!
//! The suite is synthesised from the specification by ESS, with the authored scenarios its
//! `ess-inputs.yaml` lists ([`spindle_conformance::synthesize`]), or read from `--suite`, and run
//! by ESS's runner against [`HostTarget`]. Every construct the synthesis refused is printed with
//! its code first, so a scenario the suite does not hold is not silently absent.

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result};
use ess_conformance::counts::CountRun;
use ess_conformance::known_failures::Declaration;
use ess_conformance::{AdmittedSuite, SuiteProvenance};
use spindle_capability::Environment;
use spindle_conformance::{Counts, HostTarget};
use spindle_host::HostError;

/// Exit status of a run the environment refused.
const REFUSED: u8 = 3;

/// Exit status of a run whose known-failures declaration ESS refused, as ESS's own runner exits.
const DECLARATION_REFUSED: u8 = 2;

/// The arguments of `conform`.
pub(crate) struct Conform<'a> {
    pub(crate) spec: &'a Path,
    pub(crate) environment: &'a Path,
    pub(crate) suite: Option<&'a Path>,
    pub(crate) report: Option<&'a Path>,
    pub(crate) results: Option<&'a Path>,
    pub(crate) counts: Option<&'a Path>,
    pub(crate) known_failing: Option<&'a Path>,
    pub(crate) accounting_out: Option<&'a Path>,
}

/// Runs the suite; exits 0 when every executed scenario passed, or failed as `--known-failing`
/// declares, and the counts are held; 1 otherwise, when ESS refuses an authored scenario the
/// specification lists, or when the suite is not one of this specification; 2 when ESS refuses
/// the declaration, a declared scenario that passed included; and 3 when admission is refused.
pub(crate) fn conform(arguments: &Conform<'_>) -> Result<ExitCode> {
    let loaded = spindle_spec::load(arguments.spec)?;
    let text = std::fs::read_to_string(arguments.environment)
        .with_context(|| format!("reading {}", arguments.environment.display()))?;
    let environment = Environment::parse(&text)
        .with_context(|| format!("in {}", arguments.environment.display()))?;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let synthesis = match spindle_conformance::synthesize(&loaded) {
        Ok(synthesis) => synthesis,
        Err(refused) => {
            writeln!(out, "{refused}")?;
            out.flush()?;
            return Ok(ExitCode::FAILURE);
        }
    };
    for refusal in &synthesis.refusals {
        writeln!(out, "refused: {}", refusal.to_string().trim_end())?;
    }
    writeln!(out, "{} refusal(s)", synthesis.refusals.len())?;

    let suite = match arguments.suite {
        None => AdmittedSuite::from_suite(&synthesis.suite)
            .context("admitting the synthesised suite")?,
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading {}", path.display()))?;
            let suite = AdmittedSuite::from_json(&text)
                .with_context(|| format!("admitting {}", path.display()))?;
            let expected = SuiteProvenance::of(&loaded.ir);
            let provenance = &suite.suite().provenance;
            if provenance.system != expected.system
                || provenance.spec_digest != expected.spec_digest
            {
                eprintln!(
                    "b10x-spindle: {} is a suite of {} {}, not of {}",
                    path.display(),
                    provenance.system,
                    provenance.spec_digest,
                    arguments.spec.display()
                );
                return Ok(ExitCode::FAILURE);
            }
            suite
        }
    };

    // The declaration binds this run before the target is made, as ESS's own runner binds it.
    let declaration = match arguments.known_failing {
        None => None,
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading {}", path.display()))?;
            let bound = Declaration::from_json(&text).and_then(|declaration| {
                declaration.admit(&suite)?;
                declaration.bind_build(&spindle_conformance::build())?;
                Ok(declaration)
            });
            match bound {
                Ok(declaration) => Some(declaration),
                Err(refusal) => {
                    writeln!(out, "{}: {refusal}", path.display())?;
                    out.flush()?;
                    return Ok(ExitCode::from(DECLARATION_REFUSED));
                }
            }
        }
    };

    let target = HostTarget::new(
        loaded.ir.clone(),
        spindle_conformance::in_memory(&environment),
    );
    match target.open() {
        Ok(()) => {}
        Err(HostError::Refused(refusal)) => {
            writeln!(out, "{refusal}")?;
            return Ok(ExitCode::from(REFUSED));
        }
        Err(error) => return Err(error.into()),
    }
    let run = spindle_conformance::execute(&suite, &target);
    run.render(&mut out)?;

    let known = match (&declaration, arguments.known_failing) {
        (Some(declaration), Some(path)) => match run.account(&suite, declaration) {
            Ok(known) => Some(known),
            Err(refusal) => {
                writeln!(out, "{}: {refusal}", path.display())?;
                out.flush()?;
                return Ok(ExitCode::from(DECLARATION_REFUSED));
            }
        },
        _ => None,
    };

    if let Some(path) = arguments.report {
        let report = run.report_json(&suite).context("writing the report")?;
        write(path, &report)?;
    }
    if let Some(path) = arguments.results {
        let results = CountRun::from_run(&run.executed, &suite)
            .and_then(|results| results.to_canonical_json())
            .context("writing the results")?;
        write(path, &results)?;
    }
    if let Some(known) = &known {
        write!(out, "{}", known.accounting.render_text())?;
        if let Some(path) = arguments.accounting_out {
            write(path, &known.accounting.to_canonical_json())?;
        }
    }

    let (passed, counts) = match &known {
        Some(known) => (run.passed_with(known), run.counts_with(known)),
        None => (run.passed(), run.counts()),
    };
    let mut held = true;
    if let Some(path) = arguments.counts {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let committed =
            Counts::committed(&text).with_context(|| format!("in {}", path.display()))?;
        let mut drops = run.set_drops(&committed);
        drops.extend(counts.drops(committed.clone()));
        for drop in &drops {
            writeln!(out, "{}: {drop}", path.display())?;
        }
        held = drops.is_empty();
        if held {
            writeln!(out, "counts held against {}", path.display())?;
        }
        if !run.holds_exactly(&counts, &committed) {
            write!(
                out,
                "the counts moved; where the move is intended, commit this as {}:\n{}",
                path.display(),
                run.document(&counts)
            )?;
        }
    }
    out.flush()?;
    Ok(if passed && held {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// Writes `text` to `path`, creating the directories it is in.
fn write(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}
