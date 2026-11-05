//! Running an ESS conformance suite against Spindle's executor, in one process.
//!
//! The suite is ESS's: synthesised from a specification by [`synthesize`] (what `ess conform
//! synthesize` writes) or read from a file, and admitted by [`AdmittedSuite`]. Its steps are run
//! and compared by ESS's own [`Runner`] against [`HostTarget`], every component of the system in
//! one [`Host`](spindle_host::Host) on empty in-memory stores. Spindle compares nothing itself;
//! what holds is the runner's answer.
//!
//! A scenario the target cannot carry out is reported `unsupported` by the runner, with the
//! target's reason. [`Run`] counts it as skipped, apart from the executed scenarios: those that
//! passed, failed or could not be carried out (`error`). A run passes when it executed at least
//! one scenario and every one it executed passed; with an `ess-known-failures/1` declaration
//! ([`Run::account`], ESS's own accounting), when every one that failed is declared and every
//! declared one failed. [`Counts`] holds a run's executed, skipped and known-failed counts against
//! committed ones.

mod target;

use std::io::{self, Write};

use ess_compiler::EssIr;
use ess_conformance::counts::CountReport;
use ess_conformance::known_failures::{self, Accounting, Declaration, Refusal as KnownRefusal};
use ess_conformance::{
    AdmissionError, AdmittedSuite, AdvancingClock, ConformanceReport, ConformanceTarget,
    ExecutedRun, Ids, Runner, RunnerConfig, ScenarioResult, Status, Synthesis,
};
use spindle_capability::Environment;

pub use target::HostTarget;

/// The format of a committed counts document.
pub const COUNTS_FORMAT: &str = "spindle-conformance-counts/1";

/// The store implementation every run keeps its state in.
const MEMORY: &str = "memory";

/// The suite `ir` obliges, and every construct it could not write a scenario for.
///
/// The suite's format is the newest the specification's constructs need, as `ess conform
/// synthesize` writes it.
pub fn synthesize(ir: &EssIr) -> Synthesis {
    let mut synthesis = ess_conformance::synthesize(ir);
    synthesis.suite.select_fresh_format_for(ir);
    synthesis
}

/// `environment` with every store capability it provides bound to memory only.
pub fn in_memory(environment: &Environment) -> Environment {
    let mut memory = environment.clone();
    for (capability, implementations) in &mut memory.provides {
        if capability.starts_with("store.") {
            *implementations = vec![MEMORY.to_owned()];
        }
    }
    memory
}

/// Runs every scenario of `suite` against `target`.
pub fn execute<T: ConformanceTarget>(suite: &AdmittedSuite, target: &T) -> Run {
    let executed = Runner::new(
        RunnerConfig::default(),
        AdvancingClock::default(),
        Ids::for_suite(suite.suite()),
    )
    .run_admitted(suite, target);
    Run { executed }
}

/// One run of a suite: the runner's report.
pub struct Run {
    /// What the runner reported, scenario by scenario.
    pub executed: ExecutedRun,
}

/// How many scenarios of a run came to what.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    /// Every scenario of the suite.
    pub total: usize,
    /// The scenarios the target carried out or tried to: passed, failed and error.
    pub executed: usize,
    /// Executed and passed.
    pub passed: usize,
    /// Executed and contradicted by the target.
    pub failed: usize,
    /// Executed, and a step could not be carried out.
    pub error: usize,
    /// Not executed: the target cannot expose what the scenario needs (`unsupported`).
    pub skipped: usize,
    /// Of the failed, those a known-failures declaration names ([`Run::counts_with`]).
    pub known_failed: usize,
}

impl Run {
    /// The runner's report.
    pub fn report(&self) -> &ConformanceReport {
        &self.executed
    }

    /// How many scenarios came to what.
    pub fn counts(&self) -> Counts {
        let mut counts = Counts {
            total: self.executed.scenarios.len(),
            ..Counts::default()
        };
        for result in &self.executed.scenarios {
            match result.status {
                Status::Passed => counts.passed += 1,
                Status::Failed => counts.failed += 1,
                Status::Error => counts.error += 1,
                Status::Unsupported => counts.skipped += 1,
            }
        }
        counts.executed = counts.passed + counts.failed + counts.error;
        counts
    }

    /// Whether the run executed a scenario and every one it executed passed.
    pub fn passed(&self) -> bool {
        let counts = self.counts();
        counts.executed > 0 && counts.executed == counts.passed
    }

    /// The exit status of the run: 0 when it [passed](Self::passed), 1 otherwise.
    pub fn exit_status(&self) -> u8 {
        u8::from(!self.passed())
    }

    /// Writes one line per scenario that did not pass, then the counts.
    ///
    /// A failed or errored scenario is `failed <id>` or `error <id>`, followed by each check
    /// that did not pass with what was expected and observed. A skipped one is
    /// `skipped <id>: <reason>`, the target's reason.
    ///
    /// # Errors
    /// When `out` cannot be written.
    pub fn render(&self, out: &mut dyn Write) -> io::Result<()> {
        for result in &self.executed.scenarios {
            match result.status {
                Status::Passed => {}
                Status::Unsupported => {
                    writeln!(out, "skipped {}: {}", result.scenario, reason(result))?;
                }
                Status::Failed | Status::Error => {
                    writeln!(out, "{} {}", result.status.as_str(), result.scenario)?;
                    for check in result
                        .checks
                        .iter()
                        .filter(|check| check.status != Status::Passed)
                    {
                        writeln!(out, "  {check}")?;
                        if let Some(diagnostic) = &check.diagnostic {
                            for line in diagnostic.to_string().lines() {
                                writeln!(out, "    {line}")?;
                            }
                        }
                    }
                }
            }
        }
        let counts = self.counts();
        writeln!(
            out,
            "{} scenario(s): {} executed, {} passed, {} failed, {} error, {} skipped",
            counts.total,
            counts.executed,
            counts.passed,
            counts.failed,
            counts.error,
            counts.skipped
        )
    }
}

/// Why `result` was skipped: the first thing the target said of a check it could not support.
fn reason(result: &ScenarioResult) -> String {
    result
        .checks
        .iter()
        .find(|check| check.status == Status::Unsupported)
        .map(|check| {
            check
                .diagnostic
                .as_ref()
                .and_then(|diagnostic| diagnostic.observed.first().cloned())
                .unwrap_or_else(|| check.about.clone())
        })
        .filter(|reason| !reason.trim().is_empty())
        .unwrap_or_else(|| "the runner recorded no reason".to_owned())
}

// ---- known failures --------------------------------------------------------------------------

/// The public build identity a known-failures declaration binds a run of this crate to:
/// `sha256:` of `b10x-spindle <version>`.
///
/// ESS's own runner binds to the SHA-256 of its executable. A Spindle declaration is committed
/// beside the specification and checked on every host and every rebuild, so it binds to the
/// released version instead: two builds of one version are one build here.
pub fn build() -> String {
    known_failures::sha256(format!("b10x-spindle {}", env!("CARGO_PKG_VERSION")).as_bytes())
}

/// What a run comes to against an `ess-known-failures/1` declaration, as ESS accounts it.
#[derive(Debug, Clone)]
pub struct Known {
    /// ESS's accounting: the declared failures and the failures the declaration does not name.
    pub accounting: Accounting,
}

impl Run {
    /// The report/2 of this run of `suite`, as canonical JSON.
    ///
    /// # Errors
    /// ESS's refusal when the run is not one of `suite`.
    pub fn report_json(&self, suite: &AdmittedSuite) -> Result<String, AdmissionError> {
        CountReport::from_run(&self.executed, suite)?.to_canonical_json()
    }

    /// Accounts this run of `suite` against `declaration`, through ESS's
    /// [`known_failures::account`] at [`build`].
    ///
    /// # Errors
    /// ESS's refusal: a declaration of another specification, suite, implementation or build, a
    /// declared scenario that passed (stale) or ended any other way than `failed`, or a report
    /// ESS cannot read back.
    pub fn account(
        &self,
        suite: &AdmittedSuite,
        declaration: &Declaration,
    ) -> Result<Known, KnownRefusal> {
        let report = self.report_json(suite).map_err(|error| {
            KnownRefusal::new(
                known_failures::RefusalKind::Malformed,
                format!("ess-conformance-report/2: {error}"),
            )
        })?;
        known_failures::account(&report, suite, declaration, &build())
            .map(|accounting| Known { accounting })
    }

    /// Whether the run passes with `known` declared: it executed a scenario, none ended `error`,
    /// and every one that failed is declared.
    pub fn passed_with(&self, known: &Known) -> bool {
        let counts = self.counts();
        counts.executed > 0
            && counts.error == 0
            && known.accounting.unexpected_failed.is_empty()
            && counts.failed == known.accounting.known_failed.len()
    }

    /// [`Self::counts`], with the declared failures counted apart.
    pub fn counts_with(&self, known: &Known) -> Counts {
        Counts {
            known_failed: known.accounting.known_failed.len(),
            ..self.counts()
        }
    }
}

// ---- held counts ------------------------------------------------------------------------------

/// The counts a run is held to, as committed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Committed {
    /// The fewest scenarios executed.
    pub executed: usize,
    /// The most skipped.
    pub skipped: usize,
    /// The most failures a known-failures declaration names.
    pub known_failed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Held {
    format: String,
    executed: usize,
    skipped: usize,
    known_failed: usize,
}

/// Why a counts document could not be read.
#[derive(Debug, thiserror::Error)]
pub enum CountsError {
    /// Not TOML of the expected shape.
    #[error("not a counts document: {0}")]
    Shape(#[from] toml::de::Error),
    /// Another format.
    #[error("format `{0}`, expected `{COUNTS_FORMAT}`")]
    Format(String),
}

impl Counts {
    /// The counts of a committed counts document.
    ///
    /// # Errors
    /// [`CountsError`] for a document of another shape or format.
    pub fn committed(text: &str) -> Result<Committed, CountsError> {
        let held: Held = toml::from_str(text)?;
        if held.format != COUNTS_FORMAT {
            return Err(CountsError::Format(held.format));
        }
        Ok(Committed {
            executed: held.executed,
            skipped: held.skipped,
            known_failed: held.known_failed,
        })
    }

    /// Every way these counts fall short of `committed`: fewer scenarios executed, more skipped,
    /// or more declared known failures. Empty when they are held.
    pub fn drops(&self, committed: Committed) -> Vec<String> {
        let mut drops = Vec::new();
        if self.executed < committed.executed {
            drops.push(format!(
                "executed dropped from {} to {}",
                committed.executed, self.executed
            ));
        }
        if self.skipped > committed.skipped {
            drops.push(format!(
                "skipped grew from {} to {}",
                committed.skipped, self.skipped
            ));
        }
        if self.known_failed > committed.known_failed {
            drops.push(format!(
                "known failures grew from {} to {}",
                committed.known_failed, self.known_failed
            ));
        }
        drops
    }

    /// Whether these counts are exactly `committed`.
    pub fn equal(&self, committed: Committed) -> bool {
        (self.executed, self.skipped, self.known_failed)
            == (
                committed.executed,
                committed.skipped,
                committed.known_failed,
            )
    }

    /// These counts as a committed counts document.
    pub fn document(&self) -> String {
        format!(
            "format = \"{COUNTS_FORMAT}\"\nexecuted = {}\nskipped = {}\nknown_failed = {}\n",
            self.executed, self.skipped, self.known_failed
        )
    }
}
